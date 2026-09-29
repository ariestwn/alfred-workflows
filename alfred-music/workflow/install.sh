#!/usr/bin/env bash
# Installs the watcher as a user LaunchAgent. Idempotent.
set -euo pipefail

LABEL="com.ariestwn.apple-music-audio-format"
APP_NAME="Apple Music Audio Watcher.app"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SUPPORT="$HOME/Library/Application Support/$LABEL"
APP_PATH="$SUPPORT/$APP_NAME"
WATCHER="$APP_PATH/Contents/MacOS/Apple Music Audio Watcher"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"
CACHE_DIR="$HOME/Library/Caches/$LABEL"

mkdir -p "$SUPPORT" "$CACHE_DIR" "$HOME/Library/LaunchAgents"

# Explicit install resets any prior `npstop` intent.
rm -f "$SUPPORT/daemon.off"

# Migration from the pre-rename label `com.ariestwn.applemusic-format`.
OLD_LABEL="com.ariestwn.applemusic-format"
OLD_SUPPORT="$HOME/Library/Application Support/$OLD_LABEL"
OLD_CACHE="$HOME/Library/Caches/$OLD_LABEL"
OLD_PLIST="$HOME/Library/LaunchAgents/$OLD_LABEL.plist"
if launchctl print "gui/$(id -u)/$OLD_LABEL" >/dev/null 2>&1; then
    launchctl bootout "gui/$(id -u)/$OLD_LABEL" 2>/dev/null || true
fi
rm -f "$OLD_PLIST"
# Preserve existing apply.log / watcher.err if user had any history.
if [ -d "$OLD_CACHE" ] && [ ! -d "$CACHE_DIR" ]; then
    mv "$OLD_CACHE" "$CACHE_DIR" 2>/dev/null || true
fi
rm -rf "$OLD_SUPPORT"

# Install the .app bundle (replaces any prior version atomically).
if [ -d "$HERE/$APP_NAME" ]; then
    rm -rf "$APP_PATH"
    cp -R "$HERE/$APP_NAME" "$APP_PATH"
    chmod +x "$WATCHER"
    # Migration: remove the old standalone watcher binary left over from
    # pre-bundle installs.
    rm -f "$SUPPORT/apple-music-audio-watcher"
elif [ -f "$HERE/apple-music-audio-watcher" ]; then
    # Backwards-compatibility fallback when the bundle isn't present.
    install -m 0755 "$HERE/apple-music-audio-watcher" "$SUPPORT/apple-music-audio-watcher"
    WATCHER="$SUPPORT/apple-music-audio-watcher"
fi
# Helper for auto-applying the detected format to the output device.
if [ -f "$HERE/audio_format" ]; then
    install -m 0755 "$HERE/audio_format" "$SUPPORT/audio_format"
fi
# Migration: remove old helper name left over from pre-merge installs.
rm -f "$SUPPORT/audio_format_set"

# Thin universal binaries down to the host architecture — the bundle ships
# universal so it works on Intel + Apple Silicon, but each install only
# needs its own slice (~half the size, native execution).
HOST_ARCH=$(uname -m)
if [ "$HOST_ARCH" = "arm64" ] || [ "$HOST_ARCH" = "x86_64" ]; then
    for bin in "$WATCHER" "$SUPPORT/audio_format"; do
        [ -f "$bin" ] || continue
        if /usr/bin/file "$bin" 2>/dev/null | /usr/bin/grep -q "universal binary"; then
            /usr/bin/lipo -thin "$HOST_ARCH" -output "$bin.thin" "$bin" \
                && mv "$bin.thin" "$bin"
        fi
    done
    # Re-codesign the bundle since we modified the executable inside.
    if [ -d "$APP_PATH" ]; then
        /usr/bin/codesign --sign - --force \
            --identifier com.ariestwn.apple-music-audio-format \
            "$APP_PATH" 2>/dev/null || true
    fi
fi

cat > "$PLIST" <<PLIST_EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>$LABEL</string>
    <key>ProgramArguments</key>
    <array>
        <string>$WATCHER</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardErrorPath</key>
    <string>$CACHE_DIR/watcher.err</string>
    <key>StandardOutPath</key>
    <string>$CACHE_DIR/watcher.out</string>
</dict>
</plist>
PLIST_EOF

# Reload cleanly: bootout ignores missing, bootstrap loads fresh. The brief
# wait gives launchd time to fully release the previous instance before we
# re-register — without it bootstrap can race and fail with "Input/output
# error" (5).
launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null || true
for _ in 1 2 3 4 5; do
    launchctl print "gui/$(id -u)/$LABEL" >/dev/null 2>&1 || break
    sleep 0.3
done
launchctl bootstrap "gui/$(id -u)" "$PLIST"

echo "installed: $LABEL"
echo "  watcher: $WATCHER"
echo "  plist:   $PLIST"
echo "  cache:   $CACHE_DIR/nowplaying.json"
