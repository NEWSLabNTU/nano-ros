#!/usr/bin/env bash
# scripts/zephyr/resolve-fvp-bin.sh
#
# Phase 214.A — resolve the directory containing the ARM Fast Models
# `FVP_BaseR_AEMv8R` binary, the way Zephyr's `cmake/emu/armfvp.cmake`
# expects (`find_program(ARMFVP PATHS ENV ARMFVP_BIN_PATH NAMES
# ${ARMFVP_BIN_NAME})`).
#
# Resolution order (first hit wins):
#   1. `ARMFVP_BIN_PATH` (Zephyr's canonical env; honoured verbatim).
#   2. `ARM_FVP_DIR` -- an Arm FVP install root; the binary is looked for at
#      the root and under `models/Linux64_GCC-*` and `bin/`.
#   3. `dirname $(command -v FVP_BaseR_AEMv8R)` -- PATH fallback.
#
# Prints the absolute directory containing the FVP binary on stdout, or
# exits non-zero with a clear hint on stderr (callers can capture stdout
# and bail on non-zero).
#
# The FVP is USER-PROVIDED (issue 2802): nano-ros does not download,
# install or distribute it. The user installs the AEMv8-R FVP from Arm and
# points one of the settings above at it. This script never downloads.

set -euo pipefail

BIN_NAME="${ARMFVP_BIN_NAME:-FVP_BaseR_AEMv8R}"

emit_dir() {
    # $1 = candidate directory; verify the binary actually lives there.
    if [ -x "$1/$BIN_NAME" ]; then
        # Canonicalise (FVP install may be symlinked).
        cd "$1" && pwd -P
        return 0
    fi
    return 1
}

# 1. ARMFVP_BIN_PATH (Zephyr canonical).
if [ -n "${ARMFVP_BIN_PATH:-}" ]; then
    if emit_dir "$ARMFVP_BIN_PATH"; then
        exit 0
    fi
    echo "resolve-fvp-bin: ARMFVP_BIN_PATH=$ARMFVP_BIN_PATH does not contain $BIN_NAME" >&2
    exit 1
fi

# 2. ARM_FVP_DIR -- a user-provided Arm FVP install root.
if [ -n "${ARM_FVP_DIR:-}" ]; then
    if [ ! -d "$ARM_FVP_DIR" ]; then
        echo "resolve-fvp-bin: ARM_FVP_DIR=$ARM_FVP_DIR is not a directory" >&2
        exit 1
    fi
    # Try common Linux64 GCC sublayouts (Arm ships /models/Linux64_GCC-<ver>).
    for cand in "$ARM_FVP_DIR" "$ARM_FVP_DIR"/models/Linux64_GCC-* \
                "$ARM_FVP_DIR"/bin "$ARM_FVP_DIR"/Base_RevC_AEMv8R_pkg/models/Linux64_GCC-* \
                "$ARM_FVP_DIR"/Base_RevC_AEMv8R_pkg/bin; do
        if emit_dir "$cand" 2>/dev/null; then
            exit 0
        fi
    done
    echo "resolve-fvp-bin: $BIN_NAME not found under ARM_FVP_DIR=$ARM_FVP_DIR" >&2
    echo "resolve-fvp-bin: hint — expected at \$ARM_FVP_DIR/models/Linux64_GCC-<ver>/$BIN_NAME" >&2
    exit 1
fi

# 3. PATH fallback.
if cmd_path=$(command -v "$BIN_NAME" 2>/dev/null); then
    emit_dir "$(dirname "$cmd_path")"
    exit 0
fi

# Nothing worked — emit setup hint.
cat >&2 <<EOF
resolve-fvp-bin: no $BIN_NAME found.

Set one of:
  ARMFVP_BIN_PATH  Directory containing $BIN_NAME (Zephyr-canonical env).
  ARM_FVP_DIR     Arm FVP install root (\$ARM_FVP_DIR/models/Linux64_GCC-*/$BIN_NAME).
  PATH             Add the directory containing $BIN_NAME.

nano-ros does not download or distribute the FVP. Install the AEMv8-R FVP
(FVP_Base_AEMv8R, x86_64 Linux) from Arm:

  https://developer.arm.com/downloads/-/arm-ecosystem-fvps

then export ARMFVP_BIN_PATH=<install>/bin (or ARM_FVP_DIR=<install>).
The model needs libatomic.so.1 at runtime (Debian/Ubuntu: libatomic1).
EOF
exit 1
