#!/usr/bin/env bash
# A stand-in for nkw that speaks contract S2 well enough to test world-perf.sh: takes nkw's
# flags, plays the --script (wait/key/text/spawn/quit), prints NKW_STATS lines on stderr and
# writes the key events a spawned `nkw-testclient --log PATH` would log. Knobs (env):
#   FAKE_NKW_TICK_MS     idle loop period, one wakeup per tick (2000); script deadlines wake it earlier
#   FAKE_NKW_BURN        % of each tick spent busy (0)
#   FAKE_NKW_P50_US / FAKE_NKW_P95_US   summary frame times (600 / 1200)
#   FAKE_NKW_DROPPED     summary dropped frames (0)
#   FAKE_NKW_RSS_KB      summary RSS (61440)
#   FAKE_NKW_FIRST_MS    summary first frame (120)
#   FAKE_NKW_EXT_US      summary ext_tick_us_p95 (40)
#   FAKE_NKW_RATES       summary rate_wishes_max (1)
#   FAKE_NKW_DRAWS / FAKE_NKW_TRIS      per-frame counters (40 / 20000)
#   FAKE_NKW_KEY_LAT_US  injected key → client log delay (300)
#   FAKE_NKW_EXIT        exit code (0)
#   FAKE_NKW_NO_SUMMARY  1 = die without the summary line
set -euo pipefail

seconds=0
script=""
while (($#)); do
	case $1 in
	--seconds) seconds=$2; shift ;;
	--script) script=$2; shift ;;
	--size | --scale | --socket | --shot) shift ;;
	--) break ;;
	esac
	shift
done

tick=${FAKE_NKW_TICK_MS:-2000}
burn=${FAKE_NKW_BURN:-0}
lat=${FAKE_NKW_KEY_LAT_US:-300}
draws=${FAKE_NKW_DRAWS:-40}
tris=${FAKE_NKW_TRIS:-20000}

lines=()
if [ -n "$script" ]; then
	if [ "$script" = - ]; then mapfile -t lines; else mapfile -t lines <"$script"; fi
fi

# A read with a timeout on a pipe nobody writes: sleeping without forking.
exec {sleep_fd}<> <(:)

now=${EPOCHREALTIME/[.,]/}
start=$now
end=$((start + seconds * 1000000))
due=$start
cursor=0
frames=0
focus=false
clients=()

inject() { # key name
	local k=$1 c
	now=${EPOCHREALTIME/[.,]/}
	printf 'nkw-stats {"inject":{"key":"%s","t_us":%d}}\n' "$k" "$now" >&2
	# NAV keys stay in the world; in FOCO they reach the focused client.
	if [ "$focus" = true ]; then
		for c in "${clients[@]}"; do
			printf '{"ev":"key","keycode":30,"sym":"%s","state":"pressed","t_us":%d}\n' "$k" $((now + lat)) >>"$c"
			printf '{"ev":"key","keycode":30,"sym":"%s","state":"released","t_us":%d}\n' "$k" $((now + lat + 50)) >>"$c"
		done
	fi
	[ "$k" = Return ] && focus=true
	return 0
}

step() { # one script line
	local l=$1 w a i t
	read -r w a <<<"$l"
	case $w in
	'' | '#'*) ;;
	wait) due=$((due + a * 1000)) ;;
	key) inject "$a" ;;
	text)
		t=${a#\"}
		t=${t%\"}
		for ((i = 0; i < ${#t}; i++)); do inject "${t:i:1}"; done
		;;
	spawn)
		# shellcheck disable=SC2086 # split the command line into words
		set -- $a
		while (($#)); do
			if [ "$1" = --log ] && [ $# -gt 1 ]; then
				clients+=("$2")
				printf '{"ev":"configure","w":640,"h":480,"states":[],"t_us":%d}\n' "$now" >"$2"
			fi
			shift
		done
		;;
	quit) end=$now ;;
	esac
}

while ((now < end)); do
	now=${EPOCHREALTIME/[.,]/}
	while ((cursor < ${#lines[@]} && now >= due)); do
		step "${lines[cursor]}"
		cursor=$((cursor + 1))
	done
	frames=$((frames + 1))
	printf 'nkw-stats {"t_us":%d,"frame":%d,"pass":"world","update_us":50,"cull_us":10,"submit_us":200,"compose_us":0,"draws":%d,"tris":%d,"gpu_us":null}\n' \
		"$now" "$frames" "$draws" "$tris" >&2
	busy_until=$((now + tick * burn * 10))
	while ((now < busy_until)); do now=${EPOCHREALTIME/[.,]/}; done
	rest=$((tick * (100 - burn) / 100))
	# Like an event loop: wake for the next script deadline if it comes first.
	if ((cursor < ${#lines[@]})); then
		next=$(((due - now) / 1000))
		((next < 1)) && next=1
		((next < rest)) && rest=$next
	fi
	if ((rest > 0)); then
		printf -v tmo '%d.%03d' $((rest / 1000)) $((rest % 1000))
		read -r -t "$tmo" -u "$sleep_fd" _ || true
	fi
	now=${EPOCHREALTIME/[.,]/}
done

if [ "${FAKE_NKW_NO_SUMMARY:-0}" != 1 ]; then
	printf 'nkw-stats {"summary":true,"frames":%d,"world_passes":%d,"p50_us":%d,"p95_us":%d,"dropped":%d,"first_frame_ms":%d,"rss_kb":%d,"ext_tick_us_p95":%d,"rate_wishes_max":%d}\n' \
		"$frames" "$frames" "${FAKE_NKW_P50_US:-600}" "${FAKE_NKW_P95_US:-1200}" "${FAKE_NKW_DROPPED:-0}" \
		"${FAKE_NKW_FIRST_MS:-120}" "${FAKE_NKW_RSS_KB:-61440}" "${FAKE_NKW_EXT_US:-40}" "${FAKE_NKW_RATES:-1}" >&2
fi
exit "${FAKE_NKW_EXIT:-0}"
