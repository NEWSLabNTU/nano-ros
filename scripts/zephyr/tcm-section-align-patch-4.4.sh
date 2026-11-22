#!/usr/bin/env bash
# scripts/zephyr/tcm-section-align-patch-4.4.sh
#
# phase-474 (I16) -- backport of two upstream Zephyr fixes that are NOT in
# v4.4.0:
#   de3deb75d44d "fix: DTCM section alignment for DMA TCD pool" (#107114,
#                issue #107058)
#   c456d2b1279b "arch: arm: cortex_m: linker: preserve input section
#                alignment for ITCM"
#
# Cortex-M's linker.ld (and the CMake linker generator's linker.cmake) declare
# the ITCM and DTCM output sections with SUBALIGN(4). SUBALIGN does not only
# align the output section: it FORCES every input section in it to 4-byte
# alignment, overriding a stricter one. eDMA's `dma_tcdpool0` is declared
# __aligned(32) in .dtcm_noinit, so it lands wherever the preceding DTCM
# content happens to end. On the S32K344 safety island it landed at
# 0x200128b8 once code relocation moved more .bss into DTCM, and the first
# LPUART async rx_enable asserted in EDMA_TcdSetTransferConfig
# (fsl_edma.c:517, `((uint32_t)tcd & 0x1FU) == 0U`).
#
# ALIGN(4) keeps the 4-byte section start and lets input alignment stand.
# Drop this script once the pinned Zephyr line contains both commits.
# Idempotent (an already-ALIGN(4) tree is reported as applied).
set -euo pipefail

# shellcheck source=scripts/lib/zephyr-patchset.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../lib" && pwd)/zephyr-patchset.sh"

# --- self-test --------------------------------------------------------------
#
# Stub copies of both files in upstream's SUBALIGN spelling: check mode on the
# pristine pair fails and writes nothing, a normal run patches, check mode then
# passes, and a SECOND normal run leaves both files' bytes AND mtimes alone.
#
#   bash scripts/zephyr/tcm-section-align-patch-4.4.sh --self-test
if [ "${1:-}" = "--self-test" ]; then
    _st="$(mktemp -d)"
    trap 'rm -rf "$_st"' EXIT
    _ld="$_st/ws/zephyr/include/zephyr/arch/arm/cortex_m/scripts/linker.ld"
    _cm="$_st/ws/zephyr/cmake/linker_script/arm/linker.cmake"
    mkdir -p "$(dirname "$_ld")" "$(dirname "$_cm")"
    printf '%s\n' \
        'SECTION_PROLOGUE(_ITCM_SECTION_NAME,,SUBALIGN(4))' \
        'SECTION_PROLOGUE(_DTCM_BSS_SECTION_NAME, (NOLOAD),SUBALIGN(4))' \
        'SECTION_PROLOGUE(_DTCM_NOINIT_SECTION_NAME, (NOLOAD),SUBALIGN(4))' \
        'SECTION_PROLOGUE(_DTCM_DATA_SECTION_NAME,,SUBALIGN(4))' > "$_ld"
    printf '%s\n' \
        'zephyr_linker_section(NAME .itcm GROUP ITCM_REGION SUBALIGN 4)' \
        'zephyr_linker_section(NAME .dtcm_bss GROUP DTCM_REGION SUBALIGN 4 TYPE BSS)' \
        'zephyr_linker_section(NAME .dtcm_noinit GROUP DTCM_REGION SUBALIGN 4 TYPE NOLOAD NOINIT)' \
        'zephyr_linker_section(NAME .dtcm_data GROUP DTCM_REGION SUBALIGN 4)' > "$_cm"
    _state() { stat -c '%Y.%y %s' "$_ld" "$_cm"; sha256sum "$_ld" "$_cm"; }
    # An mtime far in the past, so any rewrite moves it.
    _age() { touch -d '@1000000000' "$_ld" "$_cm"; }
    _fail=0
    _bad() { echo "tcm-section-align-patch self-test: $1" >&2; _fail=1; }
    _age; _s0="$(_state)"
    _rc=0; NROS_PATCH_CHECK=1 bash "${BASH_SOURCE[0]}" "$_st/ws" >/dev/null 2>&1 || _rc=$?
    [ "$_rc" -eq 1 ] || _bad "check mode on a pristine tree exited $_rc, want 1"
    [ "$(_state)" = "$_s0" ] || _bad "check mode wrote to a pristine tree"
    bash "${BASH_SOURCE[0]}" "$_st/ws" >/dev/null 2>&1 || _bad "the first normal run failed"
    [ "$(grep -c 'SUBALIGN' "$_ld" "$_cm" | awk -F: '{s+=$2} END {print s}')" = 0 ] \
        || _bad "the first normal run left a SUBALIGN"
    _rc=0; NROS_PATCH_CHECK=1 bash "${BASH_SOURCE[0]}" "$_st/ws" >/dev/null 2>&1 || _rc=$?
    [ "$_rc" -eq 0 ] || _bad "check mode on a patched tree exited $_rc, want 0"
    _age; _s1="$(_state)"
    _out="$(bash "${BASH_SOURCE[0]}" "$_st/ws" 2>&1)" || _bad "the second normal run failed"
    [ "$(_state)" = "$_s1" ] || _bad "the second normal run rewrote an applied file (mtime or bytes moved)"
    case "$_out" in *"already applied"*) ;; *) _bad "the second normal run did not say 'already applied': $_out" ;; esac
    if [ "$_fail" -eq 0 ]; then
        echo "tcm-section-align-patch self-test: OK (check mode, apply, and an applied tree left untouched)"
        exit 0
    fi
    exit 1
