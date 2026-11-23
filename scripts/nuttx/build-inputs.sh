# shellcheck shell=bash
#
# The board inputs of `build-nuttx.sh`, resolved ONCE (issues 2914, 2915).
#
# Sourced, never run. Self-test: `scripts/nuttx/check-build-inputs.sh`
# (`just check nuttx-build-inputs`).

# nros_nuttx_resolve_defconfig <given> <caller-cwd>
#
# Issue 2915 — `build-nuttx.sh` enters the NuttX tree before it reads the
# defconfig, so a path relative to where the CALLER stood was read from inside
# that tree, missed, and was reported as "declares no CONFIG_ARCH". Resolve it
# against the caller's cwd first, and refuse a path that is not a file, naming
# it both as given and as resolved. Prints the absolute path.
nros_nuttx_resolve_defconfig() {
    local given="$1" cwd="$2" abs
    case "$given" in
        /*) abs="$given" ;;
        *) abs="$cwd/$given" ;;
    esac
    if [ ! -f "$abs" ]; then
        echo "build-nuttx.sh: NUTTX_DEFCONFIG '$given' is not a file" \
            "(resolved against $cwd: $abs)" >&2
        return 1
    fi
    printf '%s\n' "$abs"
}

# nros_nuttx_defconfig_value <defconfig> <SYMBOL>
#
# The value of `CONFIG_<SYMBOL>` as the defconfig itself states it, quotes
# stripped; empty when it states none. Only the defconfig's own lines are read:
# an `#include`d upstream config is NuttX's to expand, not ours.
nros_nuttx_defconfig_value() {
    sed -n "s/^CONFIG_$2=\"\{0,1\}\([^\"]*\)\"\{0,1\}\$/\1/p" "$1" | head -n 1
}

# nros_nuttx_board_makedefs <nuttx-dir> <defconfig> [override]
#
# Issue 2914 — the board's Make.defs is a fact the defconfig already states
# (`CONFIG_ARCH` / `CONFIG_ARCH_CHIP` / `CONFIG_ARCH_BOARD` name the NuttX
# board dir `boards/<arch>/<chip>/<board>`). It used to be a second, independent
# input that defaulted to the qemu-arm board, so a riscv defconfig alone was
# configured with the ARM Toolchain.defs and compiled by the host gcc with
# `-mfloat-abi=soft`. Now:
#   - no override: DERIVED from the defconfig;
#   - an override (`NUTTX_BOARD_MAKEDEFS`, relative to the NuttX tree) is kept
#     only when it is under `boards/<arch>/` for the defconfig's arch, and is
#     refused otherwise, naming both files.
# Prints the absolute Make.defs path; refuses one that does not exist.
nros_nuttx_board_makedefs() {
    local nuttx_dir="$1" defconfig="$2" override="${3:-}"
    local arch chip board sym rel
    arch="$(nros_nuttx_defconfig_value "$defconfig" ARCH)"
    chip="$(nros_nuttx_defconfig_value "$defconfig" ARCH_CHIP)"
    board="$(nros_nuttx_defconfig_value "$defconfig" ARCH_BOARD)"
    if [ -n "$override" ]; then
        rel="${override#./}"
        if [ -z "$arch" ]; then
            echo "build-nuttx.sh: $defconfig states no CONFIG_ARCH, so the board" \
                "Make.defs '$override' cannot be checked against it" >&2
            return 1
        fi
        case "$rel" in
            "boards/$arch/"*) ;;
            *)
                echo "build-nuttx.sh: NUTTX_BOARD_MAKEDEFS '$override' is not a" \
                    "'$arch' board's, but the defconfig is:" >&2
                echo "  defconfig: $defconfig (CONFIG_ARCH=\"$arch\")" >&2
                echo "  Make.defs: $override" >&2
                echo "  Unset NUTTX_BOARD_MAKEDEFS to take the defconfig's own" \
                    "(boards/$arch/${chip:-<chip>}/${board:-<board>}/scripts/Make.defs)." >&2
                return 1
                ;;
        esac
    else
        for sym in ARCH ARCH_CHIP ARCH_BOARD; do
            if [ -z "$(nros_nuttx_defconfig_value "$defconfig" "$sym")" ]; then
                echo "build-nuttx.sh: $defconfig states no CONFIG_$sym, so its" \
                    "board Make.defs cannot be derived; state it, or set" \
                    "NUTTX_BOARD_MAKEDEFS" >&2
                return 1
            fi
        done
        rel="boards/$arch/$chip/$board/scripts/Make.defs"
    fi
    if [ ! -f "$nuttx_dir/$rel" ]; then
        echo "build-nuttx.sh: no board Make.defs at $nuttx_dir/$rel" \
            "(for $defconfig)" >&2
        return 1
    fi
    printf '%s\n' "$nuttx_dir/$rel"
}
