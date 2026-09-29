#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"
# TARGET selects a Rust target triple, e.g. x86_64-apple-darwin; unset builds for this Mac.
./scripts/cargo.sh build --release --locked ${TARGET:+--target "$TARGET"}
mkdir -p workflow dist
cp "target/${TARGET:+$TARGET/}release/alfred-quickai" workflow/quickai
chmod +x workflow/quickai
if [ "$(uname -s)" = Darwin ]; then
    /usr/bin/codesign --force --sign - --identifier com.ariestwn.quickai workflow/quickai
fi
/usr/bin/python3 scripts/package.py

