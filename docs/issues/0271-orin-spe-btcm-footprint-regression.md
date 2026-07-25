---
id: 271
title: "Orin SPE BTCM footprint regressed ~+195 KB between d9af52be and 21a3a4248 — minimal Executor::open+spin image no longer fits 256 KB"
status: open
type: regression
severity: high
area: embedded
related: [issue-0257]
---

## Finding (autoware_sentinel phase-14 pin bump, 2026-07-25)

On pin `d9af52be` (post-11.3.C size campaign) the sentinel's DEFAULT SPE
image (`Executor::open` + `spin` over zenoh-pico/IVC, no algorithm
wiring) fit the 256 KB BTCM with **31 KB headroom** (~224 KB
text+data+bss). On `21a3a4248`, the same build — same slot rightsizing
envs (`ZPICO_MAX_PUBLISHERS=8 / SUBSCRIBERS=4 / QUERYABLES=2 /
LIVELINESS=16`, `NROS_EXECUTOR_MAX_CBS=8`, 256 B buffers), same
`.cargo` hardening (armv7r-none-eabi softfp, build-std core+alloc,
`panic=immediate-abort`, `-Os` + LTO) — **overflows BTCM by 164 KB**
(ld names `.data`): a ~+195 KB swing.

Staticlib pre-gc totals for scale: text 464 KB / bss 158 KB;
`compiler_builtins` alone contributes 118 KB text pre-gc.

Suspected contributors (unverified): the rmw-cffi vtable seam now on the
default path, the node-registry/wake/monitor machinery (phase 271/273 /
RFC-0052 tables), and zenoh-pico feature growth (interest/matching).
Needs an 11.3.C-style per-component size audit on the new pin; the
sentinel's fallback is the 11.3.E DRAM/AST mapping, but a 2× regression
of the minimal image hurts every 256 KB-class target, not just the SPE.

## Repro

autoware_sentinel branch `phase-14`:

```sh
just build-spe-image
# → ld: region `btcm' overflowed by 164464 bytes
```

## Side notes from the same bump (consumer-side, already absorbed)

- The retired `nros-platform-orin-spe` crate used to compile the
  platform C port; `nros-platform-freertos` is now a source-only C
  package with no compiler at the SPE site — the sentinel added a
  build.rs compiling `platform.c`/`timer.c` against the FSP headers
  (with `configTOTAL_HEAP_SIZE` undefined there — see the define
  fallback in that build.rs).
- zpico.c's session-seed path (`#elif defined(CLOCK_REALTIME)`) calls
  `clock_gettime`, which newlib does not back on the SPE; the sentinel
  shims it off `nros_platform_time_now_ms`. Consider gating that seed
  branch off for `ZENOH_ORIN_SPE` builds.
