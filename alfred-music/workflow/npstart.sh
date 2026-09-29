#!/usr/bin/env bash
# Run Script action: clear the `daemon.off` flag set by npstop and
# bootstrap the LaunchAgent. Use after npstop to revive the daemon
# without leaving Alfred.
set -u

LABEL="com.ariestwn.apple-music-audio-format"
SUPPORT="$HOME/Library/Application Support/$LABEL"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

rm -f "$SUPPORT/daemon.off"
bash "$HERE/install.sh" >/dev/null 2>&1

echo "Apple Music Audio Watcher started"
