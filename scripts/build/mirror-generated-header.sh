#!/usr/bin/env bash
# Mirror a PLACED build-script header into a leaf's include dir. Issue 1824.
#
# The source is the copy `cargo-out-dir-headers.py` placed from cargo's own
# `$OUT_DIR` (`nros_config_header_place` runs the import's cargo under it), so
# it was written by the cargo run THIS build just made, cache hit or not. It is
# the only source: this script copies it to <dest> when the content differs
# (copy_if_different, so an unchanged header does not re-stamp the dest and
# recompile every consumer TU), records that it ran (`--marker`, issue 1911),
# and fails loudly when the placed copy is missing rather than mirror nothing
# and let a consumer reach the committed stub (issue 0088).
#
# History: until issue 1824 the source was CHOSEN — between the leaf's own
# corrosion copy and the leaf-independent `$CARGO_TARGET_DIR/nros-{c,cpp}-generated/`
# side channel under `<build>/cargo/*/`. Every rule of that choice was a fix for
# the previous one: prefer the leaf copy (0805), which can outlive the run that
# wrote it (0978); prefer the shared copy, of which the glob can find several
# (0987); take the newest, which may be a sibling checkout's (1904). Placement
# removes the choice, and the side channel is no longer written, so
# <gen-subdir> must be `-`; any other value is a command from a configure
# older than 1824 and is refused, naming the issue — re-configure.
#
# Usage: mirror-generated-header.sh <placed-src> <build-dir> - <name> <dest>
#        mirror-generated-header.sh --resolve <placed-src> <build-dir> - <name>
#        mirror-generated-header.sh --marker <build-dir> <dest>
#        mirror-generated-header.sh --self-test
#
# `--resolve` prints the source this script WOULD copy and exits 1 when there
# is none. `check-sizes-header-mirrors` asks it rather than re-deriving the
# rule (issue 1792).
set -euo pipefail

