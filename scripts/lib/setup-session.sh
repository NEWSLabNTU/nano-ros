# shellcheck shell=bash
# A provisioning SESSION — phase-447 E2 / issue 1274 / RFC-0099 D7.
#
# A bootstrap is several `nros setup` processes on purpose (RFC-0099 D6: `just
# <platform> setup` stays the command a user would run for that platform), and
# each one used to compose its own system-package ask. One contained-runner
# bootstrap printed three overlapping `apt install` lines with different
# subsets, and acted on none of them.
#
# `nros setup` de-duplicates the ask through a LEDGER named by
# `NROS_SETUP_SESSION`: a key already asked for in this session is not asked
# again. This file is the ONE place a driver declares that session, so the
# `just setup` recipe and `scripts/ci/runner-provision.sh` cannot grow two
# spellings of it.
#
# Scope, and all of it: the ledger lives exactly as long as the outermost
# driver. A nested driver (runner-provision -> `just setup base`) joins the
# session it inherits rather than opening its own, and only the driver that
# opened it removes it. Unset, every `nros setup` asks its whole set, as before.

# nros_setup_session_begin <repo-root>
nros_setup_session_begin() {
    if [ -n "${NROS_SETUP_SESSION:-}" ]; then
        return 0
    fi
    local root="${1:?nros_setup_session_begin: repo root}"
    mkdir -p "$root/tmp" || return 0
    local ledger
    # Best-effort: without a ledger the only cost is a repeated ask.
    ledger="$(mktemp "$root/tmp/setup-session.XXXXXX")" || return 0
    NROS_SETUP_SESSION="$ledger"
    export NROS_SETUP_SESSION
    _NROS_SETUP_SESSION_OWNER=1
    trap nros_setup_session_end EXIT
}

# nros_setup_session_end — remove the ledger, if this shell opened it.
nros_setup_session_end() {
    if [ "${_NROS_SETUP_SESSION_OWNER:-0}" = 1 ] && [ -n "${NROS_SETUP_SESSION:-}" ]; then
        rm -f "$NROS_SETUP_SESSION"
    fi
}
