#!/usr/bin/env bash
# Mirror a build.rs-generated header into a leaf's include dir. Issue 0805.
#
# nros-c / nros-cpp's build script writes each generated header TWICE:
#
#   1. `$CORROSION_BUILD_DIR/<name>`  — this leaf's own cmake binary dir
#   2. `$CARGO_TARGET_DIR/nros-{c,cpp}-generated/nros/<name>` — leaf-independent
#
# (1) only exists if the build script RAN for this leaf. Once leaves share a
# cargo target dir, cargo skips the script for every leaf after the first, so a
# freshly-configured leaf has no (1) — measured: it fails with no header and no
# binary. Making the script re-run per leaf is what
# `rerun-if-env-changed=CORROSION_BUILD_DIR` used to do, and that is precisely
# the issue-0491 path-variable fingerprint that made every leaf recompile
# `nros-c` + `nros-cpp` (459 s -> 9 s of cargo time when removed).
#
# 0805 preferred (1) and fell back to (2) only when (1) was ABSENT, on the
# stated grounds that they are "the same bytes". Issue 0978 — that premise holds
# only for a (1) written by the SAME build-script run as (2). A leaf configured
# before a size change keeps ITS (1) from that older run forever: the script
# never runs there again, so (1) is present, the fallback never fires, and the
# leaf mirrors a museum header against an archive the shared build has since
# rebuilt. Measured 2026-09-01: 19 of 20 native leaves carried a six-day-old
# header, and the issue-0369 size anchor caught it as
# `undefined reference to nros_config_variant_sz_<hash>` at link.
#
# So prefer (2), the leaf-INDEPENDENT copy, and fall back to (1). (2) is written
# by `write_header_to_target_dir` in the same run that writes (1) and is
# refreshed by ANY leaf's run, so it is always at least as fresh as (1) — never
# staler. (1) survives as the fallback for a build with no resolvable cargo
# target dir, where (2) is never written at all.
#
# "Present" is not "current" whenever a file can outlive the run that wrote it.
#
# Usage: mirror-generated-header.sh <corrosion-src> <build-dir> <gen-subdir> <name> <dest>
#        mirror-generated-header.sh --self-test
set -euo pipefail

if [ "${1:-}" = "--self-test" ]; then
    self="$(cd "$(dirname "$0")" && pwd)/$(basename "$0")"
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    fails=0
    ok() { printf '  ok   %s\n' "$1"; }
    bad() { printf '  FAIL %s: %s\n' "$1" "$2" >&2; fails=$((fails + 1)); }

    # Two shared candidates: `aaa_old` sorts BEFORE `zzz_new` by name and is
    # OLDER by mtime, so first-match picks the stale one and newest-match picks
    # the current one. That is issue 0987's tree in miniature.
    _st_stage_two() {
        d="$tmp/two$RANDOM$RANDOM"
        mkdir -p "$d/cargo/aaa_old/gen/nros" "$d/cargo/zzz_new/gen/nros" "$d/leaf"
        printf 'OLD\n' > "$d/cargo/aaa_old/gen/nros/h.h"
        printf 'NEW\n' > "$d/cargo/zzz_new/gen/nros/h.h"
        touch -d '2020-01-01 00:00:00' "$d/cargo/aaa_old/gen/nros/h.h"
        printf 'LEAF\n' > "$d/leaf/h.h"
        echo "$d"
    }

    stage() { # <shared-content|-> <leaf-content|->  -> echoes the build dir
        d="$tmp/case$RANDOM$RANDOM"
        mkdir -p "$d/cargo/ws_abc/gen/nros" "$d/leaf"
        [ "$1" = "-" ] || printf '%s\n' "$1" > "$d/cargo/ws_abc/gen/nros/h.h"
        [ "$2" = "-" ] || printf '%s\n' "$2" > "$d/leaf/h.h"
        echo "$d"
    }

    # Issue 0978 — the regression. A leaf keeps a header from an older build
    # script run; the shared copy has since moved. "Present" must not beat
    # "current".
    d="$(stage NEW STALE)"
    bash "$self" "$d/leaf/h.h" "$d" gen h.h "$d/dest.h" >/dev/null
    [ "$(cat "$d/dest.h")" = "NEW" ] \
        && ok "a stale leaf copy loses to the shared one" \
        || bad "a stale leaf copy loses to the shared one" "got $(cat "$d/dest.h")"

    # Issue 0987 — the glob can match SEVERAL shared dirs, and the stale one
    # sorts FIRST by name here, exactly as `cargo/build/` sorts before
    # `cargo/nano-ros_1147c/` in the tree. Name order must not decide it.
    d="$(_st_stage_two)"
    bash "$self" "$d/leaf/h.h" "$d" gen h.h "$d/dest.h" >/dev/null
    [ "$(cat "$d/dest.h")" = "NEW" ] \
        && ok "the newest shared copy wins, not the first by name" \
        || bad "the newest shared copy wins, not the first by name" "got $(cat "$d/dest.h")"

    # Issue 0805's original case, still handled: a leaf whose build script never
    # ran has no copy at all.
    d="$(stage NEW -)"
    bash "$self" "$d/leaf/h.h" "$d" gen h.h "$d/dest.h" >/dev/null
    [ "$(cat "$d/dest.h")" = "NEW" ] \
        && ok "a leaf with no copy uses the shared one" \
        || bad "a leaf with no copy uses the shared one" "got $(cat "$d/dest.h")"

    # No resolvable shared target dir (the header is never written there): the
    # leaf copy is the only source, and must still work.
    d="$(stage - ONLY)"
    bash "$self" "$d/leaf/h.h" "$d" gen h.h "$d/dest.h" >/dev/null
    [ "$(cat "$d/dest.h")" = "ONLY" ] \
        && ok "with no shared copy the leaf copy is used" \
        || bad "with no shared copy the leaf copy is used" "got $(cat "$d/dest.h")"

    # Neither: fail loudly rather than mirror nothing and let a consumer reach
    # the committed stub (issue 0088's latent-for-a-phase failure).
    d="$(stage - -)"
    if bash "$self" "$d/leaf/h.h" "$d" gen h.h "$d/dest.h" >/dev/null 2>"$d/err"; then
        bad "no header anywhere is an error" "exited 0"
    elif grep -q 0978 "$d/err"; then
        ok "no header anywhere is an error naming both paths"
    else
        bad "no header anywhere is an error" "message did not name the issue"
    fi

    # copy_if_different: an unchanged header must not re-stamp the dest, or
    # every consumer TU recompiles on every build.
    d="$(stage SAME SAME)"
    printf 'SAME\n' > "$d/dest.h"
    touch -d '2020-01-01 00:00:00' "$d/dest.h"
    bash "$self" "$d/leaf/h.h" "$d" gen h.h "$d/dest.h" >/dev/null
    [ "$(date -r "$d/dest.h" +%Y)" = "2020" ] \
        && ok "an unchanged header does not re-stamp the dest" \
        || bad "an unchanged header does not re-stamp the dest" "mtime moved"

    [ "$fails" -eq 0 ] || exit 1
    echo "mirror-generated-header self-test OK"
    exit 0
