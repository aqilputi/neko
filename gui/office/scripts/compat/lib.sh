# Shared by scripts/compat/*.sh and scripts/world-compat.sh (sourced, not run).
# Every run gets its own XDG_RUNTIME_DIR-free temp dir, a process group of its own (setsid) and a
# trap that kills the whole group and removes the temp dirs: nothing started here outlives it.

COMPAT_HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
OFFICE=$(cd "$COMPAT_HERE/../.." && pwd)
COMPAT_OUT=${COMPAT_OUT:-$OFFICE/target/compat}

# The nkw binary: $NKW_BIN, else the world profile, else the dev build.
compat_bin() {
	if [ -n "${NKW_BIN:-}" ]; then echo "$NKW_BIN"; return; fi
	local b
	for b in "$OFFICE/target/world/nkw" "$OFFICE/target/debug/nkw"; do
		[ -x "$b" ] && { echo "$b"; return; }
	done
	echo "world-compat: nkw não compilado (cargo build -p nkw -p nkw-testclient)" >&2
	return 1
}

# The testclient next to the nkw binary.
compat_testclient() {
	local d
	d=$(dirname "$1")
	[ -x "$d/nkw-testclient" ] && { echo "$d/nkw-testclient"; return; }
	echo "world-compat: $d/nkw-testclient não existe" >&2
	return 1
}

COMPAT_PGIDS=()
COMPAT_TMPS=()

# Kill every process group we started (TERM, then KILL) and remove our temp dirs.
compat_cleanup() {
	local g
	for g in "${COMPAT_PGIDS[@]:-}"; do
		[ -n "$g" ] && kill -TERM -- "-$g" 2>/dev/null || true
	done
	sleep 0.5
	for g in "${COMPAT_PGIDS[@]:-}"; do
		[ -n "$g" ] && kill -KILL -- "-$g" 2>/dev/null || true
	done
	COMPAT_PGIDS=()
	local t
	for t in "${COMPAT_TMPS[@]:-}"; do
		[ -n "$t" ] && [ -d "$t" ] && rm -rf -- "$t"
	done
	COMPAT_TMPS=()
}
trap compat_cleanup EXIT INT TERM

compat_tmp() {
	local t
	t=$(mktemp -d "${TMPDIR:-/tmp}/nkw-compat.XXXXXX")
	COMPAT_TMPS+=("$t")
	echo "$t"
}

# Wait until a socket exists (≤ $2 s).
compat_wait_socket() {
	local i
	for ((i = 0; i < ${2:-5} * 20; i++)); do
		[ -S "$1" ] && return 0
		sleep 0.05
	done
	return 1
}

# A testclient JSON-lines log of `--list-globals` → a sorted JSON array [{name, version}].
compat_globals_json() {
	jq -s '[.[] | select(.ev == "global") | {name, version}] | sort_by(.name)' "$1"
}
