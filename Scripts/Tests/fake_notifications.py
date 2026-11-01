#!/usr/bin/env python3
"""A recording stand-in for a desktop's notification server, for the chat session panel E2E
(Scripts/Tests/test-chat-session-panel-e2e.sh, scenario notification_click).

    fake_notifications.py serve --log FILE
    fake_notifications.py click --id N

`serve` owns org.freedesktop.Notifications on the session bus named by DBUS_SESSION_BUS_ADDRESS
(the E2E's private one) and implements the Desktop Notifications Specification 1.2 interface at
/org/freedesktop/Notifications:

- Notify(susssasa{sv}i) -> u: app_name, replaces_id, app_icon, summary, body, actions (a flat
  list of key/label pairs), hints, expire_timeout. It returns an id from 1 up, or replaces_id
  when that names a notification the server holds, and appends one JSON line to FILE:
  {"id", "app_name", "summary", "body", "actions"}.
- GetCapabilities -> as: ["actions", "body"].
- GetServerInformation -> (name, vendor, version, spec_version).
- CloseNotification(u): emits NotificationClosed(id, 3), "closed by a call to
  CloseNotification"; an id the server does not hold is an error.

and one method of its own, on the same object:

- InvokeDefault(u): emits ActionInvoked(id, "default") and then NotificationClosed(id, 2),
  "dismissed by the user", which is what a desktop sends when its user clicks a notification
  that offers a `default` action. An id the server does not hold, or one that offered no
  `default` action, is an error: a desktop has nothing to click there. The notification stays
  held (only CloseNotification drops it), so the caller can repeat a click that was lost.

`serve` prints `ready` on stdout once it owns the name and runs until SIGTERM or SIGINT.
`click` calls InvokeDefault over the same bus and exits 0, or 1 with the error on stderr.

The signals are broadcasts, as a real server sends them: the client (notify-rust, under gpui)
receives one only after it has added its match rule, which it does just after the Notify
reply. A click sent in that gap is lost, as it would be on a desktop; the caller repeats it.

Only the standard library and PyGObject's Gio are used.
"""

import argparse
import json
import signal
import sys

import gi

gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib  # noqa: E402

BUS_NAME = "org.freedesktop.Notifications"
OBJECT_PATH = "/org/freedesktop/Notifications"
INTERFACE = "org.freedesktop.Notifications"

# NotificationClosed reasons, from the specification.
REASON_DISMISSED = 2
REASON_CLOSED_BY_CALL = 3

INTROSPECTION = """
<node>
  <interface name="org.freedesktop.Notifications">
    <method name="Notify">
      <arg type="s" name="app_name" direction="in"/>
      <arg type="u" name="replaces_id" direction="in"/>
      <arg type="s" name="app_icon" direction="in"/>
      <arg type="s" name="summary" direction="in"/>
      <arg type="s" name="body" direction="in"/>
      <arg type="as" name="actions" direction="in"/>
      <arg type="a{sv}" name="hints" direction="in"/>
      <arg type="i" name="expire_timeout" direction="in"/>
      <arg type="u" name="id" direction="out"/>
    </method>
    <method name="CloseNotification">
      <arg type="u" name="id" direction="in"/>
    </method>
    <method name="GetCapabilities">
      <arg type="as" name="capabilities" direction="out"/>
    </method>
    <method name="GetServerInformation">
      <arg type="s" name="name" direction="out"/>
      <arg type="s" name="vendor" direction="out"/>
      <arg type="s" name="version" direction="out"/>
      <arg type="s" name="spec_version" direction="out"/>
    </method>
    <method name="InvokeDefault">
      <arg type="u" name="id" direction="in"/>
    </method>
    <signal name="NotificationClosed">
      <arg type="u" name="id"/>
      <arg type="u" name="reason"/>
    </signal>
    <signal name="ActionInvoked">
      <arg type="u" name="id"/>
      <arg type="s" name="action_key"/>
    </signal>
  </interface>
</node>
"""


