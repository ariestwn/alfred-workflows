#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"
# TARGET selects a Rust target triple, e.g. x86_64-apple-darwin; unset builds for this Mac.
./scripts/cargo.sh build --release --locked ${TARGET:+--target "$TARGET"}
cp "target/${TARGET:+$TARGET/}release/alfred-screenshots-rust" workflow/shots
chmod +x workflow/shots workflow/grid
/usr/bin/codesign --force --sign - --identifier com.ariestwn.screenshots-rust workflow/shots
/usr/bin/python3 -B scripts/package.py
/usr/bin/python3 -B scripts/verify_workflow.py
