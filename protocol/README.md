# The Sirio host protocol

This directory is the host protocol's contract, written for any client to
read: the app, `sirioctl`, a probe, or a program in another language. The
Rust types in `rust/crates/sirio_host_protocol` are the source of truth;
everything here is committed beside them. `host-v1/capabilities.json` is the
ledger: each capability, the minor it arrived in, its methods and topics.
`host-v1/schema.json` is the JSON Schema of every wire type, generated from
the types. `host-v1/conformance/*.json` are request / expected-response
cases, each `{name, major, steps: [{send, expect}]}` run in order on one
connection against a fresh `sirio-host`. `expect` is a subset match: every
key in it must be present and equal in the response, and extra keys are
allowed, which is what a minor's additivity means. A `"$generation"` string
in `send` stands for the `generation` of the Welcome the case received.
Design: `docs/superpowers/specs/2026-10-05-host-foundation-design.md` §6.

The rules every change to the protocol keeps (spec §6.4):

1. The major must equal one the client speaks; the minor is additive only.
2. Unknown fields are ignored; unknown enum values degrade to `Other` — the
   same posture `sirio_claude` takes toward Claude Code's protocol.
3. A new method or topic is enabled only through a capability in `Welcome`,
   never by probing a method and reading the error.
4. A change lands **host first**: the host gains the capability before any
   client relies on it.
5. A change that a peer of the same major could misread — a new frame kind,
   a field whose absence changes meaning, a narrowed type — is a **major**.
   A major is rare by discipline, and comes with the N−1 adapter in
   `sirio_host_client`, removed one stable release after.

A test fails when a wire type no longer matches `schema.json`, so a change
to the contract is committed, and reviewed, on purpose. To regenerate the
schema after such a change, from `rust/`:

```
cargo run -p sirio_host_protocol --example write_schema -- ../protocol/host-v1/schema.json
```

The conformance cases run against a real host, not against the types, so a
ledger that names a method the host does not serve is caught there.