if [ "${1:-}" = "--self-test" ]; then
    self="$(cd "$(dirname "$0")" && pwd)/$(basename "$0")"
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    fails=0
    ok() { printf '  ok   %s\n' "$1"; }
    bad() { printf '  FAIL %s: %s\n' "$1" "$2" >&2; fails=$((fails + 1)); }

    # <side-channel-content|-> <placed-content|->  -> echoes the build dir.
    # The side-channel copy under `cargo/*/` is what the pre-1824 script read;
    # it is staged so a case can prove nothing consults it any more.
    stage() {
        d="$tmp/case$RANDOM$RANDOM"
        mkdir -p "$d/cargo/ws_abc/gen/nros" "$d/placed"
        [ "$1" = "-" ] || printf '%s\n' "$1" > "$d/cargo/ws_abc/gen/nros/h.h"
        [ "$2" = "-" ] || printf '%s\n' "$2" > "$d/placed/h.h"
        echo "$d"
    }

    # The placed copy is the only source: a NEWER side-channel copy beside it
    # must not win (that preference is what 0978/0987/1904 kept re-fixing).
    d="$(stage SHARED PLACED)"
    touch -d '2020-01-01 00:00:00' "$d/placed/h.h"
    bash "$self" "$d/placed/h.h" "$d" - h.h "$d/dest.h" >/dev/null
    [ "$(cat "$d/dest.h")" = "PLACED" ] \
        && ok "the placed source is copied, never a side-channel copy" \
        || bad "the placed source is copied, never a side-channel copy" "got $(cat "$d/dest.h")"
    [ "$(bash "$self" --resolve "$d/placed/h.h" "$d" - h.h)" = "$d/placed/h.h" ] \
        && ok "--resolve names the placed source" \
        || bad "--resolve names the placed source" "got $(bash "$self" --resolve "$d/placed/h.h" "$d" - h.h)"

    # A content change is copied.
    printf 'PLACED2\n' > "$d/placed/h.h"
    bash "$self" "$d/placed/h.h" "$d" - h.h "$d/dest.h" >/dev/null
    [ "$(cat "$d/dest.h")" = "PLACED2" ] \
        && ok "a changed placed source re-mirrors" \
        || bad "a changed placed source re-mirrors" "got $(cat "$d/dest.h")"

    # No placed source: an error naming the issue, never a fallback to the
    # side-channel copy, and nothing written.
    d="$(stage SHARED -)"
    if bash "$self" "$d/placed/h.h" "$d" - h.h "$d/dest.h" >/dev/null 2>"$d/err"; then
        bad "no placed source is an error" "exited 0, copied $(cat "$d/dest.h" 2>/dev/null)"
    elif [[ "$(cat "$d/err")" == *1824* ]] && [ ! -e "$d/dest.h" ]; then
        ok "no placed source is an error naming the issue, never the shared copy"
    else
        bad "no placed source is an error" "message did not name the issue, or dest written"
    fi
    if bash "$self" --resolve "$d/placed/h.h" "$d" - h.h >/dev/null 2>&1; then
        bad "--resolve with no placed source fails" "exited 0"
    else
        ok "--resolve with no placed source fails"
    fi

    # A pre-1824 command (a real <gen-subdir>) is refused even when both the
    # leaf and side-channel copies exist: the choice between them is gone.
    d="$(stage SHARED PLACED)"
    if bash "$self" "$d/placed/h.h" "$d" gen/nros h.h "$d/dest.h" >/dev/null 2>"$d/err"; then
        bad "a gen-subdir other than - is refused" "exited 0, copied $(cat "$d/dest.h" 2>/dev/null)"
    elif [[ "$(cat "$d/err")" == *1824* ]] && [ ! -e "$d/dest.h" ]; then
        ok "a gen-subdir other than - is refused, naming the issue"
    else
        bad "a gen-subdir other than - is refused" "message did not name the issue, or dest written"
    fi
    if bash "$self" --resolve "$d/placed/h.h" "$d" gen/nros h.h >/dev/null 2>&1; then
        bad "--resolve refuses a gen-subdir other than -" "exited 0"
    else
        ok "--resolve refuses a gen-subdir other than -"
    fi

    # copy_if_different: an unchanged header must not re-stamp the dest, or
    # every consumer TU recompiles on every build.
    d="$(stage - SAME)"
    printf 'SAME\n' > "$d/dest.h"
    touch -d '2020-01-01 00:00:00' "$d/dest.h"
    bash "$self" "$d/placed/h.h" "$d" - h.h "$d/dest.h" >/dev/null
    [ "$(date -r "$d/dest.h" +%Y)" = "2020" ] \
        && ok "an unchanged header does not re-stamp the dest" \
        || bad "an unchanged header does not re-stamp the dest" "mtime moved"

    # Issue 1911 -- every run records that it ran, at the path `--marker`
    # names, even when the copy was skipped as unchanged (just above).
    m="$(bash "$self" --marker "$d" "$d/dest.h")"
    [ -f "$m" ] && case "$m" in "$d"/CMakeFiles/*) true ;; *) false ;; esac \
        && ok "a run records that it ran, inside its build dir" \
        || bad "a run records that it ran, inside its build dir" "no marker at $m"
    # Issue 1927 -- the marker name is BOUNDED. A dest whose path, flattened
    # into one component, is past NAME_MAX (255) must still record its run, and
    # two dests that share a long tail must still get two markers.
    deep="$d/$(printf 'a%.0s' $(seq 1 120))/$(printf 'b%.0s' $(seq 1 120))"
    if bash "$self" "$d/placed/h.h" "$d" - h.h "$deep/x/dest.h" >/dev/null 2>"$d/err"; then
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
# it past NAME_MAX and the run failed with `File name too long`.
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
src="$1"; build_dir="$2"; gen_subdir="$3"; name="${4:-}"; dest="${5:-}"

if [ "$gen_subdir" != "-" ]; then
    echo "nros: mirror command for $name names gen-subdir '$gen_subdir'; only a" >&2
    echo "      PLACED source ('-') is mirrored since issue 1824, and the" >&2
    echo "      \$CARGO_TARGET_DIR side channel it read is no longer written." >&2
    echo "      This command is from an older configure: re-configure $build_dir." >&2
    exit 1
fi
if [ ! -f "$src" ]; then
    echo "nros: no placed $name at $src — the cargo build that" >&2
    echo "      places it from \$OUT_DIR has not run (issue 1824)" >&2
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
