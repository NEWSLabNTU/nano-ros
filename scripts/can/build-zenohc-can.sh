#!/usr/bin/env bash
# Build a libzenohc.so that carries a CAN link.
#
#   scripts/can/build-zenohc-can.sh --zenoh <path-to-zenoh-fork> [options]
#
#   --link can        the RFC-0080 multicast CAN link (phase-378). Default
#   --link isotp      the RFC-0083 CAN unicast link over ISO-TP (phase-393)
#   --zenoh <dir>     checkout of the zenoh fork carrying zenoh-link-<link>,
#                     on a branch whose version matches the zenoh-c tag below
#   --version <tag>   zenoh-c tag to build. Default: whatever the installed
#                     ROS zenoh_cpp_vendor package ships, so the result is
#                     ABI-substitutable for it
#   --out <dir>       where to leave libzenohc.so. Default: <work>/lib
#   --work <dir>      scratch dir for the zenoh-c checkout
#
# Why this script exists rather than a patch file: the redirection to the fork
# is a set of absolute paths, and there is a trap that costs an hour to find if
# you apply the obvious patch by hand. See "the opaque-types trap" below.
#
# Which link you want depends on what you need to work. The multicast CAN link
# carries pushed data only, because zenoh routes queries and liveliness to
# unicast faces -- so ROS topics work over it and services, actions, parameters
# and graph introspection do not. The ISO-TP link is a real unicast face and
# carries all of them.
#
# The output is a drop-in replacement for the vendored library:
#
#   source /opt/ros/$ROS_DISTRO/setup.bash
#   export LD_LIBRARY_PATH=<out>:$LD_LIBRARY_PATH
#
# `librmw_zenoh_cpp.so` and `rmw_zenohd` name libzenohc.so as a plain DT_NEEDED
# with no RPATH or RUNPATH, and the vendored library carries no DT_SONAME, so
# prepending a directory substitutes it wholesale. No ROS rebuild is needed
# because a cargo feature adds no C API.
set -euo pipefail

# issue 0726 — `grep -q` in a CONDITIONAL cannot tell a non-match (exit 1) from
# the tool failing to run at all (exit >=2). Under a 32-way fan-out a forked
# grep that failed to start was once reported as a missing force-link anchor:
# a false, specific claim, and only under load. `nros_grep_q` exits 2 on a tool
# failure instead of reporting a finding.
# shellcheck source=scripts/lib/grep-q.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../lib" && pwd)/grep-q.sh"

ZENOH_DIR=""
ZENOHC_TAG=""
OUT_DIR=""
WORK_DIR=""
LINK="can"
VENDOR_PREFIX="/opt/ros/humble/opt/zenoh_cpp_vendor"

while [ $# -gt 0 ]; do
    case "$1" in
        --link)    LINK="$2"; shift 2 ;;
        --zenoh)   ZENOH_DIR="$2"; shift 2 ;;
        --version) ZENOHC_TAG="$2"; shift 2 ;;
        --out)     OUT_DIR="$2"; shift 2 ;;
        --work)    WORK_DIR="$2"; shift 2 ;;
        --vendor-prefix) VENDOR_PREFIX="$2"; shift 2 ;;
        -h | --help)
            awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "$0"
            exit 0
            ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
done

TAG="zenohc-$LINK"
die() { echo "[$TAG] error: $*" >&2; exit 1; }
say() { echo "[$TAG] $*"; }

case "$LINK" in
    can)
        # Each link is verified by a string only it can produce, so a feature
        # flag that silently did nothing is caught here rather than at runtime
        # on a bus.
        LINK_MARKER="CAN: no such interface"
        LINK_EXAMPLE='"can/can0#bitrate=500000;dbitrate=2000000;id=0x100;match=0;mask=0"'
        LINK_DOC="docs/design/0081-can-link-for-zenoh-rs.md"
        ;;
    isotp)
        LINK_MARKER="ISO-TP: no such interface"
        LINK_EXAMPLE='"isotp/can0#tx_id=0x7E0;rx_id=0x7E8"'
        LINK_DOC="docs/design/0083-can-unicast-over-isotp.md"
        ;;
    *) die "--link must be 'can' or 'isotp', not '$LINK'" ;;
esac
WORK_DIR="${WORK_DIR:-${TMPDIR:-/tmp}/zenohc-$LINK-build}"

[ -n "$ZENOH_DIR" ] || die "--zenoh is required: the zenoh fork carrying zenoh-link-$LINK"
ZENOH_DIR="$(cd "$ZENOH_DIR" && pwd)" || die "--zenoh path does not exist"
[ -d "$ZENOH_DIR/io/zenoh-links/zenoh-link-$LINK" ] ||
    die "$ZENOH_DIR has no io/zenoh-links/zenoh-link-$LINK; wrong checkout or wrong branch"

