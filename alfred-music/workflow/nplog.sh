#!/usr/bin/env bash
# Alfred Script Filter: shows auto-apply activity + error log with a top-item
# action to clear them. Each entry is selectable (Enter = copy to clipboard).
set -u

LABEL="com.ariestwn.apple-music-audio-format"
CACHE_DIR="$HOME/Library/Caches/$LABEL"
SUPPORT="$HOME/Library/Application Support/$LABEL"
APPLY="$CACHE_DIR/apply.log"
ERR="$CACHE_DIR/watcher.err"
APPLIER="$SUPPORT/audio_format"

json_str() {
    local s=$1
    s=${s//\\/\\\\}
    s=${s//\"/\\\"}
    s=${s//$'\n'/\\n}
    s=${s//$'\t'/\\t}
    printf '"%s"' "$s"
}

# Collect items into a temp file; assemble JSON at the end.
TMP=$(mktemp)
trap 'rm -f "$TMP"' EXIT

emit() {
    # $1=title  $2=subtitle  $3=arg  $4=valid  $5=uid
    printf '{"title":%s,"subtitle":%s,"arg":%s,"valid":%s,"uid":%s,"icon":{"path":"icon.png"}}\n' \
        "$(json_str "$1")" "$(json_str "$2")" "$(json_str "$3")" "$4" "$(json_str "$5")" >> "$TMP"
}

# Header: clear action + context.
emit "🗑  Clear logs" "Truncate apply.log and watcher.err (cache kept)" "__CLEAR__" true "clear"

device="unknown"
[ -x "$APPLIER" ] && device=$("$APPLIER" current 2>/dev/null | head -1)
emit "Device: $device" "Current default output physical format" "" false "device"

apply_lines=$( [ -f "$APPLY" ] && /usr/bin/wc -l < "$APPLY" | /usr/bin/tr -d ' ' || echo 0)
err_lines=$(   [ -f "$ERR"   ] && /usr/bin/wc -l < "$ERR"   | /usr/bin/tr -d ' ' || echo 0)
emit "Log summary" "apply.log: $apply_lines line(s) · watcher.err: $err_lines line(s)" "" false "summary"

# Most recent apply entries (latest on top, up to 40).
if [ -f "$APPLY" ] && [ "$apply_lines" -gt 0 ]; then
    idx=0
    while IFS= read -r line; do
        [ -n "$line" ] || continue
        emit "▶ $line" "apply.log · Enter = copy" "$line" true "apply-$idx"
        idx=$((idx+1))
    done < <(/usr/bin/tail -n 40 "$APPLY" | /usr/bin/tail -r)
fi

# Error entries (if any).
if [ -f "$ERR" ] && [ "$err_lines" -gt 0 ]; then
    idx=0
    while IFS= read -r line; do
        [ -n "$line" ] || continue
        emit "⚠ $line" "watcher.err · Enter = copy" "$line" true "err-$idx"
        idx=$((idx+1))
    done < <(/usr/bin/tail -n 20 "$ERR" | /usr/bin/tail -r)
fi

# Wrap into {"items":[ ... ]} with comma separation.
/usr/bin/awk 'BEGIN{printf "{\"items\":["} NR>1{printf ","} {printf "%s",$0} END{print "]}"}' "$TMP"
