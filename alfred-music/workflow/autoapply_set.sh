#!/usr/bin/env bash
# Run Script action body: takes "on" or "off" as $1 (or {query}) and toggles
# the auto-apply off-switch accordingly. Echoes a one-line status for the
# Post Notification node.
set -u

LABEL="com.ariestwn.apple-music-audio-format"
SUPPORT="$HOME/Library/Application Support/$LABEL"
OFF="$SUPPORT/autoapply.off"
mkdir -p "$SUPPORT"

arg="${1:-}"
case "$arg" in
    on)
        rm -f "$OFF"
        echo "Auto-apply enabled — device will follow song rate"
        ;;
    off)
        : > "$OFF"
        echo "Auto-apply disabled — manual control only"
        ;;
    *)
        # Toggle
        if [ -e "$OFF" ]; then
            rm -f "$OFF"
            echo "Auto-apply enabled — device will follow song rate"
        else
            : > "$OFF"
            echo "Auto-apply disabled — manual control only"
        fi
        ;;
esac
