#!/usr/bin/env bash
# One real app inside `nkw --demo` (headless by default, --nested in a window of the session),
# driven by a script: map, FOCO, typing (dead keys), a popup/menu opened and closed by leaving FOCO,
# the world's selection offered to it, a second launch. The app's WAYLAND_DEBUG stream is reduced
# on the fly by wl-filter.sh (interface/message names and a few integers: never titles or text).
# Output: $COMPAT_OUT/apps/<case>[@<scale>].json (+ the run dir with --keep).
# Usage: app.sh CASE [--nested] [--scale S] [--bin PATH] [--out DIR] [--keep]
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
source "$COMPAT_HERE/cases.sh"

name=${1:?case (one of: $CASES_ALL)}
shift
nested=false
scale=""
keep=false
while (($#)); do
	case $1 in
	--nested) nested=true ;;
	--scale) scale=${2:?}; shift ;;
	--bin) export NKW_BIN=${2:?}; shift ;;
	--out) COMPAT_OUT=${2:?}; shift ;;
	--keep) keep=true ;;
	*) echo "app: opção desconhecida: $1" >&2; exit 2 ;;
	esac
	shift
done
case_def "$name" || { echo "app: caso desconhecido: $name" >&2; exit 2; }
tag=$name${scale:+@$scale}
mkdir -p "$COMPAT_OUT/apps"
outjson=$COMPAT_OUT/apps/$tag.json
if ! command -v "$BINARY" >/dev/null; then
	jq -n --arg c "$tag" --arg b "$BINARY" '{case: $c, skipped: ("não instalado: " + $b)}' >"$outjson"
	echo "app: $tag: $BINARY não instalado"
	exit 0
fi

if [ -n "$PREFLIGHT" ] && ! timeout 10 $PREFLIGHT >/dev/null 2>&1; then
	jq -n --arg c "$tag" '{case: $c, skipped: "quebrado no sistema (não inicia fora do mundo)"}' >"$outjson"
	echo "app: $tag: quebrado no sistema"
	exit 0
fi
if $keep; then
	mkdir -p "$COMPAT_OUT/runs"
	run=$(cd "$COMPAT_OUT/runs" && pwd)/$tag
	rm -rf "$run"
	mkdir -p "$run"
else
	run=$(compat_tmp)
fi
home_tmp=$(compat_tmp)   # throwaway profile/config dirs: always removed
mkdir -p "$home_tmp/profile" "$home_tmp/config" "$home_tmp/data"
chmod 700 "$home_tmp" "$home_tmp/profile"
if [ "$KIND" = browser ] && [ "$BINARY" != chromium ]; then
	# a fresh Firefox profile without first-run pages, telemetry or update checks
	cat >"$home_tmp/profile/user.js" <<'EOF'
user_pref("browser.aboutwelcome.enabled", false);
user_pref("browser.startup.homepage_override.mstone", "ignore");
user_pref("datareporting.policy.dataSubmissionEnabled", false);
user_pref("toolkit.telemetry.reportingpolicy.firstRun", false);
user_pref("app.update.auto", false);
user_pref("browser.shell.checkDefaultBrowser", false);
user_pref("trailhead.firstrun.didSeeAboutWelcome", true);
EOF
fi

cat >"$run/typed.sh" <<'EOF'
#!/bin/sh
read -r x
printf %s "$x" > "$RUN/typed"
sleep 30
EOF
chmod +x "$run/typed.sh"

# launch.sh: the app with WAYLAND_DEBUG piped through the filter (one JSON per launch)
x11_prelude=""
if [ "$KIND" = x11 ]; then
	x11_prelude='
n=90
while [ -e "/tmp/.X11-unix/X$n" ] || [ -e "/tmp/.X$n-lock" ]; do n=$((n + 1)); done
# the satellite is the Wayland client here: its protocol facts are the X11 app'"'"'s
WAYLAND_DEBUG=client xwayland-satellite ":$n" 2>&1 >/dev/null \
	| tee >(grep -av "^\[" | tail -n 20 >"$RUN/satellite.err") \
	| setsid bash "$COMPAT_HERE/wl-filter.sh" >"$RUN/wl.sat.json" &
