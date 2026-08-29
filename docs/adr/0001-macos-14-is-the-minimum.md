# 1. macOS 14 is Sirio's minimum supported version

Date: 2026-08-26

## Status

Accepted

## Context

Sirio keeps one shared browser profile, and that profile must be isolated per
binary checkout in development so two builds cannot write each other's state —
the same rule the session database already follows (`session.rs:140-160`).

On Linux and Windows the isolation is a directory, set through wry's public
`WebContext::new(Some(path))`. On macOS wry exposes no way to choose a path at
all: `wkwebview/mod.rs` contains zero references to `WebContext`. The only lever
is `WebViewBuilderExtDarwin::with_data_store_identifier([u8; 16])`, which yields
a stable, isolated store whose directory WebKit picks.

That method requires **macOS 14**. Below it the identifier is not rejected — it
is **silently ignored** (`wkwebview/mod.rs:220-247`), so every build sharing a
process name silently shares one store.

The repository declared no minimum macOS version before this decision.

## Decision

Sirio supports **macOS 14 and later**.

## Consequences

The browser profile is isolated on every supported macOS version, with no
version detection, no degraded path, and no silently-shared store.

The cost is paid outside the browser. Anything that states, checks, or ships a
platform requirement now has an answer it did not have before, and must agree
with it: the bundle's `LSMinimumSystemVersion`, the release artifacts and the
update manifest (see the auto-update map, #12), any CI matrix, and the user-facing
system requirements. macOS 13 and earlier are no longer supported at all — not
"supported without profile isolation".

This is a product decision reached from a narrow cause. It was taken knowingly:
the alternative considered was to treat pre-14 as unsupported *for isolation only*
while the app kept running there, which confines the impact to development
machines but leaves a silently shared store in the field. That alternative was
rejected in [#140](https://github.com/tillerai/tiller/issues/140).
