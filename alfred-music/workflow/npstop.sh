#!/usr/bin/env bash
# Run Script action: stop the daemon (bootout LaunchAgent + force-kill any
# orphan processes). Writes daemon.off so show.sh's auto-heal won't
# silently re-install it on the next `np`. Cleared by install.sh.
set -u

LABEL="com.ariestwn.apple-music-audio-format"
SUPPORT="$HOME/Library/Application Support/$LABEL"

mkdir -p "$SUPPORT"
touch "$SUPPORT/daemon.off"

launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null || true
sleep 1
pkill -9 -f "Apple Music Audio Watcher" 2>/dev/null || true

echo "Apple Music Audio Watcher stopped"
