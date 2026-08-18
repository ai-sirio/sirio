# Finish-line critic: sidebar-part3 — the ten half-proven F-SID/F-PRJ rows

Lane: `wf-sid2`. Scope: every `F-SID-*`/`F-PRJ-*` row in `INVENTORY-LEDGER.md` whose verdict is
NOT `PASSED` — ten rows (`F-SID-08`, `F-SID-10`, `F-SID-11`, `F-SID-14`, `F-SID-15`, `F-SID-19`,
`F-PRJ-13`, `F-PRJ-14`, `F-PRJ-17`, `F-PRJ-18`), all currently `half-proven`. Each row's existing
evidence already proves one half; this pass drives only the missing half named in that evidence
cell, per the brief. No half that was already proven is re-proven here except where needed to set
up state for the missing half.

Binary pinned per `ENVIRONMENT.md`:

```
cargo build --manifest-path rust/Cargo.toml
cp rust/target/debug/tiller /tmp/wf-sid2-tiller
export TILLER_WL_BIN=/tmp/wf-sid2-tiller TILLER_WL_LABEL=wf-sid2
```

Fixtures created this pass under `/home/enzopalmisano/`: `wf-sid2-nongit` (plain folder, no
`.git`, for F-SID-08), `wf-sid2-removeproj` (git repo, disposable, for F-SID-10), `wf-sid2-decoy`
and `wf-sid2-main` (git repos, for F-PRJ-13's exact-geometry repro), plus whatever additional
disposable fixtures each row's per-row section names.

## Per-row results

(filled in incrementally below, one row at a time, each followed by a commit)
