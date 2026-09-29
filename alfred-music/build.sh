#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

OUT="dist/Apple Music + Audio Format.alfredworkflow"
APP="workflow/Apple Music Audio Watcher.app"

# ARCHS picks the slices: "arm64 x86_64" (default, universal), "arm64", or
# "x86_64". Minimum target macOS 13 — required by Swift's regex literal
# syntax (#/.../#) used in app.swift.
ARCHS="${ARCHS:-arm64 x86_64}"
build_universal() {
    local src="$1" out="$2" extra="${3:-}" slices=()
    for arch in $ARCHS; do
        swiftc -target "$arch-apple-macos13" -O -o "${out}.$arch" "$src" $extra
        slices+=("${out}.$arch")
    done
    if [ "${#slices[@]}" -eq 1 ]; then
        mv "${slices[0]}" "$out"
    else
        lipo -create "${slices[@]}" -output "$out"
        rm -f "${slices[@]}"
    fi
}

mkdir -p workflow dist

echo "→ compiling Apple Music Audio Watcher ($ARCHS, menubar + daemon)"
build_universal src/app.swift "workflow/Apple Music Audio Watcher" "-framework AppKit -framework CoreAudio"

echo "→ compiling audio_format ($ARCHS)"
build_universal src/audio_format.swift workflow/audio_format "-framework CoreAudio"

echo "→ assembling $APP"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp src/AppBundle.Info.plist "$APP/Contents/Info.plist"
mv "workflow/Apple Music Audio Watcher" "$APP/Contents/MacOS/Apple Music Audio Watcher"
chmod +x "$APP/Contents/MacOS/Apple Music Audio Watcher"
sips -s format icns workflow/icon.png --out "$APP/Contents/Resources/AppIcon.icns" >/dev/null
codesign --sign - --force --identifier com.ariestwn.apple-music-audio-format "$APP" 2>&1 \
    | grep -v "replacing existing signature" || true

echo "→ packaging $OUT"
rm -f "$OUT"
( cd workflow && zip -q -r -X "../$OUT" . -x "*.DS_Store" )

echo "✓ built: $(pwd)/$OUT"
echo "  install: open \"$OUT\""
