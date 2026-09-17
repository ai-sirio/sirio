#!/usr/bin/env bash
# Verifies every pinned recipe against its upstream. Goes red when a
# release is deleted or a version unpublished, which is the only way to
# learn it before a reader does. Not in ci.sh: it goes to the network.
#
# Parse the recipes out of config.rs rather than duplicating them: a second
# list is the drift this whole design exists to prevent. A recipe is only as
# good as the URL and the hash written beside it, and both are only true for
# as long as upstream says so:
#
#   Recipe::Release   the tag still exists, the asset is still published
#                     under the same name, and its bytes still hash to the
#                     pinned sha256 -- checked against the digest GitHub
#                     publishes for the asset, so nothing is downloaded
#                     unless upstream published no digest (taplo 0.10.0,
#                     whose whole release predates the field).
#   Recipe::Npm       `<package>@<version>` still exists on the registry.
#   Recipe::Manual    the page it sends the reader to still resolves. This
#                     is the weakest arm by construction: a documentation
#                     page that answers 200 says nothing about whether the
#                     instructions on it are still the ones to follow.
#
# What it does NOT check: that the hash matches the bytes for the 22 assets
# whose digest the API carries (GitHub's digest IS the asset's sha256, and
# the API is the same authority the recipes were generated from), and that
# a server advertised for a platform actually runs there. The second one is
# `test-lsp-install-e2e.sh`'s job, for one server on one platform.
#
# Skips rather than fails when this machine has no network, because a
# machine with no network is not a broken recipe -- the same reason the
# siblings in this directory skip where they cannot run. When npm is
# missing only the npm arm skips, loudly.
#
# Usage: Scripts/Tests/test-lsp-recipes.sh

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CONFIG_RS="$REPO_ROOT/rust/crates/sirio_lsp/src/config.rs"

# The table these must agree with, and the count config.rs's own
# `the_shipped_table_names_twenty_one_servers` test pins. If the extractor
# stops understanding the file, the script must not print OK having checked
# nothing -- that is the failure mode a regex parser is most likely to have.
EXPECTED_ENTRIES=21
EXPECTED_NPM=8
EXPECTED_MANUAL=6
EXPECTED_RELEASE_RECIPES=7
EXPECTED_RELEASE_ASSETS=26

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

for tool in awk curl jq; do
  command -v "$tool" >/dev/null 2>&1 \
    || fail "$tool is required by this test but is not on PATH"
done
[ -f "$CONFIG_RS" ] || fail "the recipe table is not where this test expects it: $CONFIG_RS"

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

# GitHub answers 60 requests an hour unauthenticated and 5000 with one. The
# release lookups below are cached per tag, so an unauthenticated run needs
# seven; a run that is iterated on, or one red-capability pass, burns through
# 60 quickly and then the failure is about the quota rather than the recipe.
# `gh` is what this repository already uses for that; a token in the
# environment wins over it.
TOKEN="${GITHUB_TOKEN:-${GH_TOKEN:-}}"
if [ -z "$TOKEN" ] && command -v gh >/dev/null 2>&1; then
  TOKEN="$(gh auth token 2>/dev/null || true)"
fi
curl_api() {
  if [ -n "$TOKEN" ]; then
    curl -sS -H "Authorization: Bearer $TOKEN" "$@"
  else
    curl -sS "$@"
  fi
}

# --- the network is a capability, not a recipe's fault ----------------------

if ! curl -fsS --max-time 20 -o /dev/null https://api.github.com/rate_limit; then
  echo "SKIP: LSP RECIPES — this machine cannot reach api.github.com, so no recipe can be checked"
  exit 0
fi

# --- extract, then refuse to go on if the extraction came up short ----------
#
# One TAB-separated line per recipe, in the order the file writes them:
#   NPM      package version bin
#   RELEASE  id version platform url sha256 bytes bin
#   MANUAL   needs url
# Stops at `#[cfg(test)]`: the tests below `defaults()` mention
# `crate::Recipe::Manual { .. }` in a pattern match, which is not a recipe.

