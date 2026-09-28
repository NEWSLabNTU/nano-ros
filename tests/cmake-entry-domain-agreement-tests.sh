#!/usr/bin/env bash
# Issue 1550 -- the ENTRY road's domain check (`cmake/NanoRosDomainAgreement.cmake`).
#
# `nano_ros_add_executable` -> `nano_ros_entry` on Zephyr compares three
# statements of one image's ROS domain: `CONFIG_NROS_DOMAIN_ID` (what the image
# bakes), the entry's `system.toml` `domain_id`, and every Kconfig fragment the
# configure merged that states the symbol -- the transport snippet above all.
# This drives the module with a stand-in for Zephyr's `merge_config_files` and
# a real snippet directory, so it needs cmake and nothing else.
#
#   A. a snippet that states nothing, Kconfig's default 0, system.toml 10:
#      refused, naming all three (the island's `island-ethernet` shape)
#   B. the snippet states 10, Kconfig 10, system.toml 10: agrees, and the
#      provenance names the snippet
#   C. a -D on the command line overrides the snippet's 10 with 5: refused,
#      naming the snippet's 10 and the command line's 5
#   D. a board .conf states 10 and the snippet nothing: agrees (kconfig)
#   E. no system.toml and no fragment: agrees on the default (nothing to
#      disagree with), and the provenance says `default`

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

source "$SCRIPT_DIR/lib/common.sh"

MODULE="$PROJECT_ROOT/cmake/NanoRosDomainAgreement.cmake"

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

init_test_tmpdir "nros-entry-domain-agreement"
trap 'cleanup_test_tmpdir' EXIT

# A snippet the way Zephyr lays one out: a directory with snippet.yml and the
# fragment it appends.
SNIP="$TEST_TMPDIR/snippets/eth"
mkdir -p "$SNIP"
cat > "$SNIP/snippet.yml" <<'EOF'
name: island-ethernet
append:
  EXTRA_CONF_FILE: ethernet.conf
EOF
BOARD_CONF="$TEST_TMPDIR/boards/board.conf"
mkdir -p "$(dirname "$BOARD_CONF")"
EXTRA="$TEST_TMPDIR/build/zephyr/misc/generated/extra_kconfig_options.conf"
mkdir -p "$(dirname "$EXTRA")"

PROJ="$TEST_TMPDIR/proj"
mkdir -p "$PROJ"
cat > "$PROJ/CMakeLists.txt" <<'EOF'
cmake_minimum_required(VERSION 3.20)
project(nros_entry_domain_agreement_test NONE)
include("$ENV{NROS_TEST_MODULE}")
set(CONFIG_NROS_DOMAIN_ID "$ENV{NROS_TEST_KCONFIG}")
set(SNIPPET "nros-zenoh;island-ethernet")
string(REPLACE ":" ";" merge_config_files "$ENV{NROS_TEST_FRAGMENTS}")
nros_domain_provenance(_src _label)
message(STATUS "PROVENANCE=${_src}")
nros_check_domain_agreement(
    SYSTEM_DOMAIN "$ENV{NROS_TEST_SYSTEM}"
    SYSTEM_FILE "/ws/src/entry/system.toml"
    CONTEXT "nano_ros_entry(test)")
EOF

# run <kconfig> <system-domain> <fragment>... ; prints the configure output
run() {
    local kconfig="$1" system="$2"
    shift 2
    local frags
    frags="$(IFS=:; echo "$*")"
    rm -rf "$PROJ/build"
    NROS_TEST_MODULE="$MODULE" \
    NROS_TEST_KCONFIG="$kconfig" \
    NROS_TEST_SYSTEM="$system" \
    NROS_TEST_FRAGMENTS="$frags" \
        cmake -S "$PROJ" -B "$PROJ/build" 2>&1 | tr '\n' ' ' | tr -s ' '
    return "${PIPESTATUS[0]}"
}

# ---------------------------------------------------------------------------
log_info "A. a snippet that states nothing, Kconfig 0, system.toml 10 refuses"
printf 'CONFIG_NET_L2_ETHERNET=y\n' > "$SNIP/ethernet.conf"
OUT="$(run 0 10 "$SNIP/ethernet.conf")"
RC=$?
check
[ "$RC" -ne 0 ] || fail "A: the image on 0 configured against a system on 10 -- $OUT"
check
nros_grep_q "CONFIG_NROS_DOMAIN_ID = 0" <<<"$OUT" \
    || fail "A: the refusal does not name the image's value -- $OUT"
