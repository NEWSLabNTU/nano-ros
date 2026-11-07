#!/usr/bin/env bash
# The gate lane `just check fast` / `just check build` run (phase-543).
#
# Prints NROS_GATE_SCOPE when it is set. On a developer's machine an unset
# scope is `dev` (a worktree after `just setup-worktree`). In CI an unset scope
# is REFUSED: a job's lane is a property of the job, and a default chosen for a
# developer's machine is wrong on every runner. Measured: run-matrix's
# self-hosted jobs reached `check fast` through `just build tier2`, fell to
# `dev`, and stopped in the preflight naming four capabilities that runner
# never had (run 42346220231). No workflow step said `check fast`, so the job
# could not have been found by reading the workflows for it, and this refusal
# is what names it now.
set -euo pipefail
if [ -n "${NROS_GATE_SCOPE:-}" ]; then
    printf '%s\n' "$NROS_GATE_SCOPE"
    exit 0
fi
if [ "${GITHUB_ACTIONS:-}" = true ]; then
    root="$(cd "$(dirname "$0")/../.." && pwd)"
    lanes="$(sed -n 's/^\[lane\.\([a-z0-9-]*\)\]$/\1/p' "$root/.config/gate-lanes.toml" | paste -sd, -)"
    echo "gate-scope: this CI job runs the gate tiers with no NROS_GATE_SCOPE." >&2
    echo "  workflow ${GITHUB_WORKFLOW:-?}, job ${GITHUB_JOB:-?}." >&2
    echo "  Set it on the job (env: NROS_GATE_SCOPE: <lane>), one of: $lanes" >&2
    echo "  (.config/gate-lanes.toml). A lane is what the job's machine claims;" >&2
    echo "  the developer default \`dev\` claims what a set-up worktree has." >&2
    exit 2
fi
printf 'dev\n'
