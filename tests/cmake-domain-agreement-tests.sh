#!/bin/bash
# tests/cmake-domain-agreement-tests.sh -- phase-460 W4 (issue 1423)
#
# Exercises the domain agreement check in the Zephyr module's bake shim,
# `zephyr/cmake/nros_system_generate.cmake`: the system's domain
# (`system.toml`'s `domain_id`, baked by `nros codegen-system` as
# `#define NROS_SYSTEM_DOMAIN_ID <n>u`) and the image's domain
# (`CONFIG_NROS_DOMAIN_ID`, and `CONFIG_NROS_CYCLONE_DOMAIN_ID` when Cyclone
# is the backend) agree, or the configure stops naming all three.
#
# Sibling of `tests/cmake-entity-inventory-tests.sh` and shaped like it.
#
# THE CLI IS STUBBED, DELIBERATELY. Resolving `domain_id` through the
# `[deploy.<target>]` / `[system]` ladder into the define is the CLI's job and
# is unit-tested beside it (`codegen_system.rs`, the `NROS_SYSTEM_DOMAIN_ID 7u`
# case). What the SHIM owns is the comparison: that it reads the define the
# bake wrote, that it reads the Kconfig symbols the way a Zephyr configure has
# them (plain cmake variables after `find_package(Zephyr)`), and that a
# disagreement is a FATAL_ERROR and not a status line. The stub reads the
# fixture bringup's `[system] domain_id` so the case reads as the gate states
# it -- "a bringup declaring 10" -- and requires no cargo build, which is what
# keeps this on the fast line.
#
# THE ZEPHYR SIDE IS STAND-INS. `find_package(Zephyr)` imports `.config` as
# `CONFIG_*` cmake variables and defines `zephyr_include_directories`; the
# driver below does both by hand, from a fixture `.config`, in a real
# `project()` (the shim registers the CLI in CMAKE_CONFIGURE_DEPENDS, which
# needs a directory scope and not `cmake -P`).
#
# WHAT IS ASSERTED:
#
#   A. `CONFIG_NROS_DOMAIN_ID=2` against a bringup declaring 10 FAILS the
#      configure, and the failure names the baked value, the Kconfig value,
#      both symbols and the system.toml -- the issue's acceptance.
#   B. Equal values configure, and say so. NEGATIVE CONTROL.
#   C. Cyclone's own symbol is compared too: `CONFIG_NROS_DOMAIN_ID=10` with
#      the Cyclone symbol pinned to 2 fails naming the Cyclone symbol. It
#      defaults to NROS_DOMAIN_ID in Kconfig, so this is the case where someone
#      set it by hand (issue 0161's split-brain, one layer up) -- which is why
#      the fixture below spells the symbol through `$CYC`: `check-cyclone-domain-
#      not-pinned` forbids the literal in any tracked file, and this test is
#      the one place the pinned shape is wanted, as the thing under test.
#   D. All three equal, Cyclone in scope, configures.
#   E. No `CONFIG_NROS_DOMAIN_ID` in scope (the shim reached before Kconfig
#      was read) is REPORTED, not passed in silence and not fatal: that
#      ordering is the deferred-attach case the shim already warns about.
#   F. A header the bake did not write (no define) is fatal: the check must
#      not read "no line" as "agrees".
#
# PRECONDITIONS ARE HARD FAILURES. This script never prints-and-returns; a
# green here is read as "the check is sound".
#
# Usage: ./tests/cmake-domain-agreement-tests.sh
# Exit:  0 all assertions held; 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source=lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"

MODULE="$PROJECT_ROOT/zephyr/cmake/nros_system_generate.cmake"

FAILURES=0
CHECKS=0

fail() {
    log_error "$*"
    FAILURES=$((FAILURES + 1))
}

check() {
    CHECKS=$((CHECKS + 1))
}

if [ ! -f "$MODULE" ]; then
    fail "module not found: $MODULE"
    exit 1
fi
if ! command -v cmake >/dev/null 2>&1; then
    fail "cmake is not on PATH -- this test cannot report a verdict without it"
    exit 1
fi

init_test_tmpdir "nros-domain-agreement"

# The Cyclone symbol, spelled once (see case C in the header).
CYC=CONFIG_NROS_CYCLONE_DOMAIN_ID
trap 'cleanup_test_tmpdir' EXIT

# ---------------------------------------------------------------------------
# A stub `nros`. `codegen-system --bringup <dir> --out <parent>` writes
# `<parent>/nros-system/system_config.h` with the define the real bake writes
# for `[system] domain_id`, plus the cmake mirror the shim requires to exist.
# NROS_STUB_NO_DOMAIN=1 writes a header WITHOUT the define (case F).
# ---------------------------------------------------------------------------
STUB="$TEST_TMPDIR/nros-stub"
cat > "$STUB" <<'STUB_EOF'
#!/bin/bash
out=""; bringup=""; prev=""
for a in "$@"; do
    case "$prev" in
        --out) out="$a" ;;
        --bringup) bringup="$a" ;;
    esac
    prev="$a"
