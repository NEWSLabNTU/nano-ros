#!/usr/bin/env bash
# Issue 1844 — the board facts (`FREERTOS_PORT`, `FREERTOS_CONFIG_DIR`,
# `THREADX_PORT`, …) for a cargo build that a SHELL lane spawns itself.
#
# A leaf that states its board in `system.toml` gets them from its generated
# `build/<image>/nros-cargo.toml` (scripts/build/leaf-settings.sh) and needs
# nothing here. Every other cargo dir a lane builds — a workspace-shaped
# fixture, a test bin, a bench — has no such file, so the invoker must ask
# `nros ws board-facts` and export the answer, the way the cmake lanes do
# through `nros_resolve_board_facts` (cmake/NanoRosBoardFacts.cmake).
#
# Since phase-484 W2b nothing exports `FREERTOS_PORT` repo-wide, so a lane
# that asks nothing builds a FreeRTOS image with no port and `zpico-sys`
# panics — but only in a clean environment. A shell still carrying an older
# `activate.sh` export masks it, which is how the compile-check cross-build
# lane shipped broken.
#
# One spelling for every shell lane, so the "how" cannot drift between them.
#
# nros_board_facts <dir> <required|optional>
#   Prints the `KEY=VALUE` lines on stdout.
#   required — the lane cross-compiles for a board, so a refusal is FATAL:
#              the verb's own message goes to stderr and the status is 1.
#   optional — a host build may legitimately resolve no board; a refusal
#              prints nothing and returns 0. Quiet on purpose: measured over
#              `fixtures-build.sh`'s rows with no settings file, all 49
#              refusals are host, bare-metal or a NuttX bin whose board crate
#              two descriptors share, and no FreeRTOS or ThreadX row is among
#              them (every one of those resolves) — so a note per row
#              would be noise around the one line that matters.
# Needs `$NROS_CLI` (or `$NROS_CLI_BIN`) and `$NROS_REPO_ROOT` (or
# `$repo_root`). Defined as a plain function so `export -f` can carry it into
# a make leaf.
nros_board_facts() {
    local dir="$1" mode="${2:-required}"
    local cli="${NROS_CLI:-${NROS_CLI_BIN:-}}"
    local root="${NROS_REPO_ROOT:-${repo_root:-$PWD}}"
    local out rc=0
    case "$mode" in
        required | optional) ;;
        *)
            printf 'nros_board_facts: mode must be required|optional, got "%s"\n' "$mode" >&2
            return 2
            ;;
    esac
    if [ -z "$cli" ] || [ ! -x "$cli" ]; then
        if [ "$mode" = required ]; then
            printf 'nros_board_facts: no nros CLI (NROS_CLI unset) to resolve board facts for %s — just setup-cli\n' \
                "$dir" >&2
            return 1
        fi
        return 0
    fi
    # stdout is the answer and stderr the reason; kept apart so a warning on
    # a successful resolve can never become an exported "variable".
    local errf
    errf="$(mktemp)"
    out="$(NROS_REPO_DIR="$root" "$cli" ws board-facts "$dir" 2>"$errf")" || rc=$?
    if [ "$rc" -ne 0 ]; then
        if [ "$mode" = required ]; then
            printf 'nros_board_facts: "nros ws board-facts %s" refused, and this build cross-compiles for a board:\n' \
                "$dir" >&2
            cat "$errf" >&2
            rm -f "$errf"
            return 1
        fi
        rm -f "$errf"
        return 0
    fi
    rm -f "$errf"
    printf '%s\n' "$out"
}
