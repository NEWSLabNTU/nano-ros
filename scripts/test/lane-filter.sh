#!/usr/bin/env bash
# Print the nextest filter expressions that scope a run to a CI lane.
# RFC-0061 / phase-318 W4.
#
# Usage: lane-filter.sh <native|all>
#   native  -> exclusion expressions that drop every NON-host test binary
#   all     -> nothing (tier 2/3 run the whole matrix)
#
# Why derive instead of list: a hand-written exclusion list is the failure mode
# this phase exists to remove — it rots the moment a platform is added, and the
# lane then silently skips it (audit E5 / issue 0341). The tokens come from
# `PlatformId` in packages/testing/nros-tests/src/matrix.rs, so adding a platform
# to the matrix extends this filter with no second edit.
# `ci_lane::tests::lane_filter_tokens_cover_every_platform` asserts the coverage.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."

lane="${1:-all}"
case "$lane" in
    all) exit 0 ;;
    native) ;;
    *) echo "lane-filter.sh: expected 'native' or 'all', got '$lane'" >&2; exit 2 ;;
esac

matrix="packages/testing/nros-tests/src/matrix.rs"
[ -f "$matrix" ] || { echo "lane-filter.sh: $matrix not found" >&2; exit 2; }

# Variants of `enum PlatformId`, minus `Native` — everything a host lane must not
# run. Take only the LEADING CamelCase word (the platform family):
# FreertosMps2 -> freertos, ThreadxRiscv64 -> threadx, ZephyrNativeSim -> zephyr.
#
# Splitting into all words would be actively wrong: ZephyrNativeSim yields
# "native" and ThreadxLinux yields "linux", either of which would exclude the
# host binaries this lane exists to run. Family names are also what the binaries
# are actually named after (freertos_qemu, threadx_riscv64_qemu, …).
tokens="$(
    awk '/^pub enum PlatformId \{/{f=1; next} f && /^\}/{exit} f' "$matrix" \
        | grep -oE '^\s{4}[A-Z][A-Za-z0-9]*,' \
        | tr -d ' ,' \
        | grep -v '^Native$' \
        | sed -E 's/^([A-Z][a-z0-9]*).*/\1/' \
        | tr 'A-Z' 'a-z' \
        | sort -u
)"

[ -n "$tokens" ] || { echo "lane-filter.sh: extracted no platform tokens" >&2; exit 2; }

# One `not binary(~token)` per token. nextest ORs nothing here — each expression
# is ANDed by the caller, which is what "exclude all of these" needs.
while IFS= read -r t; do
    printf 'not binary(~%s)\n' "$t"
done <<< "$tokens"
