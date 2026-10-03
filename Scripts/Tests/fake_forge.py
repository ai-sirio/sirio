#!/usr/bin/env python3
"""A loopback stand-in for GitHub and GitLab, for the three forge E2E scripts:
Scripts/Tests/test-forge-e2e.sh, test-forge-ui-e2e.sh and test-forge-diff-e2e.sh.

One process serves one flavour on one port:

    fake_forge.py --flavor github|gitlab|none --port N --log FILE [--fixtures DIR]

It answers only what sirio_forge and the two CLIs ask:

- POST /graphql and /api/graphql -- including the absolute-URI form
  (`POST http://api.github.localhost/graphql`) that `gh` sends when this
  server is its HTTP proxy -- with the fixture named after the request's
  operationName, from <fixtures>/<flavour>/, where <fixtures> is --fixtures
  (default Scripts/Tests/forge-fixtures; test-forge-diff-e2e.sh passes a copy
  whose commit ids name a real repository's commits). A request with
  a non-empty `after*` variable gets `<Operation>.page2.json`; number 404
  (GitHub) or iid "404" (GitLab) gets `NotFound.json`.
- GET / and /user (read by `gh auth status`), GET /api/v4/user (read by
  `glab auth status`), GET /api/v3/meta (the GitHub Enterprise probe, which
  only the github flavour answers).
- GET <anything>/info/refs -- git's smart-HTTP discovery, the first request
  of a `git fetch` over http -- gets a 401 asking for Basic credentials, the
  answer that makes git want a password it must not prompt for.
- The none flavour answers 404 to everything: a host that is not a forge.

The credential picks the scenario, so one server covers every error path:
`good` answers normally; a missing credential or `expired` gets 401; `sso` a
403 with X-GitHub-SSO; `limited` a 403 with an exhausted rate limit;
`throttled` a 429 with Retry-After; `old` makes the gitlab flavour behave like
a server older than the newest fields. The credential is read from
`Authorization: Bearer|token <t>` (sirio_forge, gh) or `PRIVATE-TOKEN` (glab).

Writes (test-forge-actions-e2e.sh). A GraphQL mutation answers its own
fixture, `<Operation>.json`, or else a generic success naming the mutation's
field; GitLab's REST `POST /api/v4/projects/<id>/merge_requests/<iid>/approve`
answers 201. The credential picks a failure for a write only -- reads stay
normal -- so one server covers every error path of an action:

    scopeless  GitHub: 200 + INSUFFICIENT_SCOPES; GitLab: 403 insufficient_scope
    rejected   GitHub: 200 + UNPROCESSABLE and a null payload; GitLab: 200 and
               the reason in the payload's `errors`; REST: 409
    dropped    the request is read, then the connection is closed unanswered
    slow       the answer is held back 1.5 s, so two sends overlap
    readonly   reads serve `<Operation>.readonly.json` where it exists (a
               viewer who may not act); its token lists only read scopes
    finegrained  a token that reports no scopes at all
    notefails    GitLab only: `createNote` is refused with the reason in the
                 payload's `errors`, while the REST approval still answers 201
    blocked, waiting, fork
                 reads serve `<Operation>.<credential>.json` where it exists
                 (a merge blocked by a review, checks still running, a head
                 in a fork) -- the same rule as `readonly`
    deletefails  GitHub's REST `DELETE .../git/refs/heads/<branch>` answers
                 422, so a merge whose branch deletion fails can be told apart

Merge (B2b): GitHub's branch deletion is REST `DELETE /api/v3/repos/<o>/<r>/
git/refs/heads/<branch>` (204); GitLab's cancel of an auto-merge is REST
`POST /api/v4/projects/<p>/merge_requests/<iid>/cancel_merge_when_pipeline_
succeeds` (200). `ReviewerCandidates` and `LabelCandidates` answer their
fixture with the rows whose words contain the `q` variable.

A write that succeeded is remembered, and a read then serves
`<Operation>.after.<Mutation>.json` when it exists (the newest write that has
one wins): a change request closed by a mutation reads as closed. GitLab's
`MergeRequestUpdate` and `MergeRequestSetDraft` are told apart by their input,
`<Mutation>.CLOSED`, `<Mutation>.OPEN`, `<Mutation>.true`, `<Mutation>.false`;
the REST approval is `approve`. `POST /__reset` forgets every write.

Both CLIs send request bodies with Transfer-Encoding: chunked (checked with gh
2.100 and glab 1.119), so chunked bodies are decoded here.

Every request is appended to --log as one line:

    <METHOD> <path> <operation or -> interaction=<yes|no> vars=<json, sorted keys>
"""

import argparse
import http.server
import json
import os
import re
import sys
import threading
import time

FIXTURES = os.path.join(os.path.dirname(os.path.abspath(__file__)), "forge-fixtures")


