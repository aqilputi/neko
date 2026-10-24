#!/usr/bin/env bash
# Measures nkw against the budgets of docs/plano-mundo.md §1.3 (scenarios S1 to S7, plus S3r:
# S3 under --reduced-motion; S4d = S4 with full-repaint clients, reported; S6 = the library on the
# 50k-entry fixture; S7 = the phone open, browsing). Drives nkw through --script, samples /proc over the window,
# parses the NKW_STATS lines, compares with world-perf-budget.toml and with the last recorded
# run (world-perf-last.json). Exit 1 on any failure, 2 on usage errors.
#
# Sourcing it (the tests do) defines the helpers without running main.

usage() {
	cat <<'EOF'
uso: world-perf.sh [--bin PATH] [--headless] [--scenario S1[,S2,...]] [--seconds N] [--json] [--record]
                   [--warmup MS] [--testclient PATH] [--budget FILE] [--last FILE] [--scenarios DIR] [--keep]
  --bin         o nkw (padrão: target/world/nkw)
  --headless    backend surfaceless: só regressão relativa, exceto contadores (plano §1.4)
  --scenario    cenários, separados por vírgula (padrão: S1,S2,S3,S3r,S4,S4d,S5,S6,S7)
  --seconds     janela de medida por cenário (padrão: 60)
  --warmup      espera antes da janela, em ms (padrão: o #@warmup do cenário)
  --json        o relatório em JSON na saída padrão (a tabela vai para a saída de erro)
  --record      grava as medidas em world-perf-last.json se tudo passou
  --keep        não apaga o diretório de cada execução (logs do nkw e dos clientes)
EOF
}

PERF_HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PERF_OFFICE=$(dirname "$PERF_HERE")
CLK_TCK=$(getconf CLK_TCK 2>/dev/null || echo 100)

# Wall clock in µs without forking (EPOCHREALTIME's separator follows the locale).
now_us() {
	local t=${EPOCHREALTIME/[.,]/}
	echo "$t"
}

sleep_ms() {
	sleep "$(awk -v ms="$1" 'BEGIN { printf "%.3f", ms / 1000 }')"
}

# proc_sample PID → "t_us ticks ctxt": utime+stime of the whole process (all threads) and
# Σ voluntary+nonvoluntary context switches over /proc/PID/task/*/status.
proc_sample() {
	local pid=$1 stat s k v ctxt=0 f
	{ stat=$(<"/proc/$pid/stat"); } 2>/dev/null || return 1
	# Fields after "comm)" start at field 3 (state); utime/stime are fields 14/15.
	read -r -a f <<<"${stat##*) }"
	for s in /proc/"$pid"/task/*/status; do
		while read -r k v _; do
			case $k in
			voluntary_ctxt_switches: | nonvoluntary_ctxt_switches:) ctxt=$((ctxt + v)) ;;
			esac
		done <"$s" || true
	done 2>/dev/null
	echo "$(now_us) $((f[11] + f[12])) $ctxt"
}

# proc_rates "SAMPLE0" "SAMPLE1" → "cpu_pct wakeups_per_s" (cpu in % of one core).
proc_rates() {
	local a b
	read -r -a a <<<"$1"
	read -r -a b <<<"$2"
	awk -v dt="$((b[0] - a[0]))" -v dk="$((b[1] - a[1]))" -v dc="$((b[2] - a[2]))" -v hz="$CLK_TCK" \
		'BEGIN {
			if (dt <= 0) { print "0 0"; exit }
			s = dt / 1e6
			if (dc < 0) dc = 0 # a thread that exited takes its switches with it
			printf "%.4f %.3f\n", dk / hz / s * 100, dc / s
		}'
}

# stats_json STDERR [CLIENT_LOG...] → the metrics of one run as JSON. Key latency pairs each
# injected key with the first later "pressed" of the same sym in the clients' logs (< 1 s);
# injected keys no client got (NAV keys) are not latency samples.
stats_json() {
	local stderr=$1 keys=""
	shift
	# No logs must not mean "read stdin".
	(($#)) && keys=$(cat -- "$@" 2>/dev/null </dev/null |
		jq -Rc 'fromjson? | select(.ev == "key" and .state == "pressed") | {sym, t_us}' |
		jq -sc 'sort_by(.t_us)') || true
	[ -n "$keys" ] || keys='[]'
	sed -n 's/^.*nkw-stats //p' "$stderr" | jq -Rs --argjson keys "$keys" '
		def pct(p): if length == 0 then null else sort | .[((length - 1) * p | floor)] end;
		def ms: if . == null then null else . / 1000 end;
		[split("\n")[] | fromjson? | select(type == "object")] as $l
		| ($l | map(select(.summary == true)) | last) as $s
		| ($l | map(select(.pass != null))) as $f
		| def stage(p): {update: ($f | map(.update_us | numbers) | pct(p) | ms),
			import: ($f | map(.import_us | numbers) | pct(p) | ms),
			cull: ($f | map(.cull_us | numbers) | pct(p) | ms),
			submit: ($f | map(.submit_us | numbers) | pct(p) | ms),
			compose: ($f | map(.compose_us | numbers) | pct(p) | ms),
			hud: ($f | map(.hud_us | numbers) | pct(p) | ms)};
		($l | map(select(.inject != null) | .inject) | sort_by(.t_us)) as $inj
		| (reduce $inj[] as $i ({used: {}, lat: []};
			. as $st
			| ([$keys | to_entries[]
				| select(.value.sym == $i.key and .value.t_us >= $i.t_us
					and (.value.t_us - $i.t_us) < 1000000
					and ($st.used[.key | tostring] | not))] | first) as $m
			| if $m == null then . else .used[$m.key | tostring] = true | .lat += [$m.value.t_us - $i.t_us] end)
		  ) as $match
		| {
			summary: ($s != null),
			frames: $s.frames,
			world_passes: $s.world_passes,
			frame_p50_ms: ($s.p50_us | ms),
			frame_p95_ms: ($s.p95_us | ms),
			dropped: $s.dropped,
			first_frame_ms: $s.first_frame_ms,
			rss_mb: (if $s.rss_kb == null then null else ($s.rss_kb / 1024 * 10 | round) / 10 end),
			ext_tick_p95_ms: ($s.ext_tick_us_p95 | ms),
			rate_wishes_max: $s.rate_wishes_max,
			compose_hud_changes: $s.compose_hud_changes,
			look_events: $s.look_events,
			import_p50_ms: ($s.import_us_p50 | ms),
			compose_p50_ms: ($s.compose_us_p50 | ms),
			ext_plugin_p95_ms: (if $s.ext_plugin_us_p95 == null then null
				else $s.ext_plugin_us_p95 | map_values(. / 1000) end),
			stage_p50_ms: stage(0.5),
			stage_p95_ms: stage(0.95),
			slow_frames: ($f | map(select((.update_us + (.import_us // 0) + .cull_us + .submit_us + .compose_us) > 2000)) | length),
			draws_max: ($f | map(.draws | numbers) | max),
			tris_max: ($f | map(.tris | numbers) | max),
			gpu_ms: ($f | map(.gpu_us | numbers) | pct(0.95) | ms),
			keys_injected: ($inj | length),
			keys_delivered: ($match.lat | length),
			key_latency_p95_ms: ($match.lat | pct(0.95) | ms),
			key_latency_max_ms: ($match.lat | max | ms)
		}'
}

# budget_json FILE → [{id, metric, scenarios, target, fail, gated, noise}] from the TOML subset.
budget_json() {
	awk '
		function flush() {
			if (id == "") return
			printf "%s\t%s\t%s\t%s\t%s\t%s\t%s\n", id, metric, sc, target, fail, gated, noise
		}
		/^[ \t]*(#|$)/ { next }
		/^\[/ { flush(); id = $0; gsub(/[][ \t]/, "", id); metric = sc = target = fail = ""; gated = "true"; noise = "0"; next }
		/=/ {
			k = $0; sub(/=.*/, "", k); gsub(/[ \t]/, "", k)
			v = $0; sub(/^[^=]*=/, "", v); sub(/[ \t]#.*$/, "", v); gsub(/^[ \t]+|[ \t]+$/, "", v); gsub(/"/, "", v)
			if (id == "") next
			if (k == "metric") metric = v
			else if (k == "scenarios") { gsub(/[][ \t]/, "", v); sc = v }
			else if (k == "target") target = v
			else if (k == "fail") fail = v
			else if (k == "gated") gated = v
			else if (k == "noise") noise = v
		}
		END { flush() }' "$1" |
		jq -Rsc '[split("\n")[] | select(length > 0) | split("\t") | {
			id: .[0], metric: .[1],
			scenarios: (if .[2] == "" then [] else (.[2] | split(",")) end),
			target: (.[3] | tonumber? // null), fail: (.[4] | tonumber? // null),
			gated: (.[5] != "false"), noise: (.[6] | tonumber? // 0)
		}]'
}

