#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"
# TARGET selects a Rust target triple, e.g. x86_64-apple-darwin; unset builds for this Mac.
./scripts/cargo.sh build --release --locked --offline ${TARGET:+--target "$TARGET"}
cp "target/${TARGET:+$TARGET/}release/alfred-handy" workflow/handy
chmod 755 workflow/handy
/usr/bin/codesign --force --sign - --identifier com.ariestwn.handy-rust workflow/handy
/usr/bin/python3 -B scripts/package.py
/usr/bin/python3 -B scripts/verify_workflow.py