check
nros_grep_q "system.toml domain_id = 10" <<<"$OUT" \
    || fail "A: the refusal does not name system.toml's value -- $OUT"
check
nros_grep_q "snippet = (not stated; active snippets: nros-zenoh, island-ethernet)" <<<"$OUT" \
    || fail "A: the refusal does not say the snippet stated nothing -- $OUT"
check
nros_grep_q "PROVENANCE=default" <<<"$OUT" \
    || fail "A: nothing stated the domain, so its source is the default -- $OUT"

# ---------------------------------------------------------------------------
log_info "B. the snippet states 10, Kconfig 10, system.toml 10 agrees"
printf 'CONFIG_NET_L2_ETHERNET=y\nCONFIG_NROS_DOMAIN_ID=10\n' > "$SNIP/ethernet.conf"
OUT="$(run 10 10 "$SNIP/ethernet.conf")"
RC=$?
check
[ "$RC" -eq 0 ] || fail "B: three equal statements refused -- $OUT"
check
nros_grep_q "domain 10 agrees -- CONFIG_NROS_DOMAIN_ID from snippet island-ethernet" <<<"$OUT" \
    || fail "B: the agreement line does not name the snippet -- $OUT"
check
nros_grep_q "PROVENANCE=snippet" <<<"$OUT" \
    || fail "B: the provenance is not the snippet -- $OUT"

# ---------------------------------------------------------------------------
log_info "C. a command-line 5 over the snippet's 10 refuses, naming both"
printf 'CONFIG_NROS_DOMAIN_ID=5\n' > "$EXTRA"
OUT="$(run 5 "" "$SNIP/ethernet.conf" "$EXTRA")"
RC=$?
check
[ "$RC" -ne 0 ] || fail "C: the snippet said 10 and the image baked 5 -- $OUT"
check
nros_grep_q "snippet = 10" <<<"$OUT" \
    || fail "C: the refusal does not name the snippet's value -- $OUT"
check
nros_grep_q "CONFIG_NROS_DOMAIN_ID = 5 (what the image bakes; from -DCONFIG_NROS_DOMAIN_ID on the cmake/west command line)" <<<"$OUT" \
    || fail "C: the refusal does not name the command line as the source -- $OUT"
check
nros_grep_q "PROVENANCE=command-line" <<<"$OUT" \
    || fail "C: the provenance is not the command line -- $OUT"

# ---------------------------------------------------------------------------
log_info "D. a board .conf states 10 and the snippet nothing: agrees"
printf 'CONFIG_NET_L2_ETHERNET=y\n' > "$SNIP/ethernet.conf"
printf 'CONFIG_NROS_DOMAIN_ID=10\n' > "$BOARD_CONF"
OUT="$(run 10 10 "$BOARD_CONF" "$SNIP/ethernet.conf")"
RC=$?
check
[ "$RC" -eq 0 ] || fail "D: a board conf on 10 with system.toml 10 refused -- $OUT"
check
nros_grep_q "PROVENANCE=kconfig" <<<"$OUT" \
    || fail "D: the provenance is not the board conf -- $OUT"

# ---------------------------------------------------------------------------
log_info "E. nothing stated anywhere agrees on the default"
OUT="$(run 0 "" "$SNIP/ethernet.conf")"
RC=$?
check
[ "$RC" -eq 0 ] || fail "E: an image with no statement at all refused -- $OUT"
check
nros_grep_q "domain 0 agrees -- CONFIG_NROS_DOMAIN_ID from the Kconfig default" <<<"$OUT" \
    || fail "E: the agreement line does not say the value is the default -- $OUT"

# ---------------------------------------------------------------------------
if [ "$FAILURES" -eq 0 ]; then
    log_success "cmake-entry-domain-agreement: $CHECKS assertion(s) held"
    exit 0
fi
log_error "cmake-entry-domain-agreement: $FAILURES of $CHECKS assertion(s) failed"
exit 1
