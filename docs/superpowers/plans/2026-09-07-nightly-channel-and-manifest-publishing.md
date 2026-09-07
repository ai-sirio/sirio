# Nightly channel and manifest publishing — implementation plan

Ticket: [#317](https://github.com/ai-sirio/sirio/issues/317). Spec:
`docs/superpowers/specs/2026-08-29-auto-update-design.md` §2.2, §3.3–3.5, §7.4, §9.

## Why this is wider than "add a schedule"

#317 says the nightly build "signs artifacts the same way a release does". A
release does not sign today: `release.yml` never sets `SIRIO_RELEASE_CHANNEL`,
never runs `sirio-release sign`, never writes a manifest. A tagged release is
therefore a `Dev` build whose updater is compiled off — the Zed post-mortem
bug, shipped by construction. The nightly cannot be built on that. So this
plan wires the shared release path first and adds the nightly on top of it.
The DNS/CNAME/repo-public half of #312 stays human work and is not touched.

## Design

**One reusable workflow, two callers.**

- `.github/workflows/build-release.yml` (`workflow_call`) owns the three build
  jobs (macOS gate → linux, windows) and the `publish` job. Inputs: `channel`,
  `version`, `tag`, `notes`, `prerelease`, `publish`. Secrets inherited.
- `release.yml` (tag push / manual): `prepare` checks tag == `Cargo.toml`
  (existing script) and requires the hand-written notes file, then calls the
  reusable workflow with `channel: stable`.
- `nightly.yml` (schedule + manual): `prepare` stamps the version, skips when
  `main` has not moved since the last nightly release, then calls with
  `channel: nightly`.

**Channel and keys reach the compiler.** Every `cargo` invocation in the build
jobs (gate and release compile) runs with `SIRIO_RELEASE_CHANNEL=<channel>`
and `SIRIO_RELEASE_ACCEPTED_KEYS=${{ vars.SIRIO_RELEASE_ACCEPTED_KEYS }}`.
A guard step fails the build when the key variable is empty: an empty key set
is fail-closed in the app (§4.2), which means a silently dead updater. The
§3.5 gate test in `sirio_control` is extended from `stable` to `nightly`.

**Nightly version.** The updater compares strict semver
(`sirio_update::Updater::check_at`: `version <= current → UpToDate`), and
`CARGO_PKG_VERSION` comes from `rust/Cargo.toml`. A nightly built at the
workspace version would never see the next nightly. So the nightly `prepare`
job computes `<workspace>-nightly.<YYYYMMDDHHMM>` (UTC, one numeric prerelease
identifier: no leading zero, monotonic per minute) and every build job
rewrites `[workspace.package] version` before compiling, via
`Scripts/set-workspace-version.sh`. For stable the rewrite is a no-op
(`check-release-version.sh` already asserted equality).

**Artifact URLs.** Artifacts live in GitHub Releases (§2.1). Their filenames
differ in shape (`Sirio-<v>.dmg`, `Sirio-<v>-x86_64.AppImage`,
`SirioSetup-<v>.exe`), which `sirio-release sign --url-template` cannot express
with `{platform}` alone. Add a `{file}` placeholder (the artifact's basename)
so the template is `https://github.com/<repo>/releases/download/<tag>/{file}`.

**Publish order, verifiable (§9.1).** `publish` does, in order:
1. `gh release create <tag> artifacts/*` (prerelease for nightly).
2. Read the release's asset list back and fail unless every local artifact is
   on it.
3. Build `sirio-release`, sign every artifact with `secrets.SIRIO_RELEASE_SIGNING_KEY`,
   then `sirio-release verify` each one against `vars.SIRIO_RELEASE_ACCEPTED_KEYS`
   — the key in the secret must be one the binary was compiled to accept.
4. Commit `<channel>.json` onto the `gh-pages` branch (created from the empty
   tree if absent, `.nojekyll` alongside) and push. Only that one file is
   touched, so `stable.json` and any `CNAME` GitHub writes survive a nightly
   publish. A `concurrency` group serialises the two channels.

**Notes (§7.4).** Stable: `docs/release-notes/<version>.md`, hand-written,
required — `prepare` fails before the macOS gate if it is missing, and its
content becomes the manifest's `notes`. Seed it with
`Scripts/generate-changelog.sh`. Nightly: a fixed sentence naming the commit.

**Retention.** Nightly releases are not pruned here (§9.3 says never delete a
superseded release; whether that rule bends for nightlies is a decision this
ticket does not own). Flagged as follow-up.

## Steps

1. `sirio-release sign`: `{file}` placeholder — test in `sirio_release`
   first, then the one-line substitution. Update `docs/release-signing.md`.
2. `sirio_control` gate test: accept `nightly` as well as `stable`.
3. `Scripts/set-workspace-version.sh` + `Scripts/Tests/test-set-workspace-version.sh`.
4. `Scripts/Tests/test-release-workflow.sh`: extend to the reusable workflow
   and the new invariants (channel env on every cargo step, key guard, sign
   step, assets-before-manifest order, `{file}` template, stable caller
   requires the notes file). Then `build-release.yml` + new `release.yml`.
5. `Scripts/Tests/test-nightly-workflow.sh` (schedule, `channel: nightly`,
   `nightly.json` only, no `stable.json`, prerelease, skip-when-unchanged).
   Then `nightly.yml`.
6. Docs: `docs/release-signing.md` (CI secret/variable names, `{file}`),
   CLAUDE.md commands block (notes file, nightly), `docs/release-notes/README.md`.

## Verification

- `bash Scripts/Tests/test-*.sh` all PASS on macOS.
- `cargo test -p sirio_release`, `cargo test -p sirio_control`.
- `SIRIO_RELEASE_CHANNEL=nightly cargo test -p sirio_control` exercises the gate.
- `actionlint` on the three workflows if available.
- Not verifiable here: an actual run. Needs the two repo settings the
  maintainer creates (`SIRIO_RELEASE_SIGNING_KEY` secret,
  `SIRIO_RELEASE_ACCEPTED_KEYS` variable) and, for the manifest to be served,
  #312.
