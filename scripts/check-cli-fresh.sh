#!/usr/bin/env bash
#
# Issue 0363 — fail FAST when the in-tree `nros` binary is older than its
# sources, instead of failing deep inside a lane that happens to use it.
#
# The staleness predicate itself already exists twice, and correctly:
#   * `scripts/build/cargo.sh :: nros_cli_bin()` — for callers going through
#     `just`,
#   * `nros-cli-core/src/stale_guard.rs` — for the binary invoked directly
#     (0363 B; `activate.sh` puts it straight on PATH).
# This adds no third spelling: it CALLS the shell one.
#
# What it adds is POSITION. Before this, a stale CLI surfaced at
# `check-dep-chain` — minutes into `just check`, as nine failed cells whose
# printed cause was a cargo resolution error. Here it is the first thing that
# runs, and it says what to do.
#
# Why the trap is easy to fall into, and why a gate is the right answer rather
# than "remember to rebuild": CLAUDE.md tells you to run `just format` before
# broad changes, `format-cli` reformats `packages/cli` in place, and that alone
# makes the binary stale. The documented workflow creates the condition.

set -euo pipefail
cd "$(dirname "$0")/.."

# shellcheck source=/dev/null
source scripts/build/cargo.sh

# `nros_cli_bin` resolves the CLI and applies the staleness check, printing its
# own actionable message and returning 3 when stale. Anything else (no binary
# at all, an out-of-tree install) is NOT this gate's business: a fresh clone
# without a built CLI is a normal state that `just setup-cli` handles, and
# failing here would make `check-fast` unrunnable before setup.
set +e
bin="$(nros_cli_bin 2>/tmp/.nros-cli-fresh.$$)"
rc=$?
set -e
msg="$(cat /tmp/.nros-cli-fresh.$$ 2>/dev/null || true)"
rm -f /tmp/.nros-cli-fresh.$$

if [ "$rc" -eq 3 ]; then
    printf '%s\n' "$msg" >&2
    echo "" >&2
    echo "       (issue 0363 — surfaced here rather than 20 minutes later in" >&2
    echo "        check-dep-chain, where the same cause reads as a cargo error.)" >&2
    exit 1
fi

if [ -z "$bin" ]; then
    echo "cli-fresh: SKIP — no in-tree nros binary yet (run \`just setup-cli\`)."
    exit 0
fi

echo "cli-fresh: OK — $bin is newer than its sources."
