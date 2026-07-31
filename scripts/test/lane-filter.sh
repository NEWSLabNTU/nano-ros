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
#
# BINARY exclusion is not enough (issue 0357). It only works when a platform's
# tests live in a binary named after it; the matrix consumers do the opposite and
# put every platform's cases in ONE generically-named binary — `rtos_e2e` is
# entirely cross-platform and matches no token at all. A tier-1 run on 2026-07-31
# therefore executed 53 cross-platform tests on a host with none of their
# fixtures. So each token also gets a TEST-name exclusion.
#
# Two spellings per token, because nextest's `~` is a case-SENSITIVE substring
# match and the harnesses disagree:
#   rstest      platform_1_Platform__Freertos      -> needs `Freertos`
#   hand-rolled case_05_zephyr_rust                -> needs `zephyr`
# Emitting only one spelling silently covers half the suite, which is the same
# shape of miss as excluding only by binary.
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

# One expression per line; the caller ANDs them, which is what "exclude all of
# these" needs. (Each line as a SEPARATE `-E` would be wrong — nextest UNIONs
# multiple `-E` flags, and `not A or not B` is a tautology. `just test-all` joins
# with " and " into a single `-E`; any other caller must do the same.)
#
# Binary exclusions stay one-per-line. The TEST exclusions are emitted as a single
# grouped expression with a unit-test exemption:
#
#     (test(~tests::) or (not test(~a) and not test(~A) and ...))
#
# Without the exemption this lane drops host-only UNIT tests whose names merely
# mention a platform — `board::tier::tests::threadx_inverts_scale`,
# `qemu::tests::test_parse_results`, and (with some irony)
# `zephyr::tests::content_aware_staleness_ignores_mtime_only_bumps`, a phase-318
# test. Those need no fixture and no toolchain; dropping them trades one coverage
# hole for another. `tests::` is the Rust `mod tests` convention — matched WITHOUT
# leading colons so it also catches top-level `tests::freertos_key_maps_to_...`
# and `applicability_tests::preempt_threshold_rejected_off_threadx`, which a
# `::tests::` pattern misses. It does not appear in the parametrized e2e case
# names this lane exists to exclude (`entry_matrix::case_01_threadx_linux_c`,
# `test_rtos_action_e2e::platform_1_Platform__Freertos`).
test_terms=""
while IFS= read -r t; do
    # Capitalised form for the rstest-generated names (`Platform__Freertos`);
    # nextest's `~` is a case-SENSITIVE substring match, so one spelling covers
    # only half the suite.
    T="$(printf '%s' "$t" | sed -E 's/^(.)/\U\1/')"
    printf 'not binary(~%s)\n' "$t"
    for form in "$t" "$T"; do
        if [ -z "$test_terms" ]; then
            test_terms="not test(~$form)"
        else
            test_terms="$test_terms and not test(~$form)"
        fi
    done
done <<< "$tokens"

[ -n "$test_terms" ] && printf '(test(~tests::) or (%s))\n' "$test_terms"
