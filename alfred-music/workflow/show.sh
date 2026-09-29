#!/usr/bin/env bash
# Alfred Script Filter: shows currently-playing Apple Music track + audio format.
#
# Sources:
#   Music.app AppleScript  — track name/artist/album/state, and (for local files)
#                            sample rate + bit rate.
#   LaunchAgent cache      — for streaming tracks (URL tracks), the format is
#                            parsed live from the MediaToolbox log stream.
#
# Self-heals: if the LaunchAgent is not loaded, auto-installs it.

set -u

LABEL="com.ariestwn.apple-music-audio-format"
CACHE="$HOME/Library/Caches/$LABEL/nowplaying.json"
SUPPORT="$HOME/Library/Application Support/$LABEL"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

ensure_watcher() {
    # Respect explicit `npstop` — without this guard, every `np` would
    # silently re-bootstrap the daemon the user just asked to stop.
    [ -f "$SUPPORT/daemon.off" ] && return 0
    if ! launchctl print "gui/$(id -u)/$LABEL" >/dev/null 2>&1; then
        bash "$HERE/install.sh" >/dev/null 2>&1 || return 1
    fi
}

alfred_item() {
    # $1=title  $2=subtitle  $3=arg  [$4=valid]
    local valid="${4:-true}"
    printf '{"items":[{"title":%s,"subtitle":%s,"arg":%s,"valid":%s,"icon":{"path":"icon.png"}}]}\n' \
        "$(json_str "$1")" "$(json_str "$2")" "$(json_str "$3")" "$valid"
}

json_str() {
    # Minimal JSON string escape (quotes + backslash + newline).
    local s=$1
    s=${s//\\/\\\\}
    s=${s//\"/\\\"}
    s=${s//$'\n'/\\n}
    s=${s//$'\t'/\\t}
    printf '"%s"' "$s"
}

ensure_watcher

# Pull Music.app state (tab-delimited; empty lines if missing).
MUSIC_INFO=$(/usr/bin/osascript <<'AS' 2>/dev/null
on safeGet(s)
    try
        if s is missing value then return ""
        return s as string
    on error
        return ""
    end try
end safeGet

if application "Music" is not running then return "NOT_RUNNING"
tell application "Music"
    set ps to (player state as string)
    if ps is "stopped" then return "STOPPED"
    try
        set t to current track
    on error
        return "NO_TRACK"
    end try
    set nm to my safeGet(name of t)
    set ar to my safeGet(artist of t)
    set al to my safeGet(album of t)
    set kd to my safeGet(kind of t)
    set cl to (class of t as string)
    set sr to my safeGet(sample rate of t)
    set br to my safeGet(bit rate of t)
    return ps & tab & nm & tab & ar & tab & al & tab & kd & tab & cl & tab & sr & tab & br
end tell
AS
)

case "$MUSIC_INFO" in
    NOT_RUNNING) alfred_item "Apple Music not running" "Launch Music.app to see now playing" "" false; exit 0 ;;
    STOPPED)     alfred_item "Music stopped"           "Start a song then run this again"   "" false; exit 0 ;;
    NO_TRACK)    alfred_item "No track loaded"         "Play something in Music.app"        "" false; exit 0 ;;
    "")          alfred_item "Music.app didn't respond" "Check Automation permission for Alfred" "" false; exit 0 ;;
esac

IFS=$'\t' read -r STATE NAME ARTIST ALBUM KIND TCLASS AS_RATE AS_BITRATE <<< "$MUSIC_INFO"

# Resolve format.
#
# Audio path matters: anything that streams through Apple Music (URL tracks
# from search, shared tracks from your library, etc.) is decoded by
# FigStreamPlayer — its log stream is the only authoritative source of the
# actual codec/rate. AppleScript's `kind`/`sample rate`/`bit rate` for
# streaming items reflects stale catalog metadata or a previously-cached
# AAC download, not what is playing right now. We only trust AppleScript
# for genuinely local file tracks (user's own MP3/FLAC).
FMT_LINE=""
CODEC=""
case "$TCLASS" in
    *"file track"*) IS_LOCAL=1 ;;
    *)              IS_LOCAL=0 ;;
esac

if [ "$IS_LOCAL" = "1" ] && [ -n "$AS_RATE" ] && [ "$AS_RATE" != "0" ]; then
    # Genuine local file — AppleScript metadata is accurate.
    rate_khz=$(awk -v r="$AS_RATE" 'BEGIN{printf (r/1000==int(r/1000))?"%d kHz":"%.1f kHz", r/1000}')
    if [ -n "$AS_BITRATE" ] && [ "$AS_BITRATE" != "0" ]; then
        FMT_LINE="$rate_khz · ${AS_BITRATE} kbps · ${KIND:-local file}"
    else
        FMT_LINE="$rate_khz · ${KIND:-local file}"
    fi
elif [ -f "$CACHE" ]; then
    # Streaming — read latest from cache.
    read_key() { /usr/bin/sed -nE "s/.*\"$1\":\"?([^],\"}]+)\"?.*/\\1/p" "$CACHE"; }
    F_RATE=$(read_key sampleRate)
    F_BITS=$(read_key bitDepth)
    F_REND=$(read_key rendition)
    F_FMT=$(read_key format)
    F_CHN=$(read_key channels)
    F_TS=$(read_key timestamp)

    if [ -n "$F_RATE" ] && [ "$F_RATE" != "null" ]; then
        rate_khz=$(awk -v r="$F_RATE" 'BEGIN{printf (r/1000==int(r/1000))?"%d kHz":"%.1f kHz", r/1000}')
        parts="$rate_khz"
        # AAC has no fixed bit depth; daemon writes 0. Only show meaningful values.
        [ -n "$F_BITS" ] && [ "$F_BITS" != "null" ] && [ "$F_BITS" != "0" ] && parts="$parts · ${F_BITS}-bit"
        case "$F_CHN" in 1) parts="$parts · mono" ;; 2) parts="$parts · stereo" ;; esac
        # Only surface rendition when it conveys quality tier; "Stereo" is just layout.
        case "$F_REND" in
            Lossless|"Hi-Res Lossless"|"Dolby Atmos"|"Apple Digital Master")
                parts="$parts · $F_REND" ;;
        esac
        case "$F_FMT" in
            qlac|alac)         CODEC="ALAC"; parts="$parts (ALAC)" ;;
            qaac|aac|aach|aacp) CODEC="AAC";  parts="$parts (AAC)"  ;;
            lpcm|pcm)          CODEC="PCM";  parts="$parts (PCM)"  ;;
            flac)              CODEC="FLAC"; parts="$parts (FLAC)" ;;
            *)                 [ -n "$F_FMT" ] && parts="$parts ($F_FMT)" ;;
        esac
        FMT_LINE="$parts"
    fi
fi

if [ -z "$FMT_LINE" ]; then
    if [ "$IS_LOCAL" = "0" ]; then
        FMT_LINE="Format not captured yet — skip to next track then retry"
    else
        FMT_LINE="Format info unavailable"
    fi
fi

STATE_GLYPH="▶"
[ "$STATE" = "paused" ] && STATE_GLYPH="❚❚"

TITLE="$STATE_GLYPH $NAME"
[ -n "$ARTIST" ] && TITLE="$TITLE — $ARTIST"
SUBTITLE="$FMT_LINE"
[ -n "$ALBUM" ] && SUBTITLE="$SUBTITLE  ·  $ALBUM"

ARG="$NAME — $ARTIST ($FMT_LINE)"
alfred_item "$TITLE" "$SUBTITLE" "$ARG" true
