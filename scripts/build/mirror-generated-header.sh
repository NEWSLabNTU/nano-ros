#!/usr/bin/env bash
# Mirror a build.rs-generated header into a leaf's include dir. Issue 0805.
#
# nros-c / nros-cpp's build script writes each generated header TWICE:
#
#   1. `$CORROSION_BUILD_DIR/<name>`  — this leaf's own cmake binary dir
#   2. `$CARGO_TARGET_DIR/<gen-subdir>/<name>` — leaf-independent, where
#      <gen-subdir> is `nros-c-generated/rclc/detail` or `nros-cpp-generated/rclcpp/detail`
#      (`nros_build_paths::artifact`; phase-501 W2 put the C one under rclc/detail)
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
#        mirror-generated-header.sh --resolve <corrosion-src> <build-dir> <gen-subdir> <name>
#        mirror-generated-header.sh --self-test
#
# `--resolve` prints the source this script WOULD copy and exits 1 when there
# is none. `check-sizes-header-mirrors` asks it rather than re-deriving the
# choice: that gate compared every mirror against the leaf copy, which 0978
# made the FALLBACK, so the rule lived twice and disagreed (issue 1792).
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
    bash "$self" "$d/leaf/h.h" "$d" gen/nros h.h "$d/dest.h" >/dev/null
    [ "$(cat "$d/dest.h")" = "NEW" ] \
        && ok "a stale leaf copy loses to the shared one" \
        || bad "a stale leaf copy loses to the shared one" "got $(cat "$d/dest.h")"

    # Issue 0987 — the glob can match SEVERAL shared dirs, and the stale one
    # sorts FIRST by name here, exactly as `cargo/build/` sorts before
    # `cargo/nano-ros_1147c/` in the tree. Name order must not decide it.
    d="$(_st_stage_two)"
    bash "$self" "$d/leaf/h.h" "$d" gen/nros h.h "$d/dest.h" >/dev/null
    [ "$(cat "$d/dest.h")" = "NEW" ] \
        && ok "the newest shared copy wins, not the first by name" \
        || bad "the newest shared copy wins, not the first by name" "got $(cat "$d/dest.h")"

    # Issue 1904 — the build's OWN subdir (recorded by the configure) is the
    # OLDER one and a sibling checkout's subdir is newer. Newest-wins took the
    # sibling's header; the recorded subdir must win.
    _st_stage_own_older() {
        d="$tmp/own$RANDOM$RANDOM"
        mkdir -p "$d/cargo/mine_abcde/gen/nros" "$d/cargo/sibling_12345/gen/nros" \
            "$d/leaf" "$d/CMakeFiles"
        printf 'MINE\n' > "$d/cargo/mine_abcde/gen/nros/h.h"
        printf 'SIBLING\n' > "$d/cargo/sibling_12345/gen/nros/h.h"
        touch -d '2020-01-01 00:00:00' "$d/cargo/mine_abcde/gen/nros/h.h"
        printf 'mine_abcde\n' > "$d/CMakeFiles/nros-cargo-subdirs.txt"
        echo "$d"
    }
    d="$(_st_stage_own_older)"
    bash "$self" "$d/leaf/h.h" "$d" gen/nros h.h "$d/dest.h" >/dev/null
    [ "$(cat "$d/dest.h")" = "MINE" ] \
        && ok "the build's own subdir wins over a newer sibling's" \
        || bad "the build's own subdir wins over a newer sibling's" "got $(cat "$d/dest.h")"
    [ "$(bash "$self" --resolve "$d/leaf/h.h" "$d" gen/nros h.h)" = "$d/cargo/mine_abcde/gen/nros/h.h" ] \
        && ok "--resolve names the build's own subdir too" \
        || bad "--resolve names the build's own subdir too" "got $(bash "$self" --resolve "$d/leaf/h.h" "$d" gen/nros h.h)"

    # Issue 1904 — the own subdir has NO header and a sibling's does: refuse,
    # never copy the sibling's.
    d="$(_st_stage_own_older)"
    rm "$d/cargo/mine_abcde/gen/nros/h.h"
    if bash "$self" "$d/leaf/h.h" "$d" gen/nros h.h "$d/dest.h" >/dev/null 2>"$d/err"; then
        bad "only a foreign subdir has the header: refuse" "exited 0, copied $(cat "$d/dest.h" 2>/dev/null)"
    elif grep -q 1904 "$d/err"; then
        ok "only a foreign subdir has the header: refuse, naming the issue"
    else
        bad "only a foreign subdir has the header: refuse" "message did not name the issue"
    fi

    # Issue 0805's original case, still handled: a leaf whose build script never
    # ran has no copy at all.
    d="$(stage NEW -)"
    bash "$self" "$d/leaf/h.h" "$d" gen/nros h.h "$d/dest.h" >/dev/null
    [ "$(cat "$d/dest.h")" = "NEW" ] \
        && ok "a leaf with no copy uses the shared one" \
        || bad "a leaf with no copy uses the shared one" "got $(cat "$d/dest.h")"

    # No resolvable shared target dir (the header is never written there): the
    # leaf copy is the only source, and must still work.
    d="$(stage - ONLY)"
    bash "$self" "$d/leaf/h.h" "$d" gen/nros h.h "$d/dest.h" >/dev/null
    [ "$(cat "$d/dest.h")" = "ONLY" ] \
        && ok "with no shared copy the leaf copy is used" \
        || bad "with no shared copy the leaf copy is used" "got $(cat "$d/dest.h")"

    # Neither: fail loudly rather than mirror nothing and let a consumer reach
    # the committed stub (issue 0088's latent-for-a-phase failure).
    d="$(stage - -)"
    if bash "$self" "$d/leaf/h.h" "$d" gen/nros h.h "$d/dest.h" >/dev/null 2>"$d/err"; then
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
    bash "$self" "$d/leaf/h.h" "$d" gen/nros h.h "$d/dest.h" >/dev/null
    [ "$(date -r "$d/dest.h" +%Y)" = "2020" ] \
        && ok "an unchanged header does not re-stamp the dest" \
        || bad "an unchanged header does not re-stamp the dest" "mtime moved"

    d="$(stage NEW STALE)"
    [ "$(bash "$self" --resolve "$d/leaf/h.h" "$d" gen/nros h.h)" = "$d/cargo/ws_abc/gen/nros/h.h" ] \
        && ok "--resolve names the source the copy uses" \
        || bad "--resolve names the source the copy uses" "got $(bash "$self" --resolve "$d/leaf/h.h" "$d" gen/nros h.h)"
    d="$(stage - -)"
    if bash "$self" --resolve "$d/leaf/h.h" "$d" gen/nros h.h >/dev/null 2>&1; then
        bad "--resolve with no header anywhere fails" "exited 0"
    else
        ok "--resolve with no header anywhere fails"
    fi
    # Issue 1911 -- every run records that it ran, at the path `--marker` names.
    d="$(stage NEW -)"
    bash "$self" "$d/leaf/h.h" "$d" gen/nros h.h "$d/dest.h" >/dev/null
    m="$(bash "$self" --marker "$d" "$d/dest.h")"
    [ -f "$m" ] && case "$m" in "$d"/CMakeFiles/*) true ;; *) false ;; esac \
        && ok "a run records that it ran, inside its build dir" \
        || bad "a run records that it ran, inside its build dir" "no marker at $m"
    # Issue 1927 -- the marker name is BOUNDED. A dest whose path, flattened
    # into one component, is past NAME_MAX (255) must still record its run, and
    # two dests that share a long tail must still get two markers.
    deep="$d/$(printf 'a%.0s' $(seq 1 120))/$(printf 'b%.0s' $(seq 1 120))"
    if bash "$self" "$d/leaf/h.h" "$d" gen/nros h.h "$deep/x/dest.h" >/dev/null 2>"$d/err"; then
        m="$(bash "$self" --marker "$d" "$deep/x/dest.h")"
        n="$(basename "$m")"
        [ -f "$m" ] && [ "${#n}" -le 100 ] \
            && ok "a dest deeper than NAME_MAX records its run under a bounded name" \
            || bad "a dest deeper than NAME_MAX records its run under a bounded name" "marker $n (${#n} bytes)"
    else
        bad "a dest deeper than NAME_MAX records its run under a bounded name" "$(tail -1 "$d/err")"
    fi
    m1="$(bash "$self" --marker "$d" "$deep/x/dest.h")"
    m2="$(bash "$self" --marker "$d" "$d/other/$(printf 'a%.0s' $(seq 1 120))/$(printf 'b%.0s' $(seq 1 120))/x/dest.h")"
    [ "$m1" != "$m2" ] \
        && ok "two dests sharing a long tail get two markers" \
        || bad "two dests sharing a long tail get two markers" "both $m1"
    [ "$fails" -eq 0 ] || exit 1
    echo "mirror-generated-header self-test OK"
    exit 0
fi

# Issue 1911 -- where this command records that it RAN for one dest: a file in
# the build dir's CMakeFiles, named from the dest path. The copy is
# copy-if-different, so the dest's own mtime says when the content last moved,
# not when the command last ran. The drift gate needs the second: a mirror that
# differs from its source is a defect only if the command ran AFTER the source
# was written. Spelled once, here; the gate asks `--marker`.
#
# Issue 1927 -- the name is `nros_path_key <dest>`, a BOUNDED key (readable
# tail + hash of the whole path), never the dest flattened with `/` -> `_`:
# that was as long as the dest, so a build dir deep enough (a long TMPDIR) put
# it past NAME_MAX and the run failed with `File name too long`. A dir holding
# only an old-style marker reads as "no marker" and takes issue 1901's archive
# bridge until its next build writes the new one.
# shellcheck source=scripts/lib/path-key.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")/../lib" && pwd)/path-key.sh"
mirror_ran_marker() { # <build_dir> <dest>
    printf '%s/CMakeFiles/nros-mirror-ran/%s\n' "$1" "$(nros_path_key "$2")"
}
if [ "${1:-}" = "--marker" ]; then
    mirror_ran_marker "$2" "$3"
    exit 0
fi

resolve_only=0
if [ "${1:-}" = "--resolve" ]; then
    resolve_only=1
    shift
fi
leaf_src="$1"; build_dir="$2"; gen_subdir="$3"; name="${4:-}"; dest="${5:-}"

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
#
# Issue 1904 — and "newest" is only a safe order among copies THIS build could
# have written. With a shared cargo dir, `cargo` is a symlink every sharer
# writes through, and a second checkout sharing the keyed dir puts its own
# Corrosion subdir beside this one: newest-wins then copied the OTHER checkout's
# header against this build's archive. So when the configure recorded which
# subdir(s) its cargo writes (`nros_config_header_cargo_manifest`, one name per
# line), only those are candidates. Newest-across-all survives only for a build
# dir with no record (not a Corrosion import of ours) — 0987's rule, unchanged
# there.
src=""
own_record="$build_dir/CMakeFiles/nros-cargo-subdirs.txt"
cands=()
if [ -f "$own_record" ]; then
    while IFS= read -r _sub || [ -n "$_sub" ]; do
        [ -n "$_sub" ] && cands+=("$build_dir/cargo/$_sub/$gen_subdir/$name")
    done <"$own_record"
else
    cands=("$build_dir"/cargo/*/"$gen_subdir"/"$name")