def load_fixtures(root):
    """Every fixture under `root`, by flavour and then by name without `.json`.
    A request picks one by looking its name up here, so no path is ever built
    from what a request says."""
    return {
        flavor: {
            entry[: -len(".json")]: os.path.join(root, flavor, entry)
            for entry in os.listdir(os.path.join(root, flavor))
            if entry.endswith(".json")
        }
        for flavor in os.listdir(root)
        if os.path.isdir(os.path.join(root, flavor))
    }


# Operations that have a baseline variant, and the fields those variants omit.
BASELINE_OPERATIONS = {"MergeRequestList", "MergeRequestUnion", "MergeRequestForBranch", "MergeRequestHeader", "MergeRequestActionContext"}
NEWER_GITLAB_FIELDS = {"mergeRequestInteraction", "finished", "diffStatsSummary", "commitCount", "canApprove"}
# The newer fields a query can name, and the type an older GitLab would say lacks them.
NEWER_QUERY_FIELDS = (("mergeRequestInteraction", "MergeRequestReviewer"), ("canApprove", "MergeRequestPermissions"))


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


def only_matching(value, text):
    """A candidate search: every list of nodes keeps the rows whose words
    contain `text`, as the forge's own search would."""
    if isinstance(value, dict):
        out = {}
        for key, item in value.items():
            if key == "nodes" and isinstance(item, list):
                out[key] = [row for row in item if text in json.dumps(row).lower()]
            else:
                out[key] = only_matching(item, text)
        return out
    return value


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    flavor = "github"
    log_path = None
    fixture_files = {}
    applied = []
    applied_lock = threading.Lock()

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

    def remember(self, key):
        with self.applied_lock:
            self.applied.append(key)

    def fixture_for(self, name):
        """The fixture a read answers: the newest applied write that has an
        `after` overlay for it, else the plain fixture, else `None`."""
        fixtures = self.fixture_files.get(self.flavor, {})
        with self.applied_lock:
            applied = list(self.applied)
        for key in reversed(applied):
            overlay = fixtures.get(f"{name}.after.{key}")
            if overlay:
                return overlay
        credential = self.credential()
        if credential in ("readonly", "blocked", "waiting", "fork") and fixtures.get(f"{name}.{credential}"):
            return fixtures[f"{name}.{credential}"]
        return fixtures.get(name)

    def write_failure(self, field):
        """A failure for a write, chosen by the credential; `None` for a
        write that goes through (after a pause, for `slow`)."""
        credential = self.credential()
        if credential == "scopeless":
            if self.flavor == "github":
                return 200, {
                    "data": {field: None},
                    "errors": [{"type": "INSUFFICIENT_SCOPES", "path": [field],
                                "message": "Your token has not been granted the required scopes to execute this query."}],
                }, []
            return 403, {"error": "insufficient_scope", "scope": "api",
                         "error_description": "The request requires higher privileges than provided by the access token."}, []
        if credential == "rejected":
            if self.flavor == "github":
                return 200, {
                    "data": {field: None},
                    "errors": [{"type": "UNPROCESSABLE", "path": [field], "message": "Pull request is not mergeable"}],
                }, []
            return 200, {"data": {field: {"errors": ["Validation failed: title is invalid"]}}}, []
        if credential == "slow":
            time.sleep(1.5)
        return None

    def rest_write(self, path):
        """GitLab's approval: the only REST write B2a sends."""
        self.record("POST", path, None, None, None)
        if self.flavor == "gitlab" and re.fullmatch(r"/api/v4/projects/[^/]+/merge_requests/\d+/cancel_merge_when_pipeline_succeeds", path):
            error = self.scenario_error()
            if error:
                return self.answer(error[0], error[1], error[2])
            self.remember("cancel-auto-merge")
            return self.answer(200, {"iid": 201, "merge_when_pipeline_succeeds": False})
        if self.flavor != "gitlab" or not re.fullmatch(r"/api/v4/projects/[^/]+/merge_requests/\d+/approve", path):
            return self.answer(404, {"message": "404 Not Found"})
        error = self.scenario_error()
        if error:
            return self.answer(error[0], error[1], error[2])
        credential = self.credential()
        if credential == "dropped":
            self.close_connection = True
            return
        if credential == "scopeless":
            return self.answer(403, {"error": "insufficient_scope", "scope": "api",
                                     "error_description": "The request requires higher privileges than provided by the access token."})
        if credential == "rejected":
            return self.answer(409, {"message": "SHA does not match HEAD of source branch"})
        if credential == "slow":
            time.sleep(1.5)
        self.remember("approve")
        return self.answer(201, {"id": 201, "iid": 201, "approved_by": [{"user": {"username": "fake-user"}}]})

    def do_GET(self):
        path = self.plain_path()
        self.body()
        self.record("GET", path, None, None, None)
        if self.flavor == "none":
            return self.answer(404, {"message": "Not Found"})
        if path.endswith("/info/refs"):
            return self.answer(401, {"message": "Authentication required"}, [("WWW-Authenticate", 'Basic realm="fake forge"')])
        if path == "/api/v3/meta":
            if self.flavor == "github":
                return self.answer(200, {"installed_version": "3.17.0"})
            return self.answer(404, {"message": "404 Not Found"})
        signed_in = (self.flavor == "github" and path in ("/", "/user", "/api/v3/user")) or (
            self.flavor == "gitlab" and path == "/api/v4/user"
        )
        if self.flavor == "gitlab" and path == "/api/v4/personal_access_tokens/self":
            error = self.scenario_error()
            if error:
                return self.answer(error[0], error[1], error[2])
            scopes = {"good": ["api", "read_api"], "readonly": ["read_api"]}.get(self.credential())
            if scopes is None:  # a token with no record of itself: an OAuth token
                return self.answer(404, {"message": "404 Not Found"})
            return self.answer(200, {"id": 1, "name": "fake", "scopes": scopes})
        if not signed_in:
            return self.answer(404, {"message": "Not Found"})
        error = self.scenario_error()
        if error:
            return self.answer(error[0], error[1], error[2])
        who = {"login": "fake-user"} if self.flavor == "github" else {"username": "fake-user"}
        # A classic token lists its scopes; `readonly` has only read ones; a
        # fine-grained token (`finegrained`) sends no such header at all.
        scopes = {"readonly": "read:org", "finegrained": None}.get(self.credential(), "repo, read:org")
        return self.answer(200, who, [("X-OAuth-Scopes", scopes)] if scopes is not None else [])

    def do_DELETE(self):
        """GitHub's branch deletion after a merge: the only DELETE Sirio sends."""
        path = self.plain_path()
        self.body()
        self.record("DELETE", path, None, None, None)
        if self.flavor != "github" or not re.fullmatch(r"/api/v3/repos/[^/]+/[^/]+/git/refs/heads/.+", path):
            return self.answer(404, {"message": "Not Found"})
        error = self.scenario_error()
        if error:
            return self.answer(error[0], error[1], error[2])
        if self.credential() == "deletefails":
            return self.answer(422, {"message": "Reference does not exist"})
        self.send_response(204)
        self.send_header("Content-Length", "0")
        self.end_headers()

    def do_POST(self):
        path = self.plain_path()
        raw = self.body()
        if path == "/__reset":
            with self.applied_lock:
                del self.applied[:]
            return self.answer(200, {"reset": True})
        if self.flavor != "none" and path.startswith("/api/v4/"):
            return self.rest_write(path)
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
        if query.lstrip().startswith("mutation"):
            return self.mutation(operation, query, variables)
        old = self.flavor == "gitlab" and self.credential() == "old"
        for field, owner in NEWER_QUERY_FIELDS if old else ():
            if field in query:
                return self.answer(200, {"errors": [{"message": f"Field '{field}' doesn't exist on type '{owner}'"}]})
        name = operation
        if any(key.startswith("after") and value for key, value in variables.items()):
            name += ".page2"
        if variables.get("number") == 404 or variables.get("iid") == "404":
            name = "NotFound"
        path = self.fixture_for(name)
        if path is None:
            return self.answer(500, {"message": f"fake forge has no fixture {self.flavor}/{name}.json"})
        with open(path, encoding="utf-8") as fixture:
            payload = json.load(fixture)
        if operation in ("ReviewerCandidates", "LabelCandidates"):
            payload = only_matching(payload, (variables.get("q") or "").lower())
        if old and operation in BASELINE_OPERATIONS:
            payload = strip_newer(payload)
        return self.answer(200, payload, [("X-RateLimit-Remaining", "4999")])

    def mutation(self, operation, query, variables):
        found = re.search(r"mutation\s+\w+\s*(?:\([^)]*\))?\s*\{\s*(\w+)", query)
        field = found.group(1) if found else "mutation"
        if self.credential() == "dropped":
            self.close_connection = True
            return
        if self.credential() == "notefails" and self.flavor == "gitlab" and operation == "CreateNote":
            return self.answer(200, {"data": {field: {"errors": ["Note creation failed"]}}})
        failure = self.write_failure(field)
        if failure:
            return self.answer(failure[0], failure[1], failure[2])
        given = (variables.get("input") or {}) if isinstance(variables.get("input"), dict) else {}
        key = operation
        for discriminator in ("state", "draft"):
            if discriminator in given:
                key = f"{operation}.{str(given[discriminator]).lower() if isinstance(given[discriminator], bool) else given[discriminator]}"
        self.remember(key)
        path = self.fixture_files.get(self.flavor, {}).get(operation)
        if path:
            with open(path, encoding="utf-8") as fixture:
                return self.answer(200, json.load(fixture))
        if self.flavor == "github":
            return self.answer(200, {"data": {field: {"clientMutationId": None}}})
        return self.answer(200, {"data": {field: {"errors": []}}})

    def log_message(self, *args):
        pass


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--flavor", choices=("github", "gitlab", "none"), required=True)
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--log")
    parser.add_argument("--fixtures", default=FIXTURES)
    options = parser.parse_args()
    Handler.flavor = options.flavor
    Handler.log_path = options.log
    Handler.fixture_files = load_fixtures(options.fixtures)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", options.port), Handler)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    return 0


if __name__ == "__main__":
    sys.exit(main())
