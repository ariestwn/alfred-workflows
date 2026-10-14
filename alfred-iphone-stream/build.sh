#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"
case "${TARGET:-$(uname -m)}" in
    aarch64-apple-darwin|arm64) swift_target=arm64-apple-macos14 ;;
    x86_64-apple-darwin|x86_64) swift_target=x86_64-apple-macos14 ;;
    *) echo "Unsupported macOS target: $TARGET" >&2; exit 1 ;;
esac
./scripts/cargo.sh build --release --locked ${TARGET:+--target "$TARGET"}
cp "target/${TARGET:+$TARGET/}release/alfred-iphone-stream" workflow/iphone-stream
helper="workflow/iPhone Stream Helper.app/Contents"
mkdir -p "$helper/MacOS" target/swift-module-cache
export CLANG_MODULE_CACHE_PATH="$PWD/target/swift-module-cache"
export SWIFT_MODULECACHE_PATH="$CLANG_MODULE_CACHE_PATH"
/usr/bin/python3 -B scripts/package.py --metadata-only
/usr/bin/swiftc -O -module-cache-path "$SWIFT_MODULECACHE_PATH" -target "$swift_target" -framework AVFoundation -framework Foundation -framework CoreMedia \
    -Xlinker -sectcreate -Xlinker __TEXT -Xlinker __info_plist -Xlinker "$helper/Info.plist" \
    -o "$helper/MacOS/iphone-stream-daemon" swift-src/iphone-stream-daemon.swift
/usr/bin/swiftc -O -module-cache-path "$SWIFT_MODULECACHE_PATH" -target "$swift_target" -framework Foundation -o workflow/idam-helper swift-src/idam-helper.swift
chmod +x workflow/iphone-stream workflow/idam-helper workflow/run-action "$helper/MacOS/iphone-stream-daemon"
/usr/bin/codesign --force --sign - --identifier com.ariestwn.iphone-stream workflow/iphone-stream
/usr/bin/codesign --force --sign - --identifier com.ariestwn.iphone-stream.idam workflow/idam-helper
/usr/bin/codesign --force --sign - "workflow/iPhone Stream Helper.app"
/usr/bin/python3 -B scripts/package.py
/usr/bin/python3 -B scripts/verify_workflow.py
