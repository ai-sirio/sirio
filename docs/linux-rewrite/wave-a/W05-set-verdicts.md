# W05-set — adjudicated verdicts

Critic pass over `docs/linux-rewrite/wave-a/W05-set-evidence.md` (driver's report) at HEAD
`4073297`. No source touched; nothing compiled. Every capture cited below was opened and
read, including two images the driver referenced but did not re-capture this pass
(`reference/linux-progress/p111-set17-registry-error.png`,
`p111-set17-registry-recovered-by-retry.png`) and six more from the P111 build report
(`p111-set12-*`, `p111-set13-*`) that the driver's own evidence leaned on without
re-quoting — all independently re-viewed rather than taken on the report's prose.

| id | verdict | note |
|---|---|---|
| F-SET-11 | half-proven | unchanged from ledger; see notes — the could-not-reach reasoning does not fully hold |
| F-SET-12 | half-proven | up from FAILED — absent; save/clear/override proven live, refresh-now not exercised for this provider |
| F-SET-13 | PASSED | up from FAILED — absent; full VERIFY clause closed with live, discriminating evidence |
| F-SET-14 | half-proven | up from FAILED — defective; ledger's "dead control" premise was a source-citation error, corrected below |
| F-SET-15 | half-proven | unchanged; reconfirmed live |
| F-SET-17 | PASSED | up from FAILED — absent; full VERIFY clause closed across this pass + independently re-verified on-record captures |

See the structured return for full per-row reasoning and `notes` for the two disagreements
with the driver worth flagging on their own.
