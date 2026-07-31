#!/usr/bin/env bash
#
# Issue 0363 — fail FAST when the in-tree `nros` binary no longer matches its
# sources, instead of failing deep inside a lane that happens to use it.
#
# This script implements NOTHING. It asks the binary (`nros source-stamp`),
# which compares a stamp embedded at build time against one recomputed now.
# That is the whole point of the 0363 consolidation: the predicate lived in
# three places (a shell function, the binary, and this file), two of them real
# implementations that could disagree. Now there is one, in
# `nros-cli-core/src/source_stamp.rs`, shared by `build.rs` and the runtime.
#
# What this file contributes is POSITION. A stale CLI used to surface at
# `check-dep-chain`, minutes into `just check`, as nine failed cells whose
# printed cause was a cargo resolution error. Here it is the first thing that
# runs.

set -euo pipefail
cd "$(dirname "$0")/.."

BIN="packages/cli/target/release/nros"

# No in-tree binary is a normal state before `just setup-cli`, not a failure —
# gating on it would make `check-fast` unrunnable on a fresh clone.
if [ ! -x "$BIN" ]; then
    echo "cli-fresh: SKIP — no in-tree nros binary yet (run \`just setup-cli\`)."
    exit 0
fi

# A binary predating this mechanism has no `source-stamp` verb, so clap exits
# non-zero. That is the CORRECT answer rather than an error to work around: a
# binary built before the stamp existed is, by definition, built from older
# sources. Distinguish it only to print a useful message.
if ! out="$("$BIN" source-stamp 2>&1)"; then
    if grep -qiE "unrecognized subcommand|invalid value|unexpected argument" <<<"$out"; then
        echo "[ERROR] in-tree nros CLI predates the source-stamp check (issue 0363)." >&2
        echo "        It was built before this mechanism existed, so it is stale." >&2
        echo "        Rebuild it:  just setup-cli" >&2
        exit 1
    fi
    printf '%s\n' "$out" >&2
    echo "" >&2
    echo "       (issue 0363 — surfaced here rather than 20 minutes later in" >&2
    echo "        check-dep-chain, where the same cause reads as a cargo error.)" >&2
    exit 1
fi

echo "cli-fresh: OK — $out"
