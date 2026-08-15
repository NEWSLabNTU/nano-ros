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
    nostd)
        # phase-359 W9 — the no_std FLAVOUR lane. Emitted as an INCLUSION union,
        # not exclusions, and that is forced by this file's own hazard: the host
        # platform `Linux` has no token, and `ThreadxLinux` — a no_std board
        # running as a Linux process — CONTAINS "linux". So "exclude the std
        # platforms" would either be inexpressible or would silently drop the
        # very platform the lane exists to run.
        #
        # Flavour comes from scripts/check-flavour-lanes.py, which DERIVES it
        # from the board manifests (a board is std iff it enables `std` on its
        # nros/nros-platform deps, followed transitively through board->board
        # deps — that is how NuttX resolves to std). One derivation, used by the
        # gate and by this lane, so they cannot disagree.
        flavours="$(python3 "$(dirname "$0")/../check-flavour-lanes.py" --print 2>/dev/null)"
        [ -n "$flavours" ] || { echo "lane-filter.sh: flavour table unavailable" >&2; exit 2; }
        expr=""
        while IFS="$(printf '\t')" read -r platform flavour; do
            [ "$flavour" = "no_std" ] || continue
            tok="$(printf '%s' "$platform" | sed -E 's/^([A-Z][a-z0-9]*).*/\1/' | tr 'A-Z' 'a-z')"
            case " $expr " in *" test(~$tok) "*) continue ;; esac
            if [ -z "$expr" ]; then expr="test(~$tok)"; else expr="$expr or test(~$tok)"; fi
        done <<EOF
$flavours
EOF
        [ -n "$expr" ] || { echo "lane-filter.sh: no no_std platforms derived" >&2; exit 2; }
        printf '%s\n' "$expr"
        # …AND exclude the std families explicitly. The inclusion union alone is
        # NOT enough, because family tokens overlap across flavours: the
        # `QemuBaremetal` (no_std) token is "qemu", and `tests/nuttx_qemu.rs`
        # matches it — so the union would drag NuttX, which is std, into the
        # no_std lane. Caught by inspection before this shipped; it is precisely
        # the mixing this work item exists to prevent.
        #
        # `Linux` needs no exclusion and must not get one: the host has no token,
        # so the union never selects it, while excluding "linux" would drop
        # `ThreadxLinux` — a no_std platform this lane must run.
        while IFS="$(printf '\t')" read -r platform flavour; do
            [ "$flavour" = "std" ] || continue
            [ "$platform" = "Linux" ] && continue
            tok="$(printf '%s' "$platform" | sed -E 's/^([A-Z][a-z0-9]*).*/\1/' | tr 'A-Z' 'a-z')"
            printf 'not test(~%s)\n' "$tok"
        done <<EOF
$flavours
EOF
        exit 0
        ;;
    *) echo "lane-filter.sh: expected 'native', 'nostd' or 'all', got '$lane'" >&2; exit 2 ;;
esac

matrix="packages/testing/nros-tests/src/matrix.rs"
[ -f "$matrix" ] || { echo "lane-filter.sh: $matrix not found" >&2; exit 2; }

# Variants of `enum PlatformId`, minus the HOST one — everything a host lane must
# not run. Take only the LEADING CamelCase word (the platform family):
# FreertosMps2 -> freertos, ThreadxRiscv64 -> threadx, ZephyrNativeSim -> zephyr.
#
# Splitting into all words would be actively wrong: ZephyrNativeSim yields
# "native" and ThreadxLinux yields "linux", either of which would exclude the
# host binaries this lane exists to run. Family names are also what the binaries
# are actually named after (freertos_qemu, threadx_riscv64_qemu, …).
#
# The host variant is `Linux` since phase-337 W8.b (was `Native`). Missing this
# rename is not a subtle failure: `Linux` survives into the token set, the lane
# emits `not test(~Linux)`, and the tier-1 lane silently stops running the
# platform it exists for — every `platform_N_Platform__Linux` rstest case
# filtered out, reported as a fast green.
tokens="$(
    awk '/^pub enum PlatformId \{/{f=1; next} f && /^\}/{exit} f' "$matrix" \
        | grep -oE '^\s{4}[A-Z][A-Za-z0-9]*,' \
        | tr -d ' ,' \
        | grep -v '^Linux$' \
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