class Server:
    def __init__(self, connection, log_path):
        self.connection = connection
        self.log = open(log_path, "a", buffering=1)
        self.next_id = 1
        self.held = {}  # id -> the actions it offered

    def emit(self, name, signature, values):
        self.connection.emit_signal(
            None, OBJECT_PATH, INTERFACE, name, GLib.Variant(signature, values)
        )

    def error(self, invocation, text):
        invocation.return_dbus_error("org.freedesktop.DBus.Error.InvalidArgs", text)

    def on_method_call(self, _connection, _sender, _path, _interface, method, parameters, invocation):
        if method == "Notify":
            (app_name, replaces_id, _icon, summary, body, actions, _hints, _timeout) = parameters.unpack()
            if replaces_id and replaces_id in self.held:
                notification_id = replaces_id
            else:
                notification_id = self.next_id
                self.next_id += 1
            self.held[notification_id] = list(actions)
            self.log.write(
                json.dumps(
                    {
                        "id": notification_id,
                        "app_name": app_name,
                        "summary": summary,
                        "body": body,
                        "actions": list(actions),
                    }
                )
                + "\n"
            )
            invocation.return_value(GLib.Variant("(u)", (notification_id,)))
        elif method == "GetCapabilities":
            invocation.return_value(GLib.Variant("(as)", (["actions", "body"],)))
        elif method == "GetServerInformation":
            invocation.return_value(
                GLib.Variant("(ssss)", ("fake-notifications", "sirio-e2e", "1.0", "1.2"))
            )
        elif method == "CloseNotification":
            (notification_id,) = parameters.unpack()
            if notification_id not in self.held:
                return self.error(invocation, f"no notification {notification_id}")
            del self.held[notification_id]
            invocation.return_value(None)
            self.emit("NotificationClosed", "(uu)", (notification_id, REASON_CLOSED_BY_CALL))
        elif method == "InvokeDefault":
            (notification_id,) = parameters.unpack()
            if notification_id not in self.held:
                return self.error(invocation, f"no notification {notification_id}")
            # Even keys of the flat action list are the action ids, odd ones their labels.
            if "default" not in self.held[notification_id][::2]:
                return self.error(invocation, f"notification {notification_id} offered no default action")
            invocation.return_value(None)
            self.emit("ActionInvoked", "(us)", (notification_id, "default"))
            # Still held: a repeat of a click the client missed must be served, not refused.
            self.emit("NotificationClosed", "(uu)", (notification_id, REASON_DISMISSED))
        else:
            invocation.return_dbus_error("org.freedesktop.DBus.Error.UnknownMethod", method)


def serve(args):
    connection = Gio.DBusConnection.new_for_address_sync(
        Gio.dbus_address_get_for_bus_sync(Gio.BusType.SESSION, None),
        Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION,
        None,
        None,
    )
    server = Server(connection, args.log)
    info = Gio.DBusNodeInfo.new_for_xml(INTROSPECTION).interfaces[0]
    connection.register_object(OBJECT_PATH, info, server.on_method_call, None, None)
    loop = GLib.MainLoop()

    def acquired(_connection, _name):
        print("ready", flush=True)

    def lost(_connection, name):
        print(f"could not own {name}", file=sys.stderr, flush=True)
        loop.quit()

    Gio.bus_own_name_on_connection(
        connection, BUS_NAME, Gio.BusNameOwnerFlags.DO_NOT_QUEUE, acquired, lost
    )
    for number in (signal.SIGTERM, signal.SIGINT):
        GLib.unix_signal_add(GLib.PRIORITY_DEFAULT, number, loop.quit)
    loop.run()
    return 0


def click(args):
    connection = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    try:
        connection.call_sync(
            BUS_NAME,
            OBJECT_PATH,
            INTERFACE,
            "InvokeDefault",
            GLib.Variant("(u)", (args.id,)),
            None,
            Gio.DBusCallFlags.NONE,
            5000,
            None,
        )
    except GLib.Error as error:
        print(f"InvokeDefault({args.id}) failed: {error.message}", file=sys.stderr)
        return 1
    return 0


def main():
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)
    serve_parser = commands.add_parser("serve")
    serve_parser.add_argument("--log", required=True)
    click_parser = commands.add_parser("click")
    click_parser.add_argument("--id", required=True, type=int)
    args = parser.parse_args()
    return serve(args) if args.command == "serve" else click(args)


if __name__ == "__main__":
    sys.exit(main())
