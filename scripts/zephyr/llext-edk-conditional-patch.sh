#!/usr/bin/env bash
# scripts/zephyr/llext-edk-conditional-patch.sh
#
# Phase 168.X.fvp — patch zephyr-workspace/zephyr/CMakeLists.txt so
# the llext-edk `add_custom_command(OUTPUT ${llext_edk_file} ...)`
# fires only when `CONFIG_LLEXT=y`.
#
# Upstream Zephyr 3.7.0 LTS registers this command unconditionally.
# Its argument list expands `$<TARGET_PROPERTY:compiler,...>` and
# `llext_filter_zephyr_flags` (a `$<FILTER:...>` gen-expr) at the
# CMake generator phase, even when nothing depends on the
# `llext-edk` target. The host-gcc toolchain (used by native_sim)
# leaves several `compiler` properties undefined, and the cyclonedds
# branch's `zephyr_compile_options(SHELL:-include
# zephyr_ipv4_compat.h)` triggers a generator-expression evaluation
# failure under that combination:
#
#   CMake Error at .../zephyr/CMakeLists.txt:2145 (add_custom_command):
#     Error evaluating generator expression: $<JOIN:...>
#
# Upstream resolved this in PRs #83705 + #85624 (Zephyr 4.x) by
# splitting `CONFIG_LLEXT_EDK` out of `CONFIG_LLEXT` and gating the
# command on the new option. The 3.7.0 LTS we pin only has
# `LLEXT_EDK_NAME` / `LLEXT_EDK_USERSPACE_ONLY` (configuration of
# the EDK, no enable gate). This patch backports the spirit of the
# upstream fix: wrap the `add_custom_command` block in
# `if(CONFIG_LLEXT)` so the EDK rule only registers when the LLEXT
# subsystem itself is enabled. None of nano-ros's Zephyr examples
# enable LLEXT, so the wrap is functionally a no-op for them.
#
# Idempotent: detects prior application via grep and skips.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NANO_ROS_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
IN_TREE_WORKSPACE="$NANO_ROS_ROOT/zephyr-workspace"
LEGACY_WORKSPACE="$(cd "$NANO_ROS_ROOT/.." && pwd)/nano-ros-workspace"

if [ -n "${1:-}" ]; then
    WORKSPACE="$1"
elif [ -n "${NROS_ZEPHYR_WORKSPACE:-}" ]; then
    WORKSPACE="$NROS_ZEPHYR_WORKSPACE"
elif [ -d "$IN_TREE_WORKSPACE/zephyr" ]; then
    WORKSPACE="$IN_TREE_WORKSPACE"
else
    WORKSPACE="$LEGACY_WORKSPACE"
fi

CMAKE_FILE="$WORKSPACE/zephyr/CMakeLists.txt"
if [ ! -f "$CMAKE_FILE" ]; then
    echo "ERROR: $CMAKE_FILE missing" >&2
    exit 1
fi

if grep -q "nano-ros: llext-edk conditional" "$CMAKE_FILE"; then
    echo "[llext-edk-conditional-patch] already applied to $CMAKE_FILE"
    exit 0
fi

# Wrap the llext-edk block (set llext_edk_file → add_custom_target llext-edk)
# in `if(CONFIG_LLEXT) ... endif()`. The block starts with
# `# Extension Development Kit (EDK) generation.` and ends with
# `add_custom_target(llext-edk DEPENDS ${llext_edk_file})`. We use
# python for the multi-line scoped edit.

python3 - "$CMAKE_FILE" <<'PYEOF'
import sys
from pathlib import Path

path = Path(sys.argv[1])
src = path.read_text()

# Locate begin + end anchors.
begin_marker = "# Extension Development Kit (EDK) generation."
end_marker = "add_custom_target(llext-edk DEPENDS ${llext_edk_file})"

begin_idx = src.find(begin_marker)
end_idx = src.find(end_marker, begin_idx)
if begin_idx < 0 or end_idx < 0:
    print(f"ERROR: could not locate llext-edk block in {path}", file=sys.stderr)
    sys.exit(1)
end_idx += len(end_marker)

wrapped = (
    "# nano-ros: llext-edk conditional — Phase 168.X.fvp\n"
    "# Upstream Zephyr 3.7.0 LTS registers the llext-edk\n"
    "# `add_custom_command` unconditionally; the gen-expr-heavy\n"
    "# argument list fails to evaluate under host-gcc + cyclonedds\n"
    "# combinations. Gate the entire block under CONFIG_LLEXT so the\n"
    "# rule only registers when the LLEXT subsystem is enabled.\n"
    "# Upstream fix: Zephyr 4.x PRs #83705 + #85624 (CONFIG_LLEXT_EDK\n"
    "# Kconfig gate).\n"
    "if(CONFIG_LLEXT)\n"
    + src[begin_idx:end_idx]
    + "\nendif() # nano-ros: llext-edk conditional"
)

src = src[:begin_idx] + wrapped + src[end_idx:]
path.write_text(src)
PYEOF

echo "[llext-edk-conditional-patch] patched $CMAKE_FILE"
