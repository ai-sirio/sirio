#!/usr/bin/env python3
"""A fake language server for Scripts/Tests/test-editor-e2e.sh (Task 7b).

Sirio starts it as a language server, through a `[[language]]` entry in
`languages.toml` that names `python3 -I fake_lsp.py ANSWERS [LOG]`:

    fake_lsp.py ANSWERS [LOG]

It speaks stdio JSON-RPC with Content-Length framing, and answers only what
the editor's pointer features ask:

- `initialize` advertises hover, definition and document symbols, and full
  text sync. `initialized`, `shutdown` and `exit` are answered as a server
  answers them.
- `textDocument/didOpen` publishes `publishDiagnostics` for that document, and
  so does each `didChange`, unless ANSWERS sets `publish_on_change` to false (a
  test that wants the editor's own moved findings, with no server answer yet,
  sets it false for a while). The findings are the `diagnostics` of ANSWERS,
  but only for the file named by `diagnostics_file` (every other document gets
  an empty list). A finding names its text by `needle`: its range is that
  text's first occurrence in the document as the server holds it, so the
  positions follow the text through every edit. The positions are UTF-16 code
  units, as LSP has them.
- `textDocument/hover` answers the `hover` text of ANSWERS for every position.
- `textDocument/definition` answers the `definition` of ANSWERS: a file's
  `needle`, read from disk when asked, as one Location.
- `textDocument/documentSymbol` answers the `symbols` of ANSWERS, but only for
  the file named by `symbols_file`, a file name or a list of them (every other
  document gets an empty list). A
  symbol is `{"name", "kind", "needle", "children"}`; its range, and its
  selectionRange, is the first occurrence of `needle` in the document as the
  server holds it, so the lines follow the text through every edit. `children`
  are symbols of the same shape, answered nested as `DocumentSymbol` has them. A
  symbol whose needle is not in the text is left out with its children. `kind`
  is the protocol's number (default 12, a function; a method is 6).
- ANSWERS may hold `publish`: `{"<path relative to the root>": [FINDING, ...]}`
  with a FINDING of `{needle, severity, message, source}`. The server then reports
  those files without being asked, as a language server reports its whole project:
  on `initialized` and again whenever ANSWERS changes (a polling thread looks every
  200 ms), one `publishDiagnostics` per file under the root `initialize` named
  (`rootUri`), the needles found in the file on disk, or in the document the client
  opened when it did. A file that left the map is published empty, once; so is
  every open document, as `diagnostics_file` and `publish` now say. When `publish`
  goes away (the answers are put back) the thread publishes everything once more
  and rests, so a test that never sets it sees none of this. `"exit": true` ends
  the process at the next look, as a crashed server ends.
- With `"service_ready": true` in ANSWERS the server sends jdtls's
  `language/status` notification once, right after `initialized` (after the
  first publish, if there is one): `{"type": "ServiceReady", "message":
  "ServiceReady"}`, what jdtls says when it can take requests. The file is read
  at `initialized`, so the answer set when the server starts is the one that counts.
- With `"exit_after_initialized": true` in ANSWERS the server ends right after it
  reads `initialized`, before `ServiceReady` and before it handles anything else or
  publishes: a jdtls that dies while it starts, after `initialize` has answered.
  The file is read at `initialized`, like `service_ready`.
- With `"hold_build": true` in ANSWERS the server never answers `java/buildWorkspace`
  (the request is still logged): a jdtls whose full build has not finished. It goes on
  answering `shutdown` and ends at `exit`, so a client can stop it.
- `java/buildWorkspace` (jdtls's full build) is answered with `1`, the status
  jdtls gives a build that succeeded.
- Any other request is answered with null; notifications are ignored.

ANSWERS is read again for every message, so a test may rewrite it between
requests. Every message the client sends with a method is logged to LOG
(default ANSWERS + ".log"), one line per message: `request <method>` or
`notification <method>`. A client's answer to one of the server's requests is
neither logged nor answered, and a malformed frame is logged as `skipped ...`
and dropped, so the server keeps going. The E2E reads the log to show that the
server was really asked.

Python standard library only; run it as `python3 -I`.
"""

import json
import os
import sys
import threading
import time
import traceback
from pathlib import Path
from urllib.parse import unquote, urlparse


def utf16_position(text, index):
    line = text.count("\n", 0, index)
    line_start = text.rfind("\n", 0, index) + 1
    character = len(text[line_start:index].encode("utf-16-le")) // 2
    return {"line": line, "character": character}


def range_of(text, needle):
    index = text.find(needle)
    if index < 0:
        return None
    return {
        "start": utf16_position(text, index),
        "end": utf16_position(text, index + len(needle)),
    }


