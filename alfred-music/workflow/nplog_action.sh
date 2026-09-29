#!/usr/bin/env bash
# Run Script action: if arg == __CLEAR__ truncate the logs; otherwise copy
# the arg (a log line) to the clipboard. Echoes a one-liner for notification.
set -u

LABEL="com.ariestwn.apple-music-audio-format"
CACHE_DIR="$HOME/Library/Caches/$LABEL"
APPLY="$CACHE_DIR/apply.log"
ERR="$CACHE_DIR/watcher.err"

arg="${1:-}"
case "$arg" in
    __CLEAR__)
        : > "$APPLY" 2>/dev/null
        : > "$ERR"   2>/dev/null
        echo "Logs cleared"
        ;;
    "")
        echo "Nothing selected"
        ;;
    *)
        printf '%s' "$arg" | /usr/bin/pbcopy
        echo "Copied to clipboard"
        ;;
esac
