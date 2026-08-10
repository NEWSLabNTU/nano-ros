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

CORE="packages/cli/nros-cli-core/src"

# Function bodies that write a sync-owned, concurrently-read file. Grepping the
# whole file would drag in its unit tests, which legitimately use `fs::write` to
# set up scratch fixtures.
#
#   <file>:<fn name>
GUARDED=(
    "$CORE/orchestration/metadata_refresh.rs:stamp_provenance"
    "$CORE/orchestration/metadata_refresh.rs:mark_unprobeable"
    "$CORE/orchestration/metadata_build.rs:relativise_source_artifacts"
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
    if grep -q 'fs::write' <<<"$body"; then
        echo "ERROR: $file: fn $fn writes a sync-owned file with fs::write" >&2
        fail=1
    fi
done

# The GENERATED metadata harness is a standalone crate that cannot depend on the
# CLI, so it inlines the discipline instead of calling the helper. Assert the
# emitted source renames rather than truncating.
harness="$CORE/orchestration/metadata_build.rs"
if ! grep -q 'std::fs::rename(&tmp, out)' "$harness"; then
    echo "ERROR: $harness: the generated metadata harness no longer renames its output" >&2
    echo "       (it must write a temp sibling and rename; see issue 0498)" >&2
    fail=1
fi

# One spelling of the helper. A second private `fn atomic_write` is how the
# first one failed to reach the sidecar.
dupes="$(grep -rln 'fn atomic_write' "$CORE" | grep -v 'atomic_file.rs' || true)"
if [ -n "$dupes" ]; then
    echo "ERROR: a second atomic_write implementation exists — use nros_cli_core::atomic_file:" >&2
    echo "$dupes" | sed 's/^/  /' >&2
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
