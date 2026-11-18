#!/usr/bin/env bash
# check-c-image-one-cargo-run: a C image's sizes header and the Rust archive it
# links come from ONE cargo run (phase-578 W8).
#
# A C image used to run cargo twice. The nros-c crate's run wrote the C sizes
# header the image compiled against, and the nros-cpp crate's run wrote the
# archive it linked (`libnros_cpp.a` bundles nros-c, and the C++ umbrella is the
# one picked whenever both exist, which is always on the CMake road). Two
# producers of one fact is the shape behind issues 0805, 0978 and 1824. Now
# `nros_c-static` is an interface over the nros-cpp archive, and the C header
# is the copy that same run placed.
#
# Builds the C talker for the host board and reads the answer from the BUILD,
# never from the CMake sources:
#   1. the build graph has one runtime cargo target, `_cargo-build_nros_cpp`,
#      and no `_cargo-build_nros_c`;
#   2. every C translation unit that includes the C sizes header (ninja's deps
#      log) compiled the copy the nros-cpp run placed, byte for byte;
#   3. the image's link inputs (`ninja -t query`) hold one runtime archive, and
#      it defines the variant symbol that header names.
# `selftest` runs the two comparisons against made-up inputs, a mismatch of
# each kind, on every run.
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
mkdir -p "$repo/tmp"
work="$(mktemp -d "$repo/tmp/c-image-one-cargo-run.XXXXXX")"
trap 'rm -rf "$work"' EXIT
say() { echo "check-c-image-one-cargo-run: $*"; }
# shellcheck source=../build/cargo.sh
. "$repo/scripts/build/cargo.sh"
# shellcheck source=../lib/grep-q.sh
. "$repo/scripts/lib/grep-q.sh"

# The variant symbol a sizes header names, or empty.
header_variant() { # <header>
    grep -oE 'nros_config_variant_sz_[0-9a-f]+' "$1" | head -1 || true
}

# Why a header and an archive's symbol list are not one cargo run's, or empty.
pairing_problem() { # <header> <placed-header> <nm-output>
    local v
    if ! cmp -s "$1" "$2"; then
        echo "the compiled header $1 differs from the archive run's placed copy $2"
        return
    fi
    v="$(header_variant "$1")"
    if [ -z "$v" ]; then
        echo "the header $1 names no variant symbol"
        return
    fi
    if ! nros_grep_q " $v\$" "$3"; then
        echo "the linked archive does not define $v, the variant its header names"
    fi
}

selftest() {
    local d="$work/selftest" sym=nros_config_variant_sz_00aa
    mkdir -p "$d"
    printf 'extern const unsigned char %s;\n' "$sym" > "$d/h"
    cp "$d/h" "$d/placed"
    printf '00000000 V %s\n' "$sym" > "$d/nm"
    [ -z "$(pairing_problem "$d/h" "$d/placed" "$d/nm")" ] \
        || { say "selftest: a matching pair was refused"; exit 1; }
    printf 'extern const unsigned char nros_config_variant_sz_00bb;\n' > "$d/other"
    [ -n "$(pairing_problem "$d/other" "$d/placed" "$d/nm")" ] \
        || { say "selftest: a header from ANOTHER run was accepted"; exit 1; }
    printf '00000000 V nros_config_variant_sz_00bb\n' > "$d/nm2"
    [ -n "$(pairing_problem "$d/h" "$d/placed" "$d/nm2")" ] \
        || { say "selftest: an archive without the header's variant was accepted"; exit 1; }
}
selftest

if ! nros="$(nros_cli_bin)" || [ -z "$nros" ]; then
    say "no nros CLI (just setup-cli, or source ./activate.sh)"; exit 1
fi
(cd "$work" && "$nros" new one_run --board linux --template talker --lang c) \
    > "$work/new.log" 2>&1 || { say "nros new failed:"; tail -5 "$work/new.log"; exit 1; }
if ! (cd "$work/one_run" && CMAKE_GENERATOR=Ninja "$nros" build) > "$work/build.log" 2>&1; then
    say "nros build failed:"
    grep -E -i '(^|[[:space:]])error|FATAL|undefined' "$work/build.log" | head -12 | sed 's/^/    /'
    exit 1
fi
if ! b="$(find "$work/one_run/build" -path '*nros-metadata*' -prune -o -name build.ninja -print | head -1)"; then
    b=""
fi
[ -n "$b" ] || { say "no build.ninja under $work/one_run/build"; exit 1; }
b="$(dirname "$b")"
fail=0

targets="$(ninja -C "$b" -t targets all)"
if nros_grep_q '_cargo-build_nros_c:' <<<"$targets"; then
    say "the build graph still runs nros-c's own cargo build (_cargo-build_nros_c)"; fail=1
fi
nros_grep_q '_cargo-build_nros_cpp:' <<<"$targets" \
    || { say "no _cargo-build_nros_cpp in the build graph"; fail=1; }

if ! placed="$(find "$b" -path '*/nros-cpp/nros-placed/*' -name nros_config_generated.h | head -1)"; then
    placed=""
fi
[ -n "$placed" ] || { say "nros-cpp placed no C sizes header under $b"; exit 1; }
ninja -C "$b" -t deps > "$work/deps.txt" || { say "ninja -t deps failed in $b"; exit 1; }
mapfile -t compiled < <(grep -oE '/[^ ]*/rclc/detail/nros_config_generated\.h$' "$work/deps.txt" | sort -u || true)
wait $! || true
[ "${#compiled[@]}" -gt 0 ] || { say "no TU compiled a C sizes header (ninja -t deps)"; fail=1; }

if ! entry="$(find "$b" -maxdepth 1 -type f -name '*_entry' | head -1)"; then
    entry=""
fi
[ -n "$entry" ] || { say "no *_entry image in $b"; exit 1; }
ninja -C "$b" -t query "$(basename "$entry")" > "$work/query.txt" \
    || { say "ninja -t query $(basename "$entry") failed in $b"; exit 1; }
mapfile -t archives < <(grep -oE '[^ ]*libnros_c(pp)?\.a$' "$work/query.txt" | sort -u || true)
wait $! || true
if [ "${#archives[@]}" -ne 1 ]; then
    say "the image links ${#archives[@]} runtime archive(s), want one: ${archives[*]:-none}"; fail=1
else
    nm -A "$b/${archives[0]}" > "$work/nm.txt" 2>/dev/null || true
    for h in "${compiled[@]}"; do
        why="$(pairing_problem "$h" "$placed" "$work/nm.txt")"
        [ -z "$why" ] || { say "$why"; fail=1; }
    done
fi

[ "$fail" -eq 0 ] || exit 1
say "OK — the C talker ran cargo once (_cargo-build_nros_cpp); ${#compiled[@]} C sizes header path(s) compiled, all the copy that run placed; the one runtime archive (${archives[0]}) defines the header's $(header_variant "${compiled[0]}"); selftest: a header from another run and an archive missing the variant are refused"
