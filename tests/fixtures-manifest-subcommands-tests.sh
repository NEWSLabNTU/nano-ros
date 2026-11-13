#!/bin/bash
# tests/fixtures-manifest-subcommands-tests.sh -- every subcommand runs
#
# `scripts/build/fixtures-manifest.py` is read by build scripts through
# `while ... done < <(python3 fixtures-manifest.py <cmd>)`, and a process
# substitution drops the producer's exit status (issue 2424). So a subcommand
# that crashes reads, to its consumer, as an EMPTY listing. That happened:
# #3449 added a module function `west_rows()` while `main()` later assigned a
# local `west_rows = [...]`, which made the name local for all of `main()`;
# `west-overrides` raised UnboundLocalError and every Zephyr row built with no
# west overrides, silently, until issue 2363's fix.
#
# This runs every subcommand against the real manifest and fails on a non-zero
# exit or a Python traceback. The subcommand list is read from the script's
# own argparse `--help`, so a new subcommand is covered without editing this
# file; one that needs arguments gets them from ARGS below, and a subcommand
# this file has never seen still runs with none.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib/grep-q.sh
source "$ROOT/scripts/lib/grep-q.sh"
manifest_py="$ROOT/scripts/build/fixtures-manifest.py"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# The argparse choices line: `{list,list-workspaces,...}`.
help_rc=0
help="$(python3 "$manifest_py" --help 2>&1)" || help_rc=$?
if [ "$help_rc" -ne 0 ]; then
    echo "FAIL: fixtures-manifest.py --help exited $help_rc:" >&2
    sed 's/^/       /' <<< "$help" >&2
    exit 1
fi
if ! braces="$(grep -m1 -oE '\{[a-z0-9,-]+\}' <<< "$help")"; then
    echo "FAIL: no subcommand list found in fixtures-manifest.py --help" >&2
    exit 1
fi
choices="${braces//[\{\}]/}"
choices="${choices//,/ }"

# Arguments for the subcommands that refuse to run without one.
first_id="$(grep -m1 -oE '^id = "[^"]+"' "$ROOT/examples/fixtures.toml" | cut -d'"' -f2)"
# `lane-dirs` refuses without a lane's coordinates (it names a scope); give it
# one real coordinate.
printf 'linux,rust,zenoh\n' > "$WORK/coords"
declare -A ARGS=(
    [describe-id]="--id $first_id"
    [lane-dirs]="--coords-from $WORK/coords"
)

fail=0
n=0
for cmd in $choices; do
    n=$((n + 1))
    rc=0
    # shellcheck disable=SC2086  # ARGS values are deliberately word-split
    python3 "$manifest_py" "$cmd" ${ARGS[$cmd]:-} >"$WORK/out" 2>"$WORK/err" || rc=$?
    traced=1
    nros_grep_q '^Traceback' "$WORK/err" || traced=$?
    if [ "$rc" -ne 0 ] || [ "$traced" -eq 0 ]; then
        echo "FAIL: fixtures-manifest.py $cmd ${ARGS[$cmd]:-} exited $rc:" >&2
        tail -n 8 "$WORK/err" | sed 's/^/       /' >&2
        fail=1
    else
        echo "ok   $cmd"
    fi
done

if [ "$fail" -ne 0 ]; then
    exit 1
fi
echo "fixtures-manifest-subcommands: OK ($n subcommands ran clean against examples/fixtures.toml)"
