# Probing how the host leaves the app's process group

`Scripts/Tests/probe-host-detach.sh` drives
`rust/crates/sirio_host_client/examples/detach_probe.rs`, which starts a child
through the real `sirio_host_client::detach::spawn_detached` — the function the
app will start the host with — kills the parent's world, and checks the child
is still writing its heartbeat. It is the evidence for §5.4 of
`docs/superpowers/specs/2026-10-05-host-foundation-design.md`: the table there
is a hypothesis, and a row stays in it only while this probe shows it working.

## Run it

    Scripts/Tests/probe-host-detach.sh --out-dir "$HOME/.cache/sirio-detach-probe"

One `ROW <name> PASS|FAIL|SKIP` line per row, then `DETACH PROBE OK` (or
`DETACH PROBE FAILED` and exit 1). `--out-dir` keeps each row's directory: the
parent's stdout (`child-method=<method>`), both pids and the last heartbeat.
The probe's scratch directory is `mktemp -d` under `$TMPDIR` (default `/tmp`).

The probe has three roles in one binary — `parent <dir>` detaches a `child`,
`child <dir>` writes `<dir>/heartbeat` (a counter) every 200 ms for 120 s, and
on Windows `job <dir>` runs `parent` inside a kill-on-close job object and then
closes the job. A row passes when the heartbeat advances over the 1.5 s after
the kill.

## What each row proves

| Platform | Mechanism (`DetachMethod`) | Row | Protects against | Verified |
|---|---|---|---|---|
| Linux | `systemd-run --user --scope`, plus `setsid` (`SystemdScope`) | `sigkill` | `kill -9` of the parent | 2026-10-05, this machine: PASS |
| Linux | same | `scope` | the desktop stopping the app's `app-*.scope` | 2026-10-05, this machine: PASS |
| Linux | `setsid` alone (`Setsid`, when no user manager answers) | `sigkill` | `kill -9` of the parent | 2026-10-05, this machine, run with `XDG_RUNTIME_DIR` unset: PASS. Not offered against a scope stop — see below |
| macOS | `launchctl bootstrap gui/<uid>` of a generated plist (`Launchd`) | `sigkill` | `kill -9` of the parent | CI, `macos-check.yml` (Task 11) |
| macOS | same | Force Quit | the app's coalition being terminated | manual, procedure below — not yet run |
| Windows | `DETACHED_PROCESS \| CREATE_NEW_PROCESS_GROUP \| CREATE_BREAKAWAY_FROM_JOB` (`WindowsBreakaway`) | `job` | a job object with kill-on-close around the app | CI, `windows-check.yml` (Task 11) |

The `setsid` arm is not a defence against a scope stop: a cgroup scope kills
every process in it whatever its session. It is chosen only when no user
systemd manager answers, which is exactly when there is no scope to stop.

A job without `JOB_OBJECT_LIMIT_BREAKAWAY_OK` refuses breakaway with
`ERROR_ACCESS_DENIED`; `spawn_detached` then starts the host anyway and
returns `WindowsNoBreakaway`, and the probe's `job` row is the place a host
that is still inside the job shows up (the script prints
`note: breakaway refused by the job`).

## Linux run, 2026-10-05

Linux 7.2.3-arch1-3, systemd user manager `degraded` (so the script's own
`degraded` branch is what let the `scope` row run), `XDG_RUNTIME_DIR` set:

    $ Scripts/Tests/probe-host-detach.sh --out-dir "$HOME/.cache/sirio-detach-probe"
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

## Finding while probing: the "is there a user manager" check

`systemd_user_available` first judged `systemctl --user is-system-running` by
its exit code. That reads a failed connection (exit 1, nothing on stdout) as a
manager that answers: with `XDG_RUNTIME_DIR` pointing at a directory with no
manager, `spawn_detached` chose `systemd-run`, which failed after being
spawned, and returned `SystemdScope` with no child ever started. It now reads
the state from stdout (`running`, `degraded`, `starting`, `initializing`) and
falls back to `Setsid` otherwise; the same experiment then prints
`child-method=Setsid` and the child beats.

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

The probe loads a launchd job labelled `app.sirioai.sirio.host.probe` (the
plist is under the probe directory's `launchd/`). It stays loaded after the
child is killed; remove it with
`launchctl bootout gui/$(id -u)/app.sirioai.sirio.host.probe`.
