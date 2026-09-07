#!/usr/bin/env bash
# Refuse a Zephyr workspace that lives inside a DIFFERENT nano-ros checkout.
#
# phase-431 W1 gave the CLI an ownership guard: if the directory `nros` is
# operating on sits inside a nano-ros checkout, the running binary must be that
# checkout's own `packages/cli/target/**` build. A foreign binary emits with ITS
# OWN codegen, which can differ from this checkout's while carrying the same
# codegen version — the version catches an incompatible emitter, not one that
# merely moved.
#
# The guard is right. WHERE it fires is the problem. A shared west workspace
# nested inside a second checkout is a property of the HOST, decided once at
# provisioning time, but the refusal arrives ~15 minutes into a tier-2 fixture
# build, inside a cmake configure, as an error about a binary — naming neither
# the workspace, nor the second checkout, nor the fix. On the self-hosted runner
# that shape cost the tier-2 lane two consecutive nights (2026-09-06, -07):
#
#   == zephyr == FAILED (rc=2)
#   Error: this `nros` does not belong to the checkout it is being run against.
#   FATAL ERROR: ... -B/mnt/<disk>/<user>/nano-ros/zephyr-workspace/...
#
# (the path is written generically on purpose: `<user>/nano-ros` spelled out
# is a forbidden string here, since it reads as a GitHub org that is not ours)
#
# ...while the checkout under test was /home/aeon/actions-runner/_work/... .
# Nothing in this repository selects that path (it is the runner's own
# environment), so a reader of the tree could not discover the cause from it —
# which is why the check belongs here, where the resolution happens.
#
# The three conditions below MIRROR the guard's exactly, so this can never be
# stricter than the thing it front-runs: a workspace outside any checkout is
# fine (the guard's own first silent case), our own checkout is fine, and a
# checkout carrying no `packages/cli` sources is fine because nothing there
# could have built a CLI to be foreign to. `NROS_SKIP_STALE_CHECK=1` disables
# the guard, so it disables this too — otherwise this would fail a host that
# has deliberately opted out.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"

[ "${NROS_SKIP_STALE_CHECK:-}" = "1" ] && exit 0

ws="${NROS_ZEPHYR_WORKSPACE:-}"
if [ -z "$ws" ]; then
    for cand in "$repo_root/zephyr-workspace" "$repo_root/../nano-ros-workspace" \
                "$repo_root/../nano-ros-workspace-4.4"; do
        [ -d "$cand/zephyr" ] && ws="$cand" && break
    done
fi
# No workspace at all is not this check's business — the caller already warns.
[ -n "$ws" ] && [ -d "$ws" ] || exit 0

# Resolve symlinks: a `zephyr-workspace` symlinked onto a big disk reaches the
# same second checkout as an absolute NROS_ZEPHYR_WORKSPACE, and the build tools
# hand cmake the resolved path either way.
ws_real="$(cd "$ws" 2>/dev/null && pwd -P)" || exit 0

# The guard's marker, and its lexical walk upward.
owner=""
dir="$ws_real"
while [ -n "$dir" ] && [ "$dir" != "/" ]; do
    if [ -f "$dir/packages/core/nros-core/Cargo.toml" ]; then owner="$dir"; break; fi
    dir="$(dirname "$dir")"
done

[ -n "$owner" ] || exit 0                            # outside any checkout — fine
[ "$owner" != "$repo_root" ] || exit 0               # our own checkout — fine
[ -f "$owner/packages/cli/Cargo.toml" ] || exit 0    # no CLI sources — nothing to be foreign to

cat >&2 <<EOF
The Zephyr workspace sits inside a DIFFERENT nano-ros checkout, so every
\`nros\` invocation against it will be refused by the phase-431 W1 ownership
guard — not here, but deep inside the fixture build.

  workspace: $ws_real
  owned by:  $owner
  this tree: $repo_root

That second checkout has \`packages/cli\`, so the guard resolves ownership to
IT and expects $owner/packages/cli/target/**/nros — never this tree's build.

Fix by moving the workspace OUT of any checkout, not by pointing it at one:

  NROS_ZEPHYR_WORKSPACE=$(dirname "$owner")/zephyr-workspace

A workspace outside every checkout takes the guard's own first silent case
("the cwd is not in a checkout"), so nothing is weakened and no code changes.
Setting NROS_SKIP_STALE_CHECK=1 also silences it, but that disables a guard
whose whole job is to stop a foreign CLI emitting different codegen under the
same version — a loud failure traded for a quiet wrong answer.
EOF
exit 1
