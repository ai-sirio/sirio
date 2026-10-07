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
    blocked, waiting, fork, mannequin
                 reads serve `<Operation>.<credential>.json` where it exists
                 (a merge blocked by a review, checks still running, a head
                 in a fork, a review request Sirio cannot send back) -- the
                 same rule as `readonly`
    deletefails  GitHub's REST `DELETE .../git/refs/heads/<branch>` answers
                 422 for a protected branch, so a merge whose branch deletion
                 fails can be told apart
    autodeleted  the same DELETE answers 422 "Reference does not exist", as
                 GitHub does once its own delete-on-merge setting got there first
    approvefails GitLab only: the REST approval answers 403, while every other
                 write succeeds

Merge (B2b): GitHub's branch deletion is REST `DELETE /api/v3/repos/<o>/<r>/
git/refs/heads/<branch>` (204); GitLab's cancel of an auto-merge is REST
`POST /api/v4/projects/<p>/merge_requests/<iid>/cancel_merge_when_pipeline_
succeeds` (200). `ReviewerCandidates` and `LabelCandidates` answer their
fixture with the rows whose words contain the `q` variable.

Draft notes (B3c): GitLab's `GET .../merge_requests/<iid>/draft_notes`
lists the draft notes `POST /__drafts` seeded, empty until a stage posts
some; `POST` adds one (a reply names `in_reply_to_discussion_id`, a line
comment a text `position`), `PUT .../draft_notes/<id>` edits one,
`DELETE .../draft_notes/<id>` deletes one, and `POST .../bulk_publish`
publishes them all.

A write that succeeded is remembered, and a read then serves
`<Operation>.after.<Mutation>.json` when it exists (the newest write that has
one wins): a change request closed by a mutation reads as closed. GitLab's
`MergeRequestUpdate` and `MergeRequestSetDraft` are told apart by their input,
`<Mutation>.CLOSED`, `<Mutation>.OPEN`, `<Mutation>.true`, `<Mutation>.false`, and an
accept with a strategy `MergeRequestAccept.MERGE_WHEN_CHECKS_PASS`;
the REST approval is `approve`, the REST cancel `cancel-auto-merge`. `POST /__reset`
forgets every write; `POST /__push` stands for a push to the head branch, and
reads then serve `<Operation>.after.push.json`; `POST /__checking` stands for
the forge still working out whether it can merge (`.after.checking.json`)
until the next `/__reset`.

`POST /__slowlog?seconds=N` answers every GitLab trace N seconds late, and
`GET /__stats` says how many traces were ever answered at once.
`POST /__drafts` seeds the draft notes `GET …/draft_notes` lists, empty
until a stage posts some.
`POST /__ratelimit?seconds=N` rate limits every read with a reset N seconds
ahead; `POST /__throttle` answers 429 with Retry-After and no reset until
`/__reset`. That reset also clears both limits. `ChangeRequestSearch` keeps
only rows containing every free word of `q` (words without `:`).

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
import urllib.parse

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
BASELINE_OPERATIONS = {"MergeRequestList", "MergeRequestUnion", "MergeRequestForBranch", "MergeRequestHeader", "MergeRequestByNumber", "MergeRequestActionContext", "MergeRequestThreads"}
NEWER_GITLAB_FIELDS = {"mergeRequestInteraction", "finished", "diffStatsSummary", "commitCount", "canApprove",
                       "canMerge", "detailedMergeStatus", "squashOnMerge", "squashReadOnly", "autoMergeEnabled",
                       "availableAutoMergeStrategies", "shouldRemoveSourceBranch", "truncatedDiffLines"}
# Mutations an older GitLab lacks: the `old` credential answers them as such a
# server would, with a schema error naming the field.
NEWER_GITLAB_MUTATIONS = {"mergeRequestSetReviewers", "mergeRequestSetLabels"}

DRAFTS = r"/api/v4/projects/[^/]+/merge_requests/\d+/draft_notes"
# The newer fields a query can name, and the type an older GitLab would say lacks them.
NEWER_QUERY_FIELDS = (("mergeRequestInteraction", "MergeRequestReviewer"), ("canApprove", "MergeRequestPermissions"), ("truncatedDiffLines", "Discussion"))


ESC = "\x1b"


def github_log(job):
    """A GitHub job log the way Actions writes one: a BOM, a time on every
    line, groups, colours, a progress line and an error marker."""
    lines = [
        "##[group]Run actions/checkout@v4",
        "with:",
        "  repository: acme/widgets",
        "##[endgroup]",
        "##[group]Run cargo test",
        f"{ESC}[36;1mcargo test --workspace{ESC}[0m",
        "   Compiling widgets v0.1.0",
        "Downloading 10%\rDownloading 55%\rDownloading 100%",
        f"test parses ... {ESC}[32mok{ESC}[0m",
        f"test renders ... {ESC}[1;31mFAILED{ESC}[0m",
        f"{ESC}[38;5;208mwarning{ESC}[0m: unused {ESC}[38;2;10;20;30mvariable{ESC}[0m",
        "##[endgroup]",
        f"##[error]Process completed with exit code {100 + job}.",
        "Post job cleanup.",
    ]
    stamped = [f"2026-09-27T10:00:{i:02d}.0000000Z {line}" for i, line in enumerate(lines)]
    return ("﻿" + "\n".join(stamped) + "\n").encode()


def gitlab_log(job):
    """A GitLab trace: sections, colours, a progress line; job 3 is still
    running (half a trace), job 4 is manual (none)."""
    if job == 4:
        return b""
    lines = [
        f"{ESC}[0KRunning with gitlab-runner 17.0.0",
        f"section_start:1727431200:prepare_script[collapsed=true]\r{ESC}[0K{ESC}[0K{ESC}[36;1mPreparing environment{ESC}[0;m",
        "Running on runner-abc...",
        f"section_end:1727431201:prepare_script\r{ESC}[0K",
        f"section_start:1727431202:step_script\r{ESC}[0K{ESC}[0K{ESC}[36;1mExecuting \"step_script\" stage of the job script{ESC}[0;m",
        f"{ESC}[32;1m$ bundle exec rspec{ESC}[0;m",
        "Progress: |====      |\rProgress: |==========|",
        f"{ESC}[31mFailures:{ESC}[0m",
        "  1) Widget renders",
        f"section_end:1727431260:step_script\r{ESC}[0K",
        f"{ESC}[31;1mERROR: Job failed: exit code 1{ESC}[0;m",
    ]
    if job == 3:
        lines = lines[:7]
    return ("\n".join(lines) + "\n").encode()


def huge_log():
    """Beyond the bounded CLI stdout buffer as well as the download cap."""
    return b"huge\n" * ((80 * 1024 * 1024) // 5 + 1)


def oversize_log():
    """Just over the 64 MiB download limit, with short lines."""
    line = b"oversized\n"
    return line * ((64 * 1024 * 1024) // len(line) + 1)


def big_log():
    """Over the 4 MiB tail: about 5.6 MB of numbered lines."""
    return "".join(f"line {i:07d} " + "x" * 80 + "\n" for i in range(60000)).encode()


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


def without_group_labels(value):
    """GitLab lists a project's labels and its own group's; the `GroupLabel`
    rows of the fixture stand for an organisation's labels above that, which
    come only with `includeAncestorGroups: true`."""
    if isinstance(value, dict):
        return {
            key: [row for row in item if "GroupLabel" not in str(row.get("id"))]
            if key == "nodes" and isinstance(item, list)
            else without_group_labels(item)
            for key, item in value.items()
        }
    return value


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    flavor = "github"
    log_path = None
    fixture_files = {}
    applied = []
    applied_lock = threading.Lock()
    drafts = []
    draft_seq = [100]
    drafts_lock = threading.Lock()
    limit_until = 0.0
    throttled = False
    expired = False
    # A GitLab trace answered this many seconds late, and how many traces
    # were ever being answered at once.
    slow_log = 0.0
    traces_at_once = 0
    traces_at_once_max = 0
    traces_lock = threading.Lock()

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

    def answer_text(self, status, data, headers=()):
        self.send_response(status)
        self.send_header("Content-Type", "text/plain; charset=utf-8")
        for name, value in headers:
            self.send_header(name, value)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def scenario_error(self):
        if Handler.throttled:
            return 429, {"message": "Retry later"}, [("Retry-After", "30")]
        if time.time() < Handler.limit_until:
            return 403, {"message": "API rate limit exceeded"}, [
                ("X-RateLimit-Remaining", "0"),
                ("X-RateLimit-Reset", str(int(Handler.limit_until))),
            ]
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
        if credential in ("readonly", "blocked", "waiting", "fork", "mannequin") and fixtures.get(f"{name}.{credential}"):
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

    def rest_write(self, path, raw):
        """GitLab REST writes: approvals, cancellations and diff notes."""
        self.record("POST", path, None, None, json.loads(raw or b"{}"))
        if self.flavor == "gitlab" and re.fullmatch(DRAFTS + "/bulk_publish", path):
            if self.credential() == "readonly":
                return self.answer(403, {"message": "403 Forbidden"})
            with self.drafts_lock:
                Handler.drafts = []
            self.remember("bulk-publish")
            self.send_response(204)
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        if self.flavor == "gitlab" and re.fullmatch(DRAFTS, path):
            body = json.loads(raw or b"{}")
            if self.credential() == "readonly":
                return self.answer(403, {"message": "403 Forbidden"})
            position = body.get("position")
            if not (isinstance(body.get("note"), str) and body["note"].strip()) or not (
                isinstance(body.get("in_reply_to_discussion_id"), str)
                or (isinstance(position, dict) and position.get("position_type") == "text"
                    and all(position.get(key) for key in ("base_sha", "start_sha", "head_sha", "old_path", "new_path")))
            ):
                return self.answer(400, {"message": "400 Bad request - note is missing, or the position is not valid"})
            with self.drafts_lock:
                note = {"id": Handler.draft_seq[0], "author_id": 1, "merge_request_id": 201,
                        "resolve_discussion": False, "discussion_id": body.get("in_reply_to_discussion_id"),
                        "note": body["note"], "commit_id": None, "line_code": None,
                        "position": position if isinstance(position, dict) else None}
                Handler.draft_seq[0] += 1
                Handler.drafts.append(note)
            self.remember("draft-note")
            return self.answer(201, note)
        if self.flavor == "gitlab" and re.fullmatch(r"/api/v4/projects/[^/]+/merge_requests/\d+/discussions", path):
            body = json.loads(raw or b"{}")
            if self.credential() == "readonly":
                return self.answer(403, {"message": "403 Forbidden"})
            position = body.get("position")
            valid = (
                isinstance(body.get("body"), str)
                and bool(body["body"].strip())
                and isinstance(position, dict)
                and position.get("position_type") == "text"
                and all(position.get(key) for key in ("base_sha", "start_sha", "head_sha", "old_path", "new_path"))
                and (position.get("old_line") is not None or position.get("new_line") is not None)
            )
            if valid and "line_range" in position:
                line_range = position["line_range"]
                valid = isinstance(line_range, dict) and all(
                    isinstance(line_range.get(end), dict)
                    and re.fullmatch(r"[0-9a-f]{40}_\d+_\d+", str(line_range[end].get("line_code", "")))
                    and line_range[end].get("type") in ("old", "new")
                    for end in ("start", "end")
                )
            if not valid:
                return self.answer(400, {"message": '400 Bad request - Note {:line_code=>["must be a valid line code"]}'})
            self.remember("line-comment")
            return self.answer(201, {"id": "0"})
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
        if credential == "approvefails":
            return self.answer(403, {"message": "403 Forbidden - You cannot approve this merge request"})
        if credential == "rejected":
            return self.answer(409, {"message": "SHA does not match HEAD of source branch"})
        if credential == "slow":
            time.sleep(1.5)
        self.remember("approve")
        return self.answer(201, {"id": 201, "iid": 201, "approved_by": [{"user": {"username": "fake-user"}}]})

    def github_rerun(self, path, raw):
        """GitHub's re-runs are REST: a job, or a run's failed jobs."""
        self.record("POST", path, None, None, json.loads(raw or b"{}"))
        error = self.scenario_error()
        if error:
            return self.answer(error[0], error[1], error[2])
        credential = self.credential()
        if credential == "readonly":
            return self.answer(403, {"message": "Must have admin rights to Repository."})
        if credential == "slow":
            time.sleep(1.5)
        self.remember("rerun")
        return self.answer(201, {})

    def github_review_comment(self, path, raw):
        """A published line comment, addressed by the diff position."""
        body = json.loads(raw or b"{}")
        self.record("POST", path, None, None, body)
        if self.credential() == "readonly":
            return self.answer(403, {"message": "Must have push access"})
        if (
            not all(key in body for key in ("body", "commit_id", "path", "line", "side"))
            or not isinstance(body.get("commit_id"), str)
            or not re.fullmatch(r"[0-9a-fA-F]{40}", body["commit_id"])
            or body.get("side") not in ("LEFT", "RIGHT")
        ):
            return self.answer(422, {
                "message": "Unprocessable Entity",
                "errors": ["pull_request_review_thread.line must be part of the diff"],
            })
        self.remember("line-comment")
        return self.answer(201, {"id": 1})

    def do_GET(self):
        path = self.plain_path()
        self.body()
        if path == "/__stats":
            return self.answer(200, {"traces_at_once_max": Handler.traces_at_once_max})
        if path.startswith("/__blob/"):
            # The signed-URL host: what reaches it must carry no credential.
            carried = "present" if (self.headers.get("Authorization") or self.headers.get("PRIVATE-TOKEN")) else "none"
            self.record("GET", path, None, None, {"authorization": carried})
            name = path[len("/__blob/"):]
            if name == "huge.log":
                return self.answer_text(200, huge_log())
            if name == "oversize.log":
                return self.answer_text(200, oversize_log())
            if name == "big.log":
                return self.answer_text(200, big_log())
            found = re.fullmatch(r"github-job-(\d+)\.log", name)
            if not found:
                return self.answer(404, {"message": "Not Found"})
            return self.answer_text(200, github_log(int(found.group(1))))
        self.record("GET", path, None, None, None)
        if self.flavor == "none":
            return self.answer(404, {"message": "Not Found"})
        if path.endswith("/info/refs"):
            return self.answer(401, {"message": "Authentication required"}, [("WWW-Authenticate", 'Basic realm="fake forge"')])
        if path == "/api/v3/meta":
            if self.flavor == "github":
                return self.answer(200, {"installed_version": "3.17.0"})
            return self.answer(404, {"message": "404 Not Found"})
        if self.flavor == "gitlab" and re.fullmatch(DRAFTS, path):
            return self.answer(200, Handler.drafts)
        job = re.fullmatch(r"(?:/api/v3)?/repos/[^/]+/[^/]+/actions/jobs/(\d+)(/logs)?", path)
        if self.flavor == "github" and job:
            error = self.scenario_error()
            if error:
                return self.answer(error[0], error[1], error[2])
            number = int(job.group(1))
            if Handler.expired or number == 404:
                return self.answer(404, {"message": "Not Found"})
            running = number == 3
            if not job.group(2):
                return self.answer(200, {"id": number, "status": "in_progress" if running else "completed"})
            if running:
                return self.answer(404, {"message": "Not Found"})
            port = self.server.server_address[1]
            blob = {"biglog": "big.log", "oversizelog": "oversize.log", "hugelog": "huge.log"}.get(self.credential(), f"github-job-{number}.log")
            # Another host name for the same loopback server, as GitHub's
            # signed URL names another host.
            return self.answer(302, {}, [("Location", f"http://localhost:{port}/__blob/{blob}?sig=fake")])
        job = re.fullmatch(r"/api/v4/projects/[^/]+/jobs/(\d+)(/trace)?", path)
        if self.flavor == "gitlab" and job:
            error = self.scenario_error()
            if error:
                return self.answer(error[0], error[1], error[2])
            number = int(job.group(1))
            if Handler.expired or number == 404:
                return self.answer(404, {"message": "404 Not Found"})
            if not job.group(2):
                status = {1: "success", 2: "failed", 3: "running", 4: "manual"}.get(number, "failed")
                return self.answer(200, {"id": number, "status": status})
            with Handler.traces_lock:
                Handler.traces_at_once += 1
                Handler.traces_at_once_max = max(Handler.traces_at_once_max, Handler.traces_at_once)
            try:
                time.sleep(Handler.slow_log)
                return self.answer_text(200, gitlab_log(number))
            finally:
                with Handler.traces_lock:
                    Handler.traces_at_once -= 1
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

    def do_PUT(self):
        path = self.plain_path()
        raw = self.body()
        body = json.loads(raw or b"{}")
        self.record("PUT", path, None, None, body)
        found = re.fullmatch(DRAFTS + r"/(\d+)", path)
        if self.flavor != "gitlab" or not found:
            return self.answer(404, {"message": "404 Not Found"})
        with self.drafts_lock:
            note = next((note for note in Handler.drafts if note["id"] == int(found.group(1))), None)
            if note is None:
                return self.answer(404, {"message": "404 Draft Note Not Found"})
            if "note" in body:
                note["note"] = body["note"]
        self.remember("draft-edit")
        return self.answer(200, note)

    def do_DELETE(self):
        """GitHub's branch deletion after a merge, and GitLab's deletion of
        one draft note: the only DELETEs Sirio sends."""
        path = self.plain_path()
        self.body()
        self.record("DELETE", path, None, None, None)
        found = re.fullmatch(DRAFTS + r"/(\d+)", path)
        if self.flavor == "gitlab" and found:
            with self.drafts_lock:
                before = len(Handler.drafts)
                Handler.drafts = [note for note in Handler.drafts if note["id"] != int(found.group(1))]
                gone = len(Handler.drafts) < before
            if not gone:
                return self.answer(404, {"message": "404 Draft Note Not Found"})
            self.remember("draft-delete")
            self.send_response(204)
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        if self.flavor != "github" or not re.fullmatch(r"(/api/v3)?/repos/[^/]+/[^/]+/git/refs/heads/.+", path):
            return self.answer(404, {"message": "Not Found"})
        error = self.scenario_error()
        if error:
            return self.answer(error[0], error[1], error[2])
        if self.credential() == "deletefails":
            return self.answer(422, {"message": "Cannot delete this protected branch"})
        if self.credential() == "autodeleted":
            return self.answer(422, {"message": "Reference does not exist"})
        self.send_response(204)
        self.send_header("Content-Length", "0")
        self.end_headers()

    def do_POST(self):
        path = self.plain_path()
        raw = self.body()
        if path == "/__expire":
            Handler.expired = True
            return self.answer(200, {"expired": True})
        if path == "/__push":
            # Someone pushed to the head branch: reads serve `.after.push`.
            self.remember("push")
            return self.answer(200, {"pushed": True})
        if path == "/__checking":
            # The forge has not yet worked out whether it can merge.
            self.remember("checking")
            return self.answer(200, {"checking": True})
        if path == "/__ratelimit":
            # Every request is rate limited until N seconds from now.
            seconds = float(dict(urllib.parse.parse_qsl(urllib.parse.urlsplit(self.path).query)).get("seconds", "5"))
            Handler.limit_until = time.time() + seconds
            return self.answer(200, {"limited_until": int(Handler.limit_until)})
        if path == "/__slowlog":
            # Every GitLab trace is answered N seconds late.
            Handler.slow_log = float(dict(urllib.parse.parse_qsl(urllib.parse.urlsplit(self.path).query)).get("seconds", "0"))
            return self.answer(200, {"slow_log": Handler.slow_log})
        if path == "/__throttle":
            # Every request answers 429 with no reset time, until /__reset.
            Handler.throttled = True
            return self.answer(200, {"throttled": True})
        if path == "/__reset":
            with self.applied_lock:
                del self.applied[:]
            Handler.limit_until = 0.0
            Handler.throttled = False
            Handler.expired = False
            Handler.slow_log = 0.0
            Handler.traces_at_once_max = 0
            Handler.drafts = []
            return self.answer(200, {"reset": True})
        if path == "/__drafts":
            seeded = json.loads(raw or b"[]")
            with self.drafts_lock:
                Handler.drafts = seeded
                Handler.draft_seq[0] = max([100] + [note["id"] for note in seeded]) + 1
            return self.answer(200, {"drafts": len(seeded)})
        if self.flavor == "github" and re.fullmatch(r"(/api/v3)?/repos/[^/]+/[^/]+/pulls/\d+/comments", path):
            return self.github_review_comment(path, raw)
        if self.flavor != "none" and path.startswith("/api/v4/"):
            return self.rest_write(path, raw)
        if self.flavor == "github" and re.fullmatch(
            r"(/api/v3)?/repos/[^/]+/[^/]+/actions/(runs/\d+/rerun-failed-jobs|jobs/\d+/rerun)", path
        ):
            return self.github_rerun(path, raw)
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
        if operation == "ChangeRequestSearch":
            # GitHub's search: the qualifiers are the fake's to ignore; every
            # free word the user typed must appear in a row.
            for word in [w for w in (variables.get("q") or "").lower().split() if ":" not in w]:
                payload = only_matching(payload, word)
        if self.flavor == "gitlab" and operation == "LabelCandidates" and not re.search(r"includeAncestorGroups:\s*true", query):
            payload = without_group_labels(payload)
        if old and operation in BASELINE_OPERATIONS:
            payload = strip_newer(payload)
        return self.answer(200, payload, [("X-RateLimit-Remaining", "4999")])

    def mutation(self, operation, query, variables):
        found = re.search(r"mutation\s+\w+\s*(?:\([^)]*\))?\s*\{\s*(\w+)", query)
        field = found.group(1) if found else "mutation"
        if self.credential() == "dropped":
            self.close_connection = True
            return
        if self.flavor == "gitlab" and self.credential() == "old" and field in NEWER_GITLAB_MUTATIONS:
            return self.answer(200, {"errors": [{"message": f"Field '{field}' doesn't exist on type 'Mutation'"}]})
        if self.credential() == "notefails" and self.flavor == "gitlab" and operation == "CreateNote":
            return self.answer(200, {"data": {field: {"errors": ["Note creation failed"]}}})
        failure = self.write_failure(field)
        if failure:
            return self.answer(failure[0], failure[1], failure[2])
        given = (variables.get("input") or {}) if isinstance(variables.get("input"), dict) else {}
        key = operation
        for discriminator in ("state", "draft", "strategy", "resolve"):
            if discriminator in given:
                key = f"{operation}.{str(given[discriminator]).lower() if isinstance(given[discriminator], bool) else given[discriminator]}"
        if operation == "AddPullRequestReview" and "event" in given:
            key = f"{operation}.{given['event']}"
        if operation == "AddPullRequestReviewThreadReply" and "pullRequestReviewId" in given:
            key = f"{operation}.review"
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