for i in $(seq 100); do [ -S "/tmp/.X11-unix/X$n" ] && break; sleep 0.05; done
export DISPLAY=":$n"
echo ":$n" >"$RUN/display"'
fi
cat >"$run/launch.sh" <<EOF
#!/usr/bin/env bash
export RUN="$run" PROFILE="$home_tmp/profile" HOME_TMP="$home_tmp" COMPAT_HERE="$COMPAT_HERE"
$x11_prelude
n=\$(ls "\$RUN"/wl.*.json 2>/dev/null | wc -l)
export WAYLAND_DEBUG=client $ENV
timeout -k 3 40 bash -c '$CMD' 2>&1 >/dev/null \\
	| tee >(grep -av '^\[' | tail -n 20 >"\$RUN/stderr.\$n.tail") \\
	| setsid bash "$COMPAT_HERE/wl-filter.sh" >"\$RUN/wl.\$n.json"
EOF
# (the filter runs in a session of its own: the group kill at the end takes the app, and the
# filter, seeing EOF, still writes its JSON)

popup_key=""
case $KIND in
gtk) popup_key=shift+F10 ;;
browser | electron | x11) popup_key=shift+F10 ;;
esac
popup_key=${COMPAT_POPUP_KEY:-$popup_key}
{
	echo "spawn bash {run}/launch.sh"
	if [ -n "$APPID" ]; then
		# slow starters: until the first window says who it is, then a moment to draw
		echo "wait json /screens/0/app_id \"$APPID\" $((WAIT + 20000))"
		echo "wait 1500"
	else
		echo "wait $WAIT"
	fi
	echo "dump {run}/d0.json"
	echo "key Return"
	echo "wait 800"
	echo "dump {run}/d1.json"
	if [ "$KIND" = terminal ]; then
		echo 'text "olá ção"'
		echo "key dead_acute"
		echo "key e"
		echo "key Return"
	else
		echo 'text "compat"'
	fi
	echo "wait 500"
	if [ -n "$popup_key" ]; then
		echo "key $popup_key"
		echo "wait 1200"
		echo "key Escape"
		echo "wait 400"
		echo "key $popup_key"
		echo "wait 1200"
	fi
	echo "key ctrl+bracketright"
	echo "wait 600"
	echo "dump {run}/d2.json"
	echo "spawn testclient --app-id compat.copy --copy compat-texto --size 200x120 --color 336699"
	echo "wait 400"
	echo "key Return"
	echo "wait 400"
	echo "key ctrl+bracketright"
	echo "wait 300"
	echo "key h"
	echo "wait 300"
	echo "key Return"
	echo "wait 1000"
	echo "dump {run}/d3.json"
	echo "key ctrl+bracketright"
	echo "wait 300"
	if [ "$KIND" != terminal ] && [ "$KIND" != video ]; then
		echo "spawn bash {run}/launch.sh"
		echo "wait $WAIT"
		echo "dump {run}/d4.json"
	fi
	echo "quit"
} >"$run/case.nks"

# the script's own length (waits + up to 5 s per spawn) plus a margin; rule 4: ≤ 120 s
secs=$(awk '/^wait /{ms+=$2} /^spawn /{ms+=5000} END{print int(ms/1000)+8}' "$run/case.nks")
[ "$secs" -le 120 ] || secs=120
nks_args=(--keep "$run" --secs "$secs")
$nested && nks_args+=(--nested)
extra=()
[ -n "$scale" ] && extra=(-- --scale "$scale")
t0=$(date +%s)
bash "$COMPAT_HERE/run-nks.sh" "$run/case.nks" "${nks_args[@]}" "${extra[@]}" >/dev/null
t1=$(date +%s)
sleep 1   # the filters flush when their app is gone

# merge the launches' facts; read only ids, sizes, modes and flags from the dumps (no titles)
jq -s '{binds: (map(.binds) | add), req: (map(.req) | reduce .[] as $o ({}; . as $a | $o | to_entries | reduce .[] as $e ($a; .[$e.key] += $e.value))),
        ev: (map(.ev) | reduce .[] as $o ({}; . as $a | $o | to_entries | reduce .[] as $e ($a; .[$e.key] += $e.value))),
        args: (map(.args) | add), errors: (map(.errors) | add | unique)}' "$run"/wl.*.json >"$run/wl.json" 2>/dev/null ||
	echo '{"binds":{},"req":{},"ev":{},"args":{},"errors":[]}' >"$run/wl.json"
