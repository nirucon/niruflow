#!/bin/sh
# X11 end-to-end checks in a private Xvfb: first frame, memory, idle CPU,
# then the main flows by mouse and keyboard: cleanup preview and quarantine,
# restore from History, a storage scan with permanent deletion behind both
# confirmations, the theme switch and closing.
# Usage: tests/xvfb.sh PATH/TO/niruflow [DISPLAY_NUMBER] [SHOTS_DIR]
set -eu
BIN=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
DNUM=${2:-$((70 + $$ % 20))}
SHOTS=${3:-}
SB=$(mktemp -d)
PIDS=""
cleanup() { for p in $PIDS; do kill "$p" 2>/dev/null || true; done; rm -rf "$SB"; }
trap cleanup EXIT INT TERM
export HOME="$SB/home" XDG_CONFIG_HOME="$SB/home/.config" XDG_STATE_HOME="$SB/home/.local/state" XDG_DATA_HOME="$SB/home/.local/share"
unset XDG_CACHE_HOME || true
mkdir -p "$HOME/Downloads" "$HOME/big/deep" "$XDG_CONFIG_HOME/niruflow"
echo old > "$HOME/Downloads/old.txt"
head -c 300000 /dev/zero > "$HOME/big/deep/huge.bin"
echo small > "$HOME/big/small.txt"
printf '{"rules":[{"id":"downloads","name":"Downloads","path":"%s/Downloads","hours":0.0001,"enabled":true,"recursive":false,"automatic":true}],"theme":"Niru Noir"}\n' "$HOME" > "$XDG_CONFIG_HOME/niruflow/config.json"
D=":$DNUM"
Xvfb "$D" -screen 0 1400x900x24 -nolisten tcp >/dev/null 2>&1 &
PIDS="$PIDS $!"
export DISPLAY=$D
sleep 1
fail() { echo "FAIL: $*" >&2; [ -n "$SHOTS" ] && import -window root "$SHOTS/fail.png" 2>/dev/null; exit 1; }
ERR=$SB/err
shot() { [ -n "$SHOTS" ] && import -window root "$SHOTS/$1.png" 2>/dev/null || true; }
# The coordinates of a clickable thing in the newest frame.
hit() { tac "$ERR" | awk -v n="hit $1 " 'NR > 1 && /^hits-end/ {exit} index($0, n) == 1 {print $3, $4; exit}'; }
click() {
    xy=$(hit "$1"); [ -n "$xy" ] || fail "no $1 on screen"
    # shellcheck disable=SC2086
    xdotool mousemove $xy click 1; sleep 0.4
}
key() { xdotool key --delay 60 "$@"; sleep 0.4; }

sleep 1
NIRUFLOW_TIMING=1 NIRUFLOW_HITS=1 "$BIN" 2>"$ERR" &
A=$!
PIDS="$PIDS $A"
sleep 1.2
grep -q 'first frame after' "$ERR" || fail "no first frame"
ms=$(sed -n 's/.*first frame after \([0-9]*\)\..*/\1/p' "$ERR")
[ "$ms" -lt 1000 ] || fail "first frame took $ms ms"
rss=$(awk '/VmRSS/ {print $2}' /proc/$A/status)
[ "$rss" -lt 60000 ] || fail "memory $rss kB"
t0=$(awk '{print $14 + $15}' /proc/$A/stat); sleep 3; t1=$(awk '{print $14 + $15}' /proc/$A/stat)
echo "first frame ${ms} ms, ${rss} kB, idle CPU $((t1 - t0)) ticks in 3 s"
[ $((t1 - t0)) -le 2 ] || fail "busy while idle"
shot overview

# File Cleanup: a preview with a preset age finds nothing new (the ctime
# gate protects fresh files) and changes nothing.
click nav:File_Cleanup
click combo:CleanupAge
click pop:0
click btn:CleanupAnalyze
sleep 0.6
[ -z "$(hit dlg:Ok)" ] || fail "cleanup preview failed"
[ -f "$HOME/Downloads/old.txt" ] || fail "a preview changed files"
shot cleanup
echo "cleanup preview ok"

# Automations: dry run, then run now into quarantine (confirm Yes).
click nav:Automations
click row:Rules:0
click btn:RulePreview
sleep 0.6
click dlg:Ok
click btn:RuleRun
shot confirm
click dlg:Ok
sleep 0.8
[ ! -e "$HOME/Downloads/old.txt" ] || fail "quarantine did not move the file"
ls "$XDG_STATE_HOME/niruflow/quarantine/" | grep -q '_old.txt$' || fail "file not in quarantine"
click dlg:Ok
echo "quarantine ok"

# History: restore the action.
click nav:History
click row:History:0
click btn:HistoryRestore
click dlg:Ok
sleep 0.5
[ -f "$HOME/Downloads/old.txt" ] || fail "restore"
shot history
echo "restore ok"

# Storage: largest files, delete the biggest after both confirmations.
click nav:Storage
click btn:StorageLargest
sleep 1.5
[ -n "$(hit row:Storage:0)" ] || fail "no storage rows"
shot storage
click row:Storage:0
click btn:StorageDelete
click dlg:Ok
xdotool type --delay 30 "DELETE"; sleep 0.2
key Return
sleep 0.8
[ ! -e "$HOME/big/deep/huge.bin" ] || fail "permanent delete"
[ -f "$HOME/big/small.txt" ] || fail "deleted too much"
click dlg:Ok
echo "storage delete ok"

# A cancelled typed confirmation changes nothing.
click row:Storage:0
click btn:StorageDelete
click dlg:Ok
xdotool type --delay 30 "nope"; sleep 0.2
key Return
sleep 0.5
[ -f "$HOME/big/small.txt" ] || fail "wrong phrase still deleted"
echo "typed confirmation guard ok"

# Theme from the header list.
click combo:Theme
click pop:2
grep -q '"theme": "Satie"' "$XDG_CONFIG_HOME/niruflow/config.json" || fail "theme not saved"
shot satie
echo "theme ok"

# Every page draws.
for p in Overview Automations Projects Docker Media_Library Settings; do click "nav:$p"; done
shot settings
key ctrl+q
sleep 0.5
kill -0 "$A" 2>/dev/null && fail "Ctrl+Q did not close"
echo "PASS"
