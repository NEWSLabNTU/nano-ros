# shellcheck shell=bash
#
# The board inputs of `build-nuttx.sh`, resolved ONCE (issue 2915).
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
