#!/usr/bin/env bash
# The live end-to-end test of installing a language server: one pinned
# recipe out of `config.rs`, the real `sirio_registry` installer, a real
# download into a temporary store, and the real server child answering a
# real definition request.
#
# Why this exists on top of the unit tests. Every stage below is covered by
# a unit test with something injected — a fake npm, a hostile in-memory
# archive, a stubbed fetch — which is the right shape for the installer and
# leaves the live chain untested: a pinned URL that 404s, a hash that no
# longer matches the bytes, an archive the unpacker cannot handle, a
# `bin` that is not the file the package publishes. None of those can be
# seen from inside the process, and all of them arrive at the reader as the
# same empty context menu that 0.18.0 shipped: a Java file reading "No
# language server offers definitions here" on a machine whose only problem
# was a missing `jdtls`. `lsp_probe` exists to tell "there is no server"
# from "the server said no"; this script is what gives it a server to ask.
#
# What it proves, in order, with one 22 MB download:
#
#   1. With the store empty and nothing to launch, the probe says the
#      command is not installed and that there is nothing to ask — the
#      distinct answer, not silence. (Skipped where `marksman` is already
#      on PATH: that machine cannot show the negative.)
#   2. The recipe's fields, read out of `config.rs`, install through the
#      real installer into a temporary store, and the manifest it writes
#      names an executable that exists, is executable, and is the size the
#      recipe declared.
#   3. That executable, launched by the name the shipped table gives, answers
#      a `textDocument/definition` on a Markdown link with the target file —
#      one target, naming `b.md` in the fixture.
#   4. The temporary store is removed and nothing is left behind.
#
# It touches no installed copy of Sirio: the store is a temp directory, and
# the probe is given nothing else to read. Marksman is the smallest of the
# pinned servers (a bare executable, ~22 MB, against clangd's 114 MB), which
# is the only reason the download is affordable.
#
# Not in ci.sh, deliberately, for the same reason test-lsp-recipes.sh is
# not: it goes to the network.
#
# Usage: Scripts/Tests/test-lsp-install-e2e.sh
#   SIRIO_LSP_E2E_VERBOSE=1   print the probe's output on a passing run too

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CARGO_DIR="$REPO_ROOT/rust"
CONFIG_RS="$CARGO_DIR/crates/sirio_lsp/src/config.rs"

# `sirio_registry::current_platform_key()`'s spelling. No CLI prints it, so
# it is derived here; a mismatch shows up as the installer refusing an agent
# with no artifact rather than as a silent pass.
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*)
    echo "SKIP: LSP INSTALL E2E — this test launches the installed server by name on PATH, which an extensionless Windows artifact cannot match; the app launches installed servers by absolute path and is not covered by this script"
    exit 0
    ;;
  Darwin)
    case "$(uname -m)" in
      arm64|aarch64) PLATFORM=darwin-aarch64 ;;
      *)             PLATFORM=darwin-x86_64 ;;
    esac
    ;;
  Linux)
    case "$(uname -m)" in
      x86_64)        PLATFORM=linux-x86_64 ;;
      aarch64|arm64) PLATFORM=linux-aarch64 ;;
      *)             PLATFORM=unsupported ;;
    esac
    ;;
  *) PLATFORM=unsupported ;;
esac

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

for tool in cargo curl awk; do
  command -v "$tool" >/dev/null 2>&1 \
    || fail "$tool is required by this test but is not on PATH"
done
[ -f "$CONFIG_RS" ] || fail "the recipe table is not where this test expects it: $CONFIG_RS"

# --- the recipe, read out of config.rs --------------------------------------
#
# Only the four fields of one asset are needed here, so this is not
# test-lsp-recipes.sh's full parser on purpose — but it is the same file and
# the same drift argument: a copy of the URL and the hash in this script
# would be a second list to forget. `Recipe::Release { id: "marksman"`
# through the closing `],` of its `assets` array is exactly one recipe.
marksman_recipe() {
  sed -n '/^[ \t]*id: "marksman",[ \t]*$/,/^[ \t]*\],[ \t]*$/p' "$CONFIG_RS"
}

RECIPE="$(marksman_recipe)"
[ -n "$RECIPE" ] || fail "no 'Recipe::Release { id: \"marksman\"' in $CONFIG_RS; this test cannot run blind"
# The recipe's own version, not a copy of it: it is what the store records.
VERSION=$(sed -n 's/^[ \t]*version: "\(.*\)",$/\1/p' <<<"$RECIPE" | head -1)
[ -n "$VERSION" ] || fail "the marksman recipe parsed without a version"

