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

# Issue 0451 — resolve OUR OWN path in every shell that sources us, not just
# bash. activate.sh already does this three-way dance for itself; this file is
# sourced FROM it, so under zsh `BASH_SOURCE` is unset and `$0` is the
# interactive shell, which resolved the repo root to something arbitrary.
# The CONDITION tests $BASH_VERSION, not ${BASH_SOURCE[0]} — a plain POSIX
# sh rejects the array subscript as "Bad substitution" ON THE EXECUTED LINE
# even inside `[ -n … ]` (activate.sh's identical guard was the first thing
# dash died on). The untaken branch bodies are safe: a substitution only
# errors on the line that RUNS. Under plain sh, activate.sh has already
# verified $NROS_REPO_DIR before sourcing us, so prefer it over $0 (which is
# the SHELL when sourced, not this file).
if [ -n "${BASH_VERSION:-}" ]; then
    _nros_sdk_env_script="${BASH_SOURCE[0]}"
elif [ -n "${ZSH_VERSION:-}" ]; then
    _nros_sdk_env_script="${(%):-%N}"
elif [ -n "${NROS_REPO_DIR:-}" ]; then
    _nros_sdk_env_script="$NROS_REPO_DIR/scripts/sdk-env.sh"
else
    _nros_sdk_env_script="$0"
fi
_nros_sdk_env_root="$(cd "$(dirname "${_nros_sdk_env_script}")/.." && pwd)"

# Portable "is this variable set / what is its value", for bash AND zsh.
# `${!name}` is bash-only indirect expansion; zsh spells it `${(P)name}` and
# reports `bad substitution` for the bash form — which is how the whole SDK
# block silently did nothing under zsh. `eval` with `${VAR+x}` is POSIX and
# behaves identically in both.
_nros_sdk_env_is_set() { eval "[ -n \"\${$1+x}\" ]"; }
_nros_sdk_env_get() { eval "printf '%s' \"\${$1-}\""; }

# Issue 0451 — the variable list is DERIVED from the SSoT, never mirrored.
#
# This used to be a hand-written array of 14 names while `just/sdk-env.just`
# defined 23. The nine it omitted were exactly the first-party ones
# (`NROS_PLATFORM_*_SRC`, `NROS_C_INCLUDE`, `NROS_CPP_INCLUDE`,
# `NROS_LAN9118_LWIP_DIR`, `NROS_VIRTIO_NET_NETX_DIR`, `TBAND_DIR`), so an
# activated shell got every third-party SDK root and none of the paths a
# first-party build script reads — which is why 0451 reads as "activate.sh does
# not set these" even though it sets most of them, and why 13 leaf
# `.cargo/config.toml` files carry `[env]` blocks re-stating two of the nine.
#
# A list that must be kept in step by hand is the mirror-drift class CLAUDE.md
# names; the fix is to stop having a second list.
_nros_sdk_env_names() {
    sed -n 's/^export \([A-Za-z_][A-Za-z0-9_]*\)[[:space:]]*:=.*/\1/p' \
        "${_nros_sdk_env_root}/just/sdk-env.just"
}

# One `just --evaluate` for ALL variables instead of one per name. The old
# per-variable form re-parsed the whole justfile 14 times on every activation;
# `--evaluate` with no argument dumps every variable as `NAME := "value"`.
_nros_sdk_env_dump() {
    XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp}" \
        just --justfile "${_nros_sdk_env_root}/justfile" \
             --working-directory "${_nros_sdk_env_root}" \
             --evaluate 2>/dev/null
}

_nros_sdk_env_eval() {
    local var="$1"
    XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp}" \
        just --justfile "${_nros_sdk_env_root}/justfile" \
             --working-directory "${_nros_sdk_env_root}" \
             --evaluate "$var"
}

# Emit `NAME<TAB>VALUE` for every SSoT variable, taking an already-set value
# from the environment (an explicit override wins over the default, which is
# what `env(NAME, default)` means on the just side too).
_nros_sdk_env_pairs() {
    local names dump line name value
    names="|$(_nros_sdk_env_names | tr '\n' '|')"
    dump="$(_nros_sdk_env_dump)" || return $?
    while IFS= read -r line; do
        case "$line" in
            *':= "'*) ;;
            *) continue ;;
        esac
        name="${line%%[[:space:]]*}"
        case "$names" in
            *"|${name}|"*) ;;
            *) continue ;;
        esac
        if _nros_sdk_env_is_set "$name"; then
            value="$(_nros_sdk_env_get "$name")"
        else
            value="${line#*:= \"}"
            value="${value%\"}"
        fi
        printf '%s\t%s\n' "$name" "$value"
    done <<EOF
