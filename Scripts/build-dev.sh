#!/bin/bash
# Dev build+launch loop: xcodegen generate -> xcodebuild Debug -> relaunch.
set -euo pipefail
cd "$(dirname "$0")/.."

xcodegen generate

# ENABLE_DEBUG_DYLIB=NO avoids a stale-DerivedData codesign failure seen
# locally when rebuilding on top of a prior debug-dylib build.
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -derivedDataPath DerivedData ENABLE_DEBUG_DYLIB=NO build | tail -5

APP_PATH="DerivedData/Build/Products/Debug/Tiller.app"
pkill -x Tiller || true
open "$APP_PATH"