budget_regression_pct() {
	awk '/^\[/ { exit } /^[ \t]*regression_pct[ \t]*=/ { sub(/^[^=]*=/, ""); gsub(/[ \t]/, ""); print; found = 1 }
		END { if (!found) print 20 }' "$1"
}

# evaluate SCENARIO METRICS_JSON ROWS_JSON LAST_METRICS_JSON HEADLESS(true|false) PCT → checks JSON
evaluate() {
	jq -nc --arg sc "$1" --argjson m "$2" --argjson rows "$3" --argjson last "$4" \
		--argjson headless "$5" --argjson pct "$6" '
		def num: if type == "number" then . else null end;
		[$rows[] | select(any(.scenarios[]; . == $sc)) | . as $r
		| ($m[$r.metric] | num) as $v
		| (($last // {})[$r.metric] | num) as $l
		| (if $v == null then "missing"
		   elif $r.fail != null and $v > $r.fail then "fail"
		   elif $r.target != null and $v > $r.target then "warn"
		   else "ok" end) as $abs
		| (if $headless and $abs == "fail" and (["draws_max", "tris_max", "binary_mb"] | index([$r.metric]) | not)
		   then "warn" else $abs end) as $abs
		| (if $v != null and $l != null and ($v - $l) > ([$r.noise, $l * $pct / 100] | max)
		   then (if $l == 0 then null else (($v - $l) / $l * 100 | round) end) // "new" else null end) as $reg
		| {row: $r.id, metric: $r.metric, value: $v, target: $r.target, fail: $r.fail,
		   gated: $r.gated, last: $l, regression_pct: $reg,
		   status: (if ($r.gated | not) then (if $v == null then "n/a" else "info" end)
		            elif $abs == "fail" or $abs == "missing" or $reg != null then "fail"
		            else $abs end)}]'
}

# One table line: "S1   fail  cpu_pct=0.62!(0.1/0.5)  wakeups_per_s=0.8(1/5) ..."
table_line() {
	jq -r --arg sc "$1" '
		def f: if . == null then "-" elif type == "number" then (. * 1000 | round / 1000 | tostring) else tostring end;
		def mark: if .status == "fail" then "!" elif .status == "warn" then "~" else "" end;
		([.checks[] | "\(.metric)=\(.value | f)\(mark)(\(.target | f)/\(.fail | f))"
			+ (if .regression_pct != null then "^\(.regression_pct)%" else "" end)]
		 + (if .error then ["erro: \(.error)"] else [] end)) as $cells
		| "\($sc | . + "    " | .[0:4]) \(.status | . + "     " | .[0:5]) \($cells | join("  "))"'
}

# expand_script FILE RUN_DIR WINDOW_MS TESTCLIENT → the script nkw runs: ${RUN}/${TESTCLIENT}
# substituted, "#@every MS A | B" unrolled into A, wait MS, B, wait MS, ... over the window.
expand_script() {
	awk -v run="$2" -v win="$3" -v tc="$4" '
		{ gsub(/\$\{RUN\}/, run); gsub(/\$\{TESTCLIENT\}/, tc) }
		/^#@every / {
			ms = $2; rest = $0; sub(/^#@every[ \t]+[0-9]+[ \t]+/, "", rest)
			n = split(rest, cmds, /[ \t]*\|[ \t]*/)
			count = int(win / ms) + 1
			for (i = 0; i < count; i++) { print cmds[(i % n) + 1]; print "wait " ms }
			next
		}
		/^#@/ { next }
		{ print }' "$1"
}

# run_scenario SC → metrics JSON (with "error" when the run itself broke).
run_scenario() {
	local sc=$1 file="$SCENARIO_DIR/$1.nks" run warm extra=() more line pid s0 s1 rc=0 rates stats total err=""
	if [ ! -f "$file" ]; then
		jq -nc --arg e "sem roteiro: $file" '{error: $e}'
		return
	fi
	warm=2000
	while IFS= read -r line; do
		case $line in
		'#@args '*)
			read -r -a more <<<"${line#\#@args }"
			extra+=("${more[@]}")
			;;
		'#@warmup '*) warm=${line#\#@warmup } ;;
		esac
	done <"$file"
	[ -n "$WARMUP_MS" ] && warm=$WARMUP_MS
	run=$(mktemp -d "${TMPDIR:-/tmp}/nkw-perf-$sc.XXXXXX")
	expand_script "$file" "$run" $((warm + SECONDS_WINDOW * 1000)) "$TESTCLIENT" >"$run/script.nks"
	total=$(((warm + 999) / 1000 + SECONDS_WINDOW + 1))
	[ "$HEADLESS" = true ] && extra=(--headless "${extra[@]}")
	# per-plugin ticks too; the warm-up does not count in the percentiles
	NKW_STATS=1,ext NKW_STATS_WARMUP_MS=$warm "$BIN" --demo "${extra[@]}" --socket "nkw-perf-$$-$sc" --seconds "$total" \
		--script "$run/script.nks" >"$run/stdout" 2>"$run/stderr" &
	pid=$!
	sleep_ms "$warm"
	if s0=$(proc_sample "$pid"); then
		sleep "$SECONDS_WINDOW"
		s1=$(proc_sample "$pid") || err="o nkw saiu antes do fim da janela"
	else
		err="o nkw saiu antes da janela"
	fi
	wait "$pid" || rc=$?
	if [ -z "$err" ]; then rates=$(proc_rates "$s0" "$s1"); else rates="null null"; fi
	[ "$rc" -ne 0 ] && err="${err:+$err; }o nkw saiu com $rc: $(grep -v '^nkw-stats ' "$run/stderr" | tail -1)"
	stats=$(stats_json "$run/stderr" "$run"/client*.json)
	jq -nc --argjson st "$stats" --argjson cpu "${rates% *}" --argjson wk "${rates#* }" \
		--argjson rc "$rc" --arg err "$err" --arg run "$run" --argjson keep "$KEEP" '
		$st + {cpu_pct: $cpu, wakeups_per_s: $wk, exit_code: $rc}
		+ (if $err != "" then {error: $err} elif $st.summary | not then {error: "sem resumo do NKW_STATS"} else {} end)
		+ (if $keep then {run_dir: $run} else {} end)'
	[ "$KEEP" = true ] || rm -rf "$run"
}

main() {
	set -euo pipefail
	BIN="$PERF_OFFICE/target/world/nkw"
	HEADLESS=false
	SCENARIOS="S1,S2,S3,S3r,S4,S4d,S5,S6,S7"
	SECONDS_WINDOW=60
	WARMUP_MS=""
	JSON=false
	RECORD=false
	KEEP=false
	TESTCLIENT=""
	BUDGET="$PERF_HERE/world-perf-budget.toml"
	LAST="$PERF_HERE/world-perf-last.json"
	SCENARIO_DIR="$PERF_HERE/scenarios"
	while (($#)); do
		case $1 in
		--bin) BIN=${2:?}; shift ;;
		--headless) HEADLESS=true ;;
		--scenario) SCENARIOS=${2:?}; shift ;;
		--seconds) SECONDS_WINDOW=${2:?}; shift ;;
		--warmup) WARMUP_MS=${2:?}; shift ;;
		--json) JSON=true ;;
		--record) RECORD=true ;;
		--keep) KEEP=true ;;
		--testclient) TESTCLIENT=${2:?}; shift ;;
		--budget) BUDGET=${2:?}; shift ;;
		--last) LAST=${2:?}; shift ;;
		--scenarios) SCENARIO_DIR=${2:?}; shift ;;
		-h | --help) usage; return 0 ;;
		*) echo "world-perf: opção desconhecida: $1" >&2; usage >&2; return 2 ;;
		esac
		shift
	done
	[[ $SECONDS_WINDOW =~ ^[1-9][0-9]*$ ]] || { echo "world-perf: --seconds quer um inteiro > 0" >&2; return 2; }
	[[ -z $WARMUP_MS || $WARMUP_MS =~ ^[0-9]+$ ]] || { echo "world-perf: --warmup quer ms" >&2; return 2; }
	command -v jq >/dev/null || { echo "world-perf: precisa do jq" >&2; return 2; }
	[ -x "$BIN" ] || { echo "world-perf: nkw não encontrado em $BIN (make world-build)" >&2; return 2; }
	[ -n "$TESTCLIENT" ] || TESTCLIENT="$(dirname "$BIN")/nkw-testclient"

	local rows pct mode last report status table out=1 sc metrics checks
	rows=$(budget_json "$BUDGET")
	pct=$(budget_regression_pct "$BUDGET")
	mode=$([ "$HEADLESS" = true ] && echo headless || echo winit)
	if [ -f "$LAST" ]; then last=$(jq -c --arg m "$mode" '.runs[$m] // {}' "$LAST"); else last='{}'; fi
	[ "$JSON" = true ] && out=2

	report=$(jq -nc --arg mode "$mode" --argjson s "$SECONDS_WINDOW" \
		'{version: 1, mode: $mode, seconds: $s, scenarios: {}}')
	local size
	size=$(stat -L -c %s "$BIN")
	for sc in bin ${SCENARIOS//,/ }; do
		if [ "$sc" = bin ]; then
			metrics=$(jq -nc --argjson b "$size" '{binary_mb: ($b / 1048576 * 1000 | round / 1000)}')
		else
			metrics=$(run_scenario "$sc")
		fi
		checks=$(evaluate "$sc" "$metrics" "$rows" "$(jq -c --arg sc "$sc" '.[$sc] // {}' <<<"$last")" "$HEADLESS" "$pct")
		report=$(jq -c --arg sc "$sc" --argjson m "$metrics" --argjson c "$checks" '
			.scenarios[$sc] = {metrics: $m, checks: $c, error: $m.error,
				status: (if $m.error != null or any($c[]; .status == "fail") then "fail"
				         elif any($c[]; .status == "warn") then "warn" else "pass" end)}' <<<"$report")
		table_line "$sc" <<<"$(jq -c --arg sc "$sc" '.scenarios[$sc]' <<<"$report")" >&$out
	done
	report=$(jq -c '.status = (if any(.scenarios[]; .status == "fail") then "fail" else "pass" end)' <<<"$report")
	status=$(jq -r .status <<<"$report")
	[ "$JSON" = true ] && jq . <<<"$report"

	if [ "$RECORD" = true ]; then
		if [ "$status" = pass ]; then
			local prev='{}'
			[ -f "$LAST" ] && prev=$(cat "$LAST")
			jq --arg m "$mode" --argjson r "$report" '
				.version = 1
				| .runs[$m] = ((.runs[$m] // {}) + ($r.scenarios | map_values(.metrics
					| with_entries(select(.value | type == "number")))))' <<<"$prev" >"$LAST.tmp"
			mv "$LAST.tmp" "$LAST"
			echo "registrado em $LAST ($mode)" >&$out
		else
			echo "não registrado: houve falha" >&$out
		fi
	fi
	[ "$status" = pass ]
}

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
	main "$@"
fi
