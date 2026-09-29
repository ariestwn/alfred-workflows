#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"
if [ ! -f data/emoji.tsv ]; then /usr/bin/python3 -B scripts/prepare_data.py; fi
if [ ! -d workflow/icons ]; then swift scripts/artwork.swift "$PWD"; fi
# TARGET selects a Rust target triple, e.g. x86_64-apple-darwin; unset builds for this Mac.
./scripts/cargo.sh build --release --locked ${TARGET:+--target "$TARGET"}
cp "target/${TARGET:+$TARGET/}release/alfred-emoji" workflow/emoji
chmod +x workflow/emoji workflow/grid
/usr/bin/codesign --force --sign - --identifier com.ariestwn.emoji workflow/emoji
/usr/bin/python3 -B scripts/package.py
