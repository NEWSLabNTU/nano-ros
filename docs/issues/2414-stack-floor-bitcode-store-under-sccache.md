---
id: 2414
title: "Measuring stack floors every build bypasses sccache for the measured crates (about 2x warm build), because a sccache hit restores the rlib but not the save-temps bitcode; a bitcode store keyed by crate output would remove the cost"
status: open
type: enhancement
severity: low
area: [build, tooling, memory]
found: 2026-11-10
related: [issue-2400, phase-548, phase-557]
---

**Home: [phase-548](../roadmap/phase-548-three-languages-every-board.md)**, with
issue 2400. **Deferred by ruling:** build this only if someone measures real
pain from the bypass.

## What

Issue 2400 measures stack floors in every build from `-C save-temps` bitcode.
sccache caches those crates as usual, but a cache hit restores the `.rlib` and
`.o`, not the save-temps files. Measured (sccache 0.15.0, nros-cpp thumbv7em
release): the second fresh build got 74 Rust hits, and only 11 of 27 crates
still had their `.rcgu.bc`.

The rlib's embedded `.llvmbc`, which sccache does restore, is not a
substitute. It is the LTO pre-link module, and 174 of 1,977 frames differ
from the shipped object (122 lower). On nros-cpp, SPIN reads 3,684 B against
3,692 B.

So issue 2400 bypasses sccache for the measured crates when measuring. Tools
and build scripts stay cached.

| fresh target dir, warm sccache | wall |
| --- | ---: |
| plain, sccache | 5.4 s |
| save-temps, sccache (bitcode lost on hits) | 5.3 s |
| save-temps, no sccache for the measured crates (exact) | 11.2 s |

## Proposal (deferred)

A bitcode store keyed by each crate's output hash (the rlib's sha256):

- Whenever save-temps produced `.bc` (a miss), the walker files it under that
  key.
- When a hit leaves the `.bc` missing, the walker takes it from the store.
- A crate with no store entry is recompiled once with sccache off.

Steady state, this is the warm-sccache cost plus llc on the changed crates.

## Acceptance

- A warm-sccache build that measures is within noise of 5.4 s on the table
  above.
- Its floors are identical to the bypass build's.