fi
# -1, not 0: a candidate whose mtime cannot be read scores 0, and it must still
# beat "no candidate at all" — otherwise an unreadable stat would silently
# demote a real shared copy to the leaf fallback, which is the staleness this
# whole family is about.
_newest=-1
for cand in "${cands[@]}"; do
    [ -f "$cand" ] || continue
    # GNU first, BSD/macOS second, and 0 if neither answers — an unreadable
    # mtime must not silently win over a candidate whose mtime we know.
    _m="$(stat -c %Y "$cand" 2>/dev/null || stat -f %m "$cand" 2>/dev/null || echo 0)"
    if [ "$_m" -gt "$_newest" ]; then
        _newest="$_m"
        src="$cand"
    fi
done

# Issue 1904 — the recorded subdir has no header but another one does. Never
# take the foreign copy: either the record is wrong or Corrosion moved its
# target dir (the formula `nros_config_header_cargo_manifest` re-derives), and
# both mean this build cannot tell which header matches its archive.
if [ -z "$src" ] && [ -f "$own_record" ]; then
    for cand in "$build_dir"/cargo/*/"$gen_subdir"/"$name"; do
        [ -f "$cand" ] || continue
        echo "nros: no generated $name in this build's own cargo subdir(s)" >&2
        echo "      ($(tr '\n' ' ' <"$own_record")recorded in $own_record)," >&2
        echo "      but $cand has one. Refusing to mirror another build's header" >&2
        echo "      (issue 1904): re-configure, and if Corrosion was upgraded check" >&2
        echo "      that nros_config_header_cargo_manifest still matches its layout." >&2
        exit 1
    done
fi

if [ -z "$src" ]; then
    src="$leaf_src"
fi

if [ ! -f "$src" ]; then
    echo "nros: no generated $name to mirror (looked in $build_dir/cargo/*/$gen_subdir/" >&2
    echo "      and the leaf's corrosion dir $leaf_src) — issues 0805, 0978" >&2
    exit 1
fi

if [ "$resolve_only" -eq 1 ]; then
    printf '%s\n' "$src"
    exit 0
fi

mkdir -p "$(dirname "$dest")"
# copy_if_different semantics, so an unchanged header does not re-stamp mtime
# and re-trigger every consumer TU.
if [ ! -f "$dest" ] || ! cmp -s "$src" "$dest"; then
    cp -- "$src" "$dest"
fi
marker="$(mirror_ran_marker "$build_dir" "$dest")"
mkdir -p "$(dirname "$marker")"
: >"$marker"
