#!/usr/bin/env bash
# The ONE resolution of "which launch-resolution toolchain produced this?" —
# issue 1454.
#
# `nros sync` turns a bringup's launch tree into a SystemModel by spawning
# `nros-launch-resolve`, which STATICALLY LINKS the `play_launch_parser` crate
# from the `packages/cli/third-party/play_launch` submodule. That resolver is a
# build input of every fixture whose staging runs `nros sync` — and nothing
# recorded it, so a parser bump left those fixtures reading fresh while they
# held codegen evidence from the previous one (issue 1454's museum case).
#
# MEASURED, 2026-09-24, before choosing this identity: a full
# `nav2_compat_smoke` fixture build traced with `strace -f -e trace=execve`
# executes `nros-launch-resolve` twice and `nros` four times across 29,828
# `execve` calls, and the `play_launch_parser` BINARY in the SDK store
# (`~/.nros/sdk/play_launch_parser/bin/`) **zero** times. The store binary is a
# standalone CLI nothing in the build path spawns; the submodule half is what
# parses. So the identity below names the resolver, not the store tool — an
# edge on the store path would have watched a file no build reads.
#
# WHAT the identity is: the `play_launch` commit the resolver compiled in
# (`NROS_PLAY_LAUNCH_SHA`, reported by `--version`). Three reasons it is that
# and not a binary hash:
#
#   1. it is the SAME FACT `nros sync` already stamps into every model it
#      resolves (`meta.resolver.version`, issue 0427) and that
#      `model_gate::provenance_stale` already treats as staleness one layer
#      down — so this carries an existing fact to a new consumer rather than
#      minting a second one;
#   2. it is EMITTED rather than hashed, so it moves iff the parser moves —
#      the same reasoning `codegen-fingerprint.sh` records for `tool:nros`
#      (93 % of CLI rebuilds emit byte-identical code);
#   3. it is CONTENT, not a path. An edge recorded against a store PATH is
#      issue 0491's trap and, worse, issue 1454's fix-candidate 1: the SDK
#      store accumulates (issue 0500), so a bump changes the path rather than
#      the file, and an edge on the old path names something that still exists
#      and never fires.
#
# The ladder mirrors `nros_codegen_fingerprint`'s and must not be reordered:
#
#   1. `--version`'s `play_launch <sha>` field — the emitted identity;
#   2. the binary HASH, for a resolver predating that field — an
#      over-approximation, wrong only in the expensive direction;
#   3. never "assume unchanged". No resolver ⇒ exit 1 with no output, so each
#      caller spells its own stable absent-marker.

# nros_launch_resolver_identity <repo_root>
#
# stdout: the identity (no trailing newline). exit 0.
# exit 1, no output: there is no resolver to ask.
nros_launch_resolver_identity() {
    local root="${1:?usage: nros_launch_resolver_identity <repo_root>}"
    local bin=""

    # Resolution mirrors `launch_resolver_path()` in `nros-cli-core` — with the
    # same deliberate omission of `$PATH` (issue 0285: an unrelated ROS 2
    # `play_launch` won that race). Explicit override, then the per-checkout
    # build, then the installed layout beside `nros`.
    if [ -n "${NROS_LAUNCH_RESOLVE:-}" ] && [ -x "${NROS_LAUNCH_RESOLVE}" ]; then
        bin="$NROS_LAUNCH_RESOLVE"
    else
        local crate="$root/packages/cli/nros-launch-resolve"
        # profile-literal-ok: host tool: the launch resolver's own binary
        local built="${CARGO_TARGET_DIR:-$crate/target}/release/nros-launch-resolve"
        if [ -x "$built" ]; then
            bin="$built"
        # profile-literal-ok: host tool: the installed layout puts both side by side
        elif [ -x "$root/packages/cli/target/release/nros-launch-resolve" ]; then
            # profile-literal-ok: host tool: same path, taken
            bin="$root/packages/cli/target/release/nros-launch-resolve"
        fi
    fi
    [ -n "$bin" ] || return 1

    local ver sha
    if ver="$("$bin" --version 2>/dev/null)" && [ -n "$ver" ]; then
        # `nros-launch-resolve 0.5.0 (play_launch <sha>)`
        sha="${ver##*play_launch }"
        sha="${sha%%)*}"
        sha="${sha%%[[:space:]]*}"
        # `unknown` is what the build stamps with no git — a real answer
        # ("cannot verify"), distinct from "no resolver", so pass it through
        # rather than falling to the hash: two hosts that both cannot verify
        # must agree, or every such pair reads as a bump.
        if [ -n "$sha" ] && [ "$sha" != "$ver" ]; then
            printf '%s' "$sha"
            return 0
        fi
    fi
    printf 'binary:%s' "$(sha256sum "$bin" | awk '{print $1}')"
}

# Runnable as well as sourceable, so the Rust fixture resolver
# (`nros_tests::fixtures`) can ask the same question without a second
# implementation of the ladder — CLAUDE.md's "ONE shared helper rather than a
# second spelling".
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
    _lri_root="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
    if nros_launch_resolver_identity "$_lri_root"; then
        echo
        exit 0
    fi
    exit 1
fi
