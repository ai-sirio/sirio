# Release signing: Ed25519 keys and the channel manifest

How release artifacts get signed, what the app accepts, and how a signing key
is rotated. The verifier lives in the `sirio_release` crate; the signing CLI
is its `sirio-release` binary. This document is the format contract the
updater (#311) codes against.

## Threat model, in one paragraph

TLS proves you talked to the right host and nothing about what that host
served — and a host is exactly the thing that gets compromised. Artifact URLs
are deliberately re-hostable (§2.2 of the auto-update design), which is only
safe if trust rests on the payload, not on where it was fetched from. A hash
alone does not do that: the manifest is served over the same channel by the
same authority, so whoever can serve a bad artifact can serve a matching hash.
The hash catches **corruption**; the signature catches **substitution**. An
AppImage the updater swapped in is validated by nothing at the OS level, so
this check is load-bearing, not belt-and-braces.

## Key generation and custody

Run once, in the signing environment:

```bash
cargo run -p sirio_release --bin sirio-release -- keygen --out /secure/sirio-signing
# private key: /secure/sirio-signing/sirio-release-signing.key   (base64 seed, 0600 on unix)
# public key:  /secure/sirio-signing/sirio-release-signing.pub   (base64, compile into the app)
```

Rules for the private half (`sirio-release-signing.key`):

- It lives only in the signing environment — a maintainer machine or a secret
  store dedicated to release signing. Never in the repository, never on the
  download host, never in CI logs.
- If the Windows OV Authenticode signing route is ever taken, that credential
  must **not** share a home with this key: one compromised machine would
  otherwise yield both signatures.
- Losing it is not fatal — that is what rotation is for. Leaking it is: see
  "Compromise" below.

The public half is pasted as standard base64 into the app's `AcceptedKeys`
(the updater compiles the set into the binary, next to the manifest URL).

## What the release job needs

`.github/workflows/build-release.yml` — the path both channels share — reads
the two halves from the repository, and refuses to build or publish when
either is missing:

| Where | Name | Value |
| --- | --- | --- |
| Repository **variable** | `SIRIO_RELEASE_ACCEPTED_KEYS` | The contents of `sirio-release-signing.pub`, space-separated if more than one key (rotation below). Compiled into every binary of both channels; the build fails when it is empty, because an empty set is fail-closed and looks exactly like a dead updater. |
| Repository **secret** | `SIRIO_RELEASE_SIGNING_KEY` | The contents of `sirio-release-signing.key` (the base64 seed). Read by the `publish` job only, written to a `0600` file for the length of one `sign` call, then deleted. |

The `publish` job then, in this order and never another (spec §9.1): creates
the GitHub release as a **draft** with every artifact attached (or re-uploads
into an existing one, so a re-run is idempotent), reads the asset list back
and refuses to continue unless it matches what was built, signs the artifacts
into `<channel>.json`, verifies that manifest against the *public* keys the
binaries were compiled with — so a key mismatch between secret and variable
is caught before anything is public — makes the release public, and only
then commits the manifest onto the `gh-pages` branch, which is what
`dl.sirioai.app` serves (#312). A failure before the un-draft leaves a draft
to retry, never a public release without a manifest.
Only that one file is added on the branch: the other channel's manifest and
GitHub's own `CNAME` file survive a publish. Last, it reads the manifest back
from the branch and polls the Pages site until it serves that version; until
Pages is configured the run ends with a warning saying nothing serves it.

Every build job also asks the `sirioctl` it just built for its compiled
channel and version (`sirioctl version --json`, via
`Scripts/assert-built-channel.sh`), so a binary that fell back to the `dev`
channel is refused before it is bundled or signed.

The Stable job takes its notes from `docs/release-notes/<version>.md` and
refuses a tag without one (see `docs/release-notes/README.md`). The Nightly
job compiles `<workspace>-nightly.<YYYYMMDDHHMM>` in and says only that it
tracks `main`.

## Signing by hand

The same CLI the job runs, for a release signed outside CI:

```bash
cargo run -p sirio_release --bin sirio-release -- sign \
  --key /secure/sirio-signing/sirio-release-signing.key \
  --channel stable --version 0.6.1 \
  --notes "$(cat docs/release-notes/0.6.1.md)" \
  --url-template 'https://github.com/ai-sirio/sirio/releases/download/v0.6.1/{file}' \
  --artifact darwin-aarch64=./dist/Sirio-0.6.1.dmg \
  --artifact linux-x86_64=./dist/Sirio-0.6.1-x86_64.AppImage \
  --artifact windows-x86_64=./dist/SirioSetup-0.6.1.exe \
  --out stable.json
```

Each `--artifact <platform>=<file>` is hashed (SHA-256, lowercase hex) and
signed (Ed25519 over the exact file bytes) in one pass. The URL template
substitutes `{channel}`, `{version}`, `{platform}` and `{file}` — the
artifact's own file name — per artifact; `{file}` is what lets the template
name the three differently-shaped files GitHub Releases keeps. Sanity-check
the manifest before publishing — no signature survives a re-uploaded file:

```bash
sirio-release verify --manifest stable.json --platform darwin-aarch64 \
  --artifact ./dist/Sirio-0.6.1.dmg \
  --pub-key "$(cat /secure/sirio-signing/sirio-release-signing.pub)"
```

## The manifest format (contract for #311)

`https://dl.sirioai.app/stable.json` / `nightly.json`, one file per channel:

```json
{
  "schema": 1,
  "channel": "stable",
  "version": "0.6.1",
  "notes": "- Fixed the pane restore crash\n- Nightly now uses less CPU",
  "artifacts": {
    "darwin-aarch64": {
      "url": "https://github.com/ai-sirio/sirio/releases/download/v0.6.1/Sirio-0.6.1.dmg",
      "sha256": "86c40f34762a9bdd7cbd4ce25dea5dd4d4687b14466c280c72f8c85881a03d61",
      "signature": "cOeO6MCkN/sQFkMg3+KfNln+6Cl4PLGQ4plGMXRQiDOYd18bBo1FAa1VI3wrK32MN8tHAIKrNeLZgJdBRUwmBQ=="
    },
    "linux-x86_64": {
      "url": "https://github.com/ai-sirio/sirio/releases/download/v0.6.1/Sirio-0.6.1-x86_64.AppImage",
      "sha256": "…",
      "signature": "…"
    },
    "windows-x86_64": {
      "url": "https://github.com/ai-sirio/sirio/releases/download/v0.6.1/SirioSetup-0.6.1.exe",
      "sha256": "…",
      "signature": "…"
    }
  }
}
```

| Field | Rules |
| --- | --- |
| `schema` | Must be `1`. Anything else is refused, so the format can evolve without old binaries guessing. |
| `channel` | `stable` or `nightly` — the manifest's own channel. |
| `version` | The released version; derives from `[workspace.package] version`, never the other way round (§3.2). |
| `notes` | Release notes as text, shipped inside the manifest: zero extra requests, no public repo needed. |
| `artifacts` keys | Platform keys, same spelling as `sirio_registry::current_platform_key`: `darwin-aarch64`, `darwin-x86_64`, `linux-x86_64`, `linux-aarch64`, `windows-x86_64`, `windows-aarch64`. |
| `artifacts.*.url` | Where the artifact lives today; re-read on every check, hosting can move. Not trusted by itself. |
| `artifacts.*.sha256` | Lowercase hex SHA-256 of the exact artifact bytes. |
| `artifacts.*.signature` | Standard base64 of the 64-byte Ed25519 signature over the exact artifact bytes. **Required** — a missing, empty or malformed signature rejects the artifact; there is no "accept unverified" path. |

Parsing is strict (`deny_unknown_fields`): a typo'd field is a parse error,
not silently ignored security-relevant data.

### What the app accepts

The binary carries a *set* of accepted Ed25519 public keys (`AcceptedKeys`).
An artifact is accepted only when **both** hold:

1. SHA-256 of the downloaded bytes equals the manifest's `sha256`, and
2. the signature verifies (strict check) under at least one accepted key.

Concretely rejected, each with its own error: a tampered artifact
(`HashMismatch`), a valid signature from a key outside the set
(`NoAcceptedKey`), a correct hash with a wrong signature (`NoAcceptedKey` —
the hash does not vouch for the signature), and a missing signature (parse
error / `BadSignatureEncoding`).

## Rotation

A binary trusts the keys it was compiled with, forever — a key change cannot
reach installed copies retroactively. Rotation therefore rides the accepted
set:

1. **Release N:** add the new public key to `AcceptedKeys`. The new key signs
   nothing yet; every binary that can still update now knows it.
2. **Release N+1:** start signing with the new key. Old binaries accept the
   signature because they already carry the key; new binaries can drop the
   old key from the set once nothing still updating could be running it.
3. A copy old enough to predate the rotation never learns the new key and
   must reinstall by hand. That is inherent to compiled-in trust; the
   one-release-ahead rule keeps the window narrow instead of unbounded.

**Compromise is not rotation.** A signature made with a stolen key verifies
perfectly against the stolen key — adding a replacement key convinces nobody.
The remedy is: remove the stolen key from the accepted set, rebuild and
republish from a clean environment, and announce manually to installs that
cannot be reached. The Windows OV credential separation above exists so one
leak cannot take both signature chains at once.
