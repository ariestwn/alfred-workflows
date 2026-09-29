#!/usr/bin/env bash
# Alfred Script Filter: shows menubar visibility state + offers toggle action.
# Presence of menubar.off in the support folder = hidden.
set -u

LABEL="com.ariestwn.apple-music-audio-format"
OFF="$HOME/Library/Application Support/$LABEL/menubar.off"

if [ -e "$OFF" ]; then
    next_title="Show menubar icon"
    next_sub="Music note will reappear in the menu bar"
    keep_title="Menubar icon: HIDDEN"
    keep_sub="Daemon still running in background. Press ↵ above to show."
    next="show"
else
    next_title="Hide menubar icon"
    next_sub="Daemon keeps running in background; only the icon disappears"
    keep_title="Menubar icon: VISIBLE"
    keep_sub="Currently shown in the menu bar."
    next="hide"
fi

cat <<JSON
{"items":[
  {"title":"$next_title","subtitle":"$next_sub","arg":"$next","valid":true,"icon":{"path":"icon.png"}},
  {"title":"$keep_title","subtitle":"$keep_sub","arg":"","valid":false,"icon":{"path":"icon.png"}}
]}
JSON