awk '
  function quoted(s,   start, end) {
    start = index(s, "\"")
    if (start == 0) return ""
    end = index(substr(s, start + 1), "\"")
    if (end == 0) return ""
    return substr(s, start + 1, end - 1)
  }
  /^#\[cfg\(test\)\]/ { exit }
  /^[ \t]*(Some\()?Recipe::Npm \{/     { kind = "npm"; pkg = ""; ver = ""; bin_ = ""; next }
  /^[ \t]*(Some\()?Recipe::Release \{/ { kind = "release"; id = ""; ver = ""; in_asset = 0; next }
  /^[ \t]*(Some\()?Recipe::Manual \{/  { kind = "manual"; needs = ""; url = ""; next }

  kind == "npm" && /^[ \t]*package:/ { pkg = quoted($0); next }
  kind == "npm" && /^[ \t]*version:/ { ver = quoted($0); next }
  kind == "npm" && /^[ \t]*bin:/     { bin_ = quoted($0); next }
  kind == "npm" && /^[ \t]*\}\)/     { print "NPM\t" pkg "\t" ver "\t" bin_; kind = ""; next }

  kind == "release" && /^[ \t]*id:/      { id = quoted($0); next }
  kind == "release" && /^[ \t]*version:/ { ver = quoted($0); next }
  kind == "release" && /^[ \t]*Asset \{/ { in_asset = 1; u = ""; h = ""; b = ""; n = ""; next }
  in_asset && /^[ \t]*url:/    { u = quoted($0); next }
  in_asset && /^[ \t]*sha256:/ { h = quoted($0); next }
  in_asset && /^[ \t]*bytes:/  { b = $2; sub(/,.*/, "", b); next }
  in_asset && /^[ \t]*bin:/    { n = quoted($0); next }
  in_asset && /^[ \t]*\},/     { print "RELEASE\t" id "\t" ver "\t" plat "\t" u "\t" h "\t" b "\t" n; in_asset = 0; next }
  kind == "release" && !in_asset && /^[ \t]*"/ { plat = quoted($0); next }

  kind == "manual" && /^[ \t]*needs:/ { needs = quoted($0); next }
  kind == "manual" && /^[ \t]*url:/   { url = quoted($0); next }
  kind == "manual" && /^[ \t]*\}\)/   { print "MANUAL\t" needs "\t" url; kind = ""; next }
' "$CONFIG_RS" > "$WORK/recipes.tsv"

NPM_LINES=$(grep -c '^NPM' "$WORK/recipes.tsv" || true)
MANUAL_LINES=$(grep -c '^MANUAL' "$WORK/recipes.tsv" || true)
RELEASE_LINES=$(grep -c '^RELEASE' "$WORK/recipes.tsv" || true)
RELEASE_RECIPES=$(awk -F'\t' '$1 == "RELEASE" { print $2 }' "$WORK/recipes.tsv" | sort -u | wc -l)
ENTRIES=$((NPM_LINES + MANUAL_LINES + RELEASE_RECIPES))

if [ "$NPM_LINES" != "$EXPECTED_NPM" ] || [ "$MANUAL_LINES" != "$EXPECTED_MANUAL" ] \
  || [ "$RELEASE_LINES" != "$EXPECTED_RELEASE_ASSETS" ] || [ "$RELEASE_RECIPES" != "$EXPECTED_RELEASE_RECIPES" ] \
  || [ "$ENTRIES" != "$EXPECTED_ENTRIES" ]; then
  echo "----- what was extracted -----" >&2
  cut -f1,2,3 "$WORK/recipes.tsv" >&2
  echo "------------------------------" >&2
  fail "extracted $ENTRIES recipes from config.rs (npm $NPM_LINES, manual $MANUAL_LINES, release $RELEASE_LINES in $RELEASE_RECIPES rows); expected $EXPECTED_ENTRIES ($EXPECTED_NPM + $EXPECTED_MANUAL + $EXPECTED_RELEASE_RECIPES). The table moved, not the upstream -- teach the extractor its new shape rather than deleting this check"
fi

echo "extracted $ENTRIES recipes from config.rs: $NPM_LINES npm, $MANUAL_LINES manual, $RELEASE_RECIPES release in $RELEASE_LINES platform assets"

# --- Recipe::Release --------------------------------------------------------

# The release object for a tag, from cache: several assets of one recipe
# share a tag, and the rate limit is per request.
release_json() {
  local owner="$1" repo="$2" tag="$3"
  local stem
  stem=$(printf '%s-%s-%s' "$owner" "$repo" "$tag" | tr -c 'A-Za-z0-9._-' '_')
  local cache="$WORK/$stem.json"
  if [ ! -f "$cache" ]; then
    local code
    code=$(curl_api -o "$cache" -w '%{http_code}' \
      "https://api.github.com/repos/$owner/$repo/releases/tags/$tag" || true)
    if [ "$code" != 200 ]; then
      grep -o '"message":"[^"]*"' "$cache" 2>/dev/null | head -1 || true
      fail "GitHub has no release $owner/$repo tagged $tag (HTTP $code); the recipe's URL is dead${TOKEN:+}"
    fi
  fi
  printf '%s' "$cache"
}

while IFS=$'\t' read -r _ id version platform url sha256 bytes bin; do
  # https://github.com/<owner>/<repo>/releases/download/<tag>/<asset>
  rest="${url#https://github.com/}"
  [ "$rest" != "$url" ] || fail "$id on $platform is not a github.com release URL: $url"
  owner="${rest%%/*}"; rest="${rest#*/}"
  repo="${rest%%/*}"; rest="${rest#*/}"
  case "$rest" in
    releases/download/*) ;;
    *) fail "$id on $platform does not point at a release download: $url" ;;
  esac
  rest="${rest#releases/download/}"
  tag="${rest%%/*}"
  asset="${rest##*/}"

  json=$(release_json "$owner" "$repo" "$tag")

  published=$(jq -r --arg name "$asset" '.assets[] | select(.name == $name) | .size' "$json" | head -1)
  [ -n "$published" ] || fail "$owner/$repo tag $tag no longer publishes an asset named '$asset' ($id on $platform)"

  # The declared size is free -- it rides in the same response -- and it
  # catches a re-upload even where the digest is checked below.
  [ "$published" = "$bytes" ] \
    || fail "$id on $platform declares $bytes bytes but upstream's '$asset' is $published"

  digest=$(jq -r --arg name "$asset" '.assets[] | select(.name == $name) | .digest // empty' "$json" | head -1)
  if [ -n "$digest" ]; then
    upstream="${digest#sha256:}"
    [ "$(printf '%s' "$upstream" | tr 'A-F' 'a-f')" = "$(printf '%s' "$sha256" | tr 'A-F' 'a-f')" ] \
      || fail "$id on $platform pins sha256 $sha256 but upstream's '$asset' is $upstream"
    echo "OK: release $id $version on $platform — $asset, $bytes bytes, digest matches"
  else
    # No digest published: the only way to the hash is the bytes.
    echo "DOWNLOADED: $asset ($bytes bytes) — upstream publishes no digest for $id $version"
    curl -fsSL --max-time 600 -o "$WORK/asset" "$url" \
      || fail "$id on $platform could not be downloaded: $url"
    got_bytes=$(wc -c < "$WORK/asset")
    [ "$got_bytes" = "$bytes" ] \
      || fail "$id on $platform downloaded $got_bytes bytes, not the declared $bytes"
    got_sha=$(sha256sum "$WORK/asset" | cut -d' ' -f1)
    [ "$got_sha" = "$sha256" ] \
      || fail "$id on $platform pins sha256 $sha256 but $asset hashes to $got_sha"
    echo "OK: release $id $version on $platform — $asset, $bytes bytes, hash matches the downloaded bytes"
  fi
done < <(grep '^RELEASE' "$WORK/recipes.tsv")

# --- Recipe::Npm ------------------------------------------------------------

if command -v npm >/dev/null 2>&1; then
  while IFS=$'\t' read -r _ package version bin; do
    published=$(npm view "$package@$version" version 2>/dev/null | tr -d '\r' | tail -1)
    [ "$published" = "$version" ] \
      || fail "npm no longer serves $package@$version (asked for $version for '$bin', got '${published:-nothing}')"
    echo "OK: npm $package@$version — $bin"
  done < <(grep '^NPM' "$WORK/recipes.tsv")
else
  echo "SKIP: the 8 npm recipes — npm is not on PATH, so no pinned package version was checked"
fi

# --- Recipe::Manual ---------------------------------------------------------

while IFS=$'\t' read -r _ needs url; do
  code=$(curl -sSL --max-time 30 -o /dev/null -w '%{http_code}' "$url" || true)
  case "$code" in
    2*) ;;
    *) fail "$url ($needs) no longer resolves (HTTP ${code:-none})" ;;
  esac
  echo "OK: manual page $url ($needs)"
done < <(grep '^MANUAL' "$WORK/recipes.tsv")

echo "LSP RECIPES OK"