dumpq='if . == null then null else {mode, focus: (.focus.screen // null),
  screens: [.screens[] | {id, app_id, w: .size[0], h: .size[1], fullscreen, parent}]} end'
for i in 0 1 2 3 4; do
	if [ -f "$run/d$i.json" ]; then jq -c "$dumpq" "$run/d$i.json"; else echo null; fi
done | jq -s . >"$run/dumps.json"
typed=null
[ -f "$run/typed" ] && typed=$(jq -Rs . <"$run/typed")
jq -n --arg c "$tag" --arg kind "$KIND" --argjson nested "$nested" --arg scale "$scale" --arg note "$NOTE" \
	--argjson exit "$(cat "$run/exit")" --argjson secs "$((t1 - t0))" \
	--argjson typed "$typed" --slurpfile wl "$run/wl.json" --slurpfile d "$run/dumps.json" '
	$wl[0] as $w | $d[0] as $ds |
	([$ds[] | select(. != null) | .screens[] | select(.app_id != "compat.copy") | {id}] | unique) as $s0 |
	{case: $c, kind: $kind, nested: $nested, scale: (if $scale == "" then null else $scale end),
	 exit: $exit, secs: $secs, typed: $typed, note: (if $note == "" then null else $note end),
	 app_ids: ([$ds[] | select(. != null) | .screens[] | select(.app_id != "compat.copy") | .app_id] | unique),
	 maps: ([$ds[] | select(. != null) | .screens[] | select(.app_id != "compat.copy" and .w > 0)] | length > 0),
	 foco: ($ds[1] != null and $ds[1].mode == "FOCO" and ($s0 | any(.id == $ds[1].focus))),
	 foco_again: ($ds[3] != null and $ds[3].mode == "FOCO" and ($s0 | any(.id == $ds[3].focus))),
	 fullscreen: ([$ds[] | select(. != null) | .screens[] | select(.fullscreen)] | length > 0),
	 dialogs: ([$ds[] | select(. != null) | .screens[] | select(.parent != null)] | length),
	 windows_after_second: (if $ds[4] == null then null else [$ds[4].screens[] | select(.app_id != "compat.copy")] | length end),
	 keys: ($w.ev["wl_keyboard.key"] // 0),
	 popups: ($w.req["xdg_surface.get_popup"] // 0),
	 popup_done: ($w.ev["xdg_popup.popup_done"] // 0),
	 popup_grab: ($w.req["xdg_popup.grab"] // 0),
	 selection_offers: ($w.ev["wl_data_device.selection"] // 0),
	 set_selection: ($w.req["wl_data_device.set_selection"] // 0),
	 set_cursor: ($w.req["wl_pointer.set_cursor"] // 0),
	 set_shape: ($w.req["wp_cursor_shape_device_v1.set_shape"] // 0),
	 activate: ($w.req["xdg_activation_v1.activate"] // 0),
	 set_fullscreen: ($w.req["xdg_toplevel.set_fullscreen"] // 0),
	 buffer_scale: ($w.args["wl_surface.set_buffer_scale"] // []),
	 preferred_scale: ($w.args["wp_fractional_scale_v1.preferred_scale"] // []),
	 viewport_dest: ($w.req["wp_viewport.set_destination"] // 0),
	 decoration_asked: ($w.args["zxdg_toplevel_decoration_v1.set_mode"] // []),
	 decoration_given: ($w.args["zxdg_toplevel_decoration_v1.configure"] // []),
	 dmabuf: (($w.req["zwp_linux_buffer_params_v1.create_immed"] // 0) + ($w.req["zwp_linux_buffer_params_v1.create"] // 0) > 0),
	 binds: ($w.binds | keys), errors: $w.errors}' >"$outjson"
echo "app: $tag → $(jq -c '{maps, foco, keys, popups, popup_done, typed, exit, secs}' "$outjson")"
