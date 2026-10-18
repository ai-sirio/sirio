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
import sys
from pathlib import Path


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

    def log_line(text):
        log.write(text + "\n")
        log.flush()

    def answers():
        with open(answers_path, "rb") as handle:
            return json.loads(handle.read().decode("utf-8"))

    def send(message):
        body = json.dumps(message).encode("utf-8")
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

    def diagnostics_for(uri, text, config):
        if uri.rsplit("/", 1)[-1] != config.get("diagnostics_file", ""):
            return []
        found = []
        for finding in config.get("diagnostics", []):
            found_range = range_of(text, finding["needle"])
            if found_range is None:
                continue
            found.append({
                "range": found_range,
                "severity": finding["severity"],
                "message": finding["message"],
                "source": "fake-lsp",
            })
        return found

    def publish(uri, text):
        send({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {"uri": uri, "diagnostics": diagnostics_for(uri, text, answers())},
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
        elif method == "shutdown":
            send({"jsonrpc": "2.0", "id": message["id"], "result": None})
        elif method == "exit":
            break
        elif method == "textDocument/didOpen":
            uri = params["textDocument"]["uri"]
            documents[uri] = params["textDocument"]["text"]
            publish(uri, documents[uri])
        elif method == "textDocument/didChange":
            uri = params["textDocument"]["uri"]
            documents[uri] = params["contentChanges"][-1]["text"]
            if answers().get("publish_on_change", True):
                publish(uri, documents[uri])
        elif method == "textDocument/didClose":
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
        elif is_request:
            send({"jsonrpc": "2.0", "id": message["id"], "result": None})
    log.close()


if __name__ == "__main__":
    main()
