# P37 Git Mutation Doors Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refuse staging of conflicted paths and expose the five existing Changes mutations through the live control socket and `tillerctl`.

**Architecture:** Keep conflict detection and all Git mutations in `tiller_git`. The UI and socket both call those same public functions; the app-side socket handler only resolves a worktree and translates the result to a control response. The mounted Changes surface continues to observe external mutations through its existing refresh loop.

**Tech Stack:** Rust 2024, Cargo workspace, `tiller_git`, `tiller_control`, GPUI app shell, real temporary Git repositories, NDJSON Unix-socket control protocol.

## Global Constraints

- `stage` refuses a conflicted path with an actionable typed error.
- `stage_all` preflights status and aborts before mutation when any conflicted path exists, naming every conflicted path.
- The five wire methods are `surface.changes.stage`, `.unstage`, `.discard`, `.stage_all`, and `.discard_all`.
- Single-path methods require a repo-relative `path`; all methods accept optional `worktree`.
- UI and automation call the same public `tiller_git` mutation functions.
- Do not edit `rust/crates/tiller_ui/src/changes.rs`; pi owns Git-error rendering and Retry behavior.
- Verify mutations against real `git status --porcelain`, then run `./Scripts/ci-linux.sh` and require `CI OK`.

---

### Task 1: Refuse staging of conflicted paths in `tiller_git`

**Files:**
- Modify: `rust/crates/tiller_git/src/error.rs`
- Modify: `rust/crates/tiller_git/src/actions.rs`
- Test: `rust/crates/tiller_git/tests/git_integration.rs`

**Interfaces:**
- Produces `GitError::ConflictedPaths { paths: Vec<PathBuf> }`.
- Preserves `stage(repo, path)`, `stage_all(repo)`, and their existing `Result<(), GitError>` signatures.
- `stage` returns one matching conflicted path; `stage_all` returns every conflicted path sorted as returned by the status snapshot.

- [ ] **Step 1: Add failing real-repository tests**

Extract the existing inline conflict fixture into a helper that creates two conflicting files (`f.txt` and `g.txt`): commit both on `main`, commit different contents on `side`, commit different contents on `main`, and run `git merge side` while asserting exit code `1`. Add a `status_porcelain(repo)` helper that runs `git status --porcelain` and returns stdout.

Add these tests to `git_integration.rs`:

```rust
#[test]
fn stage_refuses_a_conflicted_path_without_changing_git() {
    let repo = make_conflicted_repo();
    let before = status_porcelain(repo.path());

    let error = stage(repo.path(), Path::new("f.txt")).expect_err("conflict must be refused");

    assert_eq!(
        error,
        tiller_git::GitError::ConflictedPaths {
            paths: vec![PathBuf::from("f.txt")],
        }
    );
    assert_eq!(status_porcelain(repo.path()), before);
}

#[test]
fn stage_all_refuses_every_conflicted_path_before_mutation() {
    let repo = make_conflicted_repo();
    let before = status_porcelain(repo.path());

    let error = stage_all(repo.path()).expect_err("batch conflict must be refused");

    assert_eq!(
        error,
        tiller_git::GitError::ConflictedPaths {
            paths: vec![PathBuf::from("f.txt"), PathBuf::from("g.txt")],
        }
    );
    assert_eq!(status_porcelain(repo.path()), before);
}
```

- [ ] **Step 2: Run the focused tests and verify red**

Run:

```bash
cd rust && cargo test -p tiller_git --test git_integration stage_refuses -- --exact
cd rust && cargo test -p tiller_git --test git_integration stage_all_refuses -- --exact
```

Expected: compilation fails because `GitError::ConflictedPaths` and the new fixture helper do not yet exist.

- [ ] **Step 3: Add the typed error and preflight guard**

In `error.rs`, add:

```rust
ConflictedPaths { paths: Vec<std::path::PathBuf> },
```

and render it as `refusing to stage conflicted path(s): <comma-separated paths>`, with the singular/plural wording chosen from `paths.len()`. In `actions.rs`, add a private helper:

```rust
fn conflicted_paths(repo: &Path) -> Result<Vec<std::path::PathBuf>, GitError> {
    Ok(crate::status::status(repo)?
        .entries
        .into_iter()
        .filter(|entry| entry.is_conflicted())
        .map(|entry| entry.path)
        .collect())
}
```