fi

leaf_src="$1"; build_dir="$2"; gen_subdir="$3"; name="$4"; dest="$5"

# `<build>/cargo/<workspace>_<hash>/...` — one entry in practice; the glob
# avoids hardcoding Corrosion's hash, and this path is identical whether
# `cargo` is a real directory or the shared-store symlink.
# Issue 0987 — the NEWEST candidate, not the first.
#
# This used to `break` on the first match, and glob expansion is SORTED, so the
# winner was decided by name. Measured 2026-09-02 in
# `packages/testing/nros-tests/bins/action-raw-goal-probe/build-zenoh`, which
# carries two:
#
#   cargo/build/...            2026-08-19  sz_886681abade04db2   <- won, by name
#   cargo/nano-ros_1147c/...   2026-09-02  sz_9a3e918900c9d46d
#
# `cargo/build/` is residue from the pre-phase-340 target-dir layout (issue
# 0488's class). Nothing rewrites it, so it is frozen and shadowed the live
# store on every run, and `action_raw_goal_probe` failed to link on issue 0369's
# size anchor against archives that were all current.
#
# Issue 0978's premise — "refreshed by ANY leaf's run, so it is always at least
# as fresh as (1), never staler" — is true of THE shared copy. The glob can
# match several, and the selection rule was never stated: it was inherited from
# a comment reading "one entry in practice", which stopped being true silently,
# because first-match still returns A file and the failure lands one layer down
# as a link error naming a hash and no path.
#
# Same shape as issue 0500, whose remedy is the rule missing here: a store that
# ACCUMULATES needs an ordering. There the SDK prefixes are enumerated
# newest-VERSION-first because `find_package` takes the first that resolves;
# here the residue is not distinguishable by name, so the order is by MTIME.
src=""
# -1, not 0: a candidate whose mtime cannot be read scores 0, and it must still
# beat "no candidate at all" — otherwise an unreadable stat would silently
# demote a real shared copy to the leaf fallback, which is the staleness this
# whole family is about.
_newest=-1
for cand in "$build_dir"/cargo/*/"$gen_subdir"/nros/"$name"; do
    [ -f "$cand" ] || continue
    # GNU first, BSD/macOS second, and 0 if neither answers — an unreadable
    # mtime must not silently win over a candidate whose mtime we know.
    _m="$(stat -c %Y "$cand" 2>/dev/null || stat -f %m "$cand" 2>/dev/null || echo 0)"
    if [ "$_m" -gt "$_newest" ]; then
        _newest="$_m"
        src="$cand"
    fi
done

if [ -z "$src" ]; then
    src="$leaf_src"
fi

if [ ! -f "$src" ]; then
    echo "nros: no generated $name to mirror (looked in $build_dir/cargo/*/$gen_subdir/nros/" >&2
    echo "      and the leaf's corrosion dir $leaf_src) — issues 0805, 0978" >&2
    exit 1
fi

mkdir -p "$(dirname "$dest")"
# copy_if_different semantics, so an unchanged header does not re-stamp mtime
# and re-trigger every consumer TU.
if [ ! -f "$dest" ] || ! cmp -s "$src" "$dest"; then
    cp -- "$src" "$dest"
fi
