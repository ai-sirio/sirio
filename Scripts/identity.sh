# Sirio's packaging identity — the single source every packaging file reads
# instead of retyping the facts (#304, spec §8.4 of
# docs/superpowers/specs/2026-08-29-auto-update-design.md).
#
# Consumers: Scripts/build-app-bundle.sh (generates Info.plist),
# Scripts/build-dev.sh (dev bundle), and later the Linux .desktop entry and
# the Windows Inno .iss (#309, #310). The format is a contract:
#
#   - one `KEY="value"` line per fact, nothing else but comments and blanks;
#   - safe to `source` from bash, trivial to parse from any other language
#     (read line by line, split on the first `=`, strip the quotes);
#   - path values are relative to the repository root.
#
# SIRIO_INNO_APP_ID must never change: Inno decides upgrade-in-place versus
# side-by-side by comparing it, and the update path re-runs the installer
# expecting an in-place upgrade (spec §8.2). The identifier is shared by
# release and dev builds on every OS (spec §8.1).

SIRIO_APP_IDENTIFIER="app.sirioai.sirio"
SIRIO_DISPLAY_NAME="Sirio"
SIRIO_ICON_PATH="rust/assets/app-icon/icon.icns"
SIRIO_INNO_APP_ID="581478B4-5A82-4E6D-9886-E3470E3A663C"
