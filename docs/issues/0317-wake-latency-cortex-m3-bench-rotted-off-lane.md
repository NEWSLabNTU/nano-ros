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

## Progress — BUILD rot FIXED (phase-313, 2026-07-28); one runtime blocker remains

The three filed build-rot items are all fixed and the bench now
**builds → boots → connects → publishes** under QEMU:

1. **Entry.** `_start` → `#[unsafe(no_mangle)] extern "C" fn main() -> !`.
2. **`run` → `run_bare`.** `Mps2An385::run_bare(config, |config| …)` (the freertos
   `run_bare` was widened to pass `&Config` for this).
3. **platform-freertos drift + libc conflict.** Dropped `platform-freertos` from
   the `nros`/`nros-node` deps (+ a direct `cortex-m-semihosting`); root-caused
   the `duplicate symbol: strtol` to `nros-platform-mps2-an385` hardcoding
   `nros-baremetal-common/libc-stubs`. Fixed by gating that behind a **default-on
   `libc-stubs` feature** on `nros-platform-mps2-an385`; this freertos bench (has
   picolibc from the board) sets `default-features = false`. Baremetal consumers
   keep libc-stubs via default.
4. **Stale locator port.** The baked `tcp/10.0.2.2:7451` predated the port
   renumber; corrected to `7800` (= `FREERTOS.zenohd_port` = 7000 + FreertosMps2
   index 2 * 400). The bench now opens the session + publishes.

Added to a build lane (`just freertos build-fixture-extras` builds it `--release`)
so the compile can't rot again.

## Remaining (the now-primary blocker) — same-image pub→sub can't self-deliver

`wake_latency_cortex_m3_p99_within_bound` still fails: the bench pub→subs the
SAME topic in ONE image and expects delivery to fire the wake-cb probe, but no
sample arrives (0 CSV → 30 s timeout). Root cause: `nros-zpico-build`
(`src/lib.rs`) bakes `Z_FEATURE_LOCAL_SUBSCRIBER 0` for **embedded** targets (a
deliberate SRAM-budget call — the loopback + write-filter code is unbudgeted), so
zenoh-pico does not deliver a session's own publication to its own subscriber;
and a vanilla zenohd router does not echo a sample back to the publishing
session. So the bench's "route through zenohd, not local" design (main.rs top
comment) cannot receive on embedded.

**Investigated (2026-07-28): local loopback is the WRONG fix.** Baking
`Z_FEATURE_LOCAL_SUBSCRIBER 1` (tried via a temporary `ZPICO_LOCAL_LOOPBACK` env
override; verified the header flipped to 1) makes the same-session sub receive —
but through the **in-process loopback path (`src/session/loopback.c`), NOT the
transport-arrival path** that the probe hooks (`nros_rmw_runtime_wake_cb` /
`dispatch_one`). So the wake-cb still never fires and the probe still captures 0
samples. The bench's own top comment says it deliberately wants transport-arrival
"not a local short-circuit" — local delivery defeats the measurement. The env
override was reverted (no valid consumer).

**Direction: redesign the bench as TWO images** — a separate publisher image and
the subscriber-under-test image — so the zenohd router genuinely routes the
sample between two distinct sessions, giving a real transport-arrival wake that
fires the probe. (A single-session pub→sub through a vanilla router does not
echo back to the publishing session.) That is a Phase-141-class bench rework,
distinct from the build rot resolved above; the host runner
`nros-tests::wake_latency_cortex_m3` + `QemuProcess` harness can drive two QEMU
instances the way the freertos talker/listener interop tests already do.
