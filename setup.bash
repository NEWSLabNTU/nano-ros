#!/usr/bin/env bash
# nano-ros activation file (bash / zsh).
#
# Source this file once per shell session to put every shipped
# nano-ros binary on PATH:
#
#   source ./setup.bash
#   zenohd --listen tcp/127.0.0.1:7447 &
#   nros --help
#
# Idempotent — re-sourcing rebuilds PATH entries without duplicating.
# Skips dirs that don't exist (e.g. before `just setup`); re-source
# after the build to pick them up.

# Resolve the script's directory regardless of how it was sourced.
# Works under bash + zsh.
if [ -n "${BASH_SOURCE[0]:-}" ]; then
    _nros_script="${BASH_SOURCE[0]}"
elif [ -n "${(%):-%x}" ]; then
    # zsh
    _nros_script="${(%):-%x}"
else
    echo "nano-ros setup.bash: cannot resolve script path. Source from bash or zsh." >&2
    return 1 2>/dev/null || exit 1
fi

NROS_ROOT="$(cd "$(dirname "${_nros_script}")" && pwd)"
export NROS_ROOT
unset _nros_script

# Export repo-local SDK defaults from the just/sdk-env.just SSoT.
# Existing caller-provided variables are preserved.
if [ -f "${NROS_ROOT}/scripts/sdk-env.sh" ]; then
    # shellcheck source=/dev/null
    source "${NROS_ROOT}/scripts/sdk-env.sh"
fi

# Binary directories shipped by nano-ros builds. Each may or may not
# exist on a given clone depending on which `just <module> setup`
# recipes have run.
_nros_bin_dirs=(
    "${HOME}/.local/bin"                                               # pipx / pip --user tools (pio, west, etc.)
    "${NROS_ROOT}/build/zenohd"                                          # zenohd
    "${NROS_ROOT}/build/qemu/bin"                                        # patched qemu-system-arm + qemu-ga
    "${NROS_ROOT}/build/xrce-agent"                                      # MicroXRCEAgent
    "${NROS_HOME:-${HOME}/.nros}/bin"                                    # nros (prebuilt release; Phase 195.D)
)

# Phase 197.4 — nros-store tool bins (`~/.nros/sdk/<tool>/<ver>/bin`): the
# toolchains/tools `nros setup <board>` provisions (arm-none-eabi-gcc, qemu,
# riscv-none-elf-gcc, …), so a build uses the pinned index versions rather than
# whatever apt happens to ship. Glob-expanded — 0+ entries depending on what has
# been provisioned. Appended AFTER the build/* dirs so a locally-built patched
# tool (e.g. build/qemu) still wins when present.
_nros_sdk_store="${NROS_HOME:-${HOME}/.nros}/sdk"
for _nros_d in "${_nros_sdk_store}"/*/*/bin; do
    [ -d "${_nros_d}" ] && _nros_bin_dirs+=("${_nros_d}")
done
unset _nros_d

# Strip any previous nano-ros entries from PATH before re-adding, so
# repeated sourcing doesn't grow PATH unboundedly.
_nros_strip_path() {
    local IFS=':' p clean=()
    for p in $PATH; do
        case "$p" in
            "${HOME}/.local/bin"|"${NROS_ROOT}/build/"*|"${NROS_ROOT}/packages/codegen/"*) ;;
            "${NROS_HOME:-${HOME}/.nros}/bin"|"${_nros_sdk_store}/"*) ;;
            *) clean+=("$p") ;;
        esac
    done
    PATH=$(IFS=':'; echo "${clean[*]}")
}
_nros_strip_path
unset -f _nros_strip_path

# Prepend each existing dir to PATH (in reverse so the first list
# entry ends up frontmost).
_nros_added=()
for ((i=${#_nros_bin_dirs[@]}-1; i>=0; i--)); do
    d="${_nros_bin_dirs[i]}"
    if [ -d "$d" ]; then
        PATH="$d:$PATH"
        _nros_added+=("$d")
    fi
done
export PATH

# Convenience env vars pointing at the canonical binaries (when
# present). Downstream scripts can prefer `$NROS_ZENOHD` over a bare
# `zenohd` lookup if they need the absolute path.
_nros_set_if_exists() {
    local var=$1 path=$2
    if [ -x "$path" ]; then
        export "$var"="$path"
    fi
}

_nros_set_if_exists NROS_ZENOHD             "${NROS_ROOT}/build/zenohd/zenohd"
_nros_set_if_exists NROS_QEMU_SYSTEM_ARM    "${NROS_ROOT}/build/qemu/bin/qemu-system-arm"
_nros_set_if_exists NROS_XRCE_AGENT         "${NROS_ROOT}/build/xrce-agent/MicroXRCEAgent"
_nros_set_if_exists NROS_CLI                "${NROS_HOME:-${HOME}/.nros}/bin/nros"
_nros_set_if_exists NROS_CODEGEN            "${NROS_HOME:-${HOME}/.nros}/bin/nros"

unset -f _nros_set_if_exists

# Confirmation banner. Print only when sourced interactively.
if [ -t 1 ] && [ "${NROS_QUIET_SETUP:-}" != "1" ]; then
    echo "[nano-ros] NROS_ROOT=${NROS_ROOT}"
    if [ ${#_nros_added[@]} -eq 0 ]; then
        echo "[nano-ros] No shipped binaries on PATH yet — run 'just setup' first."
    else
        echo "[nano-ros] Binaries on PATH:"
        for d in "${_nros_added[@]}"; do
            echo "[nano-ros]   $d"
        done
    fi
fi
unset _nros_added _nros_bin_dirs d i
