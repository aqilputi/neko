#!/usr/bin/env bash
# The compatibility suite (S23).
#   --headless (default): globals.json + `cargo test -p nkw --test compat_protocols` (testclient only;
#                         NKW_E2E=1 adds `--test compat_apps`, the real apps, headless).
#   --nested:             the real-app matrix in windows of your session (≤ 2 min per run, throwaway
#                         profiles, own D-Bus): target/compat/matrix.md + globals.json.
# Usage: world-compat.sh [--headless] [--nested] [--apps "foot kitty ..."] [--out DIR] [--bin PATH] [--yes]
set -euo pipefail
HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
OFFICE=$(dirname "$HERE")
export COMPAT_OUT=$OFFICE/target/compat
headless=false
nested=false
apps=""
yes=false
while (($#)); do
	case $1 in
	--headless) headless=true ;;
	--nested) nested=true ;;
	--apps) apps=${2:?}; shift ;;
	--out) COMPAT_OUT=${2:?}; shift ;;
	--bin) export NKW_BIN=${2:?}; shift ;;
	--yes) yes=true ;;
	-h | --help) sed -n '2,8p' "$0"; exit 0 ;;
	*) echo "world-compat: opção desconhecida: $1" >&2; exit 2 ;;
	esac
	shift
done
$headless || $nested || headless=true
mkdir -p "$COMPAT_OUT"
export PATH="$HOME/.cargo/bin:$PATH" CARGO_INCREMENTAL=0

if [ -z "${NKW_BIN:-}" ]; then
	(cd "$OFFICE" && cargo build -q -p nkw -p nkw-testclient)
	export NKW_BIN=$OFFICE/target/debug/nkw
fi
bash "$HERE/compat/globals.sh" --bin "$NKW_BIN" --out "$COMPAT_OUT"

if $headless; then
	tests=(--test compat_protocols)
	[ "${NKW_E2E:-}" = 1 ] && tests+=(--test compat_apps)
	(cd "$OFFICE" && cargo test -q -p nkw "${tests[@]}")
fi

if $nested; then
	if ! $yes; then
		echo "vai abrir janelas na sua sessão por até 2 min por app (perfis descartáveis); enter para seguir, ctrl+c para parar"
		read -r _
	fi
	rm -rf "$COMPAT_OUT/apps"
	# shellcheck disable=SC2086
	bash "$HERE/compat/apps.sh" --nested --bin "$NKW_BIN" --out "$COMPAT_OUT" $apps
	# the fractional column comes from headless runs at 1,25 (no extra windows)
	# shellcheck disable=SC2086
	bash "$HERE/compat/apps.sh" --scale 1.25 --bin "$NKW_BIN" --out "$COMPAT_OUT" \
		${apps:-foot kitty weston-terminal gtk4-widget-factory firefox chromium mpv}
	bash "$HERE/compat/matrix.sh" --out "$COMPAT_OUT"
fi
