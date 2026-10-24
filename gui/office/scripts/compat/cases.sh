# The real-app cases of the compatibility matrix (sourced by app.sh).
# `case_def NAME` sets:
#   BINARY   the program that must be on PATH (else the case is skipped)
#   CMD      the command, as one shell string run by launch.sh ($PROFILE, $RUN, $HOME_TMP exist)
#   KIND     terminal | gtk | browser | video | electron | x11
#   WAIT     ms to wait after the spawn for the first window
#   ENV      extra "K=V" words for the app
# Rule 4 of the wave: throwaway profiles only, never the user's; own D-Bus where an app would
# otherwise reach the user's running instance, keyring or portal.
# terminals run $RUN/typed.sh: it reads one line and writes it to $RUN/typed (dead keys checked)
TYPED='sh "$RUN/typed.sh"'

case_def() {
	ENV=""
	APPID=""
	PREFLIGHT=""   # a command that must work first (else: "quebrado no sistema")
	NOTE=""        # what to say when it does not map
	WAIT=2500
	case $1 in
	foot)
		BINARY=foot KIND=terminal
		CMD="foot -c /dev/null -o app-id=foot $TYPED" ;;
	weston-terminal)
		BINARY=weston-terminal KIND=terminal
		CMD="weston-terminal --shell=\"\$RUN/typed.sh\"" ;;
	kitty)
		BINARY=kitty KIND=terminal
		CMD="kitty --config NONE -o allow_remote_control=no -o linux_display_server=wayland $TYPED" ;;
	gtk4-demo)
		BINARY=gtk4-demo KIND=gtk WAIT=3000 PREFLIGHT="gtk4-demo --help"
		CMD="gtk4-demo" ;;
	gtk4-widget-factory)
		BINARY=gtk4-widget-factory APPID=org.gtk.WidgetFactory4 KIND=gtk WAIT=3000
		CMD="gtk4-widget-factory" ;;
	nautilus)
		BINARY=nautilus APPID=org.gnome.Nautilus KIND=gtk WAIT=5000
		CMD="XDG_CONFIG_HOME=\$HOME_TMP/config XDG_DATA_HOME=\$HOME_TMP/data dbus-run-session -- nautilus --new-window \"\$HOME_TMP\"" ;;
	firefox)
		BINARY=firefox APPID=firefox KIND=browser WAIT=9000
		CMD="dbus-run-session -- firefox --new-instance --no-remote --profile \"\$PROFILE\" about:blank" ;;
	zen-browser)
		BINARY=zen-browser APPID=zen KIND=browser WAIT=9000
		CMD="dbus-run-session -- zen-browser --new-instance --no-remote --profile \"\$PROFILE\" about:blank" ;;
	chromium)
		BINARY=chromium APPID=chromium KIND=browser WAIT=7000
		CMD="dbus-run-session -- chromium --user-data-dir=\"\$PROFILE\" --no-first-run --no-default-browser-check --password-store=basic --ozone-platform-hint=auto about:blank" ;;
	chromium-default)
		# what nkw's launcher gives it today: no hint, no DISPLAY
		BINARY=chromium APPID=chromium KIND=browser WAIT=7000
		CMD="dbus-run-session -- chromium --user-data-dir=\"\$PROFILE\" --no-first-run --no-default-browser-check --password-store=basic about:blank" ;;
	chromium-x11)
		BINARY=chromium APPID=Chromium KIND=x11 WAIT=8000
		CMD="dbus-run-session -- chromium --user-data-dir=\"\$PROFILE\" --no-first-run --no-default-browser-check --password-store=basic --ozone-platform=x11 about:blank" ;;
	mpv)
		BINARY=mpv KIND=video
		CMD="mpv --no-config --vo=gpu --gpu-context=wayland --hwdec=no --loop=inf --idle=no av://lavfi:testsrc=size=640x360:rate=30" ;;
	mpv-fs)
		BINARY=mpv KIND=video
		CMD="mpv --no-config --fs --vo=gpu --gpu-context=wayland --hwdec=no --loop=inf av://lavfi:testsrc=size=640x360:rate=30" ;;
	discord)
		# rule 4: ≤ 20 s; a throwaway config makes it download the app first
		BINARY=discord APPID=discord KIND=electron WAIT=0 NOTE="não abre em 20 s (perfil novo baixa o app)"
		CMD="XDG_CONFIG_HOME=\$HOME_TMP/config dbus-run-session -- discord" ;;
	discord-ozone)
		BINARY=discord APPID=discord KIND=electron WAIT=0 NOTE="não abre em 20 s (perfil novo baixa o app)"
		ENV="ELECTRON_OZONE_PLATFORM_HINT=auto"
		CMD="XDG_CONFIG_HOME=\$HOME_TMP/config dbus-run-session -- discord --ozone-platform-hint=auto" ;;
	*) return 1 ;;
	esac
}

CASES_ALL="foot weston-terminal kitty gtk4-demo gtk4-widget-factory nautilus firefox zen-browser chromium chromium-default mpv mpv-fs discord discord-ozone chromium-x11"
