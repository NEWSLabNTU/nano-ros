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
# `|| true`: finding nothing is this gate's own negative control and grep
# spells it exit 1, so without this the [FAIL] below never prints (issue 1249).
# issue 1616 (W7): the probes are HARVESTED — every non-test fn in the test
# crate's sources whose body opens a probe (`staleness::begin_probe()`) — not
# the `require_prebuilt_binary_fresh*` name prefix, which missed
# `require_prebuilt_row_binary_fresh`: its `record_fresh` could be deleted with
# this gate green. Comments are stripped and `#[cfg(test)]` items blanked.
probe_report="$(python3 - "$SRC" <<'PY'
import re, sys
from pathlib import Path
sys.path.insert(0, "scripts/lib")
import comments, per_item, tracked as _t

REQUIRED = ("staleness::begin_probe()", "staleness::stale_error", "staleness::record_fresh")
FN = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)")

def probes(text):
    code = per_item.rust_cfg_test_blank(comments.strip_comments(text, "rust"))
    out = []
    for m, a, b in per_item.blocks(code, FN):
        body = code[a:b]
        # The innermost fn owns the call: skip a body that only CONTAINS a probe fn.
        if "staleness::begin_probe()" in body and not FN.search(body):
            out.append((m.group(1), [r for r in REQUIRED if r not in body]))
    return out

# Normal-path selftest.
ok = 'fn a() { staleness::begin_probe(); return Err(staleness::stale_error(x)); staleness::record_fresh(p); }'
assert probes(ok) == [("a", [])], probes(ok)
bad = 'fn b() { staleness::begin_probe(); return Err(staleness::stale_error(x)); }'
assert probes(bad) == [("b", ["staleness::record_fresh"])]
assert probes("#[cfg(test)]\nmod t { fn c() { staleness::begin_probe(); } }\n") == []

n = 0
for path in _t.tracked(sys.argv[1], suffix=".rs"):
    for name, missing in probes(path.read_text(errors="replace")):
        n += 1
        for r in missing:
            print(f"MISSING {path} {name} {r}")
print(f"COUNT {n}")
PY
)" || { echo "[FAIL] the probe harvest did not run" >&2; exit 1; }
checked="$(sed -n 's/^COUNT //p' <<<"$probe_report")"
if [ -z "$checked" ] || [ "$checked" -lt 1 ]; then
    echo "[FAIL] no staleness probe (a fn calling staleness::begin_probe()) found under $SRC" >&2
    exit 1
fi
while read -r _tag file name required; do
    echo "[FAIL] $file: $name is missing \`$required\`" >&2
    fail=1
done < <(grep '^MISSING ' <<<"$probe_report" || true)

if [ "$fail" != 0 ]; then
    echo "" >&2
    echo "  A staleness verdict replaces the runtime result nobody then sees" >&2
    echo "  (issue 0445). It has to say what it compared, and a coordinate that" >&2
    echo "  runs has to clear its non-running count." >&2
    exit 1
fi

echo "staleness-probe-exemptions OK — $checked probe(s) share one exemption rule and one verdict."
