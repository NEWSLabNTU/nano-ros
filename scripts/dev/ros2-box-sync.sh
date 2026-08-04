#!/usr/bin/env bash
# Mirror this checkout into the ROS distrobox's OWN tree (issues 0400, 0401).
#
# WHY A SECOND TREE AT ALL
#
# Host and box ran against one checkout, and every build artifact in it is
# glibc- and toolchain-specific. That single fact produced a different failure
# every time it surfaced:
#
#   - a host-built `build-script-build` re-run in the box   -> GLIBC_2.39 not found
#   - a host-configured CMake cache reused in the box       -> `sccache` not found,
#                                                              then `strlcpy` undefined
#   - a host-built `nros` / `nros-launch-resolve`           -> cannot exec / wrong libpython
#   - fixtures built into the box's redirected target dir   -> tests stat leaf paths and
#                                                              find the HOST's stale binary
#
# Each was fixed where it appeared. There were five of them in one session,
# because the premise — one tree, two incompatible toolchains — was never
# addressed. `CARGO_TARGET_DIR` redirection cannot address it either: the
# fixture contract is LEAF-RELATIVE by design (`examples/**/target-<rmw>/…`), so
# redirecting the box's cargo output moves fixtures away from where the tests
# look. That is issue 0401: the two mechanisms are mutually exclusive.
#
# Two trees make the question go away. The box compiles into its own checkout,
# at its own paths, and nothing it writes is visible to the host.
#
# WHY A MIRROR AND NOT `git worktree`
#
# A worktree cannot check out the branch the host already has, and — decisive —
# it carries only COMMITTED state. The normal loop here is edit, build in the
# box, test; a worktree would test the last commit instead of the edit. So this
# rsyncs the working tree, uncommitted changes included.
#
# `.git` comes along (1.3 GB): build scripts read it for source stamps, and the
# play_launch pin check (#409) runs `git -C` inside a submodule. Without it the
# box would stamp `unknown` everywhere and silently lose those guards.
#
# ONE-WAY, always. Edits belong on the host; the box is a build target. The
# mirror is refreshed before box work, never merged back.
set -euo pipefail

src="$(cd -P "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
dst="${NROS_BOX_TREE:-$(cd -P "$src/.." && pwd -P)/$(basename "$src")-box}"

if [ "$src" = "$dst" ]; then
    echo "ros2-box-sync: source and destination are the same path ($src)" >&2
    exit 1
fi

# Build outputs are per-tree by definition and are the whole point of the split:
# never copy them, and never delete the box's own (rsync protects excluded paths
# from --delete unless --delete-excluded is passed, which it deliberately is not).
#
# `.cargo-target-box` is the box's redirected dir from before this existed; it
# sits beside the checkout and must not be dragged in.
exclusions=(
    --exclude 'target'
    --exclude 'target-*'
    --exclude 'build'
    --exclude 'build-*'
    --exclude '.cargo-target-box'
    --exclude 'tmp'
    --exclude 'test-logs'
    --exclude 'node_modules'
)

mkdir -p "$dst"
echo "ros2-box-sync: $src -> $dst"
rsync -a --delete "${exclusions[@]}" "$src/" "$dst/"

# `nros sync` writes ABSOLUTE paths into each leaf's `.cargo/config.toml`
# (RFC-0048 W9), so a mirrored leaf still points at the SOURCE tree and cargo
# resolves `nros` against crates.io instead of the checkout. This is the
# documented "moved checkout -> re-run `nros sync`" rule; it applies to a mirror
# for exactly the same reason.
echo "ros2-box-sync: re-run \`nros sync <workspace>\` inside the box for anything you build —"
echo "               leaf .cargo/config.toml files carry absolute paths to the source tree."
echo ""
echo "ros2-box-sync: done. Enter the box against the MIRROR:"
echo "    DBX_CONTAINER_MANAGER=docker distrobox enter ros2 -- \\"
echo "        bash -c 'cd $dst && . scripts/dev/ros2-box-env.sh && <cmd>'"
