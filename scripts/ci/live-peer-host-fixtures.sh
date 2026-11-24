#!/usr/bin/env bash
# phase-591 W3 -- live-peer's host fixture rows, as units a shard can build.
#
#   live-peer-host-fixtures.sh units              print every unit, one per line
#   live-peer-host-fixtures.sh build <k>/<n>      build this shard's units
#   live-peer-host-fixtures.sh paths              print the paths the cells read
#   live-peer-host-fixtures.sh roots              print the roots `paths` walks
#   live-peer-host-fixtures.sh check-roots        every manifest row's output is under a root
#
# The rows are DERIVED from the host cells' tests, exactly as the job's
# single fixture step derived them before it was sharded
# (`check-interop-verdicts.py --list-passing --runner host` with
# --host-workspace-fixtures, --host-builds and --host-compile-checks). A unit
# is `<kind>:<id>` with no spaces:
#
#   ws:<row-id>          scripts/build/workspace-fixtures-build.sh linux --id <row-id>
#   fb:<a>,<b>,<c>       scripts/build/fixtures-build.sh <a> <b> <c>
#   cc:<row-id>          NROS_FIXTURE_ID=<row-id> scripts/build/compile-check-fixtures.sh
#
# `build` picks its units with `fixture-shards.sh select` over
# `.config/live-peer-fixture-weights.txt` (measured per-unit wall time), and
# keeps the old step's disk discipline: issue 1846's free-space line after
# every unit, issue 2130's release of compile intermediates.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

WEIGHTS=.config/live-peer-fixture-weights.txt
# Where the units leave what the cells read is DERIVED from the fixtures
# manifest, never listed: every buildable row's leaf `dir` (its build output,
# `generated/` tree and sync files) and its `artifact_root`, the same columns
# the test-side resolvers attribute by (`fixtures-manifest.py coords`), plus
# the repo's `build/` (shared group dirs, compile checks, stamps). The second
# proof dispatch, 45370439146, packed a hand-listed `examples` and `build`, and
# 32 cells failed on "fixture not prebuilt" for the single-node binaries under
# `packages/testing/nros-tests/bins`, which no list had named.
roots() {
    local rows
    if ! rows="$(python3 scripts/build/fixtures-manifest.py coords)"; then
        echo "live-peer-host-fixtures: could not read the fixtures manifest" >&2
        return 1
    fi
    {
        echo build
        awk -F'\037' '{ if ($5 != "") print $5; if ($7 != "") print $7 }' <<<"$rows"
    } | python3 -c '
import sys
# Keep only the outermost: a root inside another root adds nothing.
roots = sorted({l.strip().rstrip("/") for l in sys.stdin if l.strip()})
kept = []
for r in roots:
    if not any(r == k or r.startswith(k + "/") for k in kept):
        kept.append(r)
print("\n".join(kept))'
}

check_roots() {
    # Self-test: every row's output lies under a root `paths` walks. The roots
    # are derived from these same rows, so this guards the derivation itself
    # (an awk column that moved, a filter that drops rows) rather than a list.
    local rows r bad=0 n=0 have_text outs
    local -a have
    have_text="$(roots)" || return 1
    mapfile -t have <<<"$have_text"
    [ "${#have[@]}" -gt 1 ] || { echo "check-roots: derived no roots" >&2; return 1; }
    rows="$(python3 scripts/build/fixtures-manifest.py coords)" || return 1
    outs="$(awk -F'\037' '{ if ($5 != "") print $5; if ($7 != "") print $7 }' <<<"$rows")"
    while IFS= read -r r; do
        [ -n "$r" ] || continue
        n=$((n + 1))
        local ok=0 k
        for k in "${have[@]}"; do
            case "$r/" in "$k"/*) ok=1; break ;; esac
        done
        [ "$ok" = 1 ] || { echo "check-roots: $r is under no packed root" >&2; bad=1; }
    done <<<"$outs"
    [ "$bad" = 0 ] || return 1
    echo "check-roots: OK ($n row outputs under ${#have[@]} derived roots)"
}

avail() { echo "  disk: $(df -h --output=avail / | tail -n 1 | tr -d ' ') free"; }
free_gb() { df -BG --output=avail / | tail -n 1 | tr -dc '0-9'; }
ignored_build_dirs() {
    # Leaf build dirs under the derived roots (issue 2130's release). Not the
    # repo's top-level `build/`, whose own name matches the leaf pattern: the
    # old single step never released it.
    local -a rs
    local text
    text="$(roots)" || return 1
    mapfile -t rs <<<"$text"
    local i
    for i in "${!rs[@]}"; do [ "${rs[$i]}" != build ] || unset 'rs[i]'; done
    git ls-files -o -i --exclude-standard --directory -- "${rs[@]}" \
        | grep -E '/(build|target)[^/]*/$' || true
}
release() {
    # shellcheck disable=SC2046
    scripts/ci/release-fixture-intermediates.sh build $(ignored_build_dirs)
}

