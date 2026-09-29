#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"
# TARGET selects a Rust target triple, e.g. x86_64-apple-darwin; unset builds for this Mac.
./scripts/cargo.sh build --release --locked --offline ${TARGET:+--target "$TARGET"}
app="workflow/My Schedule.app"
mkdir -p "$app/Contents/MacOS"
cp "target/${TARGET:+$TARGET/}release/alfred-my-schedule" "$app/Contents/MacOS/my-schedule"
cp native/Info.plist "$app/Contents/Info.plist"
chmod 755 "$app/Contents/MacOS/my-schedule"
/usr/bin/codesign --force --sign - --identifier com.ariestwn.my-schedule-rust.helper "$app"
CLANG_MODULE_CACHE_PATH="${TMPDIR:-/tmp}/alfred-schedule-clang" /usr/bin/swift scripts/artwork.swift workflow
/usr/bin/python3 -B scripts/licenses.py
/usr/bin/python3 -B scripts/package.py
/usr/bin/python3 -B scripts/verify_workflow.py
