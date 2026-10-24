#!/usr/bin/env bash
# Tests world-perf.sh against fake-nkw.sh: the /proc arithmetic, NKW_STATS parsing, the
# budget file, threshold and regression failures, --record and the --json shape.
set -euo pipefail

HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=world-perf.sh
source "$HERE/world-perf.sh"

TMP=$(mktemp -d "${TMPDIR:-/tmp}/test-world-perf.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
pass=0
failures=0

ok() { pass=$((pass + 1)); }
bad() {
	failures=$((failures + 1))
	echo "FALHOU: $*" >&2
}
check() { # description, then a command that must succeed
	local d=$1
	shift
	if "$@"; then ok; else bad "$d"; fi
}
jqe() { jq -e "$@" >/dev/null; }

# perf ARGS... → runs world-perf.sh on the fake with a 1 s window; sets RC and OUT (the JSON).
perf() {
	RC=0
	OUT=$("$HERE/world-perf.sh" --bin "$HERE/fake-nkw.sh" --seconds 1 --warmup 200 \
		--last "$TMP/last.json" --json "$@" 2>"$TMP/table") || RC=$?
}

# ---- /proc arithmetic ----
CLK_TCK=100
check "50 ticks em 1 s a 100 Hz = 50 %" test "$(proc_rates "1000000 10 100" "2000000 60 110")" = "50.0000 10.000"
check "janela vazia não divide por zero" test "$(proc_rates "5 1 1" "5 2 2")" = "0 0"
check "switches de threads mortas não ficam negativos" test "$(proc_rates "0 0 50" "1000000 0 10")" = "0.0000 0.000"
CLK_TCK=$(getconf CLK_TCK)

FAKE_NKW_BURN=50 FAKE_NKW_TICK_MS=50 "$HERE/fake-nkw.sh" --seconds 3 2>/dev/null &
fake=$!
sleep 0.3
s0=$(proc_sample "$fake")
sleep 1
s1=$(proc_sample "$fake")
read -r cpu wk <<<"$(proc_rates "$s0" "$s1")"
kill "$fake" 2>/dev/null || true
wait "$fake" 2>/dev/null || true
check "amostra do /proc vê o fake queimando CPU (cpu=$cpu)" awk -v c="$cpu" 'BEGIN { exit !(c > 10 && c < 110) }'
check "amostra do /proc conta wakeups (wk=$wk; $s0 → $s1)" awk -v w="$wk" 'BEGIN { exit !(w >= 5 && w < 5000) }'
check "processo morto não amostra" bash -c "! (source '$HERE/world-perf.sh'; proc_sample $fake) 2>/dev/null"

# ---- NKW_STATS parsing ----
cat >"$TMP/stderr" <<'EOF'
uma linha qualquer do nkw
nkw-stats {"t_us":10,"frame":1,"pass":"world","update_us":1,"import_us":2100,"cull_us":1,"submit_us":1,"compose_us":0,"draws":12,"tris":900,"gpu_us":null}
nkw-stats {"t_us":20,"frame":2,"pass":"compose","update_us":1,"cull_us":1,"submit_us":1,"compose_us":5,"draws":30,"tris":100,"gpu_us":1500}
nkw-stats {"t_us":30,"frame":3,"pass":"world","update_us":2500,"cull_us":1,"submit_us":1,"compose_us":0,"draws":5,"tris":50,"gpu_us":null}
nkw-stats {"inject":{"key":"Return","t_us":1000}}
nkw-stats {"inject":{"key":"a","t_us":2000}}
nkw-stats {"inject":{"key":"a","t_us":3000}}
nkw-stats quebrado {
nkw-stats {"summary":true,"frames":2,"world_passes":1,"p50_us":800,"p95_us":2500,"dropped":1,"first_frame_ms":140,"rss_kb":51200,"compose_hud_changes":3,"look_events":7,"ext_plugin_us_p95":{"cats":1500,"wm":10},"import_us_p50":300,"compose_us_p50":450}
EOF
cat >"$TMP/client.json" <<'EOF'
{"ev":"configure","w":640,"h":480,"states":[],"t_us":5}
{"ev":"key","keycode":30,"sym":"a","state":"pressed","t_us":2300}
{"ev":"key","keycode":30,"sym":"a","state":"released","t_us":2400}
{"ev":"key","keycode":30,"sym":"a","state":"pressed","t_us":3900}
EOF
st=$(stats_json "$TMP/stderr" "$TMP/client.json")
check "resumo lido" jqe '.summary and .frames == 2 and .frame_p50_ms == 0.8 and .frame_p95_ms == 2.5' <<<"$st"
check "dropped/first/rss" jqe '.dropped == 1 and .first_frame_ms == 140 and .rss_mb == 50' <<<"$st"
check "máximos por quadro" jqe '.draws_max == 30 and .tris_max == 900 and .gpu_ms == 1.5' <<<"$st"
check "latência casa tecla com tecla (Return fica no mundo)" \
	jqe '.keys_injected == 3 and .keys_delivered == 2 and .key_latency_max_ms == 0.9 and .key_latency_p95_ms == 0.3' <<<"$st"
check "divisão por estágio e quadros lentos" \
	jqe '.stage_p95_ms.update == 0.001 and .stage_p50_ms.compose == 0 and .stage_p95_ms.compose == 0 and .slow_frames == 2' <<<"$st"
check "importação separada do cull (onda 4)" \
	jqe '.stage_p95_ms.import == 2.1 and .import_p50_ms == 0.3 and .compose_p50_ms == 0.45' <<<"$st"
check "campos novos do resumo (S14)" \
	jqe '.compose_hud_changes == 3 and .look_events == 7 and .ext_plugin_p95_ms.cats == 1.5 and .ext_plugin_p95_ms.wm == 0.01' <<<"$st"
check "sem resumo: summary false" jqe '.summary == false and .frame_p95_ms == null' <<<"$(stats_json /dev/null)"

# ---- budget file mirrors plan §1.3 ----
rows=$(budget_json "$HERE/world-perf-budget.toml")
row() { jq -c --arg id "$1" '.[] | select(.id == $id) | [.target, .fail, .gated]' <<<"$rows"; }
check "regression_pct = 20" test "$(budget_regression_pct "$HERE/world-perf-budget.toml")" = 20
check "S1 cpu 0,1/0,5" test "$(row cpu_idle)" = '[0.1,0.5,true]'
check "S1 wakeups 1/5" test "$(row wakeups_idle)" = '[1,5,true]'
check "S2 cpu 1/3" test "$(row cpu_typing)" = '[1,3,true]'
check "S3 cpu 3/6" test "$(row cpu_animated)" = '[3,6,true]'
check "quadro p50 1, p95 2/3" test "$(row frame_cpu_p50)$(row frame_cpu_p95)" = '[1,null,true][2,3,true]'
check "GPU 2/4 só relatado" test "$(row frame_gpu)" = '[2,4,false]'
check "tecla 1 ms / 1 quadro" test "$(row key_latency)" = '[1,16.7,true]'
check "RAM 90 e 110 / 150" test "$(row ram_empty)$(row ram_windows)" = '[90,150,false][110,150,true]'
check "RAM da biblioteca só relatada" test "$(row ram_library)" = '[140,null,false]'
check "composições por mudança no HUD só relatadas" test "$(row compose_hud_changes)" = '[null,null,false]'
check "binário 15/25" test "$(row binary)" = '[15,25,true]'
check "partida 250/500 e 1000/2000" test "$(row startup_first_frame)$(row startup_cats)" = '[250,500,true][1000,2000,false]'
check "build 240/480 e 15/30" test "$(row build_cold)$(row build_incremental)" = '[240,480,false][15,30,false]'
check "desenho 80/200, 100k" test "$(row draws)$(row triangles)" = '[80,200,true][100000,null,true]'
check "S1 vale em S1" jqe '.[] | select(.id == "cpu_idle") | .scenarios == ["S1"]' <<<"$rows"
check "p95 em S3 e S4" jqe '.[] | select(.id == "frame_cpu_p95") | .scenarios == ["S3","S4"]' <<<"$rows"
check "S4d (repintura inteira) só relatado" test "$(row frame_cpu_stress_p50)$(row frame_cpu_stress_p95)" = '[1,null,false][2,null,false]'
check "importação e composição só relatadas" test "$(row frame_import)$(row frame_compose)" = '[null,null,false][null,null,false]'
check "S7: composição do telefone 1,5 relatada" test "$(row phone_compose)" = '[1.5,null,false]'
check "S7: vontades Rate ≤ 4 barram" jqe '.[] | select(.id == "rate_wishes") | (.scenarios | index("S7")) != null and .fail == 4 and .gated' <<<"$rows"
check "S4d e S7 perdem quadros? só S4/S5 barram" jqe '.[] | select(.id == "camera_dropped") | .scenarios == ["S4","S5"] and .fail == 1' <<<"$rows"
# the strip's clock rides the clock object's minute frame: S1 (recorded, nested) did not wake more
last_s1=$(jq -c '.runs.winit.S1 // {}' "$HERE/world-perf-last.json")
check "S1 registrado: ≤ 1 wakeup/s e ≤ 0,1 % com a faixa (S1=$last_s1)" \
	jqe '.wakeups_per_s <= 1 and .cpu_pct <= 0.1' <<<"$last_s1"

# ---- script expansion ----
printf '%s\n' 'spawn ${TESTCLIENT} --log ${RUN}/c.json' '#@args --reduced-motion' '#@every 100 key l | key h' >"$TMP/x.nks"
exp=$(expand_script "$TMP/x.nks" /r 250 /bin/tc)
check "expansão de \${RUN}/#@every" test "$exp" = "$(printf '%s\n' 'spawn /bin/tc --log /r/c.json' 'key l' 'wait 100' 'key h' 'wait 100' 'key l' 'wait 100')"

# ---- end to end against the fake ----
perf --scenario S1,S2,S4
check "fake padrão passa (rc=$RC)" test "$RC" = 0
check "--json: forma" jqe '.version == 1 and .mode == "winit" and .seconds == 1 and .status == "pass"
	and (.scenarios | keys == ["S1","S2","S4","bin"])
	and (.scenarios.S1.metrics | has("cpu_pct") and has("wakeups_per_s") and has("rss_mb"))
	and (.scenarios.S1.checks | map(.row) | index("cpu_idle") != null)
	and (.scenarios.S2.metrics.key_latency_p95_ms | type == "number")
	and (.scenarios.S4.checks[] | select(.row == "frame_cpu_p95") | .value == 1.2 and .status == "ok")
	and (.scenarios.bin.metrics.binary_mb < 1)
	and (.scenarios.S4.metrics.import_p50_ms == 0.1 and .scenarios.S4.metrics.compose_p50_ms == 0.2)' <<<"$OUT"
check "tabela: uma linha por cenário" test "$(grep -cE '^(S1|S2|S4|bin) +pass' "$TMP/table")" = 4
check "campos novos do resumo (S12)" jqe '.scenarios.S1.metrics.ext_tick_p95_ms == 0.04 and .scenarios.S1.metrics.rate_wishes_max == 1' <<<"$OUT"

perf
check "padrão: S1 a S7, S3r e S4d (rc=$RC)" jqe '.scenarios | keys == ["S1","S2","S3","S3r","S4","S4d","S5","S6","S7","bin"]' <<<"$OUT"

FAKE_NKW_RATES=5 perf --scenario S1
check "mais de 4 vontades Rate falham" jqe '.scenarios.S1.checks[] | select(.row == "rate_wishes") | .status == "fail"' <<<"$OUT"

perf --scenario S1 --record
check "--record grava quando passa" jqe '.version == 1 and (.runs.winit.S1.cpu_pct | type == "number") and (.runs.winit.bin.binary_mb | type == "number")' "$TMP/last.json"

FAKE_NKW_BURN=50 perf --scenario S1
check "CPU acima do limite falha (rc=$RC)" test "$RC" = 1
check "a linha que falhou é cpu_idle" jqe '.scenarios.S1.checks[] | select(.row == "cpu_idle") | .status == "fail"' <<<"$OUT"

FAKE_NKW_TICK_MS=40 perf --scenario S1
check "wakeups acima do limite falham" jqe '.scenarios.S1.checks[] | select(.row == "wakeups_idle") | .status == "fail" and .value > 5' <<<"$OUT"

FAKE_NKW_KEY_LAT_US=30000 perf --scenario S2
check "latência de tecla > 1 quadro falha" jqe '.status == "fail" and (.scenarios.S2.checks[] | select(.row == "key_latency") | .status == "fail")' <<<"$OUT"

FAKE_NKW_DROPPED=2 perf --scenario S4
check "2 quadros perdidos falham" jqe '.scenarios.S4.checks[] | select(.row == "camera_dropped") | .status == "fail"' <<<"$OUT"

# regression: record a baseline for S4, then +58 % (still under the 3 ms limit) fails, +4 % passes.
rm -f "$TMP/last.json"
perf --scenario S4 --record
check "linha de base de S4" jqe '.runs.winit.S4.frame_p95_ms == 1.2' "$TMP/last.json"
before=$(cat "$TMP/last.json")
FAKE_NKW_P95_US=1900 perf --scenario S4 --record
check "regressão de 58 % falha (rc=$RC)" test "$RC" = 1
check "regressão marcada no p95" jqe '.scenarios.S4.checks[] | select(.row == "frame_cpu_p95") | .status == "fail" and .regression_pct == 58' <<<"$OUT"
check "--record não grava quando falha" test "$(cat "$TMP/last.json")" = "$before"
FAKE_NKW_P95_US=1250 perf --scenario S4
check "4 % fica no ruído (rc=$RC)" test "$RC" = 0

# headless: absolute time limits only warn; the counters still fail.
FAKE_NKW_P95_US=3500 perf --scenario S4
check "winit: p95 3,5 ms falha" test "$RC" = 1
FAKE_NKW_P95_US=3500 perf --scenario S4 --headless
check "headless: p95 3,5 ms só avisa (rc=$RC)" jqe '.mode == "headless" and .status == "pass" and
	(.scenarios.S4.checks[] | select(.row == "frame_cpu_p95") | .status == "warn")' <<<"$OUT"
FAKE_NKW_DRAWS=250 perf --scenario S4 --headless
check "headless: draws acima de 200 falham" test "$RC" = 1

FAKE_NKW_EXIT=3 perf --scenario S1
check "nkw saindo com 3 falha" jqe '.status == "fail" and (.scenarios.S1.error | test("saiu com 3"))' <<<"$OUT"
FAKE_NKW_NO_SUMMARY=1 perf --scenario S1
check "sem resumo falha" jqe '.status == "fail" and .scenarios.S1.error == "sem resumo do NKW_STATS"' <<<"$OUT"
perf --scenario S9
check "cenário sem roteiro falha" jqe '.scenarios.S9.error | test("sem roteiro")' <<<"$OUT"

perf --bogus
check "opção desconhecida sai com 2" test "$RC" = 2
perf --bin "$TMP/nada"
check "binário ausente sai com 2" test "$RC" = 2

echo "test-world-perf: $pass ok, $failures falhas"
[ "$failures" = 0 ]
