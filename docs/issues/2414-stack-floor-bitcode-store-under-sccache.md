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

**Home: [phase-557](../roadmap/phase-557-stack-floor-measured-frames.md)**, with
issue 2400. Rehomed 2026-11-15 from
[phase-548](../roadmap/archived/phase-548-three-languages-every-board.md) (archived). **Deferred by ruling:** build this only if someone measures real
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

## Measured again (2026-11-21): the cost is the walk, not the bypass

Re-measured on today's tree, each from a fresh target dir with a warm
sccache through the `just` wrapper (`scripts/bin/rustc-wrapper/sccache`):

| image | walk off, sccache | walk off, no sccache | walk on |
| --- | ---: | ---: | ---: |
| cargo road: `param-store-threadx-riscv64` (`rv-virt-threadx`) | 30.7 s | 31.5 s | 76 s |
| cmake road: `nros new` Rust talker, `nucleo-f767zi-zephyr` | 13.2 s | 16.4 s | 27.2 s |

The "no sccache" column is the most the sccache bypass can cost: about 1 s
on the cargo image and 3 s on the Zephyr one. The rest of the 2x is the
walk itself. A timestamped walk-on build of the cargo image:

| phase | time |
| --- | ---: |
| cargo, save-temps, sccache bypassed | 33.3 s |
| the walker loading its inputs, llc, binding | 3.1 s |
| the 8-order walk search (`WALK_ORDERS`) | **41.3 s** |

**So the bitcode store this issue proposes would recover 1-3 s of a 14-45 s
cost.** The cost is the walker's search: eight seeded callee orders over a
cyclic graph of thousands of functions, in Python, from scratch on every
build that relinks.

Two other things found while measuring:

- **sccache only runs under `just`.** The justfile exports `RUSTC_WRAPPER`;
  an activated shell leaves it empty. So a plain `nros build` never used
  sccache, and the bypass costs it nothing. The bypass is paid by
  `just`-driven builds (fixture lanes, CI), where the walk is on by default.
- **The off switch does not reach the cmake road.** `NROS_STACK_FLOOR` is
  read from the environment by the cargo road (`stack_floor.rs`, which
  accepts `ON`/`OFF`) but as a CMake cache variable by
  `cmake/NanoRosStackFloor.cmake` (`CHECK`/`OFF`). So
  `NROS_STACK_FLOOR=OFF nros build` still walks a Zephyr image, although
  its configure line says `NROS_STACK_FLOOR=OFF skips`, and `ON` is a
  configure error there.