# url, sha256, bytes, bin for this platform's asset, or nothing where the
# project publishes none (marksman has no linux-aarch64... it has one today,
# so an absent asset here is a real answer, not a parse failure).
ASSET=$(awk -v platform="$PLATFORM" '
  function quoted(s,   start, end) {
    start = index(s, "\"")
    if (start == 0) return ""
    end = index(substr(s, start + 1), "\"")
    if (end == 0) return ""
    return substr(s, start + 1, end - 1)
  }
  /^[ \t]*"[^"]*",[ \t]*$/ { current = quoted($0); next }
  /Asset \{/ { wanted = (current == platform); next }
  wanted && /^[ \t]*url:/    { url = quoted($0); next }
  wanted && /^[ \t]*sha256:/ { sha = quoted($0); next }
  wanted && /^[ \t]*bytes:/  { b = $2; sub(/,.*/, "", b); next }
  wanted && /^[ \t]*bin:/    { bin = quoted($0); next }
  wanted && /^[ \t]*\},/     { print url "\t" sha "\t" b "\t" bin; exit }
' <<<"$RECIPE")

if [ -z "$ASSET" ]; then
  echo "SKIP: LSP INSTALL E2E — the marksman recipe pins no asset for $PLATFORM"
  exit 0
fi
IFS=$'\t' read -r URL SHA256 BYTES BIN <<<"$ASSET"
[ -n "$URL" ] && [ -n "$SHA256" ] && [ -n "$BYTES" ] && [ -n "$BIN" ] \
  || fail "the marksman recipe for $PLATFORM parsed to an incomplete asset: url='$URL' sha256='$SHA256' bytes='$BYTES' bin='$BIN'"
echo "recipe: marksman $VERSION for $PLATFORM — $BIN, $BYTES bytes, sha256 $SHA256"

# --- the network is a capability, not a recipe's fault ----------------------

if ! curl -fsS --max-time 20 -o /dev/null https://api.github.com/rate_limit; then
  echo "SKIP: LSP INSTALL E2E — this machine cannot reach api.github.com, so nothing can be installed"
  exit 0
fi
# Reachability is one thing; a dead recipe is another, and this is where the
# difference is made: a 404 here is the recipe's fault and fails loudly.
CODE=$(curl -sS -L --max-time 60 -o /dev/null -w '%{http_code}' "$URL" || true)
[ "$CODE" = 200 ] || fail "the pinned marksman asset no longer resolves: $URL (HTTP ${CODE:-none})"

WORK=$(mktemp -d)
STORE="$WORK/language-servers"
FIXTURE="$WORK/project"
CONFIG_DIR="$WORK/config"   # an empty config dir: the shipped table, nothing else
mkdir -p "$WORK/nothing" "$FIXTURE" "$CONFIG_DIR"

cleanup() {
  rm -rf "$WORK"
}
trap cleanup EXIT

# --- the fixture ------------------------------------------------------------
#
# A definition in Markdown is a link target, so the assertion has to be about
# a link. `b.md` exists and is not empty: a link to a file that is not there
# is how marksman says "no target", which is the negative this test must not
# confuse with success.
#
# `.marksman.toml` is what puts marksman in multi-file mode -- without it,
# cross-file definitions and references do not work at all, which its README
# states as a FAQ answer and is true of the shipped recipe too: `roots` in the
# markdown entry names this file, and the app roots the server at the
# directory holding it. It is empty because its existence is the whole point.
: > "$WORK/project/.marksman.toml"
cat > "$FIXTURE/a.md" <<'MARKDOWN'
# A

See [B](b.md).
MARKDOWN
cat > "$FIXTURE/b.md" <<'MARKDOWN'
# B

The target of the link in `a.md`.
MARKDOWN

# `line` and `character` are the protocol's: zero-based, UTF-16 units. This
# is the `b.md` inside the link on the third line (`See [B](b.md).`).
LINE=2
CHARACTER=9

# Marksman scans the workspace on a background thread, so a definition asked
# for in the first milliseconds of a process can legitimately come back empty
# on a loaded machine -- the link is fine, the index is not built yet. The
# assertion is that the installed server answers, not that it answers inside
# one scheduling window, so the probe is run again until it does. Each run is
# a fresh process; where it never answers, the output of the last one is what
# the failure prints.
DEFINITION_ATTEMPTS=20

echo "building the probes"
(cd "$CARGO_DIR" && cargo build --quiet -p sirio_registry --example install_probe)
(cd "$CARGO_DIR" && cargo build --quiet -p sirio_lsp --example lsp_probe)
INSTALL_PROBE="$CARGO_DIR/target/debug/examples/install_probe"
LSP_PROBE="$CARGO_DIR/target/debug/examples/lsp_probe"
[ -x "$INSTALL_PROBE" ] || fail "the install probe was not built at $INSTALL_PROBE"
[ -x "$LSP_PROBE" ] || fail "the lsp probe was not built at $LSP_PROBE"

# --- running the language-server probe --------------------------------------

PROBE_OUT=""
PROBE_CODE=0
run_lsp_probe() {
  # $1 is the directory to put in front of PATH, so the shipped table's
  # `marksman` resolves to the installed one. This is deliberately not how
  # the app launches an installed server: step 1 of its ladder spawns the
  # table's `command` by name, and only when that answers NotInstalled does
  # step 2 launch the store's executable by absolute path (a name on PATH
  # would let two installed servers see each other). Launching by name here
  # is what puts the *shipped table's* command and arguments in the test,
  # which is the pair a reader's install depends on; the manifest path is
  # asserted in case 2 (install_probe re-reads it and refuses a missing
  # file). The rest of PATH is kept, because the probe itself is a Rust
  # binary.
  local front="$1"
  set +e
  PROBE_OUT=$(env \
    "PATH=$front:$(dirname "$LSP_PROBE"):$PATH" \
    "SIRIO_CONFIG_DIR=$CONFIG_DIR" \
    "$LSP_PROBE" "$FIXTURE" "$FIXTURE/a.md" "$LINE" "$CHARACTER" 2>&1)
  PROBE_CODE=$?
  set -e
}

