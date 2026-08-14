# Evidence — E11-git+tab+win (F-GIT, F-TAB, F-WIN)

Driven from worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch
`linux/gpui-waku`, HEAD `4073297`, Wayland lane label `drive-E11-git+tab+win`.

## F-GIT-REMOTE-01 (ledger line 489, half-proven)

Missing half per current ledger note: `github_owner` (`tiller_git/src/remote.rs:76`) is claimed
to have **0 app references** — proven only by unit test, not by any live UI path.

Re-checked by grep across the whole tree (`rust/`, all crates including `tiller_ui`/`tiller`):

```
grep -rn "github_owner" rust/ --include=*.rs
```

Hits: `tiller_git/src/lib.rs:62` (re-export), `tiller_git/src/remote.rs:13,15,20,76,77`
(definition + internal call from the instance-method wrapper to
`github_owner_from_url`), and `tiller_git/tests/p41_git_behaviors.rs:186,190,194,212,225`
(unit tests only). **No caller anywhere outside `tiller_git` itself** — not in `tiller_ui`,
not in the `tiller` app crate, not in `TillerControl`. This confirms the ledger note exactly:
the function is dead code from the app's perspective, reachable only through its own crate's
test suite.

This is not a gap the Wayland lane (or any live drive) can close, because there is no UI
control, no control-socket method, and no code path in the running app that calls this
function — it is unreachable by construction, not merely undemonstrated. Ran the specific
unit test as corroboration that the tested behavior itself is not broken:

```
cd rust && cargo test -p tiller_git --test p41_git_behaviors remote_parsing_supports_github_ssh_https_and_project_suffixes
-> test remote_parsing_supports_github_ssh_https_and_project_suffixes ... ok
```

**Claim: could-not-reach.** The missing half (`github_owner` exercised via the app) cannot be
driven from any input path, live or socket — the function has zero callers outside its own
crate's tests. The half already proven (`project_name`, 8 app references incl.
`tiller_ui/src/project_forms.rs:704`) stands as previously recorded; not re-driven here since
the ledger already treats it as proven and this pass targeted only the missing half.
