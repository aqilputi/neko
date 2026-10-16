#!/usr/bin/env bash
# wp-scope.sh REGEX [BASE] — fails when a work package touched files outside
# its scope: every file changed since BASE (git diff BASE...HEAD), plus staged,
# unstaged and untracked changes, must match the extended REGEX.
# BASE defaults to $WP_BASE or main.
#
#   scripts/wp-scope.sh '^(internal/core/|internal/archtest/|scripts/wp-scope\.sh$)'
set -euo pipefail

if [[ $# -lt 1 || -z ${1:-} ]]; then
	echo "uso: $0 '<regex>' [base]" >&2
	exit 2
fi
re=$1
base=${2:-${WP_BASE:-main}}

if ! git rev-parse --verify --quiet "$base^{commit}" >/dev/null; then
	echo "base desconhecida: $base" >&2
	exit 2
fi

changed=$(
	{
		git diff --name-only "$base...HEAD"
		git diff --name-only --cached
		git diff --name-only
		git ls-files --others --exclude-standard
	} | sort -u | sed '/^$/d'
)

outside=$(grep -Ev -- "$re" <<<"$changed" || true)
n=$(sed '/^$/d' <<<"$changed" | wc -l)
if [[ -n $outside ]]; then
	echo "fora do escopo ($re), base $base:" >&2
	sed 's/^/  /' <<<"$outside" >&2
	exit 1
fi
echo "escopo ok: $n arquivo(s) dentro de $re (base $base)"