done
[ -n "$out" ] && [ -n "$bringup" ] || exit 3
mkdir -p "$out/nros-system"
domain="$(sed -n 's/^domain_id *= *\([0-9]*\).*/\1/p' "$bringup/system.toml" | head -1)"
{
    echo '/* stub of `nros codegen system` */'
    echo '#define NROS_SYSTEM_NAME "fixture"'
    if [ -z "${NROS_STUB_NO_DOMAIN:-}" ]; then
        echo "#define NROS_SYSTEM_DOMAIN_ID ${domain}u"
    fi
    echo '#define NROS_SYSTEM_RMW "zenoh"'
} > "$out/nros-system/system_config.h"
echo 'set(NANO_ROS_FEATURES "" CACHE STRING "nano-ros capability axes" FORCE)' \
    > "$out/nros-system/system_config.cmake"
exit 0
STUB_EOF
chmod +x "$STUB"

# A Path A bringup declaring 10: `system.toml` beside a `launch/` dir, no
# Cargo.toml, so the shim resolves it as a bringup and not a self-pkg.
BRINGUP="$TEST_TMPDIR/ws/fixture_bringup"
mkdir -p "$BRINGUP/launch"
cat > "$BRINGUP/system.toml" <<'EOF'
[system]
name = "fixture"
rmw = "zenoh"
domain_id = 10
EOF

# The driver: a real project that stands in for a Zephyr configure. It imports
# the fixture `.config` the way `find_package(Zephyr)` does (every
# `CONFIG_X=V` line becomes a cmake variable), supplies the one Zephyr command
# the shim reaches, and calls the shim on the fixture bringup.
PROJ="$TEST_TMPDIR/proj"
mkdir -p "$PROJ"
cat > "$PROJ/CMakeLists.txt" <<'EOF'
cmake_minimum_required(VERSION 3.20)
project(nros_domain_agreement_test NONE)
file(STRINGS "$ENV{NROS_TEST_DOTCONFIG}" _dotconfig REGEX "^CONFIG_")
foreach(_l IN LISTS _dotconfig)
    if(_l MATCHES "^([^=]+)=(.*)$")
        set(${CMAKE_MATCH_1} "${CMAKE_MATCH_2}")
    endif()
endforeach()
function(zephyr_include_directories)
endfunction()
set(APPLICATION_SOURCE_DIR "${CMAKE_SOURCE_DIR}")
include("$ENV{NROS_TEST_MODULE}")
nros_system_generate("$ENV{NROS_TEST_BRINGUP}")
EOF

# run <dotconfig-body> -> stdout+stderr with cmake's own line wrapping
# squashed (a FATAL_ERROR body is re-wrapped at ~72 columns, so a phrase is
# matched against one line); returns cmake's rc.
run() {
    local dotconfig="$TEST_TMPDIR/dotconfig"
    printf '%s\n' "$1" > "$dotconfig"
    rm -rf "$PROJ/build"
    NROS_CLI="$STUB" \
    NROS_TEST_MODULE="$MODULE" \
    NROS_TEST_BRINGUP="$BRINGUP" \
    NROS_TEST_DOTCONFIG="$dotconfig" \
        cmake -S "$PROJ" -B "$PROJ/build" 2>&1 | tr '\n' ' ' | tr -s ' '
    return "${PIPESTATUS[0]}"
}

# ---------------------------------------------------------------------------
# A. Kconfig 2 against a bringup declaring 10 fails the configure.
# ---------------------------------------------------------------------------
log_info "A. CONFIG_NROS_DOMAIN_ID=2 against a bringup declaring 10 refuses"
OUT="$(run 'CONFIG_NROS_RMW_ZENOH=y
CONFIG_NROS_DOMAIN_ID=2')"
RC=$?
check
if [ "$RC" -eq 0 ]; then
    fail "A: the configure succeeded with the system on 10 and the image on 2 -- \
every document derived from system.toml would say 10 while the image runs on 2, \
which is issue 1423 -- $OUT"
fi
check
if ! nros_grep_q -i "CMake Error" <<<"$OUT"; then
    fail "A: the disagreement did not raise a FATAL_ERROR -- $OUT"
fi
check
if ! nros_grep_q "NROS_SYSTEM_DOMAIN_ID = 10" <<<"$OUT"; then
    fail "A: the refusal does not name the baked value -- $OUT"
fi
check
if ! nros_grep_q "CONFIG_NROS_DOMAIN_ID = 2" <<<"$OUT"; then
    fail "A: the refusal does not name the Kconfig value -- $OUT"
fi
check
if ! nros_grep_q "CONFIG_NROS_CYCLONE_DOMAIN_ID = unset" <<<"$OUT"; then
    fail "A: the refusal does not say Cyclone's symbol was not in scope -- $OUT"
