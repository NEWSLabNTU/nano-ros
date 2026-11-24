#!/usr/bin/env bash
# phase-591 W3 -- live-peer's host fixture rows, as units a shard can build.
#
#   live-peer-host-fixtures.sh units              print every unit, one per line
#   live-peer-host-fixtures.sh build <k>/<n>      build this shard's units
#   live-peer-host-fixtures.sh paths              print the paths the cells read
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

avail() { echo "  disk: $(df -h --output=avail / | tail -n 1 | tr -d ' ') free"; }
free_gb() { df -BG --output=avail / | tail -n 1 | tr -dc '0-9'; }
ignored_build_dirs() {
    git ls-files -o -i --exclude-standard --directory -- examples \
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
    git ls-files -o -i --exclude-standard -- examples build
}

case "${1:-}" in
    units) units ;;
    build) shift; build "$@" ;;
    paths) paths ;;
    *) sed -n '2,6p' "$0" >&2; exit 2 ;;
esac
