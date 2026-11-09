#!/bin/bash
# usage: keys.sh <label> ; sends keys to frontmost process $PROC
P=${PROC:-whatrust}
osa(){ osascript -e "tell application \"System Events\" to $1"; }
osa "set frontmost of process \"$P\" to true"; sleep 2
curl -s -XPOST --data "{\"t\":\"MARK\",\"label\":\"$1\"}" http://127.0.0.1:8765/log
screencapture -x shot-$1-0.png
osa 'keystroke "ab"'; sleep 0.6
osa 'key code 123'; sleep 0.6
osa 'keystroke "X"'; sleep 0.6
osa 'key code 124'; sleep 0.6; osa 'keystroke "Y"'; sleep 0.6
osa 'key code 126'; sleep 0.6; osa 'key code 125'; sleep 0.6
screencapture -x shot-$1-1.png
osa 'key code 115'; sleep 0.6; osa 'key code 119'; sleep 0.6
osa 'key code 116'; sleep 0.6; osa 'key code 121'; sleep 0.6
osa 'keystroke "Z"'; sleep 1
screencapture -x shot-$1-2.png
