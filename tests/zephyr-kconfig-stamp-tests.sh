#!/usr/bin/env bash
# tests/zephyr-kconfig-stamp-tests.sh -- issue 1699, phase-530 W4 (RFC-0120 D6
# case 4): a changed Kconfig DEFAULT reaches an EXISTING Zephyr build dir in ONE
# configure.
#
# WHAT THIS MEASURES
#
# Zephyr regenerates `.config` from the conf fragments only when their checksum
# moves; otherwise it feeds the OLD `.config` back in, and a value there is a
# user choice a new default never overrides. The module's stamp
# (`_nros_image_kconfig_stamp`, in `zephyr/cmake/nros_image_kconfig.cmake`) is a
# comment-only `*.conf` in the application build dir naming the digests of the
# module's Kconfig and of `Kconfig.from-system-toml`; Zephyr's checksum covers
# every `*.conf` there. The hook writes it BEFORE Kconfig, so the configure that
# sees the edit is the one that regenerates `.config`. (Written after Kconfig,
# from `zephyr/CMakeLists.txt`, it cost a second configure through the fixed
# point -- and this case never had a test.)
#
# A real Zephyr configure of a hello-world application, with a stand-in module
# whose `module_ext_root` hook calls the REAL stamp function on the module's own
# Kconfig (a prompted symbol, so the old value IS kept as a user choice without
# the stamp):
#
#   1  clean configure                 .config has the first default
#   2  edit the module Kconfig default  ONE configure, .config has the new one
#   3  edit the second file (the        ONE configure, .config has the new one
#      `Kconfig.from-system-toml` role)
#   4  the control: the same edit with  .config KEEPS the old value -- so this
#      the stamp switched off           test can see the defect at all
#
# PRECONDITIONS ARE HARD FAILURES: no Zephyr workspace is a red, never a skip
# (the `just` recipe's ledger is where a skip belongs).
#
# Usage: ./tests/zephyr-kconfig-stamp-tests.sh
#   ZEPHYR_WS (default: $HOME/.nros/workspaces/zephyr/3.7, the line issue 1699
#   was measured on; 4.x checksums Kconfig sources itself, and the control
#   reads which), BOARD (default native_sim/native/64).

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"
# shellcheck source=../scripts/lib/grep-q.sh
source "$PROJECT_ROOT/scripts/lib/grep-q.sh"
# shellcheck source=../scripts/lib/deadline.sh
source "$PROJECT_ROOT/scripts/lib/deadline.sh"

HOOK="$PROJECT_ROOT/zephyr/cmake/nros_image_kconfig.cmake"
ZEPHYR_WS="${ZEPHYR_WS:-$HOME/.nros/workspaces/zephyr/3.7}"
BOARD="${BOARD:-native_sim/native/64}"

FAILURES=0
CHECKS=0
fail() { log_error "$*"; FAILURES=$((FAILURES + 1)); }
check() { CHECKS=$((CHECKS + 1)); }

if [ ! -f "$HOOK" ]; then
    fail "hook not found: $HOOK"
    exit 1
fi
if [ ! -d "$ZEPHYR_WS/zephyr" ]; then
    fail "no Zephyr workspace at $ZEPHYR_WS (set ZEPHYR_WS; \`nros setup zephyr\` provisions one)"
    exit 1
fi
for _t in cmake ninja; do
    if ! command -v "$_t" >/dev/null 2>&1; then
        fail "$_t is not on PATH -- this test cannot report a verdict without it"
        exit 1
    fi
done
# west's interpreter for the line, when the store has one.
for _v in "$ZEPHYR_WS/.venv312/bin" "$ZEPHYR_WS/.venv/bin" \
          "$HOME/.nros/workspaces/zephyr/4.4/.venv312/bin"; do
    [ -x "$_v/python" ] && PATH="$_v:$PATH" && break
done
PY="$(command -v python3)"
export ZEPHYR_BASE="$ZEPHYR_WS/zephyr"

init_test_tmpdir "nros-kconfig-stamp"
trap 'cleanup_test_tmpdir' EXIT

MOD="$TEST_TMPDIR/mod"
APP="$TEST_TMPDIR/app"
mkdir -p "$MOD/zephyr/modules" "$APP/src"
cat > "$MOD/zephyr/module.yml" <<'EOF'
name: nros_kconfig_stamp_probe
build:
  kconfig: zephyr/Kconfig
  settings:
    module_ext_root: zephyr
EOF
cat > "$MOD/zephyr/modules/modules.cmake" <<EOF
include("$HOOK")
if(NOT PROBE_NO_STAMP)
    _nros_image_kconfig_stamp("$MOD/zephyr/Kconfig" "$MOD/zephyr/Kconfig.gen"
        "\${APPLICATION_BINARY_DIR}")
endif()
EOF
write_kconfig() { # <main default> <gen default>
    printf 'config NROS_KSTAMP_PROBE\n\tint "probe"\n\tdefault %s\n\nrsource "Kconfig.gen"\n' \
        "$1" > "$MOD/zephyr/Kconfig"
    printf 'config NROS_KSTAMP_GEN\n\tint "generated"\n\tdefault %s\n' "$2" > "$MOD/zephyr/Kconfig.gen"
}
write_kconfig 1 10
cat > "$APP/CMakeLists.txt" <<'EOF'
cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_kconfig_stamp_app)
target_sources(app PRIVATE src/main.c)
EOF
printf 'int main(void) { return 0; }\n' > "$APP/src/main.c"
: > "$APP/prj.conf"

