---
id: 966
title: "`zpico-sys` recompiled on EVERY cargo invocation — a `rerun-if-changed` on a path that does not exist"
status: resolved
type: bug
area: build, rmw
related: [issue-0490, issue-0911, issue-0899]
---

## What happened

Every `cargo build` in a zenoh-enabled tree recompiled `zpico-sys` and
`nros-rmw-zenoh`, warm or not. Not slow, not failing — just never fresh.

`CARGO_LOG=cargo::core::compiler::fingerprint=info` names it in one line:

    stale: missing ".../packages/platform/bare-metal/nros-platform.toml"

`nros-zpico-build` emitted the CROSS PRODUCT of (platform search roots) x
(platform names):

    for root in &platform_search_path {
        for name in tree.names() {
            println!("cargo:rerun-if-changed={}", root.join(name).join(FILENAME));
        }
    }

A platform's manifest lives under exactly ONE root. With both
`packages/platform` and `config` on the search path (phase-400 W1 made it a path
rather than a single directory), `bare-metal` — which lives in `config/` — also
produced `packages/platform/bare-metal/nros-platform.toml`, which does not
exist. Cargo treats a missing `rerun-if-changed` input as permanently dirty, so
that single path invalidated the crate and everything above it, forever.

## Same class as issue 0490

0490 was the same failure one crate over: a `rerun-if-changed` pointing at
`../nros-rmw-abi` after the crate moved, so the path named a directory that did
not exist and `nros-rmw-cffi` rebuilt on every invocation. CLAUDE.md already
records the rule — *"cargo treats a MISSING `rerun-if-changed` input as
permanently dirty (`StaleItem(MissingFile)`) … Silent: the build always
succeeded, it was just never fresh"*.

What is new here is the SHAPE. 0490 was one stale hand-written path. This one is
generated, and it became wrong the moment the search path grew a second root:
the loop was correct when there was one root and silently wrong afterwards.

## Fix

Emit only the manifests that exist. One `is_file()` guard.

The trade, stated in the code: a manifest CREATED later under a higher-priority
root does not by itself trigger a rebuild. Cargo cannot watch a nonexistent path
without being permanently dirty, so watching what exists is the only stable
option, and authoring a new platform descriptor is a deliberate act that arrives
with other edits. `PlatformsTree` merges by name and does not record which root
each came from; if it ever does, watch exactly those instead and the trade
disappears.

## Measured

    before:  no-op `cargo build` -> 2 crates recompiled, every time
    after:   no-op `cargo build` -> 0

and the rebuild edge still works: touching
`zenoh-pico/src/protocol/iobuf.c` recompiles, and the following no-op is clean
again.

## How it was found

Looking for [[issue-0911]] (the opposite defect — editing zenoh-pico rebuilt
NOTHING) and discovering its fix had never reached main. Porting that fix made
the always-dirty behaviour visible, because "does a no-op rebuild recompile
anything" is 0911's own first acceptance criterion and it failed for a reason
that had nothing to do with 0911.

Worth recording: my first measurement blamed the port. The control run — the
same build with my change stashed — also rebuilt, which looked like proof the
problem was pre-existing. It was, but the control was contaminated: `git stash`
rewrites the file's mtime, so both arms were dirty for a reason I had introduced
by measuring. Only a straight build-twice-with-no-git-in-between separated them.

## Acceptance

* ~~A no-op build recompiles nothing.~~ Met.
* ~~An edit under the compiled zenoh-pico set still rebuilds.~~ Met.
