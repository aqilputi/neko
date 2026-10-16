# nk serve --demo: a fake Claude Code screen. Harmless: it draws what
# NK_DEMO_BODY says and answers each line with an echo. It never runs claude.
e=$(printf '\033')
printf '%s]0;%s\007' "$e" "$NK_DEMO_TITLE"
printf '%s[2J%s[H' "$e" "$e"
printf '%s\n' "$NK_DEMO_BODY" | while IFS= read -r l; do printf '%s\r\n' "$l"; done
prompt() { printf '%s[38;5;216m> %s[0m' "$e" "$e"; }
prompt
while IFS= read -r line; do
	case "$line" in
	/exit)
		printf '%s[2m(demo) até mais%s[0m\r\n' "$e" "$e"
		exit 0
		;;
	"") ;;
	*) printf '%s[2m⏺ (demo) recebi: %s — num nk de verdade isto iria para o claude.%s[0m\r\n' "$e" "$line" "$e" ;;
	esac
	prompt
done
