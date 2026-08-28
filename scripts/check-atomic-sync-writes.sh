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

# Function bodies that write a sync-owned, concurrently-read file. Grepping the
# whole file would drag in its unit tests, which legitimately use `fs::write` to
# set up scratch fixtures.
#
#   <file>:<fn name>
GUARDED=(
    "$CORE/orchestration/metadata_refresh.rs:stamp_provenance"
    "$CORE/orchestration/metadata_refresh.rs:mark_unprobeable"
    "$CORE/orchestration/metadata_build.rs:relativise_source_artifacts"
    # issue 0562 — the probe directory IS a cmake project, so these writers
    # restamping their output costs a probe reconfigure on every sync, not just
    # a torn read.
    "$CORE/orchestration/metadata_probe_cmake.rs:run_probes"
    "$CORE/orchestration/metadata_probe_cmake.rs:write_capabilities"
)

fail=0

for entry in "${GUARDED[@]}"; do
    file="${entry%%:*}"
    fn="${entry##*:}"
    [ -f "$file" ] || {
        echo "check-atomic-sync-writes: $file missing — the guarded set is stale" >&2
        exit 2
    }
    # The function body: from `fn <name>` to the next line that starts a
    # top-level item (column 0 `fn`/`}`), whichever comes first.
    body="$(awk -v fn="fn $fn(" '
        index($0, fn) { inside = 1 }
        inside { print }
        inside && /^}/ { exit }
    ' "$file")"
    if [ -z "$body" ]; then
        echo "check-atomic-sync-writes: $file has no fn $fn — the guarded set is stale" >&2
        exit 2
    fi
    if nros_grep_q 'fs::write' <<<"$body"; then
        echo "ERROR: $file: fn $fn writes a sync-owned file with fs::write" >&2
        fail=1
    fi
done

# The GENERATED metadata harness is a standalone crate that cannot depend on the
# CLI, so it inlines the discipline instead of calling the helper. Assert the
# emitted source renames rather than truncating.
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

echo "atomic sync writes: OK (${#GUARDED[@]} guarded writer(s) + generated harness)"
