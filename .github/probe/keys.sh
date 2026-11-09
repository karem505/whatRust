#!/bin/bash
# usage: keys.sh <label>  ; sends keys to frontmost whatrust
P=${PROC:-whatrust}
osa(){ osascript -e "tell application \"System Events\" to $1"; }
osa "set frontmost of process \"$P\" to true"; sleep 2
screencapture -x shot-$1-0.png
osa 'keystroke "ab"'; sleep 0.5
osa 'key code 123'; sleep 0.5         # left
osa 'keystroke "X"'; sleep 0.5        # expect aXb
screencapture -x shot-$1-1.png
osa 'key code 124'; sleep 0.5; osa 'keystroke "Y"'; sleep 0.5   # right -> aXbY
osa 'key code 126'; sleep 0.5; osa 'key code 125'; sleep 0.5      # up, down
osa 'key code 115'; sleep 0.5; osa 'key code 119'; sleep 0.5     # home, end
osa 'key code 116'; sleep 0.5; osa 'key code 121'; sleep 0.5     # pgup, pgdn
osa 'keystroke "Z"'; sleep 1
screencapture -x shot-$1-2.png
echo done
