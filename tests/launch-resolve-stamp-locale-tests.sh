#!/usr/bin/env bash
#
# tests/launch-resolve-stamp-locale-tests.sh -- issue 1871
#
# `nros_launch_resolve_stamp` (scripts/build/launch-resolve-stale.sh) must give
# ONE digest for one tree, whatever locale the caller runs in.
#
# It sorted its file list with a bare `sort`, which collates in the caller's
# locale. en_US.UTF-8 and C.UTF-8 order `a_c.rs` and `ab.rs` differently (the
# UTF-8 collation ignores `_` at the first level; byte order puts 0x5f before
# 'b'), and the digest hashes the lines in that order. So a resolver built from
# a C.UTF-8 shell read as STALE from an en_US.UTF-8 one, and every locale
# switch made `setup-launch-resolve` rebuild an unchanged tree.
#
# WHAT IS ASSERTED:
#
#   A. agree   -- the stamp of one tree is identical under LC_ALL=C and
#                 C.UTF-8, with a stand-in `sort` first on PATH that collates
#                 DIFFERENTLY (reverse byte order) unless it runs under
#                 LC_ALL=C. The stand-in makes the case host-independent: CI
#                 images ship no en_US.UTF-8, and without a locale that
#                 collates differently the case would hold vacuously.
#   B. real    -- where en_US.UTF-8 IS installed, the real `sort` too: the
#                 stamp under en_US.UTF-8 equals the one under C (not run
#                 otherwise; A already covers the rule).
#   C. moves   -- an edited source still moves the stamp (the fix must not
#                 have reduced the digest to a constant).
#   D. NEGATIVE CONTROL -- the pre-fix function (a bare `sort`, recreated by
#                 rewriting the sourced file) under the same stand-in gives a
#                 DIFFERENT stamp for the same tree under the two locales.
#                 Without it A would also pass against a stand-in that
#                 reordered nothing.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/build/launch-resolve-stale.sh
source "$root/scripts/build/launch-resolve-stale.sh"

fail() {
    echo "[FAIL] launch-resolve-stamp-locale: $*" >&2
    exit 1
}

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# A minimal superproject with the two trees the stamp walks. Names chosen so
# byte order and en_US collation disagree: `a_c.rs` < `ab.rs` in C, `ab.rs` <
# `a_c.rs` in en_US.UTF-8.
fx="$tmp/repo"
crate="$fx/packages/cli/nros-launch-resolve"
pl="$fx/packages/cli/third-party/play_launch"
mkdir -p "$crate/src" "$pl/src/ros-launch-resolve/x"
git_q() { git -c user.name=t -c user.email=t@t -c init.defaultBranch=main "$@" >/dev/null; }
for d in "$fx" "$pl"; do git_q -C "$d" init -q; done
printf 'fn a() {}\n' >"$crate/src/a_c.rs"
printf 'fn b() {}\n' >"$crate/src/ab.rs"
printf '[package]\n' >"$crate/Cargo.toml"
printf 'fn c() {}\n' >"$pl/src/ros-launch-resolve/x/a_c.rs"
printf 'fn d() {}\n' >"$pl/src/ros-launch-resolve/x/ab.rs"
git_q -C "$pl" add -A
git_q -C "$pl" commit -q -m pl
git_q -C "$fx" add packages/cli/nros-launch-resolve
git_q -C "$fx" commit -q -m fx

stamp_in() { LC_ALL="$1" nros_launch_resolve_stamp "$fx" 2>/dev/null; }

# The stand-in: byte order under LC_ALL=C, reverse byte order otherwise.
real_sort="$(command -v sort)"
mkdir -p "$tmp/bin"
{
    echo '#!/bin/sh'
    echo "if [ \"\${LC_ALL:-}\" = C ]; then exec '$real_sort' \"\$@\"; fi"
    echo "LC_ALL=C '$real_sort' -r \"\$@\""
} >"$tmp/bin/sort"
chmod +x "$tmp/bin/sort"

# A. agree, under the stand-in
s_c="$(PATH="$tmp/bin:$PATH" stamp_in C)"
s_cu="$(PATH="$tmp/bin:$PATH" stamp_in C.UTF-8)"
[ -n "$s_c" ] || fail "empty stamp under LC_ALL=C"
[ "$s_c" = "$s_cu" ] || fail "stamp differs between C ($s_c) and a differently-collating C.UTF-8 ($s_cu)"
held=3

# B. real, where a collating locale is installed
list="$(printf '%s\n' "x/ab.rs" "x/a_c.rs")"
if [ "$(printf '%s\n' "$list" | LC_ALL=C sort)" != "$(printf '%s\n' "$list" | LC_ALL=en_US.UTF-8 sort 2>/dev/null)" ]; then
    s_en="$(stamp_in en_US.UTF-8)"
    [ "$s_c" = "$s_en" ] || fail "stamp differs between C ($s_c) and en_US.UTF-8 ($s_en)"
    held=4
fi

# D. negative control: the pre-fix function, same stand-in, same tree
sed 's/LC_ALL=C sort |/sort |/' "$root/scripts/build/launch-resolve-stale.sh" >"$tmp/prefix.sh"
case "$(cat "$tmp/prefix.sh")" in
    *'LC_ALL=C sort'*) fail "could not recreate the pre-fix function" ;;
esac
old_stamp() {
    PATH="$tmp/bin:$PATH" LC_ALL="$1" bash -c 'source "$1"; nros_launch_resolve_stamp "$2"' _ "$tmp/prefix.sh" "$fx" 2>/dev/null
}
[ "$(old_stamp C)" != "$(old_stamp C.UTF-8)" ] \
    || fail "negative control: the pre-fix stamp did not move with the collation, so case A proves nothing"

# C. moves
printf 'fn a2() {}\n' >"$crate/src/a_c.rs"
s_edit="$(stamp_in C.UTF-8)"
[ "$s_edit" != "$s_c" ] || fail "an edited source left the stamp unchanged"

echo "[PASS] launch-resolve-stamp-locale: $held case(s) held; the negative control reproduces the hazard"
