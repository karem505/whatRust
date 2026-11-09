#!/bin/bash
# usage: keys.sh <label> ; sends keys to frontmost process $PROC, hardware-like arrow flags via hwkey
P=${PROC:-whatrust}
osa(){ osascript -e "tell application \"System Events\" to $1"; }
hk(){ ../.github/probe/hwkey "$@"; }
osa "set frontmost of process \"$P\" to true"; sleep 2
curl -s -XPOST --data "{\"t\":\"MARK\",\"label\":\"$1\"}" http://127.0.0.1:8765/log
screencapture -x shot-$1-0.png
osa 'keystroke "ab"'; sleep 0.6
echo "--- synthetic (no fn flags)"; curl -s -XPOST --data '{"t":"MARK","label":"nofn"}' http://127.0.0.1:8765/log
osa 'key code 123'; sleep 0.6; osa 'keystroke "X"'; sleep 0.6
osa 'key code 124'; sleep 0.6; osa 'keystroke "Y"'; sleep 0.6
curl -s -XPOST --data '{"t":"MARK","label":"fn-flags (hardware-like)"}' http://127.0.0.1:8765/log
hk 123 fn; sleep 0.6; osa 'keystroke "P"'; sleep 0.6
hk 124 fn; sleep 0.6; osa 'keystroke "Q"'; sleep 0.6
hk 126 fn; sleep 0.6; hk 125 fn; sleep 0.6
hk 115 fn; sleep 0.6; hk 119 fn; sleep 0.6
hk 116 fn; sleep 0.6; hk 121 fn; sleep 0.6
osa 'keystroke "Z"'; sleep 1
screencapture -x shot-$1-2.png
