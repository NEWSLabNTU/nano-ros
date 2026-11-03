---
id: 2235
title: "Every C/C++ leaf's census host build compiles the whole nano-ros runtime again, in its own build dir"
status: open
type: tech-debt
area: [build, census, cmake]
found: 2026-11-03
related: [phase-530, issue-1556, issue-2086]
---

## Symptom

RFC-0120 D4 (phase-530 W2) makes the entity census the default for every
C/C++ leaf built for a board: before its cross configure, the leaf's own
sources are configured and built for the HOST (`nros dev image entity-census
take --leaf`, into `<leaf>/build/census-host`) and run once in counting mode.

Each of those host builds compiles the whole host runtime from scratch: the
cargo crates behind `libnros_c.a` / `libnros_cpp.a` (zenoh-pico, the RMW
backend, nros-core and friends), the generated message packages, and then the
leaf's own one or two source files. The leaf's sources are a few seconds; the
runtime is nearly all of it.

Measured on 2026-11-03 (one take per leaf, cold `census-host`, a shared and
loaded 32-core host):

| leaf | host census build |
| --- | --- |
| examples/zephyr/cpp/talker | 165 s |
| examples/zephyr/c/talker | 152 s |
| examples/mps2-an385-freertos/cpp/listener | 149 s |
| examples/rv-virt-nuttx/c/talker | 114 s |
| examples/threadx-linux/cpp/talker | 71 s |
| examples/rv-virt-threadx/c/talker | 734 s (host heavily loaded) |

About 75 in-tree leaves take a census now, so a cold fixture pre-pass pays
that runtime build about 75 times.

It costs DISK too: one `census-host` dir measured **1.8 GB** (three
threadx-linux C leaves, each 1.8 GB; nearly all of it the cargo target dir).
About 75 of them is about 135 GB per checkout. On 2026-11-03 a pre-pass
measurement on the shared build host ran `/home` to 100 % (`No space left on
device` in nine takes), and the write that failed truncated a cargo
registry-index cache entry (`~/.cargo/registry/index/*/.cache/rs/te/rstest`,
40960 bytes, no trailing NUL). Cargo then resolved `rstest` from the
truncated entry and refused `rstest = "^0.24"` for every build on the host
until the entry was deleted. phase-530 W2 (Ruling R30) runs the takes
side by side under the fixture build's cmake frontend budget, which bounds the
wall time but not the work.

## Why it is shaped this way

Each `census-host` is its own cmake build dir with its own Corrosion cargo
target dir, because a cargo `--target-dir` serves exactly ONE workspace root
(issue 0616) and every leaf is its own root.

## What would fix it

A host runtime built once and shared by every census host build: the host
census configure reads a prebuilt runtime (its `libnros_c.a` / `libnros_cpp.a`
and headers) instead of building one, or all census host builds share one
store keyed so that issue 0616's one-root rule holds. The census is
sizing-insensitive (`NROS_LEAF_CENSUS_HOST`: crate defaults, no seeds), so
every census host build of a given RMW needs the SAME runtime, which is what
makes sharing it sound.

Not done in phase-530 W2 by ruling (R30); this records the cost it measured.
