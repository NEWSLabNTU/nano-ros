#!/usr/bin/env bash
# Issue 0498 — a file a CONCURRENT process reads must not be written with
# `std::fs::write`.
#
# WHAT THIS CATCHES
#
# `std::fs::write` truncates the destination to zero and then fills it. Between
# those two steps a reader observes an EMPTY file — and an empty file is not a
# corrupt one, it is "EOF while parsing a value at line 1 column 0", which reads
# like a bug in whatever PRODUCED the content rather than a race. That is how
# 0498 presented: `build-test-fixtures lane=native` died on a sidecar that was
# 1345 bytes and valid when inspected seconds later.
#
# `build-test-fixtures` fans out one `nros sync` per fixture row, and several
# rows of ONE leaf (its zenoh / xrce / cyclonedds coordinates) sync the same
# directory. Any file keyed by something coarser than the fixture coordinate is
# contended by construction, whatever the target-dir split does.
#
# THE RULE
#
# Files matching the PATHS listed below are sync-owned and concurrently read, so
# every writer of one must go through `nros_cli_core::atomic_file::atomic_write`
# (temp sibling + `rename(2)`, which is atomic within a filesystem).
#
# Deliberately NOT "no `fs::write` in the CLI": most writes are to a private
# temp, a scratch dir, or a path only the writing process touches, and flagging
# those is noise — and noise gets suppressed. This gate names the four writers
# of the metadata sidecar and its marker, which is the population 0498 covered;
# extend the list when a new sync-owned, concurrently-read file appears.
#
# WHY A GATE AT ALL
#
# `cmd/ws.rs` already had a private `atomic_write` whose own doc comment called
# it "the write discipline every other sync-owned file here uses". It was not:
# the sidecar had three plain `fs::write` writers one directory over. A
# discipline that lives in one file's private helper is a habit, and the sibling
# site is exactly what a habit does not reach. Same class, same week, one file
# over: issue 0494 (`lane-coords` written with `>` while `ci-matrix` read it).
set -euo pipefail
cd "$(dirname "$0")/.."

# issue 0726 — both conditionals below turn a grep STATUS into a verdict about
# the source tree, and `grep -q` cannot tell "not present" (1) from "the grep
# never ran" (>=2). Under a 32-way gate fan-out the second kind happens, and
# `if ! grep -q rename` would then announce that the generated harness stopped
# renaming its output — a confident, specific, false claim. `nros_grep_q` exits
# 2 instead.
# shellcheck source=scripts/lib/grep-q.sh
source scripts/lib/grep-q.sh

CORE="packages/cli/nros-cli-core/src"
# The implementation moved DOWN to cargo-nano-ros (issue 0562): nros-cli-core
# depends on it, and `provider_scan` there writes a sync-owned file of its own,
# so the lower crate is the only place ONE spelling can serve both.
LOW="packages/cli/cargo-nano-ros/src"

# phase-472 W7 — the writers are HARVESTED, not listed. The authored list named
# five functions, so a NEW writer in the same modules (or `provider_scan`'s,
# the one `providers.json` writer the header names) was never asked. Now: every
# `fs::write` in a SYNC-OWNED MODULE — the metadata sidecar family and the
# provider index — outside `cfg(test)` (tests set up scratch fixtures) is a
# finding, unless it is in EXEMPT below with its reason. `scripts/lib/harvest.py`
# fails a stale or reason-less exemption.
fail=0
rc=0
sites="$(python3 - <<'PY'
import glob, os, re, sys
sys.path.insert(0, os.path.join("scripts", "lib"))
import comments, harvest, per_item
harvest.self_test()
per_item.self_test()
CORE, LOW = "packages/cli/nros-cli-core/src", "packages/cli/cargo-nano-ros/src"
MODULES = sorted(glob.glob(f"{CORE}/orchestration/metadata_*.rs")) + [f"{LOW}/provider_scan.rs"]
EXEMPT = {
    f"{CORE}/orchestration/metadata_build.rs:main":
        "the GENERATED harness: emitted as source into a standalone crate that "
        "cannot depend on the CLI; it writes a temp sibling and renames (checked below)",
}
FN = re.compile(r"\bfn\s+(\w+)")


def sites(text):
    """{fn: line} of every non-test `fs::write` in one Rust source."""
    code = per_item.rust_cfg_test_blank(comments.strip_comments(text, "rust"))
    fns = per_item.blocks(code, FN)
    out = {}
    for m in re.finditer(r"\bfs::write\s*\(", code):
        inner = [x.group(1) for x, o, e in fns if o < m.start() < e]
        out.setdefault(inner[-1] if inner else "?", per_item.line_of(code, m.start()))
    return out