# One build-system step: a configure (clean) or ninja's own re-run of it, never
# a compile -- `build.ninja` is the target ninja brings up to date first.
configure_clean() { # <build> [-D...]
    local b="$1"; shift
    STEP_OUT="$(cd "$ZEPHYR_WS" && "${NROS_DEADLINE[@]}" 900 cmake -G Ninja -S "$APP" -B "$b" \
        -DBOARD="$BOARD" -DZEPHYR_EXTRA_MODULES="$MOD" -DWEST_PYTHON="$PY" "$@" 2>&1)"
    STEP_RC=$?
}
rebuild() { # <build>
    STEP_OUT="$("${NROS_DEADLINE[@]}" 900 ninja -C "$1" build.ninja 2>&1)"
    STEP_RC=$?
}
configures() { grep -c -- '-- Configuring done' <<<"$STEP_OUT"; }
value() { # <build> <symbol>
    sed -n "s/^CONFIG_$2=//p" "$1/zephyr/.config"
}

B="$TEST_TMPDIR/build"
log_header "1. clean configure"
configure_clean "$B"
check
if [ "$STEP_RC" -eq 0 ] && [ "$(value "$B" NROS_KSTAMP_PROBE)" = "1" ] \
   && [ "$(value "$B" NROS_KSTAMP_GEN)" = "10" ] && [ -f "$B/nros-module-kconfig.conf" ]; then
    log_success "clean: .config has the first defaults and the stamp is in the build dir"
else
    fail "clean configure did not set up (rc=$STEP_RC, probe=$(value "$B" NROS_KSTAMP_PROBE)):
$STEP_OUT"
    exit 1
fi

log_header "2. the module Kconfig default moves: one configure"
sleep 1
write_kconfig 2 10
rebuild "$B"
check
if [ "$STEP_RC" -eq 0 ] && [ "$(configures)" -eq 1 ] && [ "$(value "$B" NROS_KSTAMP_PROBE)" = "2" ]; then
    log_success "the new default landed in ONE configure"
else
    fail "want 1 configure and CONFIG_NROS_KSTAMP_PROBE=2; got rc=$STEP_RC, \
$(configures) configure(s), value $(value "$B" NROS_KSTAMP_PROBE):
$STEP_OUT"
fi

log_header "3. the second file (Kconfig.from-system-toml's role) moves: one configure"
sleep 1
write_kconfig 2 11
rebuild "$B"
check
if [ "$STEP_RC" -eq 0 ] && [ "$(configures)" -eq 1 ] && [ "$(value "$B" NROS_KSTAMP_GEN)" = "11" ]; then
    log_success "a default the second file renders landed in ONE configure"
else
    fail "want 1 configure and CONFIG_NROS_KSTAMP_GEN=11; got rc=$STEP_RC, \
$(configures) configure(s), value $(value "$B" NROS_KSTAMP_GEN):
$STEP_OUT"
fi

log_header "4. control: no stamp, the old value is kept"
write_kconfig 1 10
C="$TEST_TMPDIR/control"
configure_clean "$C" -DPROBE_NO_STAMP=ON
sleep 1
write_kconfig 2 10
rebuild "$C"
# Zephyr 4.x adds every parsed Kconfig source to the `.config` checksum itself
# (`merge_kconfig_checksum` in `cmake/modules/kconfig.cmake`); 3.7 does not,
# which is the line issue 1699 was measured on. So the control's expected
# outcome is a property of the Zephyr line, READ from its kconfig.cmake.
KCONFIG_CMAKE="$ZEPHYR_BASE/cmake/modules/kconfig.cmake"
check
nros_grep_q merge_kconfig_checksum "$KCONFIG_CMAKE"
case $? in
    0) LINE_CHECKSUMS_KCONFIG=1 ;;
    1) LINE_CHECKSUMS_KCONFIG=0 ;;
    *) fail "cannot read $KCONFIG_CMAKE"; exit 1 ;;
esac
if [ "$LINE_CHECKSUMS_KCONFIG" -eq 1 ]; then
    if [ "$STEP_RC" -eq 0 ] && [ "$(value "$C" NROS_KSTAMP_PROBE)" = "2" ]; then
        log_success "control: this Zephyr line checksums its Kconfig sources itself \
($KCONFIG_CMAKE), so the edit lands without the stamp too; cases 2-3 still hold the stamp to one configure"
    else
        fail "this Zephyr line checksums its Kconfig sources, and the edit still did not land \
(rc=$STEP_RC, value $(value "$C" NROS_KSTAMP_PROBE)):
$STEP_OUT"
    fi
elif [ "$STEP_RC" -eq 0 ] && [ "$(value "$C" NROS_KSTAMP_PROBE)" = "1" ]; then
    log_success "control: without the stamp the edit does not reach the build dir (the defect is visible here)"
else
    fail "the control did not reproduce issue 1699, so cases 2 and 3 prove nothing \
(rc=$STEP_RC, value $(value "$C" NROS_KSTAMP_PROBE)):
$STEP_OUT"
fi

log_header "Summary"
if [ "$FAILURES" -eq 0 ]; then
    log_success "$CHECKS checks passed"
    exit 0
fi
log_error "$FAILURES of $CHECKS checks failed"
exit 1
