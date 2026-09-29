#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"
export MACOSX_DEPLOYMENT_TARGET=12.0
mkdir -p workflow
# TARGET selects a Rust target triple, e.g. x86_64-apple-darwin; unset builds for this Mac.
./scripts/cargo.sh build --release --locked --offline ${TARGET:+--target "$TARGET"}
cp "target/${TARGET:+$TARGET/}release/alfred-uninstaller" workflow/uninstaller
chmod 755 workflow/uninstaller
/usr/bin/codesign --force --sign - --identifier com.ariestwn.uninstaller-rust workflow/uninstaller
/usr/bin/python3 -B scripts/artwork.py
/usr/bin/python3 -B scripts/licenses.py
/usr/bin/python3 -B scripts/package.py
/usr/bin/python3 -B scripts/verify_workflow.py
