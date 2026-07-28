---
id: 317
title: "wake-latency-cortex-m3 bench is rotted (off-lane): retired `_start`/`run` entry + phase-230 platform-freertos feature drift + a libc-stub duplicate-symbol link conflict"
status: open
type: tech-debt
severity: low
area: embedded
related: [issue-0273]
---

## Finding (phase-313 #0273 fix, 2026-07-28)

`packages/testing/nros-bench/wake-latency-cortex-m3` (Phase 141.D wake-latency
P99 microbench) is in **no build lane** (not in `examples/fixtures.toml`, not in
any `just` recipe) and has silently accumulated three layers of rot — it does
not build today:

1. **Retired entry (the #0273 class).** `src/main.rs` uses
   `#[unsafe(no_mangle)] extern "C" fn _start() -> !`, but `board_mps2.c`'s
   `Reset_Handler` jumps to `main` (`_start` retired, commit d99386173) →
   `rust-lld: undefined symbol: main`. Same bug #0273 fixed in the board
   descriptor.

2. **Legacy `run` deleted (phase-313).** It calls
   `nros_board_mps2_an385_freertos::run(config, |config| …)`, but that free
   `run` was retired in phase-313 (the board moved to `Mps2An385::run_bare` /
   `BoardEntry`). The straightforward fix is `_start`→`main` +
   `run`→`Mps2An385::run_bare` (the freertos `run_bare` now passes `&Config`, so
   the closure body is unchanged).

3. **Phase-230 platform-freertos feature drift + libc conflict.** `Cargo.toml`
   requests `platform-freertos` from `nros` and `nros-node`, but phase-230 split
   the platform layer out of both (only `nros-rmw-zenoh` keeps that feature).
   Dropping it from `nros`/`nros-node` (+ adding a direct `cortex-m-semihosting`
   dep for `debug::exit`) gets it to compile, but then LINK fails:
   `duplicate symbol: strtol` / `strtoul` — `nros-baremetal-common`'s
   `libc_stubs.rs` and the freertos board's picolibc (`libc_a-strtol.o`) both
   define them. The bench's dep graph pulls a bare-metal libc-stub crate that
   conflicts with the FreeRTOS board's own libc.

## Direction

Resurrect the bench end-to-end: apply fixes 1+2 (entry + `run_bare`), modernize
the Cargo deps to the current freertos link graph (fix 3 — resolve the
`nros-baremetal-common` vs board-picolibc libc-stub conflict, likely by dropping
the bare-metal-common path from this freertos bin's graph), then **add it to a
build lane** (`just freertos` + `examples/fixtures.toml`) so it can't rot again —
the host acceptance harness `nros-tests::wake_latency_cortex_m3` (Phase 141.C.2)
already exists. Needs the freertos QEMU + zenohd + host harness to verify the
histogram output, so it's a scoped embedded task, not a mechanical edit.
