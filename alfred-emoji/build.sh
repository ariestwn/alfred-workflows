#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"
if [ ! -f data/emoji.tsv ]; then /usr/bin/python3 -B scripts/prepare_data.py; fi
if [ ! -d workflow/icons ]; then swift scripts/artwork.swift "$PWD"; fi
BGE=target/bge
if [ ! -f "$BGE/model.safetensors" ]; then
    mkdir -p "$BGE"
    for f in config.json vocab.txt model.safetensors; do
        curl -fL --progress-bar -o "$BGE/$f" "https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/main/$f"
    done
fi
# TARGET selects a Rust target triple, e.g. x86_64-apple-darwin; unset builds for this Mac.
./scripts/cargo.sh build --release --locked ${TARGET:+--target "$TARGET"}
cp "target/${TARGET:+$TARGET/}release/alfred-emoji" workflow/emoji
chmod +x workflow/emoji workflow/grid
/usr/bin/codesign --force --sign - --identifier com.ariestwn.emoji workflow/emoji
if [ ! -f workflow/model/emoji.bin ] || [ data/emoji.tsv -nt workflow/model/emoji.bin ] \
    || [ data/descriptions.tsv -nt workflow/model/emoji.bin ]; then
    workflow/emoji --build-model "$BGE" workflow/model
    chmod 644 workflow/model/*
fi
/usr/bin/python3 -B scripts/package.py
