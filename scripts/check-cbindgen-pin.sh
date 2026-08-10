#!/usr/bin/env bash
#
# Issue 0452 — the cbindgen requirement must stay EXACT, in one place.
#
# WHY AN EXACT REQUIREMENT AND NOT A PIN FILE
#
# `nros_generated.h` and `nros_cpp_ffi.h` are COMMITTED headers that `build.rs`
# regenerates IN PLACE (nros-build-helpers `generate_cbindgen_header`). cbindgen's
# output moves between patch releases: 0.29.4 narrows the C23 enum-base guard the
# committed headers carry, ~36 lines across the two files. So a graph that
# resolves a different patch release silently dirties the worktree, and
# committing that result REVERTS an upstream improvement — it had to be
# hand-reverted twice during phase-338.
#
# A caret `"0.29"` cannot prevent this. The ROOT lock pins one version, but the
# leaves that regenerate these headers have no tracked `Cargo.lock` (an example's
# lock is never committed — CLAUDE.md), so each resolves the caret freshly.
# `packages/testing/nros-bench/wake-latency-cortex-m3` was measured building
# 0.29.4 while the root lock said 0.29.3.
#
# This is deliberately NOT the `.clang-format-version` / bindgen-cli treatment.
# Those are PATH binaries with no resolver, so they need a version file plus a
# provisioning recipe. cbindgen is a cargo dependency: cargo's resolver IS the
# pinning mechanism, and a separate pin file would be a second spelling of the
# same fact — the drift class this repo keeps re-learning.
#
# Three things have to hold, and each has failed somewhere in this repo before:
#   1. the workspace requirement is exact (`=x.y.z`), not a caret;
#   2. no crate spells its own cbindgen version — all inherit the workspace one;
#   3. the resolved lock entry matches the requirement (a lock that drifted from
#      the pin builds fine locally and differently everywhere else).

set -euo pipefail
cd "$(dirname "$0")/.."

status=0

# 1. The workspace requirement is exact.
req="$(sed -n 's/^cbindgen = "\(=[0-9][^"]*\)".*/\1/p' Cargo.toml | head -1)"
if [ -z "$req" ]; then
    echo "[FAIL] Cargo.toml [workspace.dependencies] has no exact cbindgen requirement." >&2
    echo "       Expected a line like: cbindgen = \"=0.29.3\"" >&2
    echo "       A caret req lets a lockless leaf resolve a different patch release" >&2
    echo "       and rewrite the committed headers (issue 0452)." >&2
    status=1
fi
pin="${req#=}"

# 2. No crate carries its own cbindgen version.
offenders="$(git grep -n '^cbindgen = ' -- '*/Cargo.toml' | grep -v 'workspace = true' || true)"
if [ -n "$offenders" ]; then
    echo "[FAIL] these manifests spell their own cbindgen version instead of" >&2
    echo "       inheriting the workspace pin (\`cbindgen = { workspace = true }\`):" >&2
    echo "$offenders" | sed 's/^/         /' >&2
    status=1
fi

# 3. The lock resolves to exactly the pinned version.
if [ -n "$pin" ]; then
    locked="$(awk '/^name = "cbindgen"$/{getline; sub(/^version = "/,""); sub(/"$/,""); print; exit}' Cargo.lock)"
    if [ "$locked" != "$pin" ]; then
        echo "[FAIL] Cargo.lock resolves cbindgen $locked but the pin is $pin." >&2
        echo "       Move it with \`just lock-update cbindgen $pin\`, and regenerate" >&2
        echo "       the committed headers in the same commit if the output moved." >&2
        status=1
    fi
fi

if [ "$status" -eq 0 ]; then
    echo "check-cbindgen-pin: OK (cbindgen pinned =$pin, inherited everywhere, lock agrees)"
fi
exit "$status"
