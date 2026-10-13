#!/usr/bin/env bash
#
# tests/launch-resolve-stamp-locale-tests.sh -- issue 1869
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
#   A. agree   -- the stamp of one tree is identical under LC_ALL=C,
#                 C.UTF-8 and a collating locale (en_US.UTF-8).
#   B. moves   -- an edited source still moves the stamp (the fix must not
#                 have reduced the digest to a constant).
#   C. NEGATIVE CONTROL -- a bare `sort` over the SAME file list orders it
#                 differently under the two locales. Without it, A would also
#                 pass on a host whose en_US locale is missing (glibc then falls
#                 back to C silently), proving nothing. If no collating locale
#                 is installed, C cannot run and the case is NOT VERIFIED
#                 (`nros_check_unverified`), never a quiet pass.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/build/launch-resolve-stale.sh
source "$root/scripts/build/launch-resolve-stale.sh"
# shellcheck source=scripts/build/check-skip.sh
source "$root/scripts/build/check-skip.sh"

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
git_q -C "$pl" -c user.name=t -c user.email=t@t commit -q -m pl
git_q -C "$fx" add packages/cli/nros-launch-resolve
git_q -C "$fx" -c user.name=t -c user.email=t@t commit -q -m fx

stamp_in() { LC_ALL="$1" nros_launch_resolve_stamp "$fx" 2>/dev/null; }

# A. agree
s_c="$(stamp_in C)"
s_cu="$(stamp_in C.UTF-8)"
s_en="$(stamp_in en_US.UTF-8)"
[ -n "$s_c" ] || fail "empty stamp under LC_ALL=C"
[ "$s_c" = "$s_cu" ] || fail "stamp differs between C ($s_c) and C.UTF-8 ($s_cu)"
[ "$s_c" = "$s_en" ] || fail "stamp differs between C ($s_c) and en_US.UTF-8 ($s_en)"

# B. moves
printf 'fn a2() {}\n' >"$crate/src/a_c.rs"
s_edit="$(stamp_in en_US.UTF-8)"
[ "$s_edit" != "$s_c" ] || fail "an edited source left the stamp unchanged"

# C. negative control: does a bare sort reproduce the hazard on this host?
list="$(printf '%s\n' "x/ab.rs" "x/a_c.rs")"
if [ "$(printf '%s\n' "$list" | LC_ALL=C sort)" != "$(printf '%s\n' "$list" | LC_ALL=en_US.UTF-8 sort 2>/dev/null)" ]; then
    echo "[PASS] launch-resolve-stamp-locale: 3 case(s) held (C, C.UTF-8, en_US.UTF-8 agree; a bare sort disagrees)"
else
    nros_check_unverified launch-resolve-stamp-locale \
        "en_US.UTF-8 is not installed, so the negative control cannot show a bare sort disagreeing (cases A and B held)" || exit 1
    echo "[PASS] launch-resolve-stamp-locale: 2 case(s) held, negative control NOT VERIFIED"
fi