dump_probe() {
  echo "----- lsp_probe output -----" >&2
  echo "$PROBE_OUT" >&2
  echo "----------------------------" >&2
}

show_probe() {
  if [ "${SIRIO_LSP_E2E_VERBOSE:-}" = 1 ]; then
    echo "$PROBE_OUT" | sed 's/^/    /'
  fi
}

expect_prefix() {
  if ! grep -qF -- "$1" <<<"$PROBE_OUT"; then
    dump_probe
    fail "expected a probe line containing '$1'"
  fi
}

# --- 1. nothing installed: the answer is "no server", said out loud ---------

if command -v marksman >/dev/null 2>&1; then
  echo "SKIP: the empty-store half — marksman is already on PATH at $(command -v marksman)"
else
  echo "case 1: with the store empty the probe reports no server rather than nothing"
  run_lsp_probe "$WORK/nothing"
  show_probe
  [ "$PROBE_CODE" = 0 ] || { dump_probe; fail "the probe exited $PROBE_CODE with no server to launch"; }
  expect_prefix "launch                \`marksman\` is not installed"
  expect_prefix "definition_available  false (nothing to ask)"
fi

# --- 2. a real install into a temp store -----------------------------------

echo "case 2: the recipe installs through the real installer"
set +e
INSTALL_OUT=$("$INSTALL_PROBE" "$STORE" "$PLATFORM" marksman "$VERSION" "$URL" "$SHA256" "$BIN" 2>&1)
INSTALL_CODE=$?
set -e
echo "$INSTALL_OUT" | sed 's/^/    /'
[ "$INSTALL_CODE" = 0 ] || fail "the installer exited $INSTALL_CODE on the pinned marksman recipe"

EXECUTABLE=$(sed -n 's/^EXECUTABLE //p' <<<"$INSTALL_OUT")
[ -n "$EXECUTABLE" ] || fail "the installer printed no EXECUTABLE line (output above)"
case "$EXECUTABLE" in
  "$STORE"/*) ;;
  *) fail "the installer put the executable outside the store it was given: $EXECUTABLE" ;;
esac
[ -f "$EXECUTABLE" ] || fail "the store names $EXECUTABLE but there is no file there"
[ -x "$EXECUTABLE" ] || fail "$EXECUTABLE was installed without the executable bit"
[ "$(basename "$EXECUTABLE")" = "$BIN" ] \
  || fail "the recipe names '$BIN' but the store records '$(basename "$EXECUTABLE")'"

# The recipe declares the size it is asking the reader to download; a bare
# executable is the bytes that landed, so these are the same number or the
# install did not fetch what the recipe pins.
INSTALLED_BYTES=$(wc -c < "$EXECUTABLE")
[ "$INSTALLED_BYTES" = "$BYTES" ] \
  || fail "the installed $BIN is $INSTALLED_BYTES bytes; the recipe pins $BYTES"

# --- 3. launched by the shipped table's command, it answers ------------------

echo "case 3: the installed server answers a definition request"
# A recipe whose `bin` is not the name the table's `command` uses would not
# be found here (lemminx's is platform-named, `lemminx-linux-x86_64`), and
# the app would still launch it by absolute path. For marksman the two names
# are the same, so a copy of the recipe that stops saying so is caught here.
DIR="$(dirname "$EXECUTABLE")"
DEFINITION=""
for _ in $(seq 1 "$DEFINITION_ATTEMPTS"); do
  run_lsp_probe "$DIR"
  show_probe
  [ "$PROBE_CODE" = 0 ] || { dump_probe; fail "the probe exited $PROBE_CODE against the installed server"; }
  expect_prefix "launch                OK"
  expect_prefix "definition_available  true"
  expect_prefix "command               marksman"
  if grep -Eq 'definition +[1-9][0-9]* target\(s\)' <<<"$PROBE_OUT"; then
    DEFINITION=$(grep -F 'definition            ' <<<"$PROBE_OUT" | tail -1)
    break
  fi
  sleep 1
done
[ -n "$DEFINITION" ] || {
  dump_probe
  fail "the installed marksman never answered the definition request in $DEFINITION_ATTEMPTS probe runs"
}
grep -qF "$FIXTURE/b.md" <<<"$DEFINITION" \
  || { dump_probe; fail "the definition target is not the fixture's b.md: $DEFINITION"; }

# --- 4. the store is a temp directory and goes away -------------------------

echo "case 4: the temporary store is removed"
rm -rf "$STORE"
[ ! -e "$STORE" ] || fail "the store at $STORE survived the run"

echo "LSP INSTALL E2E OK"
