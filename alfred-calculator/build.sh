#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"
# TARGET selects a Rust target triple, e.g. x86_64-apple-darwin; unset builds for this Mac.
./scripts/cargo.sh build --release --locked --offline ${TARGET:+--target "$TARGET"}
cp "target/${TARGET:+$TARGET/}release/alfred-calculator" workflow/calculator
chmod 755 workflow/calculator
/usr/bin/codesign --force --sign - --identifier com.ariestwn.calculator-rust workflow/calculator
/usr/bin/python3 -B scripts/licenses.py
if [ ! -f workflow/icon.png ]; then
    CLANG_MODULE_CACHE_PATH="${TMPDIR:-/tmp}/alfred-calculator-clang" /usr/bin/swift scripts/artwork.swift workflow/icon.png
fi
/usr/bin/python3 -B scripts/package.py
/usr/bin/python3 -B scripts/verify_workflow.py
