#!/usr/bin/env bash
# scripts/zephyr/cortex-r-rust-patch.sh
#
# Phase 117.11/13 — adds AArch32 Cortex-R (ARMv7-R / ARMv8-R) support to
# zephyr-lang-rust so `west build -b s32z2xxdc2/.../rtu0` and other
# Cortex-R targets don't fail with "Rust: Add support for other target".
#
# `_rust_map_target` returns `armv7r-none-eabihf` (Rust tier-2 target,
# installed by the project's rust-toolchain). Cortex-R52 implements
# ARMv8-R which is a superset of ARMv7-R; armv7r code runs natively.
#
# Idempotent — re-running detects prior application via grep.
#
# Usage:
#   scripts/zephyr/cortex-r-rust-patch.sh [<workspace-dir>]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NANO_ROS_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
IN_TREE_WORKSPACE="$NANO_ROS_ROOT/zephyr-workspace"
LEGACY_WORKSPACE="$(cd "$NANO_ROS_ROOT/.." && pwd)/nano-ros-workspace"

if [ -n "${1:-}" ]; then
    WORKSPACE="$1"
elif [ -n "${NROS_ZEPHYR_WORKSPACE:-}" ]; then
    WORKSPACE="$NROS_ZEPHYR_WORKSPACE"
elif [ -n "${ZEPHYR_WORKSPACE:-}" ]; then
    WORKSPACE="$ZEPHYR_WORKSPACE"
elif [ -d "$IN_TREE_WORKSPACE/zephyr" ]; then
    WORKSPACE="$IN_TREE_WORKSPACE"
else
    WORKSPACE="$LEGACY_WORKSPACE"
fi

if [ ! -d "$WORKSPACE/zephyr" ] || [ ! -d "$WORKSPACE/modules/lang/rust" ]; then
    echo "ERROR: $WORKSPACE doesn't look like a Zephyr workspace" >&2
    exit 1
fi

CMAKE_FILE="$WORKSPACE/modules/lang/rust/CMakeLists.txt"
KCONFIG_FILE="$WORKSPACE/modules/lang/rust/Kconfig"

for f in "$CMAKE_FILE" "$KCONFIG_FILE"; do
    [ -f "$f" ] || { echo "ERROR: expected file not found: $f" >&2; exit 1; }
done

# ---- Patch 1: modules/lang/rust/CMakeLists.txt ----
if grep -q 'CONFIG_CPU_AARCH32_CORTEX_R' "$CMAKE_FILE"; then
    echo "[skip] CMakeLists.txt already has Cortex-R case"
else
    echo "[apply] CMakeLists.txt += Cortex-R AArch32 case"
    python3 - "$CMAKE_FILE" <<'PY'
import sys
path = sys.argv[1]
with open(path) as f:
    src = f.read()
insert = """  elseif(CONFIG_CPU_AARCH32_CORTEX_R OR CONFIG_CPU_CORTEX_R52 OR CONFIG_CPU_CORTEX_R5)
    # AArch32 Cortex-R (ARMv7-R / ARMv8-R). Phase 117.11's NXP S32Z
    # R52 lands here. armv7r is the highest tier-2 Rust target that
    # covers both ISAs — Cortex-R52 (ARMv8-R) executes armv7r code
    # natively.
    if(CONFIG_FPU)
      set(RUST_TARGET "armv7r-none-eabihf" PARENT_SCOPE)
    else()
      set(RUST_TARGET "armv7r-none-eabi" PARENT_SCOPE)
    endif()
"""
anchor = "  elseif(CONFIG_RISCV)"
if anchor not in src:
    sys.exit(f"anchor missing: {anchor!r}")
src = src.replace(anchor, insert + anchor, 1)
with open(path, "w") as f:
    f.write(src)
PY
fi

# ---- Patch 2: modules/lang/rust/Kconfig ----
if grep -q 'CPU_CORTEX_R\b' "$KCONFIG_FILE"; then
    echo "[skip] Kconfig already lists Cortex-R in RUST_SUPPORTED"
else
    echo "[apply] Kconfig += Cortex-R to RUST_SUPPORTED"
    python3 - "$KCONFIG_FILE" <<'PY'
import sys
path = sys.argv[1]
with open(path) as f:
    src = f.read()
candidates = [
    ("CPU_AARCH64_CORTEX_R ||",
     "CPU_AARCH64_CORTEX_R || CPU_AARCH32_CORTEX_R ||"),
    ("CPU_AARCH32_CORTEX_A ||",
     "CPU_AARCH32_CORTEX_A || CPU_AARCH32_CORTEX_R ||"),
    ("default y if ((CPU_CORTEX_M ||",
     "default y if ((CPU_CORTEX_M || CPU_AARCH32_CORTEX_R ||"),
]
done = False
for old, new in candidates:
    if old in src:
        src = src.replace(old, new, 1)
        done = True
        break
if not done:
    sys.exit("Kconfig anchor missing")
with open(path, "w") as f:
    f.write(src)
PY
fi

echo "Cortex-R Rust patch applied to $WORKSPACE"
