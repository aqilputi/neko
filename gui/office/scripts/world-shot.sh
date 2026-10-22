#!/usr/bin/env bash
# The screenshot set (plano-mundo §9): nkw --demo --headless plays each scenarios/shot-*.nks,
# whose `shot`/`dump` lines write into target/shots/. Only the demo world and the testclient,
# never real windows.
# Usage: world-shot.sh [--bin PATH] [--out DIR] [NAME...]   (NAME = nav, focus, ...)
set -euo pipefail

HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
OFFICE=$(dirname "$HERE")
BIN="$OFFICE/target/world/nkw"
OUT="$OFFICE/target/shots"
names=()
while (($#)); do
	case $1 in
	--bin) BIN=${2:?}; shift ;;
	--out) OUT=${2:?}; shift ;;
	-h | --help) sed -n '2,6p' "$0"; exit 0 ;;
	-*) echo "world-shot: opção desconhecida: $1" >&2; exit 2 ;;
	*) names+=("$1") ;;
	esac
	shift
done
[ -x "$BIN" ] || { echo "world-shot: nkw não encontrado em $BIN (make world-build)" >&2; exit 2; }
TESTCLIENT="$(dirname "$BIN")/nkw-testclient"
[ ${#names[@]} -gt 0 ] || for f in "$HERE"/scenarios/shot-*.nks; do
	n=${f##*/shot-}
	names+=("${n%.nks}")
done
mkdir -p "$OUT"
work=$(mktemp -d "${TMPDIR:-/tmp}/nkw-shot.XXXXXX")
trap 'rm -rf "$work"' EXIT

failed=0
for n in "${names[@]}"; do
	src="$HERE/scenarios/shot-$n.nks"
	[ -f "$src" ] || { echo "world-shot: sem roteiro $src" >&2; failed=1; continue; }
	sed -e "s|\${OUT}|$OUT|g" -e "s|\${TESTCLIENT}|$TESTCLIENT|g" "$src" >"$work/$n.nks"
	rm -f "$OUT/shot-$n.png"
	rc=0
	"$BIN" --demo --headless --socket "nkw-shot-$$-$n" --seconds 30 --script "$work/$n.nks" \
		2>"$work/$n.err" || rc=$?
	if [ "$rc" -ne 0 ] || [ ! -s "$OUT/shot-$n.png" ]; then
		echo "shot-$n: falhou (nkw saiu com $rc)" >&2
		tail -5 "$work/$n.err" >&2
		failed=1
	else
		echo "shot-$n: $OUT/shot-$n.png"
	fi
done
exit "$failed"