units() {
    local wss builds ccs
    if ! wss="$(python3 scripts/check-interop-verdicts.py --list-passing --runner host --host-workspace-fixtures)"; then
        echo "live-peer-host-fixtures: could not derive the host cells' workspace fixtures" >&2
        return 1
    fi
    if ! builds="$(python3 scripts/check-interop-verdicts.py --list-passing --runner host --host-builds)"; then
        echo "live-peer-host-fixtures: could not derive the host fixture builds" >&2
        return 1
    fi
    if ! ccs="$(python3 scripts/check-interop-verdicts.py --list-passing --runner host --host-compile-checks)"; then
        echo "live-peer-host-fixtures: could not derive the host cells' compile-check fixtures" >&2
        return 1
    fi
    sed -n 's/^\(.\+\)$/ws:\1/p' <<<"$wss"
    while IFS= read -r b; do
        [ -n "$b" ] || continue
        printf 'fb:%s\n' "$(tr -s ' ' ',' <<<"$b")"
    done <<<"$builds"
    sed -n 's/^\(.\+\)$/cc:\1/p' <<<"$ccs"
}

build_unit() {
    local u="$1" id="${1#*:}"
    case "$u" in
        ws:*)
            echo "=== workspace-fixtures-build.sh linux --id $id ==="
            bash scripts/build/workspace-fixtures-build.sh linux --id "$id"
            ;;
        fb:*)
            echo "=== fixtures-build.sh ${id//,/ } ==="
            # shellcheck disable=SC2086
            bash scripts/build/fixtures-build.sh ${id//,/ }
            ;;
        cc:*)
            echo "=== compile-check-fixtures.sh (NROS_FIXTURE_ID=$id) ==="
            NROS_FIXTURE_ID="$id" bash scripts/build/compile-check-fixtures.sh
            ;;
        *)
            echo "live-peer-host-fixtures: unknown unit '$u'" >&2
            return 1
            ;;
    esac
}

build() {
    local spec="${1:?usage: build <k>/<n>}" all mine
    all="$(mktemp)"
    units > "$all"
    echo "live-peer host fixture units: $(grep -c . "$all")"
    mine="$(bash scripts/ci/fixture-shards.sh select "$all" "$WEIGHTS" "$spec")"
    rm -f "$all"
    echo "shard $spec builds $(grep -c . <<<"$mine") unit(s):"
    printf '  %s\n' $mine
    avail
    if [[ $'\n'"$mine" == *$'\n'ws:* ]]; then
        just _codegen
    fi
    local u t0
    while IFS= read -r u; do
        [ -n "$u" ] || continue
        t0=$SECONDS
        build_unit "$u"
        echo "     unit-time: $u $((SECONDS - t0))s"
        avail
        # Workspace rows release only under pressure (the old step's rule);
        # the single-node builds and compile checks always did.
        if [ "${u%%:*}" != ws ] || [ "$(free_gb)" -lt 30 ]; then
            release
            avail
        fi
    done <<<"$mine"
    release
    avail
}

paths() {
    # What the cells read: every ignored file left under examples/ and build/
    # once the shard has released its compile intermediates (issue 2130's
    # release keeps every binary, archive and dep-info the cells and their
    # freshness probes read). Repo-relative, one per line.
    # The derivation's self-test runs on this, the normal path: a pack set
    # that misses a row's output fails here, before the tar is written.
    check_roots >&2 || return 1
    local -a rs
    local text
    text="$(roots)" || return 1
    mapfile -t rs <<<"$text"
    git ls-files -o -i --exclude-standard -- "${rs[@]}"
}

case "${1:-}" in
    units) units ;;
    build) shift; build "$@" ;;
    paths) paths ;;
    roots) roots ;;
    check-roots) check_roots ;;
    *) sed -n '2,6p' "$0" >&2; exit 2 ;;
esac
