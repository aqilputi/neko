#!/usr/bin/env bash
# Plays one script in `nkw --demo` (headless by default) with the compat rules: own runtime dir,
# own process group, a hard timeout, everything killed at the end.
# `{run}` in the script is replaced by the run directory (kept: --keep DIR, else removed).
# Usage: run-nks.sh SCRIPT [--nested] [--bin PATH] [--keep DIR] [--secs N] [-- NKW_ARGS...]
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

nks=${1:?script}
shift
nested=false
keep=""
secs=60
extra=()
while (($#)); do
	case $1 in
	--nested) nested=true ;;
	--bin) export NKW_BIN=${2:?}; shift ;;
	--keep) keep=${2:?}; shift ;;
	--secs) secs=${2:?}; shift ;;
	--) shift; extra=("$@"); break ;;
	*) echo "run-nks: opção desconhecida: $1" >&2; exit 2 ;;
	esac
	shift
done
# rule 4: a nested run never lasts more than two minutes
if $nested && [ "$secs" -gt 120 ]; then secs=120; fi
bin=$(compat_bin)
if [ -n "$keep" ]; then
	run=$keep
	mkdir -p "$run"
else
	run=$(compat_tmp)
fi
xdg=$(compat_tmp)
chmod 700 "$xdg"
sed "s|{run}|$run|g" "$nks" >"$run/script.nks"

# headless: the script's `quit` ends the run (`--seconds` would keep it up); nested: rule 4 caps
# it with --seconds as well (the script's own length plus a margin, ≤ 120 s)
args=(--demo --socket nkw-audit --script "$run/script.nks")
if $nested; then
	args+=(--size 1280x800 --seconds "$secs")
else
	args+=(--headless)
	# a nested run keeps the user's WAYLAND_DISPLAY (the host); headless needs none
fi
# (env's options must come before its assignments)
if $nested; then
	# the host socket lives in the real runtime dir, and Smithay binds ours next to it
	env_cmd=(env -u DISPLAY XKB_DEFAULT_LAYOUT=us XKB_DEFAULT_VARIANT=intl)
else
	env_cmd=(env -u DISPLAY -u WAYLAND_DISPLAY XDG_RUNTIME_DIR="$xdg"
		XKB_DEFAULT_LAYOUT=us XKB_DEFAULT_VARIANT=intl)
fi
set +e
setsid "${env_cmd[@]}" timeout -k 2 "$((secs + 5))" "$bin" "${args[@]}" "${extra[@]}" \
	>"$run/nkw.out" 2>"$run/nkw.err" &
pid=$!
COMPAT_PGIDS+=("$pid")
wait "$pid"
code=$?
set -e
echo "$code" >"$run/exit"
echo "run-nks: $(basename "$nks") → exit $code ($run)"
