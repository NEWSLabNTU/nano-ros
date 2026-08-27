#!/usr/bin/env bash
# phase-340 W2.d / issue 0517 step 3 — `--core-only` selects by the DERIVED
# variant predicate, and the authored spelling it replaced stays deleted.
#
# This gate used to assert an EQUIVALENCE: that `row_is_variant()` selected the
# same rows as "authors a `target_dir`", on every platform a caller passes. That
# was the right check while both spellings existed, and it did its job — it is
# what made the predicate swap safe to land ahead of the column's removal.
#
# The column is now gone (issue 0517 step 3 deleted all 41 keys), so the
# equivalence has nothing to compare against and the check would pass vacuously
# in one direction and fail in the other. What is left to defend is the reason
# the column went: a row's identity is its CONFIGURATION, never a directory
# somebody invented for it (RFC-0070 R2). So this gate now asserts:
#
#   1. no `[[fixture]]` row authors a `target_dir` — the column stays deleted;
#   2. `--core-only` still selects a strict, non-empty SUBSET on every platform
#      a caller actually passes, so the flag keeps meaning something.
#
# (1) matters more than it looks. Re-adding the key is the natural thing to do
# the next time two rows of one leaf need telling apart, and it would work — the
# build reads it — while quietly restoring the state where a resolver can
# identify a row by path again. The answer is `FixtureVariant` / `select_row`.
#
# `[[workspace_fixture]]` rows are NOT covered: they still author `target_dir` /
# `build_subdir`, and there it is a genuine build input —
# `workspace-fixtures-build.sh` hands it to cargo and cmake, and the artifacts
# really do land there.
set -uo pipefail
cd "$(dirname "$0")/../../../.."

M=scripts/build/fixtures-manifest.py
fail=0

# --- 1. the column stays deleted ------------------------------------------
# Read the `[[fixture]]` blocks only. `awk` rather than a TOML parse so the gate
# has no dependency the build does not already have.
authored="$(awk '
    /^\[\[fixture\]\]/            { in_fixture = 1; next }
    /^\[\[workspace_fixture\]\]/  { in_fixture = 0; next }
    /^\[\[/                       { in_fixture = 0 }
    in_fixture && /^target_dir *=/ { print NR ": " $0 }
' examples/fixtures.toml)"

if [ -n "$authored" ]; then
    echo "FAIL: a [[fixture]] row authors target_dir — the column was deleted in" >&2
    echo "      issue 0517 step 3 and a row's identity is its configuration now:" >&2
    printf '  examples/fixtures.toml:%s\n' "$authored" >&2
    cat >&2 <<'EOF'

  To tell two rows of one leaf apart, give them distinguishable CONFIGURATION
  (features / no_default_features / env) and select with
  `groups::select_row(dir, FixtureVariant::…)`. A directory cannot carry that
  any more: several rows of a leaf share `<dir>/target`, and `attribute_path`
  fails closed on them deliberately.
EOF
    fail=1
fi

# --- 2. --core-only still narrows -----------------------------------------
# Every platform any caller passes with --core-only. Derived from the tree, not
# a literal, so a new caller joins the check automatically.
# `git grep`, never `grep -r`: `scripts/` holds the gitignored Zephyr SDK
# (`scripts/zephyr/sdk/` + `downloads/`, 9.2 GB here), so the recursive form
# walks the whole toolchain looking for a flag that only ever appears in tracked
# source. Measured on this tree: 37+ minutes and still going, against 0.33 s for
# the index lookup — the same 7m36s -> 0.8s class `check-no-tracked-file-find`
# was written for. An untracked match could only be inside the SDK, which is not
# a caller, so scoping to the index loses nothing.
consumed="$(git grep -hoE '[a-z0-9-]+ +[a-z]+ +--core-only' -- just/ scripts/ 2>/dev/null \
    | awk '{print $1}' | sort -u)"
[ -n "$consumed" ] || { echo "FAIL: no --core-only caller found — has the flag been removed?" >&2; exit 1; }

for platform in $consumed; do
    all="$(python3 "$M" list --platform "$platform" --lang rust | wc -l)"
    core="$(python3 "$M" list --platform "$platform" --lang rust --core-only | wc -l)"
    if [ "$core" -eq 0 ] || [ "$core" -ge "$all" ]; then
        echo "FAIL: --core-only on '$platform' selects $core of $all rust rows —" >&2
        echo "      it must be a strict, non-empty subset or the flag means nothing." >&2
        fail=1
    else
        echo "  ok  --core-only on '$platform': $core of $all rust rows"
    fi
done

[ "$fail" -eq 0 ] || exit 1
echo "core-only predicate: the target_dir column stays deleted; --core-only still narrows"
