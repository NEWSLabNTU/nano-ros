#!/usr/bin/env bash
#
# issue 0445 / 0442 — every freshness probe uses the SAME exemption rule and
# reports what it compared.
#
# # The failure this prevents
#
# A staleness verdict is absorbing: the fixture is never launched, so the
# runtime result it would have produced is replaced by an explanation that
# reads as complete. Issue 0444 sat behind issue 0442 for exactly as long as
# the cells read STALE, and 0442 was one arm of the probe applying an exemption
# its sibling did not. Three arms each carried their own subset:
# `dep_file_newer_than` skipped in-place headers AND cargo OUT_DIR products,
# `cmake_dep_info_newer_source` only the former, `newest_source_after` neither
# (until 0442 added one of them). Any of those subsets is a guard narrower than
# the rule it enforces — issue 0196.
#
# # What it checks
#
# 1. The exemption rule is spelled ONCE, in `fixtures/staleness.rs`. No other
#    file may name the predicates; an arm that wants an exemption must add it
#    there, where every arm gets it.
# 2. Every `require_*_fresh*` entry point begins accounting, returns the shared
#    verdict, and clears the ledger on the fresh path. A probe that forgets the
#    last one makes its coordinate look permanently non-running; one that
#    forgets the first prints a verdict with no account of what it compared.

set -euo pipefail
cd "$(dirname "$0")/.."

# issue 0726 — `if ! … grep -qF -- "$required"` reads a grep that never ran as
# "this probe lost its verdict call", which is a specific claim about a function
# that is intact. `nros_grep_q` exits 2 there; it takes the same `-F --`, and it
# searches a HERESTRING so the helper's `exit` is not confined to a pipeline
# subshell.
# shellcheck source=scripts/lib/grep-q.sh
source scripts/lib/grep-q.sh

SRC="packages/testing/nros-tests/src"
OWNER="$SRC/fixtures/staleness.rs"
PROBES="$SRC/fixtures/binaries/mod.rs"
fail=0

if [ ! -f "$OWNER" ]; then
    echo "[FAIL] $OWNER is missing — the shared exemption rule has no home." >&2
    exit 1
fi

# 1. one spelling of the rule.
# `git grep -l`, not `grep -rln`: an index lookup, not a filesystem walk
# (check-no-tracked-file-find enforces this — the walk costs minutes).
stray="$(git grep -lE 'REGENERATED_INPLACE_HEADERS|is_cargo_out_dir_product|is_regenerated_inplace_header' \
    -- "$SRC" | grep -v "^$OWNER\$" || true)"
if [ -n "$stray" ]; then
    echo "[FAIL] the probe exemption rule is spelled outside its owner:" >&2
    printf '  %s\n' $stray >&2
    echo "       Add the case to \`exempt_probe_input\` in $OWNER instead — an" >&2
    echo "       arm-local copy is how issue 0442 happened." >&2
    fail=1
fi

# 2. each probe entry point accounts, reports and clears.
entries="$(grep -oE 'fn require_prebuilt_binary_fresh[a-z_]*' "$PROBES" | sed 's/^fn //' | sort -u)"
[ -n "$entries" ] || {
    echo "[FAIL] no \`require_prebuilt_binary_fresh*\` entry points found in $PROBES" >&2
    exit 1
}
checked=0
for name in $entries; do
    # The function body: from its signature to the next top-level `fn `/doc.
    body="$(awk -v pat="fn $name" '
        $0 ~ pat {inside=1}
        inside {print}
        inside && /^}/ {exit}
    ' "$PROBES")"
    checked=$((checked + 1))
    for required in "staleness::begin_probe()" "staleness::stale_error" "staleness::record_fresh"; do
        if ! nros_grep_q -F -- "$required" <<<"$body"; then
            echo "[FAIL] $name is missing \`$required\`" >&2
            fail=1
        fi
    done
done

if [ "$fail" != 0 ]; then
    echo "" >&2
    echo "  A staleness verdict replaces the runtime result nobody then sees" >&2
    echo "  (issue 0445). It has to say what it compared, and a coordinate that" >&2
    echo "  runs has to clear its non-running count." >&2
    exit 1
fi

echo "staleness-probe-exemptions OK — $checked probe(s) share one exemption rule and one verdict."