$dump
EOF
}

_nros_sdk_env_export_one() {
    local var="$1"
    local value
    if _nros_sdk_env_is_set "$var"; then
        return 0
    fi
    value="$(_nros_sdk_env_eval "$var")" || return $?
    export "$var=$value"
}

# Read `NAME<TAB>VALUE` pairs on stdin and apply them with $2 as the formatter.
_nros_sdk_env_emit() {
    local formatter="$1" name value
    while IFS="$(printf '\t')" read -r name value; do
        [ -n "$name" ] || continue
        "$formatter" "$name" "$value"
    done
}

_nros_sdk_env_do_export() { export "$1=$2"; }

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
    # issue 0373 F2 — the defaults live in `just/sdk-env.just` and are read by
    # evaluating them, so no `just` means no defaults. That is fine for the
    # book's native flow (nothing there reads FREERTOS_DIR & friends), and it
    # is NOT fine for an RTOS build, which fails much later with an unset-path
    # error. Say which of the two the reader is in, and name the remedy —
    # the bare "SDK defaults not loaded" left a first-time Arch user unable to
    # tell whether their activation had just half-failed.
    if ! command -v just >/dev/null 2>&1; then
        if [ -z "${NROS_QUIET_ACTIVATE:-}" ]; then
            echo "nano-ros sdk-env: \`just\` not found — RTOS SDK path defaults" \
                "(FREERTOS_DIR, NUTTX_DIR, THREADX_DIR, IDF_PATH, …) not loaded." >&2
            echo "  Harmless for the native/host flow. Needed for embedded builds" \
                "and every \`just\` recipe: cargo install just" >&2
        fi
        return 0
    fi
    # `<<<` would run the loop in a subshell in some shells; a here-doc into a
    # while loop keeps the `export`s in THIS shell, which is the whole point of
    # sourcing this file.
    local pairs
    pairs="$(_nros_sdk_env_pairs)" || return $?
    _nros_sdk_env_emit _nros_sdk_env_do_export <<EOF
$pairs
EOF
}

_nros_sdk_env_print_shell_one() {
    printf 'export %s=%s\n' "$1" "$(_nros_sdk_env_shell_quote "$2")"
}

_nros_sdk_env_print_fish_one() {
    printf 'set -gx %s %s\n' "$1" "$(_nros_sdk_env_fish_quote "$2")"
}

_nros_sdk_env_print_shell() {
    _nros_sdk_env_pairs | _nros_sdk_env_emit _nros_sdk_env_print_shell_one
}

_nros_sdk_env_print_fish() {
    _nros_sdk_env_pairs | _nros_sdk_env_emit _nros_sdk_env_print_fish_one
}

# Sourced or executed? The bash-only form below read TRUE under zsh (no
# BASH_SOURCE, so `$0` = `$0`), which sent a sourced zsh down the "print to
# stdout" branch — it emitted the exports as text and set nothing.
# (Same dash rule as the resolver above: guard on $BASH_VERSION so the
# array subscript never sits on an executed line in plain sh. A plain sh
# reaches the fallthrough: sourced-from-activate.sh is the only supported
# plain-sh path, so default to SOURCED there when activate.sh's root export
# is present.)
_nros_sdk_env_executed=1
if [ -n "${BASH_VERSION:-}" ]; then
    [ "${BASH_SOURCE[0]}" = "$0" ] || _nros_sdk_env_executed=0
elif [ -n "${ZSH_VERSION:-}" ]; then
    case "${ZSH_EVAL_CONTEXT:-}" in
        *:file*) _nros_sdk_env_executed=0 ;;
    esac
elif [ -n "${NROS_REPO_DIR:-}" ]; then
    _nros_sdk_env_executed=0
fi

if [ "$_nros_sdk_env_executed" = "1" ]; then
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
        _nros_sdk_env_apply _nros_sdk_env_print_shell _nros_sdk_env_names \
        _nros_sdk_env_dump _nros_sdk_env_pairs _nros_sdk_env_emit \
        _nros_sdk_env_do_export _nros_sdk_env_print_shell_one _nros_sdk_env_print_fish_one \
        _nros_sdk_env_is_set _nros_sdk_env_get \
        _nros_sdk_env_print_fish
    unset _nros_sdk_env_script _nros_sdk_env_root _nros_sdk_env_executed
fi
