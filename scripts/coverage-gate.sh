#!/usr/bin/env bash
# coverage-gate.sh PROFILE MIN — fails when the total statement coverage in
# PROFILE (go test -coverprofile) is below MIN percent. cmd/vtprobe, a debug
# tool that drives a real claude, is left out of the total.
set -euo pipefail
profile=${1:-coverage.out}
min=${2:-80}
GO=${GO:-go}

filtered=$(mktemp)
trap 'rm -f "$filtered"' EXIT
grep -v '/cmd/vtprobe/' "$profile" >"$filtered"

total=$("$GO" tool cover -func="$filtered" | awk '/^total:/ {sub("%", "", $NF); print $NF}')
echo "coverage: ${total}% (mínimo ${min}%)"
if awk -v t="$total" -v m="$min" 'BEGIN { exit !(t < m) }'; then
	echo "coverage abaixo do mínimo" >&2
	exit 1
fi
