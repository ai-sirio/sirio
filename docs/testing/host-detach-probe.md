# Probing how the host leaves the app's process group

`Scripts/Tests/probe-host-detach.sh` drives
`rust/crates/sirio_host_client/examples/detach_probe.rs`, which starts a child
through the real `sirio_host_client::detach::spawn_detached` — the function the
app will start the host with — kills the parent's world, and checks the child
is still writing its heartbeat. A row counts only when the kill really
happened: the script confirms the parent's pid is gone before it looks at the
heartbeat (a surviving parent is a FAIL, "parent survived the kill"). It is the
evidence for §5.4 of
`docs/superpowers/specs/2026-10-05-host-foundation-design.md`: the table there
is a hypothesis, and a row stays in it only while this probe shows it working.

## Run it

    Scripts/Tests/probe-host-detach.sh --out-dir "$HOME/.cache/sirio-detach-probe"

One `ROW <name> PASS|FAIL|SKIP` line per row, then `DETACH PROBE OK` (or
`DETACH PROBE FAILED` and exit 1). `--out-dir` keeps each row's directory: the
parent's stdout (`child-method=<method>`), both pids and the last heartbeat.
The probe's scratch directory is `mktemp -d` under `$TMPDIR` (default `/tmp`).

The `sigkill` row kills the parent's whole process group, not one pid: the
parent is started as the leader of a group of its own (job control, so it also
works on macOS, which has no `setsid(1)`), and a child that never left that
group dies with it. Only a child that called `setsid`, lives in a systemd scope
or in a launchd job survives. Killing a single pid would let a plain
`Command::spawn` child pass.

The probe has three roles in one binary — `parent <dir>` detaches a `child`,
`child <dir>` writes `<dir>/heartbeat` (a counter) every 200 ms for 120 s, and
on Windows `job <dir>` runs `parent` inside a kill-on-close job object and then
closes the job. A row passes when the parent is dead and the heartbeat advances
over the 1.5 s after the kill.

## What each row proves

| Platform | Mechanism (`DetachMethod`) | Row | Protects against | Verified |
|---|---|---|---|---|
| Linux | `systemd-run --user --scope`, plus `setsid` (`SystemdScope`) | `sigkill` | `kill -9` of the parent | 2026-10-05, this machine: PASS |
| Linux | same | `scope` | the desktop stopping the app's `app-*.scope` | 2026-10-05, this machine: PASS |
| Linux | `setsid` alone (`Setsid`, when no user manager answers) | `sigkill` | `kill -9` of the parent | 2026-10-05, this machine, run with `XDG_RUNTIME_DIR` unset: PASS. Not offered against a scope stop — see below |
| macOS | `launchctl bootstrap gui/<uid>` of a generated plist (`Launchd`) | `sigkill` | `kill -9` of the parent | pending: `macos-check.yml` (`macos-15`) runs it on every pull request |
| macOS | same | Force Quit | the app's coalition being terminated | manual, procedure below — not yet run |
| Windows | `DETACHED_PROCESS \| CREATE_NEW_PROCESS_GROUP \| CREATE_BREAKAWAY_FROM_JOB` (`WindowsBreakaway`) | `job` | a job object with kill-on-close around the app | pending: `windows-check.yml` (`windows-latest`) runs it on every pull request |

The two `pending` rows have not run yet: `macos-check.yml` and
`windows-check.yml` run the probe, then the lifecycle E2E, on every pull
request, and these rows are filled from this branch's first CI run, with the
run's URL. Until then they are claims this document does not make. The
Force Quit row is manual and stays so (below).

The `setsid` arm is not a defence against a scope stop: a cgroup scope kills
every process in it whatever its session. It is chosen only when no user
systemd manager answers, which is exactly when there is no scope to stop.

A job without `JOB_OBJECT_LIMIT_BREAKAWAY_OK` refuses breakaway with
`ERROR_ACCESS_DENIED`; `spawn_detached` then starts the host anyway and
returns `WindowsNoBreakaway`, and the probe's `job` row is the place a host
that is still inside the job shows up: its child dies with the job, the heartbeat
stops and the row FAILs; if the child survived anyway the row PASSes. Either way
the script prints `note: breakaway was refused by the job (WindowsNoBreakaway);
the host ran inside it`.

## Linux run, 2026-10-05 (after fix round 1)

Linux 7.2.3-arch1-3, systemd user manager `degraded` (so the script's own
`degraded` branch is what let the `scope` row run), `XDG_RUNTIME_DIR` set:

    $ Scripts/Tests/probe-host-detach.sh --out-dir "$HOME/.cache/sirio-detach-probe-r1"
    ROW sigkill PASS
    ROW scope PASS
    DETACH PROBE OK

Both rows' `parent.out` read `child-method=SystemdScope`.

The `Setsid` arm, with `XDG_RUNTIME_DIR` unset so that no user manager is
believed to answer — `sigkill` ran with `child-method=Setsid`:

    $ env -u XDG_RUNTIME_DIR Scripts/Tests/probe-host-detach.sh --out-dir ...
    ROW sigkill PASS
    ROW scope FAIL
    DETACH PROBE FAILED

