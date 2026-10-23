# Release, update, apply — three crates, one hand-off each

`sirio_release` owns the signed-artifact format: the channel manifest, the accepted
Ed25519 key set, and `AcceptedKeys::verify` (recompute SHA-256, then `verify_strict`).
`sirio_update` reads the manifest for the compiled-in channel (`sirio_control::
ReleaseChannel::RELEASE_CHANNEL`), downloads, and verifies — and **stops there**,
handing back a `VerifiedUpdate { version, notes, path, platform }`. `sirio_apply` is the
only crate that touches the running installation. The boundary is deliberate: a `Dev`
build never reaches the network at all (`updates_enabled()` is false), and refusing to
apply is a real outcome — `sirio_apply` rejects an install it cannot self-locate rather
than guessing a path. Every packaging file reads its identity from `Scripts/identity.sh`;
`SIRIO_INNO_APP_ID` must never change, because Inno decides upgrade-in-place by comparing it.

Three things in that chain are `option_env!`, baked in at build time and therefore
invisible to any unit test: the manifest URL (`SIRIO_UPDATE_MANIFEST_URL`, honoured only
in debug builds so a shipped Stable binary has no test endpoint), the channel
(`SIRIO_RELEASE_CHANNEL`) and the accepted key set (`SIRIO_RELEASE_ACCEPTED_KEYS`). A
build that never received them behaves exactly like an updater with nothing to report,
which is the failure this chain is least able to notice on its own.
`Scripts/Tests/test-update-e2e.sh` is the answer: it signs a manifest with the real
`sirio-release` CLI, serves it over loopback, compiles
`rust/crates/sirio_apply/examples/update_probe.rs` the way the release job compiles the
app, and drives the real `Updater` through check → download → verify → apply — including,
on Windows, the real silent spawn of the staged extensionless PE. Run it after any change
to the three crates or to the compiled-in environment; `Scripts/ci-linux.sh` runs it as a
stage.
