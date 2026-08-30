#!/usr/bin/env bash
#
# Is the in-tree `nros` binary one a gate may TRUST?
#
# Source this and call `nros_cli_usable`; it answers for both ways the answer
# can be no.
#
# WHY THIS EXISTS. Several `check-fast` gates shell out to the in-tree CLI —
# `check-provider-index`, `check-workspace-order`,
# `check-board-cargo-config-applied`. Each already treated an ABSENT binary as
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
    local bin="${1:-packages/cli/target/release/nros}"
    nros_cli_unusable_reason=""

    if [ ! -x "$bin" ]; then
        nros_cli_unusable_reason="no in-tree nros at $bin (just setup-cli)"
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
