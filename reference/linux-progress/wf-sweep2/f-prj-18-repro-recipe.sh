#!/bin/bash
# F-PRJ-18 repro recipe: Worktree Location "Choose..." folder picker via the
# xdg-desktop-portal FileChooser, PLUS the root-cause fix for the
# previously-documented dialog-never-appears failure mode.
#
# ROOT CAUSE of the historical hang/no-dialog symptom: xdg-desktop-portal
# picks a backend implementation (gtk.portal vs cosmic.portal vs ...) for
# each interface (FileChooser, Account, ...) by reading $XDG_CURRENT_DESKTOP
# from ITS OWN process environment at startup -- not from the D-Bus
# "activation environment" set via `dbus-update-activation-environment`,
# which only affects environments handed to NEWLY dbus-activated processes,
# not an already-running daemon, and not even a freshly manually-started one
# unless the variable is in *that process's own* env at exec time.
#
# Earlier waves' recipe called:
#   dbus-update-activation-environment --verbose WAYLAND_DISPLAY=... \
#     GDK_BACKEND=wayland XDG_RUNTIME_DIR=... DBUS_SESSION_BUS_ADDRESS=...
# -- with NO XDG_CURRENT_DESKTOP. Without it, xdg-desktop-portal cannot
# resolve which portal implementation provides org.freedesktop.impl.portal.
# FileChooser, so it never spawns xdg-desktop-portal-gtk (or any backend)
# for that interface at all. The click on "Choose..." goes out over D-Bus,
# there is no backend listening, and nothing visibly happens -- no error
# dialog, no crash, just silence. That is indistinguishable, from the
# outside, from "the picker hung".
#
# THE FIX: XDG_CURRENT_DESKTOP=GNOME must be in the xdg-desktop-portal
# daemon's OWN environment when it execs, not merely in the D-Bus
# activation environment.
#
# This script assumes: a private `dbus-daemon --session` bus is already
# running (address in $DBUS_SESSION_BUS_ADDRESS), and a nested sway
# compositor is already up on $WAYLAND_DISPLAY.

set -uo pipefail

: "${DBUS_SESSION_BUS_ADDRESS:?export the private session bus address first}"
: "${WAYLAND_DISPLAY:?export the compositor's WAYLAND_DISPLAY first}"
: "${XDG_RUNTIME_DIR:=/run/user/1000}"

# (Optional but harmless) still set the activation environment for anything
# that IS dbus-activated later -- this alone is NOT sufficient, see above.
dbus-update-activation-environment --verbose \
  WAYLAND_DISPLAY="$WAYLAND_DISPLAY" \
  GDK_BACKEND=wayland \
  XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" \
  DBUS_SESSION_BUS_ADDRESS="$DBUS_SESSION_BUS_ADDRESS" \
  XDG_CURRENT_DESKTOP=GNOME

# If an xdg-desktop-portal is already running on this bus WITHOUT
# XDG_CURRENT_DESKTOP in its own environ (check: `tr '\0' '\n' <
# /proc/<pid>/environ | grep XDG_CURRENT_DESKTOP`), kill it -- it is safe to
# kill your own private-bus portal daemon, it owns no state -- then relaunch
# it with the variable set directly in its own exec environment:

XDG_CURRENT_DESKTOP=GNOME WAYLAND_DISPLAY="$WAYLAND_DISPLAY" GDK_BACKEND=wayland \
  nohup /usr/libexec/xdg-desktop-portal -v > /tmp/portal.log 2>&1 &

sleep 1.5

# Verify: the log should now show
#   "Using gtk.portal for org.freedesktop.impl.portal.FileChooser in gnome"
#   "providing portal org.freedesktop.portal.FileChooser"
# and `ps aux | grep xdg-desktop-portal-gtk` should show a NEW backend
# process bound to the same DBUS_SESSION_BUS_ADDRESS (verify via
# `tr '\0' '\n' < /proc/<pid>/environ | grep DBUS_SESSION_BUS_ADDRESS`).
grep -E "Using gtk.portal for org.freedesktop.impl.portal.FileChooser|providing portal org.freedesktop.portal.FileChooser" /tmp/portal.log

# From here, driving the app's "Choose..." button opens a real GTK "Open
# Folder" window (title "Open Folder", floating_con covering the whole
# output in this headless/no-titlebar sway setup -- no extra `swaymsg
# floating enable`/`move position` treatment was needed this time, unlike
# the historical portal-picker recipe for other rows: this dialog rendered
# already positioned at (0,0) full-output size).
#
# Typing a path directly into the GTK location bar (Ctrl+L) is UNRELIABLE
# in this environment -- BackSpace is a confirmed no-op via `wtype -k
# BackSpace` (3x sent, zero characters removed, verified via screenshot),
# and fast `wtype "text"` calls can drop/merge characters ("/tmp/" -> "/p/"
# was observed once). The reliable path: click the search icon (top-right
# magnifying glass), type the target directory's bare name into the search
# box (search-box typing did NOT drop characters), single-click the exactly
# one match that appears, then click the accept button (labelled with the
# app's own dialog title, e.g. "Choose a folder for new worktrees").
