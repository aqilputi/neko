#!/usr/bin/env bash
# Runs app.sh for a list of cases, one at a time (each under its own trap and timeout).
# Usage: apps.sh [--nested] [--scale S] [--bin PATH] [--out DIR] [--keep] [CASE...]  (default: all)
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
source "$COMPAT_HERE/cases.sh"
opts=()
cases=()
while (($#)); do
	case $1 in
	--nested | --keep) opts+=("$1") ;;
	--scale | --bin | --out) opts+=("$1" "${2:?}"); shift ;;
	*) cases+=("$1") ;;
	esac
	shift
done
[ ${#cases[@]} -gt 0 ] || read -r -a cases <<<"$CASES_ALL"
for c in "${cases[@]}"; do
	bash "$COMPAT_HERE/app.sh" "$c" "${opts[@]}" || echo "apps: $c falhou ($?)"
done