def main():
    if len(sys.argv) < 2:
        sys.exit("usage: fake_lsp.py ANSWERS [LOG]")
    answers_path = sys.argv[1]
    log_path = sys.argv[2] if len(sys.argv) > 2 else answers_path + ".log"
    log = open(log_path, "a", encoding="utf-8")
    documents = {}
    incoming = sys.stdin.buffer
    outgoing = sys.stdout.buffer
    # One lock for every write to stdout and every read of `documents`: the main
    # loop and the polling thread share both. Re-entrant, so the thread may hold it
    # across the few sends of one round.
    lock = threading.RLock()
    root = {"path": None}
    published = set()

    def log_line(text):
        with lock:
            log.write(text + "\n")
            log.flush()

    def answers():
        with open(answers_path, "rb") as handle:
            return json.loads(handle.read().decode("utf-8"))

    def send(message):
        body = json.dumps(message).encode("utf-8")
        with lock:
            outgoing.write(b"Content-Length: %d\r\n\r\n" % len(body) + body)
            outgoing.flush()

    def read_message():
        # Headers, a blank line, then Content-Length bytes. A malformed header is
        # logged and skipped, and a message with no usable length is logged and
        # dropped, so a bad frame never ends the server; the next one is read.
        while True:
            length = None
            while True:
                line = incoming.readline()
                if not line:
                    return None
                line = line.strip()
                if not line:
                    break
                name, separator, value = line.partition(b":")
                if not separator:
                    log_line(f"skipped a header without a colon: {line!r}")
                elif name.strip().lower() == b"content-length":
                    try:
                        length = int(value.strip())
                    except ValueError:
                        log_line(f"skipped a malformed Content-Length: {value.strip()!r}")
                        length = None
            if length is not None and length >= 0:
                body = incoming.read(length)
                try:
                    return json.loads(body.decode("utf-8"))
                except ValueError:
                    log_line("skipped a message that is not JSON")
                    continue
            log_line("skipped a message without a usable Content-Length")

    def relative_path(uri):
        # The file's path under the root `initialize` named, or None outside it.
        if root["path"] is None:
            return None
        path = unquote(urlparse(uri).path)
        try:
            return Path(path).relative_to(root["path"]).as_posix()
        except ValueError:
            return None

    def finding_of(finding, text):
        found_range = range_of(text, finding["needle"])
        if found_range is None:
            return None
        return {
            "range": found_range,
            "severity": finding["severity"],
            "message": finding["message"],
            "source": finding.get("source", "fake-lsp"),
        }

    def diagnostics_for(uri, text, config):
        found = []
        if uri.rsplit("/", 1)[-1] == config.get("diagnostics_file", ""):
            found = [finding_of(f, text) for f in config.get("diagnostics", [])]
        relative = relative_path(uri)
        if relative is not None:
            found += [finding_of(f, text) for f in config.get("publish", {}).get(relative, [])]
        return [finding for finding in found if finding is not None]

    def publish(uri, text, config=None):
        send({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {
                "uri": uri,
                "diagnostics": diagnostics_for(uri, text, config or answers()),
            },
        })

    def symbol_of(entry, text):
        found_range = range_of(text, entry["needle"])
        if found_range is None:
            return None
        children = [symbol_of(child, text) for child in entry.get("children", [])]
        return {
            "name": entry["name"],
            "kind": entry.get("kind", 12),
            "range": found_range,
            "selectionRange": found_range,
            "children": [child for child in children if child is not None],
        }

    def symbols_for(uri, config):
        names = config.get("symbols_file", "")
        names = [names] if isinstance(names, str) else names
        if uri.rsplit("/", 1)[-1] not in names or uri not in documents:
            return []
        found = [symbol_of(entry, documents[uri]) for entry in config.get("symbols", [])]
        return [symbol for symbol in found if symbol is not None]

    def definition_for(config):
        target = config.get("definition")
        if not target:
            return None
        with open(target["path"], "rb") as handle:
            text = handle.read().decode("utf-8")
        found_range = range_of(text, target["needle"])
        if found_range is None:
            return None
        return {"uri": Path(target["path"]).as_uri(), "range": found_range}

    def text_of(uri):
        # The document as the client holds it, else the file on disk, else None.
        with lock:
            if uri in documents:
                return documents[uri]
        try:
            with open(unquote(urlparse(uri).path), "rb") as handle:
                return handle.read().decode("utf-8")
        except (OSError, ValueError):
            return None

    def publish_all(config):
        # Every file the map lists, every file it listed before and every open
        # document, once each; a file with no findings now is published empty.
        # The files that are listed are remembered, so leaving the map clears them.
        with lock:
            uris = set(published) | set(documents)
            if root["path"] is not None:
                for relative in config.get("publish", {}):
                    uris.add((Path(root["path"]) / relative).as_uri())
            published.clear()
            for relative in config.get("publish", {}):
                if root["path"] is not None:
                    published.add((Path(root["path"]) / relative).as_uri())
            for uri in sorted(uris):
                text = text_of(uri)
                if text is None:
                    continue
                publish(uri, text, config)

    def stamp_of_answers():
        try:
            status = os.stat(answers_path)
            return (status.st_mtime_ns, status.st_size)
        except OSError:
            return None

    def watch(seen, was_active):
        # Looks at ANSWERS every 200 ms against the stamp `seen` (taken before the
        # answers were read for the first publish); a change republishes. Reads that
        # fail (the file is being replaced) are tried again next time. The thread is
        # a daemon: it dies with the process.
        while True:
            time.sleep(0.2)
            try:
                status = os.stat(answers_path)
                stamp = (status.st_mtime_ns, status.st_size)
                if stamp == seen:
                    continue
                config = answers()
            except (OSError, ValueError):
                continue
            seen = stamp
            if config.get("exit"):
                # Under the lock the main loop writes with, so no frame is cut in half.
                with lock:
                    log_line("exit: the answers say to end")
                    os._exit(0)
            active = "publish" in config
            if active or was_active:
                publish_all(config)
            was_active = active

    def watch_loudly(*args):
        # A thread that dies must not die quietly: the test would wait for a
        # publish that never comes. It says so on stderr and in the log.
        try:
            watch(*args)
        except BaseException:
            traceback.print_exc(file=sys.stderr)
            sys.stderr.flush()
            try:
                log_line("the watch thread died; no publish will follow")
            except Exception:
                pass

    while True:
        message = read_message()
        if message is None:
            break
        method = message.get("method", "")
        # A client's answer to one of this server's requests carries an id and no
        # method: it is neither logged as a message nor answered.
        is_request = "id" in message and "method" in message
        if "method" in message:
            log_line(f"{'request' if is_request else 'notification'} {method}")
        params = message.get("params") or {}
        if method == "initialize":
            root_uri = params.get("rootUri") or ""
            root["path"] = unquote(urlparse(root_uri).path) if root_uri else None
            send({
                "jsonrpc": "2.0",
                "id": message["id"],
                "result": {
                    "capabilities": {
                        "textDocumentSync": {"openClose": True, "change": 1},
                        "hoverProvider": True,
                        "definitionProvider": True,
                        "documentSymbolProvider": True,
                    },
                    "serverInfo": {"name": "fake-lsp"},
                },
            })
        elif method == "initialized":
            stamp = stamp_of_answers()
            config = answers()
            if config.get("exit_after_initialized"):
                log_line("exit: exit_after_initialized")
                break
            # A server started while `publish` is there has to publish again when
            # it goes (a test puts the answers back), as one that saw it come does.
            threading.Thread(
                target=watch_loudly, args=(stamp, "publish" in config), daemon=True
            ).start()
            if "publish" in config:
                publish_all(config)
            if config.get("service_ready"):
                send({
                    "jsonrpc": "2.0",
                    "method": "language/status",
                    "params": {"type": "ServiceReady", "message": "ServiceReady"},
                })
        elif method == "shutdown":
            send({"jsonrpc": "2.0", "id": message["id"], "result": None})
        elif method == "exit":
            break
        elif method == "textDocument/didOpen":
            uri = params["textDocument"]["uri"]
            with lock:
                documents[uri] = params["textDocument"]["text"]
            publish(uri, documents[uri])
        elif method == "textDocument/didChange":
            uri = params["textDocument"]["uri"]
            with lock:
                documents[uri] = params["contentChanges"][-1]["text"]
            if answers().get("publish_on_change", True):
                publish(uri, documents[uri])
        elif method == "textDocument/didClose":
            with lock:
                documents.pop(params["textDocument"]["uri"], None)
        elif method == "textDocument/hover":
            text = answers().get("hover")
            result = None
            if text is not None:
                result = {"contents": {"kind": "markdown", "value": text}}
            send({"jsonrpc": "2.0", "id": message["id"], "result": result})
        elif method == "textDocument/definition":
            send({"jsonrpc": "2.0", "id": message["id"], "result": definition_for(answers())})
        elif method == "textDocument/documentSymbol":
            uri = (params.get("textDocument") or {}).get("uri", "")
            send({"jsonrpc": "2.0", "id": message["id"], "result": symbols_for(uri, answers())})
        elif method == "java/buildWorkspace":
            if answers().get("hold_build"):
                continue
            send({"jsonrpc": "2.0", "id": message["id"], "result": 1})
        elif is_request:
            send({"jsonrpc": "2.0", "id": message["id"], "result": None})
    log.close()


if __name__ == "__main__":
    main()
