#!/usr/bin/env bash
set -u
LABEL="com.ariestwn.apple-music-audio-format"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"

launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null || true
rm -f "$PLIST"
rm -rf "$HOME/Library/Application Support/$LABEL"
echo "uninstalled: $LABEL (cache retained at ~/Library/Caches/$LABEL)"
