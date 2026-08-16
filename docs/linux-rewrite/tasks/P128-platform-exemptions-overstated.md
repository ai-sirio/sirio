# P128 — seven rows were exempted as `N/A — platform` that the project's own audit calls feasible

Settles the open question `F-USE-04` and `F-USE-05` carried in their own evidence: *"scope decision
pending — not a settled exemption."* It was not a 2-row question. All **7** rows that
`INVENTORY-AUDIT.md` files under **"Fattibili con adattamento"** — feasible with adaptation, *"il
meccanismo macOS è Apple-only ma l'equivalente Linux esiste"* — were sitting in the ledger as
`N/A — platform`, which asserts the opposite.

Two of the project's own documents disagreed, and the ledger's side of the disagreement is the one
the totals gate counts. That inflated the completed fraction and lowered the apparent ceiling.

## What `N/A — platform` may and may not mean

`N/A — platform` answers a **factual** question: *can this platform express the capability at all?*
It is not a place to record *we chose not to build this*. Conflating the two lets a cost decision
nobody made wear the authority of an impossibility, which is precisely the shape of excuse the
browser rows carried before `P127` — three rows held back by an instrument misconfiguration that read
as a missing platform capability.

Flipping these to `FAILED — absent` therefore does **not** assert the features must be built. It
records that they are absent and leaves the scope call open and visible, where the user can descope
any of them in one line. That is the reversible direction; a wrong `N/A` is the one that hides.

## Row by row

Each was checked against the Rust tree, not against the earlier verdict. Two of the seven turned out
**not to be absent at all** — which is why the class was not flipped wholesale.

| row | was | now | finding |
|---|---|---|---|
| `F-USE-04` | `N/A — platform` | `FAILED — absent` | Menu-bar agent roster. No `StatusNotifierItem`/`ksni`/`tray_icon`/`AppIndicator` anywhere in `rust/crates`. COSMIC supports SNI and `zbus` is already in `Cargo.lock` transitively. |
| `F-USE-05` | `N/A — platform` | `FAILED — absent` | Jump-to-agent from the roster. The behaviour is platform-neutral end to end; both halves (select worktree, activate tab) already exist. Absent because its host surface is. |
| `F-WIN-08` | `N/A — platform` | `FAILED — absent` | The Dock is Apple-only; *panes survive window close* is not, and is core Tiller behaviour. No `hide_on_close` equivalent exists. |
| `F-WIN-09` | `N/A — platform` | `FAILED — absent` | Counterpart is the `org.gnome.desktop.wm.preferences action-double-click-titlebar` gsetting. Nothing reads any titlebar preference. |
| `F-WIN-11` | `N/A — platform` | `FAILED — absent` | **Further along than `N/A` implied.** `UpdateState`/`UpdateEvent` model every state the row names and are unit-tested, with a doc-comment saying they are deliberately transport-independent — but the only reference outside their file is the `pub use` re-export. A zero-consumer type; no toast renders it. |
| `F-CORE-AUTH-03` | `N/A — platform` | `NOT EXERCISED` | **Not absent.** The contract permits *"an explicitly chosen encrypted store"* and that option was taken: `credentials.rs` implements `get`/`set`/`delete`, the three operations the row names. `P111-report.md:263` had already flagged the row as superseded by `F-SET-12`. |
| `F-CORE-FILE-03A` | `N/A — platform` | `half-proven` | **Not absent.** `tiller_terminal/src/lib.rs:1586` handles `on_drop::<gpui::ExternalPaths>`, GPUI's XDND payload. Only the multi-file *ordering* claim is unexercised. |

## Effect on the gate

```
N/A — platform   13 -> 6        FAILED — absent   6 -> 11
half-proven      10 -> 11       NOT EXERCISED     0 -> 1
```

PASSED is unchanged at 328. The **ceiling moves 349 -> 356** (389 − 27 `UNREACHABLE` − 6
`N/A — platform`), so 28 rows are open rather than 21. The seven are sourced
`sweep P128 orchestrator audit, 2026-08-16`, which by design does **not** match the gate's
`CRITIC_PASS` pattern — they are orchestrator-judged and still owe independent provenance under
`P125`. That is honest for a `FAILED`/`half-proven` verdict, which claims absence rather than
success; none of them may reach `PASSED` on this evidence.

## Two decisions that are genuinely the user's

Neither is answered here, and both are cheap to answer:

1. **Build a tray, or descope the roster?** `F-USE-04`/`F-USE-05`/`F-WIN-08` all resolve through one
   StatusNotifierItem surface. It is a real new subsystem — GPUI ships none — but it is one
   subsystem serving three rows, and `zbus` is already in the dependency graph. Descoping all three
   explicitly is equally valid and takes one line.
2. **Is a plaintext-0600 credential store acceptable?** `credentials.rs` argues parity with the
   `~/.claude/.credentials.json` and `~/.codex/auth.json` it already reads, and rejects Secret
   Service for concrete operational reasons (unattended test runs mutating the login keyring, an
   unlocked-keyring prompt blocking a headless session, no hermetic fixture). The reasoning is sound,
   but the contract's third option says *encrypted*, and this store is not. A critic should not
   inherit that call silently — it wants an explicit ruling.

## Related

`P125` (rows without independent provenance) — item 2 of its follow-up list was "settle
`F-USE-04`/`F-USE-05` as a scope decision"; this supersedes it with a wider finding.
`P127` (browser child unavailable) — same failure shape: a capability claim that was really an
instrument or cost claim.
