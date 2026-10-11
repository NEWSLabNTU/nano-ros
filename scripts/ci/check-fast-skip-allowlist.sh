#!/usr/bin/env bash
# CI's `check fast` may skip exactly the gates `.config/ci-check-fast-skips.txt`
# names, and no others.
#
# A gate that cannot read its input reports NOT VERIFIED through the skip ledger
# (`scripts/build/check-skip.sh`) and exits 0 -- right for a bare worktree, and
# the reason CI could stop verifying a gate without anything turning red.
# `doc-commit-citations` and `roadmap-commit-refs` skipped on every CI run for
# want of history, four source gates for want of a submodule, and a local
# `just check fast` ran all of them. This turns the ledger into a contract:
#
#   * a skip NOT in the allowlist fails -- CI lost an input a gate needs;
#   * an allowlisted gate that did NOT skip fails too -- the entry is stale,
#     so delete it (the list only shrinks).
#
# Usage: check-fast-skip-allowlist.sh [<ledger>]   (default: the lane's ledger)
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
allow="$root/.config/ci-check-fast-skips.txt"
ledger="${1:-}"
if [ -z "$ledger" ]; then
    # shellcheck source=scripts/build/check-skip.sh
    . "$root/scripts/build/check-skip.sh"
    ledger="$(_nros_check_skip_file)"
fi
if [ ! -f "$ledger" ]; then
    echo "check-fast-skip-allowlist: no skip ledger at $ledger -- did \`just check fast\` run?" >&2
    exit 1
fi

skipped="$(cut -f1 "$ledger" | sort -u)"
allowed="$(grep -v '^\s*#' "$allow" | grep -v '^\s*$' | cut -f1 | sort -u)"

rc=0
unexpected="$(comm -23 <(printf '%s\n' "$skipped") <(printf '%s\n' "$allowed") | grep -v '^$' || true)"
stale="$(comm -13 <(printf '%s\n' "$skipped") <(printf '%s\n' "$allowed") | grep -v '^$' || true)"
if [ -n "$unexpected" ]; then
    echo "check-fast-skip-allowlist: FAIL -- these gates did not verify in CI:" >&2
    while IFS= read -r g; do
        printf '  %s: %s\n' "$g" "$(grep -m1 "^$g	" "$ledger" | cut -f2-)" >&2
    done <<< "$unexpected"
    echo "  Give the job the input the gate names (gate.yml, the provisioning steps" >&2
    echo "  before \`just check fast\`), or -- only if CI can never have it -- add a" >&2
    echo "  line to .config/ci-check-fast-skips.txt with the reason." >&2
    rc=1
fi
if [ -n "$stale" ]; then
    echo "check-fast-skip-allowlist: FAIL -- allowlisted but VERIFIED now; delete from" >&2
    echo "  .config/ci-check-fast-skips.txt:" >&2
    printf '  %s\n' $stale >&2
    rc=1
fi
[ "$rc" -ne 0 ] || echo "check-fast-skip-allowlist: OK ($(printf '%s\n' "$skipped" | grep -c . || true) allowlisted skip(s), none unexpected)"
exit "$rc"
