---
id: 2414
title: "A walked build takes about 2x a warm build, and the cost is the 8-order walk search, not the sccache bypass: the search should run its orders in parallel and skip a graph it has already walked"
status: open
type: enhancement
severity: low
area: [build, tooling, memory]
found: 2026-11-10
related: [issue-2400, issue-2803, phase-548, phase-557]
---

**Home: [phase-557](../roadmap/phase-557-stack-floor-measured-frames.md)**, with
issue 2400. Rehomed 2026-11-15 from
[phase-548](../roadmap/archived/phase-548-three-languages-every-board.md) (archived). Re-scoped 2026-11-21 around the measured cost (below). The
bitcode store it first proposed is DROPPED.

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

## The cost, measured (2026-11-21): the walk, not the bypass

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
- **The off switch does not reach the cmake road**: filed as
  [issue 2803](2803-stack-floor-off-switch-not-on-cmake-road.md).

## Fix (approved 2026-11-21)

1. **Run the orders in parallel**, drawing on the shared jobserver token
   pool (`scripts/build/jobserver-pool.sh`), never as raw processes: fixture
   builds run walks concurrently, and eight per walk would oversubscribe the
   host. The orders are independent and the floor is their max, so the
   answer cannot change. That is checked on the drift rows, against the
   serial run.
2. **Skip a graph already walked**: a walk-result cache, outside `target/`,
   keyed by every input that can move the answer:
   - the edges and frames;
   - the table, and the walk's flags;
   - the walker's AND the classifier's source;
   - the toolchain identity (the drift check's);
   - the order count.

   A stale entry would state a WRONG floor, the unsafe direction. So a
   negative control (one frame changed) must miss. The cache is bounded and
   pruned.
3. The bitcode store is dropped: it would recover the bypass's 1-3 s.

## Acceptance

- A walked warm build costs the walk-off build plus a few seconds (1), and
  about nothing on an unchanged graph (2).
- Floors identical to the serial walk's, on every drift row.