# The version is not a free choice. rmw_zenoh_cpp is compiled against the
# vendored zenoh-c headers, so the replacement must be the same version.
VENDOR_VERSION_FILE="$VENDOR_PREFIX/lib/cmake/zenohc/zenohcConfigVersion.cmake"
if [ -z "$ZENOHC_TAG" ]; then
    [ -f "$VENDOR_VERSION_FILE" ] ||
        die "cannot find $VENDOR_VERSION_FILE; pass --version explicitly"
    ZENOHC_TAG="$(sed -n 's/^set(PACKAGE_VERSION "\([^"]*\)").*/\1/p' "$VENDOR_VERSION_FILE" | head -1)"
    [ -n "$ZENOHC_TAG" ] || die "could not read the vendored zenoh-c version"
    say "matching the installed zenoh_cpp_vendor: zenoh-c $ZENOHC_TAG"
fi

# The fork's version must equal the zenoh-c tag, or cargo will refuse the patch.
FORK_VERSION="$(sed -n 's/^version = "\([^"]*\)".*/\1/p' "$ZENOH_DIR/Cargo.toml" | head -1)"
if [ "$FORK_VERSION" != "$ZENOHC_TAG" ]; then
    die "the fork at $ZENOH_DIR is version $FORK_VERSION but zenoh-c $ZENOHC_TAG is wanted.
     Check out the fork branch built on release/$ZENOHC_TAG."
fi

# Match the feature set the vendored library was built with. `unstable` and
# `shared-memory` move struct layouts, so a mismatch is silent memory
# corruption rather than a link error -- there is no soname to catch it.
# Transport features do not affect the ABI.
CONFIGURE_H="$VENDOR_PREFIX/include/zenoh_configure.h"
FEATURES="transport_$LINK"
if [ -f "$CONFIGURE_H" ]; then
    while read -r define feature; do
        if nros_grep_q "^#define $define\b" "$CONFIGURE_H"; then
            FEATURES="$FEATURES,$feature"
        fi
    done <<'MAP'
Z_FEATURE_UNSTABLE_API unstable
Z_FEATURE_SHARED_MEMORY shared-memory
MAP
    say "feature set matched from $CONFIGURE_H"
else
    say "warning: $CONFIGURE_H not found; building with default features plus CAN."
    say "         If the installed library was built with unstable or shared-memory,"
    say "         the struct layouts will differ and the substitution will corrupt memory."
fi

mkdir -p "$WORK_DIR"
SRC="$WORK_DIR/zenoh-c-$ZENOHC_TAG"
if [ ! -d "$SRC/.git" ]; then
    say "cloning zenoh-c $ZENOHC_TAG"
    git clone --depth 1 --branch "$ZENOHC_TAG" https://github.com/eclipse-zenoh/zenoh-c "$SRC"
else
    say "reusing $SRC"
    git -C "$SRC" checkout -- Cargo.toml build-resources/opaque-types/Cargo.toml 2>/dev/null || true
fi

PATCH_BLOCK="
# Added by nros scripts/can/build-zenohc-can.sh: build against the local zenoh
# fork carrying the $LINK link instead of the upstream release branch.
[patch.\"https://github.com/eclipse-zenoh/zenoh.git\"]
zenoh = { path = \"$ZENOH_DIR/zenoh\" }
zenoh-ext = { path = \"$ZENOH_DIR/zenoh-ext\" }
zenoh-protocol = { path = \"$ZENOH_DIR/commons/zenoh-protocol\" }
zenoh-runtime = { path = \"$ZENOH_DIR/commons/zenoh-runtime\" }
zenoh-util = { path = \"$ZENOH_DIR/commons/zenoh-util\" }
"

# THE OPAQUE-TYPES TRAP. zenoh-c's build script builds a helper crate under
# build-resources/opaque-types to compute type sizes, and hands it the PARENT's
# Cargo.lock. That crate has its own manifest, so without the same redirection
# the two disagree about where zenoh comes from, the size probe yields nothing,
# and the build fails much later and unrecognisably as:
#     no sigatures found for building generic z_take_from_loaned
# Patching only the parent manifest is the obvious thing to do and it does not work.
for manifest in "$SRC/Cargo.toml" "$SRC/build-resources/opaque-types/Cargo.toml"; do
    [ -f "$manifest" ] || die "expected $manifest; zenoh-c layout changed"
    nros_grep_q 'transport_vsock = \["zenoh/transport_vsock"\]' "$manifest" ||
        die "$manifest has no transport_vsock feature line to anchor to; layout changed"
    sed -i "s|transport_vsock = \\[\"zenoh/transport_vsock\"\\]|&\\ntransport_$LINK = [\"zenoh/transport_$LINK\"]|" "$manifest"
    printf '%s' "$PATCH_BLOCK" >> "$manifest"
done
rm -f "$SRC/build-resources/opaque-types/Cargo.lock"
say "patched both manifests (parent and opaque-types)"

say "building with --features $FEATURES"
( cd "$SRC" && cargo build --release --features "$FEATURES" )

SO="$SRC/target/release/libzenohc.so"
[ -f "$SO" ] || die "build reported success but $SO is missing"

# Prove the link is actually in there rather than trusting the feature flag.
#
# NOT `strings ... | grep -q`: under `set -o pipefail`, grep -q exits on the
# first match, strings takes SIGPIPE, and the pipeline reports failure even
# though the string was found. `grep -c` reads its input to the end, so there is
# no early close to trip over.
LINK_MARKERS="$(strings "$SO" | grep -c "$LINK_MARKER" || true)"
if [ "$LINK_MARKERS" -eq 0 ]; then
    die "$SO does not contain the $LINK link. Is the fork branch the right one?"
fi
say "verified: the $LINK link is present in the built library"

OUT_DIR="${OUT_DIR:-$WORK_DIR/lib}"
mkdir -p "$OUT_DIR"
cp -f "$SO" "$OUT_DIR/libzenohc.so"

cat <<EOF

[$TAG] done: $OUT_DIR/libzenohc.so

To use it with the stock rmw_zenoh_cpp, with no ROS rebuild:

  source /opt/ros/${ROS_DISTRO:-humble}/setup.bash
  export LD_LIBRARY_PATH=$OUT_DIR:\$LD_LIBRARY_PATH
  ldd \$(ros2 pkg prefix rmw_zenoh_cpp)/lib/rmw_zenoh_cpp/rmw_zenohd | grep zenohc

Then give a session an endpoint, in its SESSION config and not the router's:

  $LINK_EXAMPLE

See $LINK_DOC.
EOF