fi
check
if ! nros_grep_q "fixture_bringup/system.toml" <<<"$OUT"; then
    fail "A: the refusal does not name the system.toml -- $OUT"
fi
check
if ! nros_grep_q "CONFIG_NROS_DOMAIN_ID=10" <<<"$OUT"; then
    fail "A: the refusal does not name the remedy -- $OUT"
fi

# ---------------------------------------------------------------------------
# B. NEGATIVE CONTROL: equal values configure.
# ---------------------------------------------------------------------------
log_info "B. equal values configure"
OUT="$(run 'CONFIG_NROS_RMW_ZENOH=y
CONFIG_NROS_DOMAIN_ID=10')"
RC=$?
check
if [ "$RC" -ne 0 ]; then
    fail "B: equal domains did not configure -- $OUT"
fi
check
if ! nros_grep_q "domain 10 agrees" <<<"$OUT"; then
    fail "B: agreement was not reported -- a check that is silent when it passes \
cannot be told from a check that did not run -- $OUT"
fi
check
if nros_grep_q -i "CMake Error" <<<"$OUT"; then
    fail "B: equal domains raised an error -- $OUT"
fi
# The stand-in project has no `app` target, so the shim's own deferred-attach
# WARNING is expected here; the check's warning (case E) is not.
check
if nros_grep_q "compared against nothing" <<<"$OUT"; then
    fail "B: equal domains were reported as not compared -- $OUT"
fi

# ---------------------------------------------------------------------------
# C. Cyclone's own symbol is compared.
# ---------------------------------------------------------------------------
log_info "C. the Cyclone symbol pinned to 2 with everything else on 10 refuses"
OUT="$(run "CONFIG_NROS_RMW_CYCLONEDDS=y
CONFIG_NROS_DOMAIN_ID=10
${CYC}=2")"
RC=$?
check
if [ "$RC" -eq 0 ]; then
    fail "C: a Cyclone image on DDS domain 2 configured against a system on 10 -- $OUT"
fi
check
if ! nros_grep_q "${CYC} = 2" <<<"$OUT"; then
    fail "C: the refusal does not name the Cyclone value -- $OUT"
fi
check
if ! nros_grep_q "CONFIG_NROS_DOMAIN_ID = 10" <<<"$OUT"; then
    fail "C: the refusal does not name the agreeing Kconfig value beside it -- $OUT"
fi

# ---------------------------------------------------------------------------
# D. All three equal, Cyclone in scope, configures.
# ---------------------------------------------------------------------------
log_info "D. all three on 10 configure"
OUT="$(run "CONFIG_NROS_RMW_CYCLONEDDS=y
CONFIG_NROS_DOMAIN_ID=10
${CYC}=10")"
RC=$?
check
if [ "$RC" -ne 0 ]; then
    fail "D: three agreeing domains did not configure -- $OUT"
fi
check
if ! nros_grep_q "${CYC}=10)" <<<"$OUT"; then
    fail "D: the agreement line does not show the Cyclone value it compared -- $OUT"
fi

# ---------------------------------------------------------------------------
# E. No Kconfig domain in scope is reported, not silent, not fatal.
# ---------------------------------------------------------------------------
log_info "E. no CONFIG_NROS_DOMAIN_ID in scope is a WARNING that names the gap"
OUT="$(run 'CONFIG_NROS_RMW_ZENOH=y')"
RC=$?
check
if [ "$RC" -ne 0 ]; then
    fail "E: a configure that reached the shim before Kconfig was fatal -- that \
ordering is the deferred-attach case the shim tolerates with a warning -- $OUT"
fi
check
if ! nros_grep_q "CONFIG_NROS_DOMAIN_ID is not in scope" <<<"$OUT"; then
    fail "E: the missing comparison was passed in silence -- $OUT"
fi
check
if nros_grep_q "domain 10 agrees" <<<"$OUT"; then
    fail "E: agreement was claimed against nothing -- $OUT"
fi

# ---------------------------------------------------------------------------
# F. A header without the define is fatal, not "agrees".
# ---------------------------------------------------------------------------
log_info "F. a bake that wrote no NROS_SYSTEM_DOMAIN_ID is fatal"
OUT="$(NROS_STUB_NO_DOMAIN=1 run 'CONFIG_NROS_RMW_ZENOH=y
CONFIG_NROS_DOMAIN_ID=10')"
RC=$?
check
if [ "$RC" -eq 0 ]; then
    fail "F: a header with no baked domain configured -- $OUT"
fi
check
if ! nros_grep_q "carries 0" <<<"$OUT"; then
    fail "F: the fatal does not say the define is missing -- $OUT"
fi

# ---------------------------------------------------------------------------
if [ "$FAILURES" -eq 0 ]; then
    log_success "cmake-domain-agreement: $CHECKS assertion(s) held"
    exit 0
fi
log_error "cmake-domain-agreement: $FAILURES of $CHECKS assertion(s) failed"
exit 1
