# Reduces a WAYLAND_DEBUG=client stream to protocol facts, as one JSON object:
#   {"binds": {"iface": version}, "req": {"iface.msg": n}, "ev": {"iface.msg": n},
#    "args": {"iface.msg": ["1", "2"]}, "errors": ["iface code"]}
# Only interface/message names and the integer arguments of a few messages are kept, never
# strings (titles, app ids, mime contents, paths): the output carries no personal data.
function esc(s) { gsub(/\\/, "\\\\", s); gsub(/"/, "\\\"", s); return s }
BEGIN {
	# messages whose (integer) arguments are worth keeping
	keep["zxdg_toplevel_decoration_v1.set_mode"] = 1
	keep["zxdg_toplevel_decoration_v1.configure"] = 1
	keep["wp_cursor_shape_device_v1.set_shape"] = 1
	keep["wp_fractional_scale_v1.preferred_scale"] = 1
	keep["wl_surface.set_buffer_scale"] = 1
	keep["wp_content_type_v1.set_content_type"] = 1
	keep["zwp_pointer_constraints_v1.lock_pointer"] = 1
}
{
	line = $0
	gsub(/\033\[[0-9;]*m/, "", line)   # libwayland ≥ 1.23 colours its debug output
	dir = (index(line, " -> ") > 0) ? "req" : "ev"
	# libwayland writes iface#id.msg(…), wayland-rs (the satellite) iface@id.msg(…)
	if (!match(line, /[a-z_0-9]+[#@][0-9]+\.[a-z_0-9]+\(/)) next
	call = substr(line, RSTART, RLENGTH - 1)
	argstart = RSTART + RLENGTH
	iface = call; sub(/[#@].*/, "", iface)
	msg = call; sub(/.*\./, "", msg)
	key = iface "." msg
	if (dir == "req") req[key]++; else evn[key]++
	rest = substr(line, argstart)
	if (key == "wl_registry.bind" && match(rest, /"[a-z_0-9]+", [0-9]+/)) {
		b = substr(rest, RSTART, RLENGTH)
		name = b; sub(/^"/, "", name); sub(/".*/, "", name)
		ver = b; sub(/.*, /, "", ver)
		binds[name] = ver
	}
	if (key == "wl_display.error" && match(rest, /^[a-z_0-9]+[#@][0-9]+, [0-9]+/)) {
		e = substr(rest, RSTART, RLENGTH); sub(/[#@][0-9]+,/, "", e)
		errs[e]++
	}
	if ((key in keep) && match(rest, /^[0-9, ]*\)/)) {
		a = substr(rest, 1, RLENGTH - 1); gsub(/ /, "", a)
		if (!((key, a) in seen)) {
			seen[key, a] = 1
			args[key] = args[key] (args[key] == "" ? "" : ",") "\"" a "\""
		}
	}
}
function obj(arr,   k, out, sep) {
	out = "{"; sep = ""
	for (k in arr) { out = out sep "\"" esc(k) "\":" arr[k]; sep = "," }
	return out "}"
}
END {
	printf "{\"binds\":%s,\"req\":%s,\"ev\":%s,", obj(binds), obj(req), obj(evn)
	out = "{"; sep = ""
	for (k in args) { out = out sep "\"" k "\":[" args[k] "]"; sep = "," }
	printf "\"args\":%s,\"errors\":[", out "}"
	sep = ""
	for (k in errs) { printf "%s\"%s\"", sep, esc(k); sep = "," }
	print "]}"
}
