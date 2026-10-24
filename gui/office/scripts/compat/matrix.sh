#!/usr/bin/env bash
# The compatibility matrix (Markdown, pt-BR) from $COMPAT_OUT/apps/*.json and globals.json:
# one row per app (app_ids only, never window titles), ✓ ✗ ◐ — plus a few words.
# Usage: matrix.sh [--out DIR] [--title TEXT]   → $COMPAT_OUT/matrix.md
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
title="Apps dentro do mundo"
while (($#)); do
	case $1 in
	--out) COMPAT_OUT=${2:?}; shift ;;
	--title) title=${2:?}; shift ;;
	*) echo "matrix: opção desconhecida: $1" >&2; exit 2 ;;
	esac
	shift
done
apps=$COMPAT_OUT/apps
[ -d "$apps" ] || { echo "matrix: nada em $apps (rode app.sh/apps.sh)" >&2; exit 1; }

# the facts → one Markdown row (the scale run, when present, fills the "1,25" column)
row='
def mark(b; yes; no): if b then "✓ " + yes else "✗ " + no end;
def cell(s): s | gsub("\\|"; "/");
. as $a | $scale as $s |
(if .skipped then
  [.case, "— " + .skipped, "—", "—", "—", "—", "—", "—", "—", "—", "—", "—", "—"]
 else
  [ .case + (if (.app_ids | length) > 0 then " (`" + (.app_ids | join("`, `")) + "`)" else "" end),
    mark(.maps; "mapeia"; (.note // "não mapeou")),
    (if .maps | not then "—" elif .foco and .keys > 0 then "✓ teclas chegam" elif .keys > 0 then "◐ teclas, FOCO falhou" else "✗ nenhuma tecla" end),
    (if .typed == null then (if .maps then "◐ não lido (texto na tela)" else "—" end)
     elif .typed == "olá çãoé" then "✓ olá çãoé" else "✗ veio " + (.typed | length | tostring) + " chars" end),
    (if .kind == "terminal" or .kind == "video" then "— sem menus"
     elif .popups > 0 then "✓ abre (shift+F10)" elif .maps then "✗ nenhum popup" else "—" end),
    (if .popups == 0 then "—" elif .popup_done > 0 then "✓ ctrl+] fecha" else "◐ fecha, sem popup_done" end),
    (if .selection_offers > 0 then "✓ oferta chega no foco" elif .maps then "✗ sem oferta" else "—" end),
    (if .set_shape > 0 then "✓ cursor-shape"
     elif (.binds | index("wp_cursor_shape_manager_v1")) then "✓ cursor-shape (ligado)"
     elif .set_cursor > 0 then "✗ superfície própria (ignorada)"
     elif .maps then "✗ só wl_pointer (ignorado)" else "—" end),
    (if (.decoration_given | length) > 0 then "servidor" + (if (.decoration_asked | index("2")) then " (pediu)" else "" end)
     elif .maps then "CSD (não pede)" else "—" end),
    (if $s == null then "—"
     elif ($s.preferred_scale | index("150")) and $s.viewport_dest > 0 then "✓ fracionária"
     elif ($s.buffer_scale | index("2")) then "◐ inteira (2, reduzida)"
     elif $s.maps then "◐ escala 1 (esticada)" else "—" end),
    (if .fullscreen then "✓ tela cheia" elif .set_fullscreen > 0 then "✗ pediu, não ganhou" else "—" end),
    (if .windows_after_second == null then "—"
     elif .windows_after_second >= 2 then "✓ nova janela no mundo"
     elif .windows_after_second == 1 then "◐ mesma janela (aba nova)" elif .maps then "✗ sumiu" else "—" end),
    (if (.errors | length) > 0 then "✗ " + (.errors | join(", ")) elif .maps then "✓ nenhum" else "—" end)
  ] end) | map(cell(.)) | "| " + join(" | ") + " |"'

{
	echo "### $title"
	echo
	echo "| app | abre | teclado (FOCO) | acentos | menus/popups | sair do FOCO | seleção do mundo | cursor | decoração | escala 1,25 | tela cheia | 2ª instância | erros de protocolo |"
	echo "|---|---|---|---|---|---|---|---|---|---|---|---|---|"
	for f in "$apps"/*.json; do
		case $f in *@*) continue ;; esac
		c=$(basename "$f" .json)
		s=null
		[ -f "$apps/$c@1.25.json" ] && s=$(cat "$apps/$c@1.25.json")
		jq -r --argjson scale "$s" "$row" "$f"
	done
	if [ -f "$COMPAT_OUT/globals.json" ]; then
		echo
		echo "Globais anunciados: $(jq -r 'map(.name + " v" + (.version|tostring)) | join(", ")' "$COMPAT_OUT/globals.json")."
	fi
	# who binds the protocols of wave 4 (empty before they exist)
	echo
	echo "| app | protocolos novos que o app usa |"
	echo "|---|---|"
	for f in "$apps"/*.json; do
		case $f in *@*) continue ;; esac
		jq -r '
		  ["wp_cursor_shape_manager_v1", "zwp_primary_selection_device_manager_v1", "wp_presentation",
		   "zwp_pointer_constraints_v1", "zwp_relative_pointer_manager_v1", "wp_single_pixel_buffer_manager_v1",
		   "wp_content_type_manager_v1", "zxdg_exporter_v2", "zxdg_importer_v2", "zwp_idle_inhibit_manager_v1",
		   "zwp_text_input_manager_v3", "xwayland_shell_v1"] as $new |
		  if .skipped then empty else
		  "| " + .case + " | " + ([.binds[] | select(. as $b | $new | index($b))] |
		     if length == 0 then "—" else join(", ") end) + " |" end' "$f"
	done
} >"$COMPAT_OUT/matrix.md"
echo "matrix: $COMPAT_OUT/matrix.md"
