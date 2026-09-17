#!/usr/bin/env bash
# Make a Zephyr workspace's west manifest project carry NO checkout.
#
# issue 1258 / phase-449 W1.
#
# `setup.sh` used to finish workspace init with:
#
#     rm -rf  "$WORKSPACE_DIR/$NANO_ROS_NAME"
#     ln -sf  "$NANO_ROS_ROOT" "$WORKSPACE_DIR/$NANO_ROS_NAME"
#
# so the manifest project WAS the checkout that ran setup. West's project list
# is what Zephyr's module discovery reads, so every build in that workspace got
# its `nros` module from that one tree — measured in a real build dir:
#
#     "nros":"<workspace>/nano-ros"
#
# That was harmless while a workspace sat beside ONE checkout. Since phase-440
# W4 the default target is `$NROS_STORE/workspaces/zephyr/<version>`, and
# RFC-0095 D2's whole point is that one workspace is SHARED by every project
# wanting that version. The first project to provision a line then bound every
# later project on the host to its nano-ros tree, and nothing reported the mix.
# It is also RFC-0095 D1 inverted: the provisioned tree does not live inside a
# checkout, but it reaches into one, so deleting that checkout breaks every
# project's Zephyr build.
#
# After this the project is a real directory holding the manifest FILE and
# nothing else — no `zephyr/module.yml`, so no `nros` module — and each build
# names its own with `-DZEPHYR_EXTRA_MODULES=<checkout>`. Measured on a real
# `native_sim` image: `"nros":"/home/aeon/repos/nano-ros"`, and it links.
#
# Idempotent: a project that is already a plain directory is left alone, so
# this is safe to call before every build as well as from provisioning.
#
# Usage: unbind-manifest-project.sh <workspace-dir> <project-name> <manifest-file> [<manifest-source-dir>]
set -euo pipefail

ws="${1:?workspace dir}"
name="${2:?manifest project name}"
manifest="${3:?manifest file name}"
src="${4:-}"

proj="$ws/$name"

# Nothing provisioned yet is not an error: provisioning calls this after it has
# created the project, and a build calls it on a workspace that may not exist.
[ -d "$ws" ] || exit 0

if [ ! -L "$proj" ]; then
    # Already unbound, or never bound. The manifest file still has to be there
    # for `west` to resolve the workspace at all.
    if [ -d "$proj" ] && [ ! -f "$proj/$manifest" ] && [ -n "$src" ] && [ -f "$src/$manifest" ]; then
        cp "$src/$manifest" "$proj/$manifest"
    fi
    exit 0
fi

target="$(readlink -f "$proj" 2>/dev/null || true)"

# Take the manifest from the tree the link pointed at, so an unbind cannot lose
# the file — the caller's `<manifest-source-dir>` is a fallback, not the truth.
tmp="$(mktemp -d "${TMPDIR:-/tmp}/nros-unbind.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT
if [ -n "$target" ] && [ -f "$target/$manifest" ]; then
    cp "$target/$manifest" "$tmp/$manifest"
elif [ -n "$src" ] && [ -f "$src/$manifest" ]; then
    cp "$src/$manifest" "$tmp/$manifest"
else
    echo "unbind-manifest-project: no $manifest in ${target:-<broken link>} or ${src:-<none>}" >&2
    exit 1
fi

# `rm` the LINK, never its target. A `rm -rf "$proj/"` with a trailing slash
# would follow it and delete the checkout.
rm -f "$proj"
mkdir -p "$proj"
cp "$tmp/$manifest" "$proj/$manifest"

echo "unbind-manifest-project: $proj no longer points into ${target:-a checkout}" >&2
echo "  The workspace is keyed by Zephyr version alone now (issue 1258); each" >&2
echo "  build names its own module with -DZEPHYR_EXTRA_MODULES=<checkout>." >&2
