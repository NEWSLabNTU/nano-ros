#!/usr/bin/env bash
# Stage the PX4 POSIX (SITL) build inputs, PREBUILT, into a release prefix —
# phase-541 W1, RFC-0097 D13, RFC-0039 "Installed toolchain". Issue 2289.
#
# A PX4 external module links nano-ros through
# `integrations/px4/NanoRosPx4Module.cmake`, which compiles nothing of
# nano-ros's own runtime inside PX4's cmake (PX4's `-Werror -Wpedantic …` set
# does not survive it, phase-325 W1.1). It LINKS two prebuilt archives and
# compiles against the per-build headers that came out of the same cargo run.
# In a checkout a contributor builds those (`just px4 build-sitl-example`); an
# installed toolchain has no checkout to build in, and the book cannot tell a
# reader to run cargo inside the SDK root (RFC-0117, RFC-0124 Principle 5). So
# the release ships them.
#
# WHAT IS STAGED — measured by reading the module and building once:
#
#   share/nano-ros/prebuilt/px4-posix/
#     lib/libnros_cpp.a             the umbrella, built for an in-firmware uORB
#                                   module: `nros-cpp` with the module's own
#                                   feature set (read from the module below),
#                                   which carries `nros-c` and the `rmw-cffi`
#                                   slot the uORB backend registers into. NO
#                                   networked backend — a bridge module builds
#                                   its own archive (issue 0436).
#     lib/libnros_platform_posix.a  the C platform shim
#                                   (`packages/platform/nros-platform-posix`).
#     placed/                       the per-build config headers, PLACED by
#                                   `scripts/build/cargo-out-dir-headers.py`
#                                   out of the SAME cargo run's `$OUT_DIR`
#                                   (issue 1824): `nros_config_generated.h`
#                                   (C sizes) and `nros_cpp_config_generated.h`
#                                   (C++), in the layout `nros locate artifact
#                                   --placed-dir` reads.
#     nros-px4-prebuilt.toml        what this is and which commit built it.
#
# Everything else the module reads (the source include dirs, the uORB
# backend's sources, `integrations/px4/include`) is SOURCE and is already in
# the SDK root (`scripts/stage-sdk-root.sh`).
#
# The archives contain no PX4 code: the uORB backend is compiled by PX4's own
# build, from the SDK root's sources, against the PX4 tree the user brings. So
# what ships is pinned to the host ABI (glibc, x86_64), not to a PX4 version.
#
# WHY THE ARCHIVES CANNOT DISAGREE WITH THE TOOLCHAIN THEY SHIP IN
#
#   * Same job, same commit: this runs in the release job after the SDK root is
#     staged from HEAD, and builds from that checkout.
#   * The stamp records `nano_ros`, copied from `share/nros/manifest.toml`, and
#     the module compares the two at configure — a prebuilt dir carried into
#     another toolchain's prefix is refused, naming both commits.
#   * The headers and the archive are paired by content: the module's
#     `nros_assert_archive_pairs_with_header` reads each header's variant
#     symbol and refuses an archive that does not define it. This script runs
#     the same check before anything ships.
#
# Usage: scripts/stage-px4-posix-archives.sh <prefix>
#   <prefix>/share/nano-ros (stage-sdk-root.sh) and
#   <prefix>/share/nros/manifest.toml (`nros dev sdk manifest`) must exist.

set -euo pipefail