The `scope` FAIL there is the point made above, not a defect: `systemctl`
still reached the manager through `/run/user/<uid>`, the script stopped the
parent's scope, and a `setsid` child inside that scope goes with it. A
negative control confirms the row discriminates: a plain `sleep` started in a
scope under the same conditions is dead after `systemctl --user stop`.

## Negative controls: the rows can fail

A probe that cannot fail proves nothing, so each guard was shown to bite, by
hand, on 2026-10-05. None of the broken variants is committed.

Mechanism deleted — `spawn_detached`'s Linux arm temporarily reduced to a plain
`Command::spawn` (no `systemd-run`, no `setsid` pre-exec):

    ROW sigkill FAIL
    ROW scope FAIL
    DETACH PROBE FAILED

(both rows' `parent.out` read `child-method=Setsid`: the child stayed in the
parent's group and scope and died with them). A single-pid `kill -9` of the
parent would have let that variant pass the `sigkill` row.

Kill that does not happen — a copy of the script with the `scope` row's
`systemctl --user stop` replaced by `true`:

    ROW sigkill PASS
      scope: parent 2658195 survived the kill, so the row proves nothing
    ROW scope FAIL
    DETACH PROBE FAILED

## Findings while probing

**Is there a user manager?** `systemd_user_available` first judged
`systemctl --user is-system-running` by its exit code. That reads a failed
connection (exit 1, nothing on stdout) as a manager that answers: with
`XDG_RUNTIME_DIR` pointing at a directory with no manager, `spawn_detached`
chose `systemd-run`, which failed after being spawned, and returned
`SystemdScope` with no child ever started. It now reads the state from stdout
(`running`, `degraded`, `starting`, `initializing`) and falls back to `Setsid`
otherwise.

**`SystemdScope` means the child started.** Spawning `systemd-run` proves only
that it was exec'd. `spawn_detached` now waits up to 300 ms for it to exit: a
non-zero exit that early means the scope could not be created, and it falls
back to `Setsid`. The unit name is `<label>-<pid>-<counter>-<nanos-hex>`, so a
process that starts a second host while the first scope lives does not clash
on it. By hand: with a `systemd-run` on `PATH` that exits 1, the probe prints
`child-method=Setsid` and its child beats; with the real one, `SystemdScope`
and the parent's lifetime unchanged; and a process calling `spawn_detached`
twice got `SystemdScope` twice, both children beating. This still does not
mean the host is serving: the caller treats "the endpoint never appeared" as
the start failure.

**The macOS plist carries the whole environment.** Launchd starts a job with
its own environment, so `spawn_detached` writes the caller's into the plist —
tokens included. The plist therefore goes into a directory of mode 0700 as a
file of mode 0600 and is deleted as soon as `launchctl bootstrap` returns,
success or not. A variable whose name or value is not UTF-8, or holds a
character XML 1.0 forbids, is left out of the job's environment (a character
like ESC would make launchd refuse the whole plist); `&`, `<`, `>`, `"` and `'`
are escaped. The escaping and the filtering are unit-tested on every platform
(`cargo test -p sirio_host_client`); the launchctl calls themselves run only on
macOS.

## Force Quit on macOS (manual)

1. `cd rust && cargo build -p sirio_host_client --example detach_probe`
2. Wrap the probe in an app so it appears in the Force Quit window:
   `mkdir -p /tmp/Probe.app/Contents/MacOS && cp target/debug/examples/detach_probe /tmp/Probe.app/Contents/MacOS/Probe`
   and write `/tmp/Probe.app/Contents/Info.plist` with `CFBundleExecutable=Probe`, `CFBundleIdentifier=app.sirioai.probe`:

   ```xml
   <?xml version="1.0" encoding="UTF-8"?>
   <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
   <plist version="1.0"><dict>
   <key>CFBundleExecutable</key><string>Probe</string>
   <key>CFBundleIdentifier</key><string>app.sirioai.probe</string>
   <key>CFBundlePackageType</key><string>APPL</string>
   </dict></plist>
   ```
3. `open /tmp/Probe.app --args parent /tmp/probe-fq` (the probe takes its role from argv).
4. Wait for `/tmp/probe-fq/child.pid`, then ⌥⌘⎋ → Probe → Force Quit.
5. `cat /tmp/probe-fq/heartbeat; sleep 2; cat /tmp/probe-fq/heartbeat` — the number must grow.
6. Record the date, macOS version and both numbers in the table above; `kill $(cat /tmp/probe-fq/child.pid)`.

The probe loads a launchd job labelled `app.sirioai.sirio.host.probe` (its
plist is written under the probe directory's `launchd/` and deleted again once
loaded). The job stays loaded after the child is killed; remove it with
`launchctl bootout gui/$(id -u)/app.sirioai.sirio.host.probe`.
