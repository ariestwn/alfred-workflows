#!/bin/bash
# Build an Apple Silicon and an Intel archive of every workflow into release/.
set -euo pipefail
cd "$(dirname "$0")/.."
# Each build.sh copies its binary out of the project's own ./target directory.
unset CARGO_TARGET_DIR
# Keep this machine's paths out of panic messages in the binaries. The last matching prefix wins.
export CARGO_ENCODED_RUSTFLAGS="--remap-path-prefix=$HOME=~"$'\x1f'"--remap-path-prefix=$PWD=alfred-workflows"
out="$PWD/release"
rm -rf "$out"
mkdir -p "$out"

# slug|project folder|archive name written to <folder>/dist/
rust_workflows=(
    "calculator|alfred-calculator|Calculator Rust"
    "emoji|alfred-emoji|Emoji"
    "handy|alfred-handy|Handy Rust"
    "kraely-upload|alfred-kraely-upload|Upload to Remote"
    "my-schedule|alfred-my-schedule|My Schedule"
    "quickai|alfred-quickai|QuickAI"
    "screenshots-rust|alfred-screenshots-rust|Screenshots Rust"
    "uninstaller|alfred-uninstaller|Uninstaller Rust"
)

version() { plutil -extract version raw "$1/workflow/info.plist"; }

# arm64 goes last so each project's workflow/ folder ends up with native binaries.
for arch in x86_64 arm64; do
    case "$arch" in
        x86_64) triple=x86_64-apple-darwin label=intel ;;
        arm64) triple=aarch64-apple-darwin label=apple-silicon ;;
    esac
    for entry in "${rust_workflows[@]}"; do
        IFS='|' read -r slug dir name <<<"$entry"
        echo "==> $slug ($label)"
        # Several build.sh scripts build --offline; this downloads anything missing.
        "./$dir/scripts/cargo.sh" fetch --locked --quiet
        TARGET="$triple" "./$dir/build.sh"
        cp "$dir/dist/$name.alfredworkflow" "$out/$slug-$(version "$dir")-$label.alfredworkflow"
    done
    echo "==> music ($label)"
    ARCHS="$arch" ./alfred-music/build.sh
    cp "alfred-music/dist/Apple Music + Audio Format.alfredworkflow" \
        "$out/music-$(version alfred-music)-$label.alfredworkflow"
done

echo "==> ai-chat (no native code)"
/usr/bin/python3 -B alfred-ai-chat/scripts/package.py
/usr/bin/python3 -B alfred-ai-chat/scripts/verify_workflow.py
cp "alfred-ai-chat/dist/AI Chat.alfredworkflow" "$out/ai-chat-$(version alfred-ai-chat).alfredworkflow"

/usr/bin/python3 -B scripts/check_release.py "$out"
