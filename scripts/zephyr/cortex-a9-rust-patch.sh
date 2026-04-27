#!/usr/bin/env bash
# scripts/zephyr/cortex-a9-rust-patch.sh
#
# Apply the two upstream Zephyr workspace patches required to build
# nano-ros DDS examples for `qemu_cortex_a9` (Phase 92.1 + 92.4):
#
#   1. modules/lang/rust/CMakeLists.txt  + Kconfig
#      Add Cortex-A9 / Cortex-A7 (ARMv7-A) to `_rust_map_target` and
#      `RUST_SUPPORTED`, otherwise `west build -b qemu_cortex_a9`
#      fails with "Rust: Add support for other target".
#
#   2. zephyr/soc/xlnx/zynq7000/xc7zxxxs/soc.c
#      Add a flat MMU entry for the SLCR DT node (0xF8000000, 0x1000),
#      otherwise `eth_xlnx_gem_configure_clocks` data-aborts on the
#      first `sys_read32(0xf8000140)` because the SLCR page isn't
#      mapped.
#
# Idempotent: re-running detects the prior application via grep and
# skips. Safe to run from `just zephyr setup` and from CI re-provisioning.
#
# Usage:
#   scripts/zephyr/cortex-a9-rust-patch.sh [<workspace-dir>]
#
# If <workspace-dir> is omitted, falls back to the ZEPHYR_WORKSPACE env
# var, then to ../nano-ros-workspace relative to this script.

set -euo pipefail

# ---- Resolve workspace directory ----
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NANO_ROS_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
DEFAULT_WORKSPACE="$(cd "$NANO_ROS_ROOT/.." && pwd)/nano-ros-workspace"

WORKSPACE="${1:-${ZEPHYR_WORKSPACE:-$DEFAULT_WORKSPACE}}"

if [ ! -d "$WORKSPACE/zephyr" ] || [ ! -d "$WORKSPACE/modules/lang/rust" ]; then
    echo "ERROR: $WORKSPACE doesn't look like a Zephyr workspace" >&2
    echo "       (missing zephyr/ and/or modules/lang/rust/)" >&2
    echo "       Run \`just zephyr setup\` first, or pass the workspace dir explicitly." >&2
    exit 1
fi

CMAKE_FILE="$WORKSPACE/modules/lang/rust/CMakeLists.txt"
KCONFIG_FILE="$WORKSPACE/modules/lang/rust/Kconfig"
SOC_FILE="$WORKSPACE/zephyr/soc/xlnx/zynq7000/xc7zxxxs/soc.c"

for f in "$CMAKE_FILE" "$KCONFIG_FILE" "$SOC_FILE"; do
    if [ ! -f "$f" ]; then
        echo "ERROR: expected file not found: $f" >&2
        exit 1
    fi
done

# ---- Patch 1a: modules/lang/rust/CMakeLists.txt ----
if grep -q 'CONFIG_CPU_CORTEX_A9\|CONFIG_CPU_AARCH32_CORTEX_A' "$CMAKE_FILE"; then
    echo "[skip] CMakeLists.txt already has Cortex-A case"
else
    echo "[apply] CMakeLists.txt += Cortex-A9/A7 case"
    # Insert a new branch after the Cortex-M block, before the RISCV
    # branch. The anchor `elseif(CONFIG_RISCV)` is stable across the
    # zephyr-lang-rust v0.1.0 / main lineage we target.
    python3 - "$CMAKE_FILE" <<'PY'
import sys, re
path = sys.argv[1]
with open(path) as f:
    src = f.read()

insert = """  elseif(CONFIG_CPU_AARCH32_CORTEX_A OR CONFIG_CPU_CORTEX_A9 OR CONFIG_CPU_CORTEX_A7)
    # ARMv7-A (Cortex-A7 / A9). Hard-float when the FPU is present.
    if(CONFIG_FPU)
      set(RUST_TARGET "armv7a-none-eabihf" PARENT_SCOPE)
    else()
      set(RUST_TARGET "armv7a-none-eabi" PARENT_SCOPE)
    endif()
"""
anchor = "  elseif(CONFIG_RISCV)"
if anchor not in src:
    sys.exit(f"anchor missing: {anchor!r}")
new = src.replace(anchor, insert + anchor, 1)
with open(path, "w") as f:
    f.write(new)
PY
fi

# ---- Patch 1b: modules/lang/rust/Kconfig ----
if grep -q 'CPU_CORTEX_A9\|CPU_AARCH32_CORTEX_A' "$KCONFIG_FILE"; then
    echo "[skip] Kconfig already lists Cortex-A in RUST_SUPPORTED"
else
    echo "[apply] Kconfig += Cortex-A9/A7 to RUST_SUPPORTED"
    python3 - "$KCONFIG_FILE" <<'PY'
import sys
path = sys.argv[1]
with open(path) as f:
    src = f.read()
old = "default y if ((CPU_CORTEX_M ||"
new = "default y if ((CPU_CORTEX_M || CPU_AARCH32_CORTEX_A ||"
if old not in src:
    sys.exit(f"anchor missing: {old!r}")
src = src.replace(old, new, 1)
with open(path, "w") as f:
    f.write(src)
PY
fi

# ---- Patch 2: zephyr/soc/xlnx/zynq7000/xc7zxxxs/soc.c ----
if grep -q 'MMU_REGION_FLAT_ENTRY("slcr"' "$SOC_FILE"; then
    echo "[skip] soc.c already has SLCR MMU entry"
else
    echo "[apply] soc.c += SLCR MMU entry (0xF8000000, 0x1000)"
    python3 - "$SOC_FILE" <<'PY'
import sys
path = sys.argv[1]
with open(path) as f:
    src = f.read()

# Insert the SLCR entry just after the "mpcore" entry, so it sits
# alongside the other always-on system mappings.
anchor = '\tMMU_REGION_FLAT_ENTRY("mpcore",\n\t\t\t      0xF8F00000,\n\t\t\t      0x2000,\n\t\t\t      MT_STRONGLY_ORDERED | MPERM_R | MPERM_W),'
if anchor not in src:
    sys.exit(f"anchor missing — soc.c shape changed; update the patch script")

insert = '''
\tMMU_REGION_FLAT_ENTRY("slcr",
\t\t\t      0xF8000000,
\t\t\t      0x1000,
\t\t\t      MT_DEVICE | MATTR_SHARED | MPERM_R | MPERM_W),'''

src = src.replace(anchor, anchor + insert, 1)
with open(path, "w") as f:
    f.write(src)
PY
fi

echo "Cortex-A9 Rust patch applied to $WORKSPACE"
