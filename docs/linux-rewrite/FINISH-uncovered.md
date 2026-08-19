# Finish-line critic: the ten uncovered rows (wave wf-rest4)

Lane: `wf-rest4`. Ten rows, each already half-proven (or NOT EXERCISED) with one named missing
half — see the brief. This report drives exactly that missing half per row; the already-proven
half is not re-litigated. Binary pinned per `ENVIRONMENT.md`:

```
cargo build --manifest-path rust/Cargo.toml
cp rust/target/debug/tiller /tmp/wf-rest4-tiller
export TILLER_WL_BIN=/tmp/wf-rest4-tiller TILLER_WL_LABEL=wf-rest4
```

Verdict vocabulary is `PASSED | FAILED - defective | FAILED - absent | half-proven | UNREACHABLE |
NOT EXERCISED | N/A - platform`, exactly as `EVIDENCE-STANDARD.md`/the brief specify.

This file is written incrementally, one row at a time, and committed after each row lands.

---

## Status table (filled in as driven)

| row | verdict | one-line reason |
|---|---|---|
| F-TAB-09 | (in progress) | |
| F-CORE-FILE-04 | | |
| F-CORE-FILE-03A | | |
| F-GIT-RUN-01 | | |
| F-TAB-20 | | |
| F-CHAT-25 | | |
| F-CHAT-33 | | |
| F-CORE-ACT-17 | | |
| F-CORE-ACT-24 | | |
| F-AGENT-CODEX-01 | | |

---