# Negative controls on the normal path (phase-472 W7/W9): a NEW writer in a
# sync-owned module is found; a test fixture's write and an atomic write are not.
assert sites("fn new_writer(p: &Path) { std::fs::write(p, b\"x\").unwrap(); }\n"
             "#[cfg(test)]\nmod t { fn f() { std::fs::write(p, b\"x\").unwrap(); } }\n"
             "fn ok(p: &Path) { atomic_file::atomic_write(p, b\"x\")?; }\n") == {"new_writer": 1}

found = {}
for f in MODULES:
    for fn, line in sites(open(f).read()).items():
        found.setdefault(f"{f}:{fn}", line)
if not MODULES or len(MODULES) < 4:
    sys.exit(f"check-atomic-sync-writes: harvested only {len(MODULES)} sync-owned module(s)")
_checked, problems = harvest.reconcile(list(found) or ["(none)"], EXEMPT, what="fs::write site")
problems = [p for p in problems if "NO fs::write" not in p]
for p in problems:
    print(f"PROBLEM {p}")
for key in sorted(found):
    if key not in EXEMPT:
        print(f"{key}:{found[key]}")
PY
)" || rc=$?
if [ "$rc" -ne 0 ]; then
    echo "check-atomic-sync-writes: the writer harvest did not run (rc=$rc)" >&2
    exit 2
fi
while IFS= read -r site; do
    [ -n "$site" ] || continue
    case "$site" in
        PROBLEM*) echo "ERROR: ${site#PROBLEM }" >&2 ;;
        *) echo "ERROR: $site writes a sync-owned file with fs::write" >&2 ;;
    esac
    fail=1
done <<<"$sites"

harness="$CORE/orchestration/metadata_build.rs"
if ! nros_grep_q 'std::fs::rename(&tmp, out)' "$harness"; then
    echo "ERROR: $harness: the generated metadata harness no longer renames its output" >&2
    echo "       (it must write a temp sibling and rename; see issue 0498)" >&2
    fail=1
fi

# One spelling of the helper. A second private `fn atomic_write` is how the
# first one failed to reach the sidecar.
# `git grep`, not `grep -r`: check-no-tracked-file-find rejects a filesystem
# walk to locate TRACKED files (measured 7m36s -> 0.8s over the same 232 paths).
dupes="$(git grep -ln 'fn atomic_write' -- "$CORE" "$LOW" | grep -v 'atomic_file.rs' || true)"
if [ -n "$dupes" ]; then
    echo "ERROR: a second atomic_write implementation exists — use nros_cli_core::atomic_file:" >&2
    echo "$dupes" | sed 's/^/  /' >&2
    fail=1
fi

# issue 0562 — and no private TEMP+RENAME either. The atomicity rule grew four
# spellings of the same body (`facade::write_if_changed`,
# `metadata_build::write_if_changed`, an inline check in `cmd/ws.rs`, and
# `model_ingest`'s), and the sites that mattered — the probe-cmake writers and
# `providers.json` — had none of them, so an unchanged tree was restamped and
# reconfigured on every sync. A delegating wrapper is fine; a second body is not.
#
# The generated metadata harness is exempt by the same reasoning as above: it is
# emitted as source into a standalone crate that cannot depend on the CLI.
renames="$(git grep -n 'fs::rename(&tmp' -- "$CORE" "$LOW" \
    | grep -v 'atomic_file.rs' \
    | grep -v 'std::fs::rename(&tmp, out)' || true)"
if [ -n "$renames" ]; then
    echo "ERROR: a private temp+rename exists — call atomic_file::atomic_write instead:" >&2
    echo "$renames" | sed 's/^/  /' >&2
    echo "       (it is atomic AND write-if-changed; a private copy gets only half)" >&2
    fail=1
fi

if [ "$fail" -ne 0 ]; then
    echo "" >&2
    echo "  fs::write truncates to zero and then fills. A concurrent reader sees" >&2
    echo "  an EMPTY file, which surfaces as 'EOF at line 1 column 0' and reads" >&2
    echo "  like a producer bug (issue 0498). Use" >&2
    echo "  nros_cli_core::atomic_file::atomic_write — temp sibling + rename(2)." >&2
    exit 1
fi

echo "atomic sync writes: OK (every writer in the sync-owned modules is atomic, + generated harness)"