Make `stage` query this list and return `ConflictedPaths` when it contains the requested path. Make `stage_all` return the full non-empty list before calling `git::run_accepting`; otherwise retain the existing `git add -A` call. Do not alter `unstage`, `discard`, or their command arguments.

- [ ] **Step 4: Run the focused tests and verify green**

Run:

```bash
cd rust && cargo test -p tiller_git --test git_integration stage_refuses -- --exact
cd rust && cargo test -p tiller_git --test git_integration stage_all_refuses -- --exact
cd rust && cargo test -p tiller_git --test git_integration actions_stage
```

Expected: all focused tests pass, including the existing normal mutation behavior.

- [ ] **Step 5: Commit the Git-layer slice**

```bash
git add rust/crates/tiller_git/src/error.rs rust/crates/tiller_git/src/actions.rs rust/crates/tiller_git/tests/git_integration.rs
git commit -m "fix: refuse staging conflicted git paths"
```

### Task 2: Add control protocol request builders and CLI doors

**Files:**
- Modify: `rust/crates/tiller_control/src/protocol.rs`
- Modify: `rust/crates/tiller_control/src/bin/tillerctl.rs`
- Test: `rust/crates/tiller_control/tests/control_integration.rs`

**Interfaces:**
- Produces request builders `changes_stage`, `changes_unstage`, `changes_discard`, `changes_stage_all`, and `changes_discard_all`.
- Produces CLI commands under `tillerctl surface changes` with the exact spellings `stage`, `unstage`, `discard`, `stage-all`, and `discard-all`.
- Wire parameters are `path` for single-path calls and optional `worktree` for all five.

- [ ] **Step 1: Add failing request and CLI mapping tests**

Extend the protocol unit tests with assertions like:

```rust
let request = request::changes_stage("f.txt", Some("wt-1"));
assert_eq!(request.method, "surface.changes.stage");
assert_eq!(request.params.get("path").map(String::as_str), Some("f.txt"));
assert_eq!(request.params.get("worktree").map(String::as_str), Some("wt-1"));
assert_eq!(request::changes_stage_all(None).method, "surface.changes.stage_all");
```

Add `surface.changes.*` to the test handler's capability list and response match, then add one integration test that invokes all five CLI forms and asserts the handler saw the ordered methods:

```rust
let commands = [
    (vec!["surface", "changes", "stage", "f.txt"], "surface.changes.stage"),
    (vec!["surface", "changes", "unstage", "f.txt"], "surface.changes.unstage"),
    (vec!["surface", "changes", "discard", "f.txt"], "surface.changes.discard"),
    (vec!["surface", "changes", "stage-all"], "surface.changes.stage_all"),
    (vec!["surface", "changes", "discard-all"], "surface.changes.discard_all"),
];
```

- [ ] **Step 2: Run the focused tests and verify red**

Run:

```bash
cd rust && cargo test -p tiller_control --test control_integration tillerctl_exposes_changes_mutation_subcommands -- --exact
cd rust && cargo test -p tiller_control protocol::tests --lib
```

Expected: compilation fails because the builders, CLI branches, and new test-handler methods are absent.

- [ ] **Step 3: Implement request builders, parser branches, and help text**

Add the five builders beside `changes_open`/`changes_read`, using `request("surface.changes.<verb>", params)`. In `cmd_surface`, require positional argument 2 for `stage`, `unstage`, and `discard`; pass `parsed.value("worktree")`; and dispatch the all-path forms without a path. Add all five forms to `usage()`.

- [ ] **Step 4: Run the focused tests and verify green**

Run:

```bash
cd rust && cargo test -p tiller_control --test control_integration tillerctl_exposes_changes_mutation_subcommands -- --exact
cd rust && cargo test -p tiller_control --lib
```

Expected: request methods, parameters, and CLI dispatch all pass.

- [ ] **Step 5: Commit the protocol/CLI slice**

```bash
git add rust/crates/tiller_control/src/protocol.rs rust/crates/tiller_control/src/bin/tillerctl.rs rust/crates/tiller_control/tests/control_integration.rs
git commit -m "feat: expose changes mutations in tillerctl"
```

### Task 3: Route live socket calls through the shared Git functions

**Files:**
- Modify: `rust/crates/tiller/Cargo.toml`
- Modify: `rust/crates/tiller/src/main.rs`
- Test: `rust/crates/tiller/src/main.rs` existing control-handler tests

