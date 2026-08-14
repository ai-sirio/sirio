# Drive slice E01-set — 5 rows

Families: F-SET.

Exercise each row live and record what you saw. **Do not edit the ledger**;
a different agent adjudicates. For a `half-proven` row the evidence column
names the half already proven — drive only the missing half and say which.

| row | ledger line | current verdict | evidence recorded so far |
|---|---|---|---|
| `F-SET-04` | 292 | half-proven | UI half verified: drawn `general_surface_settings_flow_into_the_persistence_contract` green (tiller_ui suite, pass 14); DB half still absent — the persisted schema does not carry the surface-settings keys yet (main.rs test comment: loaded snapshots start at defaults; P58 handoff to codex11 open) |
| `F-SET-05` | 293 | half-proven | UI half verified: drawn `general_surface_settings_flow_into_the_persistence_contract` + `summarizer_picker_is_gated_on_auto_naming_and_selects` green (tiller_ui suite, pass 14); DB half absent — persisted schema lacks the key (P58 handoff open); rename behavior still has no consumer |
| `F-SET-06` | 294 | half-proven | UI half verified: drawn `general_surface_settings_flow_into_the_persistence_contract` green (tiller_ui suite, pass 14); DB half absent — persisted schema lacks the keys (P58 handoff open); trimming still has no consumer |
| `F-SET-07` | 295 | half-proven | UI half verified: drawn `general_surface_settings_flow_into_the_persistence_contract` green (tiller_ui suite, pass 14); DB half absent — persisted schema lacks the keys (P58 handoff open); eviction stays absent (ACT-26 is its own row) |
| `F-SET-09` | 297 | NOT EXERCISED | P101 independently found the current button handler calls `on_install_skill(agent_skill_install_command())`, and `main.rs` maps it to a terminal shell; headless `surface settings open --section general` also mounted General. Those are not feedback from clicking Install Skill. The required click and success/error/control-state observation could not run because the `:1` offici… |
