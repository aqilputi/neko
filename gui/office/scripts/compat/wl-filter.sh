#!/usr/bin/env bash
# stdin: a WAYLAND_DEBUG=client stream; stdout: its protocol facts as JSON (see wl-filter.awk).
exec awk -f "$(dirname "${BASH_SOURCE[0]}")/wl-filter.awk"
