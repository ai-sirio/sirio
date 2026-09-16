#!/usr/bin/env bash
# Prints the pinned release recipes as Rust source, for pasting into
# sirio_lsp/src/config.rs.
#
# The recipes are compiled in on purpose (see the design's §7): no call to
# api.github.com at install time, every binary install verified, and the
# download size known before the button says Install. The price is that
# refreshing a server takes a Sirio release, and this script is how that
# refresh is done rather than retyped.
#
# Needs `gh` authenticated. Reads no file and writes only to stdout; it
# downloads an asset only to hash it when the API published no digest.
set -euo pipefail

# server:repo:asset-regex-per-platform
# Platform keys are sirio_registry::current_platform_key()'s spelling.

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum | cut -d' ' -f1
  else shasum -a 256 | cut -d' ' -f1; fi
}

emit() {
  local name="$1" repo="$2" bin="$3"; shift 3
  local release version
  release="$(gh api "repos/$repo/releases/latest")"
  version="$(jq -r .tag_name <<<"$release")"
  printf '                // %s %s\n' "$name" "$version"
  printf '                Recipe::Release {\n'
  printf '                    id: "%s",\n                    version: "%s",\n' "$name" "$version"
  printf '                    bin: "%s",\n                    assets: &[\n' "${bin//VERSION/$version}"
  while (( $# )); do
    local key="$1" pattern="$2"; shift 2
    local asset
    asset="$(jq -r --arg p "$pattern" '.assets[] | select(.name|test($p)) | @json' <<<"$release" | head -1)"
    [[ -z "$asset" ]] && { printf '                        // %s: no asset matching %s\n' "$key" "$pattern"; continue; }
    local digest
    digest="$(jq -r '.digest // empty | sub("^sha256:";"")' <<<"$asset")"
    # Some releases predate GitHub's digest field and report null for every
    # asset (taplo 0.10.0 is the one here). Hash the bytes rather than write
    # a recipe with no hash in it.
    if [[ -z "$digest" ]]; then
      digest="$(gh api "repos/$repo/releases/assets/$(jq -r .id <<<"$asset")" \
        -H 'Accept: application/octet-stream' | sha256)"
    fi
    printf '                        ("%s", Asset { url: "%s", sha256: "%s", bytes: %s }),\n' \
      "$key" \
      "$(jq -r .browser_download_url <<<"$asset")" \
      "$digest" \
      "$(jq -r .size <<<"$asset")"
  done
  printf '                    ],\n                },\n'
}

emit clangd clangd/clangd "clangd_VERSION/bin/clangd" \
  linux-x86_64 'clangd-linux-.*\.zip' \
  darwin-aarch64 'clangd-mac-.*\.zip' \
  windows-x86_64 'clangd-windows-.*\.zip'

emit rust-analyzer rust-lang/rust-analyzer "rust-analyzer" \
  linux-x86_64 'rust-analyzer-x86_64-unknown-linux-gnu\.gz$' \
  linux-aarch64 'rust-analyzer-aarch64-unknown-linux-gnu\.gz$' \
  darwin-aarch64 'rust-analyzer-aarch64-apple-darwin\.gz$' \
  darwin-x86_64 'rust-analyzer-x86_64-apple-darwin\.gz$' \
  windows-x86_64 'rust-analyzer-x86_64-pc-windows-msvc\.zip$'

emit lua-language-server LuaLS/lua-language-server "bin/lua-language-server" \
  linux-x86_64 'linux-x64\.tar\.gz$' \
  linux-aarch64 'linux-arm64\.tar\.gz$' \
  darwin-aarch64 'darwin-arm64\.tar\.gz$' \
  windows-x86_64 'win32-x64\.zip$'

emit taplo tamasfe/taplo "taplo" \
  linux-x86_64 'taplo-linux-x86_64\.gz$' \
  linux-aarch64 'taplo-linux-aarch64\.gz$' \
  darwin-aarch64 'taplo-darwin-aarch64\.gz$' \
  windows-x86_64 'taplo-windows-x86_64\.zip$'

emit marksman artempyanykh/marksman "marksman" \
  linux-x86_64 'marksman-linux-x64$' \
  linux-aarch64 'marksman-linux-arm64$' \
  darwin-aarch64 'marksman-macos$' \
  windows-x86_64 'marksman\.exe$'

emit zls zigtools/zls "zls" \
  linux-x86_64 'zls-x86_64-linux\.tar\.xz$' \
  darwin-aarch64 'zls-aarch64-macos\.tar\.xz$' \
  windows-x86_64 'zls-x86_64-windows\.zip$'

emit lemminx redhat-developer/vscode-xml "lemminx" \
  linux-x86_64 'lemminx-linux-x86_64\.zip$' \
  darwin-aarch64 'lemminx-osx-aarch_64\.zip$' \
  windows-x86_64 'lemminx-win32\.zip$'

emit sqls sqls-server/sqls "sqls" \
  linux-x86_64 'sqls-linux-.*\.zip$' \
  darwin-aarch64 'sqls-darwin-.*\.zip$' \
  windows-x86_64 'sqls-windows-.*\.zip$'
