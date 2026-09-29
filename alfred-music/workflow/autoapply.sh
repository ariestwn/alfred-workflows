#!/usr/bin/env bash
# Alfred Script Filter: shows auto-apply state + offers toggle action.
# Presence of autoapply.off in the support folder = disabled.
set -u

LABEL="com.ariestwn.apple-music-audio-format"
OFF="$HOME/Library/Application Support/$LABEL/autoapply.off"

if [ -e "$OFF" ]; then
    state="OFF"
    next="on"
    next_title="Enable auto-apply"
    next_sub="Output device will follow song sample rate / bit depth"
    keep_title="Auto-apply is OFF"
    keep_sub="Current: manual only. Press ↵ above to enable."
else
    state="ON"
    next="off"
    next_title="Disable auto-apply"
    next_sub="Stops following song format; current device rate will stay"
    keep_title="Auto-apply is ON"
    keep_sub="Current: device follows every track change automatically."
fi

cat <<JSON
{"items":[
  {"title":"$next_title","subtitle":"$next_sub","arg":"$next","valid":true,"icon":{"path":"icon.png"}},
  {"title":"$keep_title","subtitle":"$keep_sub","arg":"","valid":false,"icon":{"path":"icon.png"}}
]}
JSON
