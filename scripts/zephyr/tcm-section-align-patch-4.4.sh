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

WORKSPACE="${1:?usage: tcm-section-align-patch-4.4.sh <workspace-dir>}"
LD="$WORKSPACE/zephyr/include/zephyr/arch/arm/cortex_m/scripts/linker.ld"
CM="$WORKSPACE/zephyr/cmake/linker_script/arm/linker.cmake"
for f in "$LD" "$CM"; do
    if [ ! -f "$f" ]; then
        echo "ERROR: $f missing" >&2
        exit 1
    fi
done

python3 - "$LD" "$CM" <<'PYEOF'
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
    for old in olds:
        new = old.replace("SUBALIGN", "ALIGN")
        if src.count(old) == 1:
            src = src.replace(old, new, 1)
            changed += 1
        elif src.count(new) != 1:
            sys.stderr.write(f"ERROR: anchor not found in {path}: {old}\n")
            sys.exit(1)
    open(path, "w").write(src)

if changed:
    print(f"[tcm-section-align-patch-4.4] patched {changed} section(s)")
else:
    print("[tcm-section-align-patch-4.4] already applied")
PYEOF
