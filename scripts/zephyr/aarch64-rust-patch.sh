#!/usr/bin/env bash
# scripts/zephyr/aarch64-rust-patch.sh
#
# Phase 117.10/13 — adds AArch64 Cortex-A support to zephyr-lang-rust
# so `west build -b fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp` (and
# future Cortex-A53/A72 targets) doesn't fail in `_rust_map_target`
# with "Rust: Add support for other target".
#
# Two-line edit:
#
#   1. modules/lang/rust/CMakeLists.txt
#      Add an `elseif(CONFIG_ARM64 ...)` branch returning
#      `aarch64-unknown-none` (rustc tier-2 bare-metal target).
#
#   2. modules/lang/rust/Kconfig
#      Add `CPU_AARCH64_CORTEX_A` to `RUST_SUPPORTED` so `CONFIG_RUST=y`
#      isn't silently auto-disabled on AArch64 boards.
#
# Idempotent — re-running detects prior application via grep.
#
# Usage:
#   scripts/zephyr/aarch64-rust-patch.sh [<workspace-dir>]

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
if grep -q 'CONFIG_CPU_AARCH64_CORTEX_A\|CONFIG_ARM64' "$CMAKE_FILE"; then
    echo "[skip] CMakeLists.txt already has AArch64 case"
else
    echo "[apply] CMakeLists.txt += AArch64 Cortex-A case"
    python3 - "$CMAKE_FILE" <<'PY'
import sys
path = sys.argv[1]
with open(path) as f:
    src = f.read()

insert = """  elseif(CONFIG_ARM64 OR CONFIG_CPU_AARCH64_CORTEX_A OR CONFIG_CPU_AARCH64_CORTEX_R OR CONFIG_CPU_CORTEX_A53 OR CONFIG_CPU_CORTEX_A72)
    # AArch64 Cortex-A / Cortex-R (Phase 117.10's FVP Base_RevC AEMv8-R SMP
    # is actually AArch64 Cortex-R despite the name; CPU_AARCH64_CORTEX_R
    # covers it).
    set(RUST_TARGET "aarch64-unknown-none" PARENT_SCOPE)
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
if grep -q 'CPU_AARCH64_CORTEX_A\|ARM64' "$KCONFIG_FILE"; then
    echo "[skip] Kconfig already lists AArch64 in RUST_SUPPORTED"
else
    echo "[apply] Kconfig += AArch64 Cortex-A to RUST_SUPPORTED"
    python3 - "$KCONFIG_FILE" <<'PY'
import sys
path = sys.argv[1]
with open(path) as f:
    src = f.read()
# Inject after CPU_AARCH32_CORTEX_A (added by cortex-a9-rust-patch.sh)
# if present, else after CPU_CORTEX_M (vanilla zephyr-lang-rust).
candidates = [
    ("CPU_AARCH32_CORTEX_A ||",
     "CPU_AARCH32_CORTEX_A || CPU_AARCH64_CORTEX_A || CPU_AARCH64_CORTEX_R ||"),
    ("default y if ((CPU_CORTEX_M ||",
     "default y if ((CPU_CORTEX_M || CPU_AARCH64_CORTEX_A || CPU_AARCH64_CORTEX_R ||"),
]
done = False
for old, new in candidates:
    if old in src:
        src = src.replace(old, new, 1)
        done = True
        break
if not done:
    sys.exit("Kconfig anchor missing — RUST_SUPPORTED line shape changed")
with open(path, "w") as f:
    f.write(src)
PY
fi

echo "AArch64 Rust patch applied to $WORKSPACE"
