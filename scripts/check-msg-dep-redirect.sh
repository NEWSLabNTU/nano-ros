#!/usr/bin/env bash
#
# Issue 0378 — every bare message-crate dependency must carry a COMMITTED
# redirect, because those names belong to somebody else on crates.io.
#
# THE EXPOSURE
#
# In-tree leaves declare their message deps by REGISTRY NAME with a wildcard:
#
#     std_msgs = { version = "*", default-features = false }
#
# nano-ros publishes nothing to crates.io. Those names are taken:
#
#     std_msgs = "0.0.0"            # "std_msgs ros2 rust generated dependencies"
#     builtin_interfaces = "0.0.0"  # "Ros2 builtin_interfaces"
#
# So the ONLY thing standing between a build and a stranger's crate is the
# `[patch.crates-io]` redirect in the leaf's `.cargo/config.toml`. Today a
# resolution that reaches the registry fails — but it fails because the
# published 4.2.3 happens to be YANKED, and a yank is not a security control.
# Publish a matching version and the same build succeeds against foreign code.
#
# WHAT THIS DOES AND DOES NOT COVER
#
# Covers: a leaf gaining a message dep without the matching redirect — the
# regression that would make the exposure reachable through the normal path.
#
# Does NOT cover: `cargo ... --manifest-path <leaf>` invoked from ANOTHER
# directory. Cargo discovers `.cargo/config.toml` from the CURRENT DIRECTORY,
# not from the manifest, so the leaf's redirect is never loaded and resolution
# goes to crates.io. That reproduces on a FULLY SYNCED tree:
#
#     $ cargo metadata --manifest-path packages/testing/nros-bench/stress-zenoh/Cargo.toml
#     error: failed to select a version for the requirement `std_msgs = "*"`
#       version 4.2.3 is yanked
#       location searched: crates.io index
#
# No repo-side config can fix that one: a `[patch]` maps a name to ONE path,
# and each leaf redirects to its own per-leaf `generated/` tree. Closing it
# structurally means one canonical vendored copy of the message crates, or
# names that do not exist on crates.io — an RFC-0048/0023 decision, tracked in
# 0378 rather than improvised here across sixteen leaves.

set -euo pipefail
cd "$(dirname "$0")/.."

# Message crates reached by registry name. Extend when codegen gains another.
MSG_CRATES='std_msgs|builtin_interfaces|example_interfaces|geometry_msgs|sensor_msgs|lifecycle_msgs|action_msgs|rosgraph_msgs|nav_msgs|diagnostic_msgs|trajectory_msgs|shape_msgs|stereo_msgs|visualization_msgs|unique_identifier_msgs|test_msgs'

status=0
checked=0

# Cargo discovers `.cargo/config.toml` by walking UP from the directory it runs
# in, so a workspace MEMBER inherits its workspace root's config. Checking only
# the member's own directory reports every `examples/workspaces/*/src/*_pkg`
# as unprotected when the redirect is one level up, where it belongs — which is
# exactly what the first draft of this gate did, on 26 packages.
#
# Walk the same way cargo does, stopping at the repo root.
#
# BOTH TOML spellings count. `nros sync` writes the inline form:
#
#     std_msgs = { path = "generated/std_msgs" }   # nros-managed
#
# but hand-written leaves use the table form, which is identical to cargo:
#
#     [patch.crates-io.std_msgs]
#     path = "generated/std_msgs"
#
# Matching only the first reported `wake-latency-cortex-m3` as unprotected when
# it is correctly redirected — a gate that flags a REAL protection as missing
# teaches people to ignore it.
redirect_for() {
    local dir="$1" dep="$2" cfg
    while :; do
        cfg="$dir/.cargo/config.toml"
        if [ -f "$cfg" ] &&
            grep -qE "^[[:space:]]*($dep[[:space:]]*=|\[patch\.crates-io\.$dep\])" "$cfg"; then
            printf '%s' "$cfg"
            return 0
        fi
        [ "$dir" = "." ] || [ -z "$dir" ] && break
        dir="$(dirname "$dir")"
    done
    return 1
}

while IFS= read -r manifest; do
    dir="$(dirname "$manifest")"
    cfg="$dir/.cargo/config.toml"

    # Registry-style message deps in this manifest: `name = { version = ...` or
    # `name = "..."`. A path/git dep is not registry-resolved and is fine.
    deps="$(grep -oE "^[[:space:]]*($MSG_CRATES)[[:space:]]*=[[:space:]]*[{\"]" "$manifest" 2>/dev/null |
        grep -oE "($MSG_CRATES)" | sort -u || true)"
    [ -z "$deps" ] && continue

    while IFS= read -r dep; do
        [ -z "$dep" ] && continue
        # A path/git form for this dep means it never touches the registry.
        if grep -qE "^[[:space:]]*$dep[[:space:]]*=[[:space:]]*\{[^}]*(path|git)[[:space:]]*=" "$manifest"; then
            continue
        fi
        checked=$((checked + 1))
        if ! found_cfg="$(redirect_for "$dir" "$dep")"; then
            status=1
            echo "[FAIL] $manifest declares registry dep '$dep' with no [patch.crates-io]" >&2
            echo "       redirect in any .cargo/config.toml from its directory up to the" >&2
            echo "       repo root. Unredirected, that name resolves to a THIRD PARTY's" >&2
            echo "       crate on crates.io (issue 0378). Run \`nros sync\` and commit it." >&2
        else
            : "${found_cfg}"
        fi
    done <<<"$deps"
done < <(git ls-files '*/Cargo.toml' | grep -v '^third-party/' | grep -v '^packages/cli/')

if [ "$status" -eq 0 ]; then
    echo "msg-dep redirects OK — $checked registry-named message dep(s), all redirected."
fi
exit "$status"