fi

WORKSPACE="${1:?usage: tcm-section-align-patch-4.4.sh <workspace-dir>}"
LD="$WORKSPACE/zephyr/include/zephyr/arch/arm/cortex_m/scripts/linker.ld"
CM="$WORKSPACE/zephyr/cmake/linker_script/arm/linker.cmake"
for f in "$LD" "$CM"; do
    if [ ! -f "$f" ]; then
        echo "ERROR: $f missing" >&2
        exit 1
    fi
done

# Check mode (issue 2831): the guard is in the Python below, so it exits 10
# where it would write, and the shared helper reports the missing edit.
_py_rc=0
python3 - "$LD" "$CM" <<'PYEOF' || _py_rc=$?
import os
import sys

ld, cm = sys.argv[1], sys.argv[2]
edits = {
    ld: [
        "SECTION_PROLOGUE(_ITCM_SECTION_NAME,,SUBALIGN(4))",
        "SECTION_PROLOGUE(_DTCM_BSS_SECTION_NAME, (NOLOAD),SUBALIGN(4))",
        "SECTION_PROLOGUE(_DTCM_NOINIT_SECTION_NAME, (NOLOAD),SUBALIGN(4))",
        "SECTION_PROLOGUE(_DTCM_DATA_SECTION_NAME,,SUBALIGN(4))",
    ],
    cm: [
        "zephyr_linker_section(NAME .itcm GROUP ITCM_REGION SUBALIGN 4)",
        "zephyr_linker_section(NAME .dtcm_bss GROUP DTCM_REGION SUBALIGN 4 TYPE BSS)",
        "zephyr_linker_section(NAME .dtcm_noinit GROUP DTCM_REGION SUBALIGN 4 TYPE NOLOAD NOINIT)",
        "zephyr_linker_section(NAME .dtcm_data GROUP DTCM_REGION SUBALIGN 4)",
    ],
}

changed = 0
for path, olds in edits.items():
    src = open(path).read()
    before = changed
    for old in olds:
        new = old.replace("SUBALIGN", "ALIGN")
        if src.count(old) == 1:
            src = src.replace(old, new, 1)
            changed += 1
        elif src.count(new) != 1:
            sys.stderr.write(f"ERROR: anchor not found in {path}: {old}\n")
            sys.exit(1)
    # A file with no edit left to make is not written, in either mode: the
    # same text written back still moves its mtime, and every build that
    # depends on the linker script then reconfigures for nothing.
    if changed == before:
        continue
    # Check mode never writes.
    if os.environ.get("NROS_PATCH_CHECK") == "1":
        sys.exit(10)
    open(path, "w").write(src)

if changed:
    print(f"[tcm-section-align-patch-4.4] patched {changed} section(s)")
else:
    print("[tcm-section-align-patch-4.4] already applied")
PYEOF
if [ "$_py_rc" -eq 10 ]; then
    nros_patch_would_apply tcm-section-align-patch-4.4 "$LD / $CM: TCM sections SUBALIGN(4) -> ALIGN(4)"
fi
exit "$_py_rc"
