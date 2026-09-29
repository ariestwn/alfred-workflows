#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"
# TARGET selects a Rust target triple, e.g. x86_64-apple-darwin; unset builds for this Mac.
./scripts/cargo.sh build --release --locked ${TARGET:+--target "$TARGET"}
mkdir -p workflow dist
cp "target/${TARGET:+$TARGET/}release/alfred-kraely-upload" workflow/kraely-upload
chmod +x workflow/kraely-upload
if [ "$(uname -s)" = Darwin ]; then
    /usr/bin/codesign --force --sign - --identifier com.ariestwn.kraely-upload workflow/kraely-upload
fi
/usr/bin/python3 scripts/package.py
