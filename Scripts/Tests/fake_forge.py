#!/usr/bin/env python3
"""A loopback stand-in for GitHub and GitLab, for Scripts/Tests/test-forge-e2e.sh.

One process serves one flavour on one port:

    fake_forge.py --flavor github|gitlab|none --port N --log FILE

It answers only what sirio_forge and the two CLIs ask:

- POST /graphql and /api/graphql -- including the absolute-URI form
  (`POST http://api.github.localhost/graphql`) that `gh` sends when this
  server is its HTTP proxy -- with the fixture named after the request's
  operationName, from Scripts/Tests/forge-fixtures/<flavour>/. A request with
  a non-empty `after*` variable gets `<Operation>.page2.json`; number 404
  (GitHub) or iid "404" (GitLab) gets `NotFound.json`.
- GET / and /user (read by `gh auth status`), GET /api/v4/user (read by
  `glab auth status`), GET /api/v3/meta (the GitHub Enterprise probe, which
  only the github flavour answers).
- The none flavour answers 404 to everything: a host that is not a forge.

The credential picks the scenario, so one server covers every error path:
`good` answers normally; a missing credential or `expired` gets 401; `sso` a
403 with X-GitHub-SSO; `limited` a 403 with an exhausted rate limit;
`throttled` a 429 with Retry-After; `old` makes the gitlab flavour behave like
a server older than the newest fields. The credential is read from
`Authorization: Bearer|token <t>` (sirio_forge, gh) or `PRIVATE-TOKEN` (glab).

Both CLIs send request bodies with Transfer-Encoding: chunked (checked with gh
2.100 and glab 1.119), so chunked bodies are decoded here.

Every request is appended to --log as one line:

    <METHOD> <path> <operation or -> interaction=<yes|no> vars=<json, sorted keys>
"""

import argparse
import http.server
import json
import os
import sys

FIXTURES = os.path.join(os.path.dirname(os.path.abspath(__file__)), "forge-fixtures")
# Operations that have a baseline variant, and the fields those variants omit.
BASELINE_OPERATIONS = {"MergeRequestList", "MergeRequestUnion", "MergeRequestForBranch", "MergeRequestHeader"}
NEWER_GITLAB_FIELDS = {"mergeRequestInteraction", "finished", "diffStatsSummary", "commitCount"}


def strip_newer(value, parent=None):
    """What an older GitLab would have answered: the newer fields removed."""
    if isinstance(value, dict):
        return {
            key: strip_newer(item, key)
            for key, item in value.items()
            if key not in NEWER_GITLAB_FIELDS and not (parent == "headPipeline" and key == "jobs")
        }
    if isinstance(value, list):
        return [strip_newer(item, parent) for item in value]
    return value


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    flavor = "github"
    log_path = None

    def credential(self):
        auth = self.headers.get("Authorization") or ""
        if " " in auth:
            return auth.split(" ", 1)[1].strip()
        return (self.headers.get("PRIVATE-TOKEN") or "").strip()

    def body(self):
        if "chunked" in (self.headers.get("Transfer-Encoding") or "").lower():
            data = b""
            while True:
                size = int(self.rfile.readline().strip().split(b";")[0], 16)
                if size == 0:
                    self.rfile.readline()
                    return data
                data += self.rfile.read(size)
                self.rfile.readline()
        length = int(self.headers.get("Content-Length") or 0)
        return self.rfile.read(length) if length else b""

    def plain_path(self):
        path = self.path
        if "://" in path:
            rest = path.split("://", 1)[1]
            path = "/" + rest.split("/", 1)[1] if "/" in rest else "/"
        return path.split("?", 1)[0]

    def record(self, method, path, operation, query, variables):
        if not self.log_path:
            return
        interaction = "yes" if "mergeRequestInteraction" in (query or "") else "no"
        line = f"{method} {path} {operation or '-'} interaction={interaction} vars={json.dumps(variables or {}, sort_keys=True)}\n"
        with open(self.log_path, "a", encoding="utf-8") as log:
            log.write(line)

    def answer(self, status, payload, headers=()):
        data = json.dumps(payload).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        for name, value in headers:
            self.send_header(name, value)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def scenario_error(self):
        credential = self.credential()
        if credential in ("", "expired"):
            return 401, {"message": "Bad credentials"}, []
        if credential == "sso":
            return 403, {"message": "Resource protected by organization SAML enforcement."}, [
                ("X-GitHub-SSO", "required; url=https://github.com/orgs/acme/sso?authorization_request=abc")
            ]
        if credential == "limited":
            return 403, {"message": "API rate limit exceeded"}, [
                ("X-RateLimit-Remaining", "0"),
                ("X-RateLimit-Reset", "4102444800"),
            ]
        if credential == "throttled":
            return 429, {"message": "Retry later"}, [("Retry-After", "30")]
        return None

    def do_GET(self):
        path = self.plain_path()
        self.body()
        self.record("GET", path, None, None, None)
        if self.flavor == "none":
            return self.answer(404, {"message": "Not Found"})
        if path == "/api/v3/meta":
            if self.flavor == "github":
                return self.answer(200, {"installed_version": "3.17.0"})
            return self.answer(404, {"message": "404 Not Found"})
        signed_in = (self.flavor == "github" and path in ("/", "/user")) or (
            self.flavor == "gitlab" and path == "/api/v4/user"
        )
        if not signed_in:
            return self.answer(404, {"message": "Not Found"})
        error = self.scenario_error()
        if error:
            return self.answer(error[0], error[1], error[2])
        who = {"login": "fake-user"} if self.flavor == "github" else {"username": "fake-user"}
        return self.answer(200, who, [("X-OAuth-Scopes", "repo, read:org")])

    def do_POST(self):
        path = self.plain_path()
        raw = self.body()
        if self.flavor == "none" or path not in ("/graphql", "/api/graphql"):
            self.record("POST", path, None, None, None)
            return self.answer(404, {"message": "Not Found"})
        request = json.loads(raw or b"{}")
        operation = request.get("operationName") or ""
        query = request.get("query") or ""
        variables = request.get("variables") or {}
        self.record("POST", path, operation, query, variables)
        error = self.scenario_error()
        if error:
            return self.answer(error[0], error[1], error[2])
        old = self.flavor == "gitlab" and self.credential() == "old"
        if old and "mergeRequestInteraction" in query:
            return self.answer(200, {"errors": [{"message": "Field 'mergeRequestInteraction' doesn't exist on type 'MergeRequestReviewer'"}]})
        name = operation
        if any(key.startswith("after") and value for key, value in variables.items()):
            name += ".page2"
        if variables.get("number") == 404 or variables.get("iid") == "404":
            name = "NotFound"
        try:
            with open(os.path.join(FIXTURES, self.flavor, name + ".json"), encoding="utf-8") as fixture:
                payload = json.load(fixture)
        except FileNotFoundError:
            return self.answer(500, {"message": f"fake forge has no fixture {self.flavor}/{name}.json"})
        if old and operation in BASELINE_OPERATIONS:
            payload = strip_newer(payload)
        return self.answer(200, payload, [("X-RateLimit-Remaining", "4999")])

    def log_message(self, *args):
        pass


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--flavor", choices=("github", "gitlab", "none"), required=True)
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--log")
    options = parser.parse_args()
    Handler.flavor = options.flavor
    Handler.log_path = options.log
    server = http.server.ThreadingHTTPServer(("127.0.0.1", options.port), Handler)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    return 0


if __name__ == "__main__":
    sys.exit(main())
