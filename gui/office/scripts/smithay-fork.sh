#!/usr/bin/env bash
# Rebuild the Smithay fork branch nkw uses through the workspace [patch] (gui/office/Cargo.toml):
# the upstream rev nkw pins + patches/smithay-winit-relative-pointer.patch, one commit.
# Usage: smithay-fork.sh [--dir DIR] [--push REMOTE_URL]
#   --dir   where to clone (default: a new temp dir; printed at the end)
#   --push  push the branch there (e.g. git@github.com:<you>/smithay.git); without it, nothing leaves the machine
# Prints the new commit SHA: put it in the [patch] entry's `rev`.
set -euo pipefail

HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
OFFICE=$(dirname "$HERE")
UPSTREAM=https://github.com/Smithay/smithay.git
REV=24647fc8d00131eb7f388efed26e35ea316b0018
BRANCH=nkw/winit-relative-pointer
PATCH="$OFFICE/patches/smithay-winit-relative-pointer.patch"
MSG="winit: emit PointerMotion from DeviceEvent::PointerMotion (relative pointer, cursor lock)"

dir=""
push=""
while (($#)); do
	case $1 in
	--dir) dir=${2:?}; shift ;;
	--push) push=${2:?}; shift ;;
	-h | --help) sed -n '2,7p' "$0"; exit 0 ;;
	*) echo "smithay-fork: opção desconhecida: $1" >&2; exit 2 ;;
	esac
	shift
done
[ -f "$PATCH" ] || { echo "smithay-fork: falta $PATCH" >&2; exit 2; }
grep -q "rev = \"$REV\"" "$OFFICE/crates/nkw/Cargo.toml" ||
	{ echo "smithay-fork: crates/nkw/Cargo.toml não fixa mais $REV; atualize REV aqui" >&2; exit 2; }
[ -n "$dir" ] || dir=$(mktemp -d "${TMPDIR:-/tmp}/smithay-fork.XXXXXX")

if [ ! -d "$dir/.git" ]; then
	git clone --quiet "$UPSTREAM" "$dir"
fi
git -C "$dir" fetch --quiet origin
git -C "$dir" checkout --quiet -B "$BRANCH" "$REV"
git -C "$dir" apply --index "$PATCH"
# a fixed identity and date: the same inputs give the same commit SHA
GIT_AUTHOR_NAME=nkw GIT_AUTHOR_EMAIL=nkw@localhost GIT_AUTHOR_DATE="2026-10-22T00:00:00Z" \
	GIT_COMMITTER_NAME=nkw GIT_COMMITTER_EMAIL=nkw@localhost GIT_COMMITTER_DATE="2026-10-22T00:00:00Z" \
	git -C "$dir" commit --quiet -m "$MSG"
sha=$(git -C "$dir" rev-parse HEAD)
if [ -n "$push" ]; then
	git -C "$dir" push --force "$push" "$BRANCH"
fi
echo "smithay-fork: $dir ($BRANCH)"
echo "$sha"
