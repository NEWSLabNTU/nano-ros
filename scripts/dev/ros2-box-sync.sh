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
# Patterns are ANCHORED where a bare name would also match SOURCE. `--exclude
# 'build'` matches any directory called `build` at any depth — including
# `scripts/build/`, which is tracked source, and dropping it broke the mirror's
# `check-board-manifest-drift` with a missing `scripts/build/cargo.sh`. Only
# `scripts/build` is tracked under that name (`git ls-files` confirms), so the
# root `/build` is anchored and everything else stays name-matched.
exclusions=(
    --exclude 'target'
    --exclude 'target-*'
    --exclude '/build'
    --exclude 'build-*'
    --exclude '.cargo-target-box'
    --exclude '/tmp'
    --exclude '/test-logs'
    --exclude 'node_modules'
    # issue 0401 follow-up — path-carrying GENERATED files must NOT be mirrored.
    # `nros sync` writes ABSOLUTE paths (RFC-0048 W9), so a copied
    # `nros-patch.toml` / leaf `.cargo/config.toml` points at the SOURCE tree.
    # Worse than useless: it half-works. The box then has some leaves rewritten
    # to box paths and a central patch still naming the host, and
    # `check-dep-chain` fails on four boards with `no matching package named
    # \`nros\`` — a resolution error that says nothing about mirroring.
    #
    # Only the ROOT `nros-patch.toml` is excluded, and only because it is
    # gitignored and purely generated. Leaf `.cargo/config.toml` files are NOT:
    # 63 of them are TRACKED source, and excluding the name globally deleted
    # those from the mirror, which broke resolution far worse (`no matching
    # package named `nros`` with no config present at all). They are mirrored,
    # and the box's own `nros sync` rewrites the absolute paths inside them.
    --exclude '/nros-patch.toml'
)

mkdir -p "$dst"
echo "ros2-box-sync: $src -> $dst"
rsync -a --delete "${exclusions[@]}" "$src/" "$dst/"

# The marker that tells `ros2-box-env.sh` this tree is box-OWNED (and so must
# NOT redirect CARGO_TARGET_DIR) lives only here, in the destination — the
# source has no such file. `--delete` therefore removed it on every re-sync, and
# the box silently fell back to the redirect it was supposed to stop using.
# Everything still built, so nothing complained; the symptom surfaced far away,
# as `check-c` compiling against headers the redirected build had written to the
# OLD location.
#
# Write it AFTER rsync, every time, so the mirror cannot exist without it.
touch "$dst/.nros-box-tree"

# A CMakeCache.txt records the ABSOLUTE source and build directories it was
# generated for. One copied from the source tree therefore points at the source
# tree, and cmake refuses outright:
#
#   CMake Error: The current CMakeCache.txt directory <dst>/…/build/CMakeCache.txt
#   is different than the directory <src>/…/build where CMakeCache.txt was created.
#
# Excluded paths are protected from `--delete`, which is right for the box's OWN
# build output but means anything copied under an EARLIER, wronger exclusion set
# stays forever. That is how these arrived: a window where the rule was anchored
# to `/build` and nested build dirs were mirrored.
#
# Remove only caches that provably belong to another tree — the ones naming a
# home directory outside this destination. The box's own caches name the box.
while IFS= read -r cache; do
    [ -n "$cache" ] || continue
    home="$(sed -n 's/^CMAKE_HOME_DIRECTORY:INTERNAL=//p' "$cache" 2>/dev/null | head -1)"
    case "$home" in
        "$dst"/*|"") continue ;;
    esac
    echo "ros2-box-sync: dropping foreign cmake cache ($home) -> $(dirname "$cache")"
    rm -rf "$(dirname "$cache")"
done < <(find "$dst" -name CMakeCache.txt 2>/dev/null)

# `nros sync` writes ABSOLUTE paths into each leaf's `.cargo/config.toml`
# (RFC-0048 W9), so a mirrored leaf still points at the SOURCE tree and cargo
# resolves `nros` against crates.io instead of the checkout. This is the
# documented "moved checkout -> re-run `nros sync`" rule; it applies to a mirror
# for exactly the same reason.
echo "ros2-box-sync: run \`just generate-bindings\` (or \`nros sync <ws>\`) inside the box before"
echo "               building: the path-carrying generated files are deliberately NOT mirrored,"
echo "               so the box writes its own with box paths."
echo ""
echo "ros2-box-sync: done. Enter the box against the MIRROR:"
echo "    DBX_CONTAINER_MANAGER=docker distrobox enter ros2 -- \\"
echo "        bash -c 'cd $dst && . scripts/dev/ros2-box-env.sh && <cmd>'"