**Interfaces:**
- Adds `tiller_git` as a direct app dependency.
- `AppControlHandler` resolves the optional `worktree` through `ControlState::working_directory` and returns a control failure for missing path or unknown/unmounted worktree.
- Successful mutation responses include the resolved `worktree` and, for single-path calls, `path`.

- [ ] **Step 1: Add failing capability and validation tests**

Extend `surface_methods_are_advertised_by_capabilities` with all five method names. Add a handler test that sends `surface.changes.stage` without `path` and asserts `ok == false` with an error naming the required parameter; add a second request with an unknown worktree and assert that it fails without a Git invocation.

- [ ] **Step 2: Run the focused app tests and verify red**

Run:

```bash
cd rust && cargo test -p tiller --bin tiller surface_methods_are_advertised_by_capabilities -- --exact
```

Expected: the capability assertion fails because the app does not advertise the mutation methods yet.

- [ ] **Step 3: Implement the app-side dispatch**

Add `tiller_git.workspace = true` to `rust/crates/tiller/Cargo.toml` and import `discard`, `discard_all`, `stage`, `stage_all`, `unstage`, and `GitError` in `main.rs`.

Add two helpers on `AppControlHandler` with function-pointer seams:

```rust
fn run_changes_path_action(
    &self,
    request: &ControlRequest,
    action: fn(&Path, &Path) -> Result<(), GitError>,
) -> ControlResponse;

fn run_changes_all_action(
    &self,
    request: &ControlRequest,
    action: fn(&Path) -> Result<(), GitError>,
) -> ControlResponse;
```

Each helper resolves `request.params.get("worktree")`, validates `path` for the path variant, calls the supplied public `tiller_git` function, maps `Ok(())` to success, and maps `GitError` with `to_string()` to `ControlResponse::failure`. Add the five match arms and advertise the methods in `system.capabilities`. This ensures socket calls and the existing UI closures execute the same Git functions, including the conflict guard.

- [ ] **Step 4: Run app tests and the package build**

Run:

```bash
cd rust && cargo test -p tiller --bin tiller surface_methods_are_advertised_by_capabilities -- --exact
cd rust && cargo test -p tiller --bin tiller
```

Expected: the capability, validation, and existing app control tests pass and the binary compiles with the direct `tiller_git` dependency.

- [ ] **Step 5: Commit the app routing slice**

```bash
git add rust/crates/tiller/Cargo.toml rust/crates/tiller/src/main.rs
git commit -m "feat: route changes mutations through control socket"
```

### Task 4: Exercise the complete mutation door against Git and run the gate

**Files:**
- No product source changes; use a temporary fixture repository and the built `tiller`/`tillerctl` binaries.
- Do not modify `rust/crates/tiller_ui/src/changes.rs`.

- [ ] **Step 1: Build the headless binaries**

Run:

```bash
cd rust && cargo build -p tiller --bin tiller -p tiller_control --bin tillerctl
```

- [ ] **Step 2: Create a fixture and start Tiller with a temporary socket**

Create a temporary repository with one committed tracked file, launch Tiller from that directory with `TILLER_SOCKET=/tmp/p37-git-mutations.sock` and both display variables unset, and obtain the current worktree selector with `tillerctl list-workspaces`.

- [ ] **Step 3: Verify the normal mutation transcript against Git**

Use the socket to perform `stage`, `unstage`, and `discard` on an edited tracked file, then create a staged edit plus an untracked file and perform `stage-all`. After every call, capture `git status --porcelain`; unstage the staged paths and use `discard-all`, confirming tracked edits disappear while the untracked file remains.

- [ ] **Step 4: Verify conflict refusal and all-path naming**

Create two real merge conflicts, call `tillerctl surface changes stage <path>`, and record the nonzero CLI response naming the requested conflict. Call `stage-all` and record the error naming both conflicted paths. Compare `git status --porcelain` before and after each refusal to prove the index did not change.

- [ ] **Step 5: Run the full Linux verification gate**

Run:

```bash
./Scripts/ci-linux.sh
```

Expected: exit code `0` and the literal final marker `CI OK`. Also run `git status --short` and confirm the diff contains only the P37 files plus the pre-existing user changes; `rust/crates/tiller_ui/src/changes.rs` must remain untouched by this piece.
