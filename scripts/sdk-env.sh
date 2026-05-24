#!/usr/bin/env bash
# Source repo-local SDK defaults from the just SSoT.
#
# Usage:
#   source scripts/sdk-env.sh
#   eval "$(scripts/sdk-env.sh --shell)"
#   scripts/sdk-env.sh --print PX4_AUTOPILOT_DIR
#
# Defaults are defined in just/sdk-env.just. This adapter only evaluates
# those variables and exports them for shells that are not launched by just.

_nros_sdk_env_script="${BASH_SOURCE[0]:-$0}"
_nros_sdk_env_root="$(cd "$(dirname "${_nros_sdk_env_script}")/.." && pwd)"

_nros_sdk_env_vars=(
    FREERTOS_DIR
    FREERTOS_PORT
    LWIP_DIR
    FREERTOS_CONFIG_DIR
    NUTTX_DIR
    NUTTX_APPS_DIR
    THREADX_DIR
    THREADX_CONFIG_DIR
    NETX_DIR
    NETX_CONFIG_DIR
    PX4_AUTOPILOT_DIR
    NROS_ESP_IDF_WORKSPACE
    NROS_ESP_IDF_ENV_SHIM
    IDF_PATH
)

_nros_sdk_env_eval() {
    local var="$1"
    XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp}" \
        just --justfile "${_nros_sdk_env_root}/justfile" \
             --working-directory "${_nros_sdk_env_root}" \
             --evaluate "$var"
}

_nros_sdk_env_export_one() {
    local var="$1"
    local value
    if [ -n "${!var+x}" ]; then
        return 0
    fi
    value="$(_nros_sdk_env_eval "$var")" || return $?
    export "$var=$value"
}

_nros_sdk_env_shell_quote() {
    printf "%q" "$1"
}

_nros_sdk_env_fish_quote() {
    local value="$1"
    value="${value//\\/\\\\}"
    value="${value//\'/\\\'}"
    printf "'%s'" "$value"
}

_nros_sdk_env_apply() {
    local var
    if ! command -v just >/dev/null 2>&1; then
        echo "nano-ros sdk-env: just not found; SDK defaults not loaded" >&2
        return 0
    fi
    for var in "${_nros_sdk_env_vars[@]}"; do
        _nros_sdk_env_export_one "$var" || return $?
    done
}

_nros_sdk_env_print_shell() {
    local var value
    for var in "${_nros_sdk_env_vars[@]}"; do
        if [ -n "${!var+x}" ]; then
            value="${!var}"
        else
            value="$(_nros_sdk_env_eval "$var")" || return $?
        fi
        printf 'export %s=%s\n' "$var" "$(_nros_sdk_env_shell_quote "$value")"
    done
}

_nros_sdk_env_print_fish() {
    local var value
    for var in "${_nros_sdk_env_vars[@]}"; do
        if [ -n "${!var+x}" ]; then
            value="${!var}"
        else
            value="$(_nros_sdk_env_eval "$var")" || return $?
        fi
        printf 'set -gx %s %s\n' "$var" "$(_nros_sdk_env_fish_quote "$value")"
    done
}

if [ "${BASH_SOURCE[0]:-$0}" = "$0" ]; then
    case "${1:---shell}" in
        --shell)
            _nros_sdk_env_print_shell
            ;;
        --fish)
            _nros_sdk_env_print_fish
            ;;
        --print)
            if [ -z "${2:-}" ]; then
                echo "usage: scripts/sdk-env.sh --print VAR" >&2
                exit 2
            fi
            _nros_sdk_env_eval "$2"
            ;;
        *)
            echo "usage: source scripts/sdk-env.sh | scripts/sdk-env.sh [--shell|--fish|--print VAR]" >&2
            exit 2
            ;;
    esac
else
    _nros_sdk_env_apply
    unset -f _nros_sdk_env_eval _nros_sdk_env_export_one \
        _nros_sdk_env_shell_quote _nros_sdk_env_fish_quote \
        _nros_sdk_env_apply _nros_sdk_env_print_shell \
        _nros_sdk_env_print_fish
    unset _nros_sdk_env_script _nros_sdk_env_root _nros_sdk_env_vars
fi
