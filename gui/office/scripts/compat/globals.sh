#!/usr/bin/env bash
# The globals nkw advertises: headless `nkw --demo` + `nkw-testclient --list-globals`
# → $COMPAT_OUT/globals.json ([{name, version}], sorted).
# With --satellite also runs `xwayland-satellite :N` against it for 4 s (RUST_LOG=info) and keeps
# what it says about missing globals in $COMPAT_OUT/satellite.txt.
# Usage: globals.sh [--bin PATH] [--satellite] [--out DIR]
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

satellite=false
while (($#)); do
	case $1 in
	--bin) export NKW_BIN=${2:?}; shift ;;
	--out) COMPAT_OUT=${2:?}; shift ;;
	--satellite) satellite=true ;;
	-h | --help) sed -n '2,6p' "$0"; exit 0 ;;
	*) echo "globals: opção desconhecida: $1" >&2; exit 2 ;;
	esac
	shift
done
bin=$(compat_bin)
tc=$(compat_testclient "$bin")
mkdir -p "$COMPAT_OUT"
tmp=$(compat_tmp)
mkdir -p "$tmp/xdg"
chmod 700 "$tmp/xdg"

XDG_RUNTIME_DIR=$tmp/xdg setsid "$bin" --demo --headless --socket nkw-compat --seconds 20 \
	>/dev/null 2>"$tmp/nkw.err" &
COMPAT_PGIDS+=($!)
compat_wait_socket "$tmp/xdg/nkw-compat" 10 || { cat "$tmp/nkw.err" >&2; exit 1; }

XDG_RUNTIME_DIR=$tmp/xdg WAYLAND_DISPLAY=nkw-compat timeout 10 "$tc" --list-globals --log "$tmp/g.json"
compat_globals_json "$tmp/g.json" >"$COMPAT_OUT/globals.json"
echo "globals: $(jq length "$COMPAT_OUT/globals.json") → $COMPAT_OUT/globals.json"

if $satellite; then
	if ! command -v xwayland-satellite >/dev/null; then
		echo "globals: xwayland-satellite não instalado" >"$COMPAT_OUT/satellite.txt"
	else
		# the first free display of 90..99 (never a display somebody uses)
		n=90
		while [ -e "/tmp/.X11-unix/X$n" ] || [ -e "/tmp/.X$n-lock" ]; do n=$((n + 1)); done
		[ "$n" -le 99 ] || { echo "globals: nenhum display livre em :90..:99" >&2; exit 1; }
		XDG_RUNTIME_DIR=$tmp/xdg WAYLAND_DISPLAY=nkw-compat RUST_LOG=info \
			timeout -k 1 4 setsid xwayland-satellite ":$n" >"$tmp/sat.log" 2>&1 &
		COMPAT_PGIDS+=($!)
		sleep 4.5
		# only the lines about globals/protocols: the satellite logs nothing personal, but keep it short
		grep -iE 'global|protocol|missing|required|support|error|panic|version' "$tmp/sat.log" |
			sed -E 's/\x1b\[[0-9;]*m//g' | sort -u >"$COMPAT_OUT/satellite.txt" || true
		echo "satellite (:$n): $(wc -l <"$COMPAT_OUT/satellite.txt") linhas → $COMPAT_OUT/satellite.txt"
	fi
fi
