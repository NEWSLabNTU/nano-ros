#!/usr/bin/env bash
#
# Is the in-tree `nros` binary one a gate may TRUST?
#
# Source this and call `nros_cli_usable`; it answers for both ways the answer
# can be no.
#
# WHY THIS EXISTS. Several `check-fast` gates shell out to the in-tree CLI —
# `check-provider-index`, `check-workspace-order`,
# `check-board-cargo-config-applied` (retired, phase-445 W6). Each already treated an ABSENT binary as
# a skip, correctly: a fresh clone has none, and `check-fast` must stay green on
# a bare worktree.
#
# None of them treated a STALE binary the same way, and that asymmetry has no
# defence. `check-fast` is contractually the source-free, CLI-free tier, so a
# gate there does not OWN the CLI and cannot demand one; and running a gate
# against a binary built from other sources is worse than not running it, because
# the verdict is about a program that no longer exists in the tree.
#
# What it cost, repeatedly, in one session: switching branches restales the
# stamp, and the next `just check fast` reported THREE unrelated red gates whose
# printed cause was `in-tree nros CLI is STALE`. Three failures naming one
# remedy, in a lane whose contract says it does not need the thing. The
# information a person needs — "run `just setup-cli`" — was already on screen
# three times and still read as three defects.
#
# It asks the BINARY (`nros source-stamp`), which is the single authority
# established by issue 0363: the predicate used to live in three places, two of
# them real implementations that could disagree. `check-cli-fresh` asks the same
# question the same way. This adds no fourth copy — only the pairing of
# "absent" with "stale" under one verdict, and one remedy string so the three
# call sites cannot drift into three phrasings.

# nros_cli_usable [<path-to-nros>]
#
# 0  = present and built from these sources.
# 1  = absent, stale, or predating the stamp. `nros_cli_unusable_reason` then
#      holds a one-line reason ending in the remedy.
#
# Deliberately NOT an exit or a skip: the caller decides, exactly as
# `nros_check_skip` does. A gate that skips a whole recipe and one that skips a
# single step need different things from the same answer.
nros_cli_usable() {
    local bin="${1:-}"
    nros_cli_unusable_reason=""
    if [ -z "$bin" ]; then
        # issue 1850 — the in-tree CLI through its one resolver.
        if ! declare -F nros_cli_bin >/dev/null; then
            # shellcheck source=scripts/build/cargo.sh
            source "$(dirname "${BASH_SOURCE[0]}")/cargo.sh"
        fi
        bin="$(nros_cli_bin --checkout-only)" || bin=""
    fi

    if [ -z "$bin" ] || [ ! -x "$bin" ]; then
        nros_cli_unusable_reason="no in-tree nros${bin:+ at $bin} (just setup-cli)"
        return 1
    fi

    local out rc
    out="$("$bin" source-stamp 2>&1)"
    rc=$?
    if [ "$rc" -eq 0 ]; then
        return 0
    fi

    # A binary predating the stamp verb exits non-zero from clap. That is the
    # CORRECT answer and not an error to work around — a binary built before
    # the mechanism existed is by definition built from older sources — but say
    # which it is, because the remedy differs in urgency, not in kind.
    case "$out" in
        *"unrecognized subcommand"*|*"unexpected argument"*)
            nros_cli_unusable_reason="in-tree nros predates \`source-stamp\`, so it is older than these sources (just setup-cli)"
            ;;
        *)
            nros_cli_unusable_reason="in-tree nros is STALE — built from other sources (just setup-cli)"
            ;;
    esac
    return 1
}

# Issue 1926 — the CLI's SELF-REFUSALS, the text `stale_guard.rs` prints before
# it dispatches any command. Output carrying one says nothing about the command
# that was asked for: the binary declined to run it. A verdict builder records a
# tool's output as an artifact, so it must never record one of these as a
# verdict — the refusal is about the environment at the moment of the build,
# and a later `just setup-cli` changes nothing the row's signature watches, so
# the recorded refusal would be reused forever (issue 0445's absorbing shape).
#
# ONE list, matched with whitespace collapsed (cmake re-indents a FATAL_ERROR
# message, cargo prefixes a build-script line). `check-verdict-cli-refusal`
# requires every entry to occur verbatim in `stale_guard.rs`, so rewording the
# refusal there turns that gate red instead of silently disarming this.
NROS_CLI_REFUSAL_MARKERS=(
    "in-tree nros CLI is STALE"
    "does not belong to the checkout it is being run against"
)

# nros_cli_refusal_in <file>...
#
# 0 = some file carries a CLI self-refusal; the matched marker is printed.
# 1 = none does (missing files count as none).
nros_cli_refusal_in() {
    local f flat m
    for f in "$@"; do
        [ -f "$f" ] || continue
        flat="$(tr -s ' \t\r\n' '    ' < "$f")"
        for m in "${NROS_CLI_REFUSAL_MARKERS[@]}"; do
            case "$flat" in
                *"$m"*) printf '%s\n' "$m"; return 0 ;;
            esac
        done
    done
    return 1
}