if [ $# -ne 1 ]; then
    echo "usage: $0 <prefix>" >&2
    exit 2
fi
prefix="$1"
repo="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)"

# Kept in step with `_NROS_PX4_PREBUILT_SUBDIR` in NanoRosPx4Module.cmake.
PREBUILT_SUBDIR="share/nano-ros/prebuilt/px4-posix"
STAMP_NAME="nros-px4-prebuilt.toml"

sdk="$prefix/share/nano-ros"
manifest="$prefix/share/nros/manifest.toml"
out="$prefix/$PREBUILT_SUBDIR"
module="$repo/integrations/px4/NanoRosPx4Module.cmake"
# shellcheck disable=SC1091
. "$repo/scripts/build/cargo.sh"
# shellcheck disable=SC1091
. "$repo/scripts/lib/grep-q.sh"

fail() { echo "nano-ros: stage-px4-posix-archives: $*" >&2; exit 1; }

[ -d "$sdk" ] || fail "$sdk is not a staged SDK root — run scripts/stage-sdk-root.sh $prefix first"
[ -f "$manifest" ] || fail "$manifest is missing — the release records its manifest before staging"
[ ! -e "$out" ] || fail "$out already exists — stage into a fresh prefix"
# THIS checkout's CLI, or nothing: the archives are built from this checkout,
# so the layout question must be asked of its own binary (issue 1850).
nros="$(nros_cli_bin --checkout-only)" \
    || fail "this checkout's nros CLI is not built — the release job builds it first"

nano_ros="$(sed -n 's/^nano_ros = "\(.*\)"$/\1/p' "$manifest")"
[ -n "$nano_ros" ] || fail "$manifest records no nano_ros commit, so the archives could not be tied to it"

# The feature set is the MODULE's, read from its one spelling, so the archive
# shipped and the archive a checkout's remedy builds cannot differ.
features="$(sed -n 's/^set(_NROS_PX4_UORB_FEATURES "\([^"]*\)")$/\1/p' "$module")"
[ -n "$features" ] || fail "could not read _NROS_PX4_UORB_FEATURES from $module"

mkdir -p "$out/lib" "$out/placed"

echo "nano-ros: building libnros_cpp.a for PX4 POSIX (features $features)"
# profile-literal-ok: prebuilt archive shipped at a fixed path in the release
python3 "$repo/scripts/build/cargo-out-dir-headers.py" \
    --package nros-cpp --package nros-c --dest "$out/placed" -- \
    cargo build --manifest-path "$repo/Cargo.toml" -p nros-cpp \
    --no-default-features --features "$features" --release \
    --message-format=json-render-diagnostics

# Where cargo put the archive, and where the placer put each header, are the
# ONE layout `nros locate artifact` answers (`nros_build_paths::artifact`, the
# function the build scripts that write the headers call) — the same question
# the module asks at configure. The CLI is the checkout's own build, which the
# release job made in its first step.
locate() { "$nros" locate artifact "$@" --index "$repo/nros-sdk-index.toml"; }
cp "$(locate cpp-archive --profile release)" "$out/lib/libnros_cpp.a"

echo "nano-ros: building libnros_platform_posix.a"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
# nros-cmake-prefix-exempt: the POSIX platform's C archive -- no Rust target in
# that project, so no Corrosion to resolve.
cmake -S "$repo/packages/platform/nros-platform-posix" -B "$work/platform" \
    -DCMAKE_BUILD_TYPE=Release >/dev/null
cmake --build "$work/platform" >/dev/null
cp "$work/platform/libnros_platform_posix.a" "$out/lib/libnros_platform_posix.a"

host="$(rustc -vV | sed -n 's/^host: //p')"
rustc_v="$(rustc -V)"
printf '%s\n' \
    "# Written by scripts/stage-px4-posix-archives.sh (phase-541). Read by" \
    "# integrations/px4/NanoRosPx4Module.cmake; do not edit." \
    "schema = 1" \
    "nano_ros = \"$nano_ros\"" \
    "target = \"$host\"" \
    "features = \"$features\"" \
    "networked_backends = []" \
    "rustc = \"$rustc_v\"" \
    >"$out/$STAMP_NAME"

# VERIFY — the module's own pairing rule, asked before anything ships: each
# placed header names a variant symbol, and the archive must define it.
pair() {  # pair <artifact name> <symbol prefix>
    local h sym rc
    h="$(locate "$1" --placed-dir "$out/placed")"
    [ -f "$h" ] || fail "the placer wrote no $1 (expected $h)"
    sym="$(grep -o -m1 "$2[A-Za-z0-9_]*" "$h" || true)"
    [ -n "$sym" ] || fail "$h carries no '$2*' variant stamp"
    rc=0
    nros_grep_q --text "$sym" "$out/lib/libnros_cpp.a" || rc=$?
    case $rc in
        1) fail "libnros_cpp.a does not define $sym, which $h names — header and archive are from different builds" ;;
    esac
    echo "nano-ros: $1 pairs with libnros_cpp.a ($sym)"
}
pair c-config-header nros_config_variant_
pair cpp-config-header nros_cpp_config_variant_

# No networked backend may ride in (issue 1050: on hosted POSIX it registers
# from an .init_array ctor before main and takes RMW slot 0). The stamp
# promises `networked_backends = []`; this is what makes the promise true.
for b in zenoh xrce cyclonedds; do
    rc=0
    nros_grep_q --text "nros_rmw_${b}_register" "$out/lib/libnros_cpp.a" || rc=$?
    case $rc in
        0) fail "libnros_cpp.a carries the $b backend; the shipped archive must be uORB-only" ;;
    esac
done

du -b "$out/lib/libnros_cpp.a" "$out/lib/libnros_platform_posix.a"
echo "nano-ros: staged the PX4 POSIX archives -> $out ($(du -sb "$out" | cut -f1) bytes)"
