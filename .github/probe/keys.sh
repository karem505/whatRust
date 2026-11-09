#!/bin/bash
P=${PROC:-whatrust}
osa(){ osascript -e "tell application \"System Events\" to $1"; }
hk(){ ../.github/probe/hwkey "$@"; }
mark(){ curl -s -XPOST --data "{\"t\":\"MARK\",\"label\":\"$1\"}" http://127.0.0.1:8765/log; }
osa "set frontmost of process \"$P\" to true"; sleep 2
mark "$1"
screencapture -x shot-$1-0.png
mark "empty field: Left Right Up Down (all no-ops)"
hk 123 fn; sleep 0.6; hk 124 fn; sleep 0.6; hk 126 fn; sleep 0.6; hk 125 fn; sleep 0.6
mark "type ab, then Right at end"
osa 'keystroke "ab"'; sleep 0.6
hk 124 fn; sleep 0.6
mark "Left Left Left (3rd at start)"
hk 123 fn; sleep 0.6; hk 123 fn; sleep 0.6; hk 123 fn; sleep 0.6
mark "Up at first line"
hk 126 fn; sleep 0.6
screencapture -x shot-$1-2.png
