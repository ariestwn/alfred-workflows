#!/usr/bin/env bash
# Run Script action: takes "show" / "hide" (or empty = toggle) and flips
# the menubar.off flag. The menubar app watches the file via DispatchSource
# and reacts immediately — no daemon restart needed.
set -u

LABEL="com.ariestwn.apple-music-audio-format"
SUPPORT="$HOME/Library/Application Support/$LABEL"
OFF="$SUPPORT/menubar.off"
mkdir -p "$SUPPORT"

arg="${1:-}"
case "$arg" in
    show)
        rm -f "$OFF"
        echo "Menubar icon shown"
        ;;
    hide)
        : > "$OFF"
        echo "Menubar icon hidden — daemon still active"
        ;;
    *)
        if [ -e "$OFF" ]; then
            rm -f "$OFF"; echo "Menubar icon shown"
        else
            : > "$OFF"; echo "Menubar icon hidden — daemon still active"
        fi
        ;;
esac
