# Release notes

One file per Stable release, `<version>.md`, matching `[workspace.package]
version` in `rust/Cargo.toml` — `0.6.0.md` for tag `v0.6.0`. The release job
(`.github/workflows/release.yml`) refuses a tag whose file is missing or
empty, before it builds anything, and ships the file's text inside the
channel manifest as the `notes` the update prompt shows (auto-update design
spec §7.4).

Write them by hand. Seed a draft with the changelog script and cut it down to
what a user should read — half the commits in a release are refactors and
test scaffolding:

```bash
Scripts/generate-changelog.sh v0.6.0 > docs/release-notes/0.6.0.md
```

Commit the file before pushing the tag. There is deliberately no
`CHANGELOG.md`: a file that must be updated in the same commit as a release
goes stale the first busy week, whereas a missing per-version file fails the
release loudly.

Nightly needs no file here: its notes say once that the channel tracks
`main`, and name the commit.
