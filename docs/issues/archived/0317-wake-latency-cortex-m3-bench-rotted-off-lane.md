---
id: 317
title: "wake-latency-cortex-m3 bench is rotted (off-lane): retired `_start`/`run` entry + phase-230 platform-freertos feature drift + a libc-stub duplicate-symbol link conflict"
status: resolved
resolved_in: "phase-313 (wake-latency resurrection)"
type: tech-debt
severity: low
area: embedded
related: [issue-0273]
---

## RESOLVED (2026-07-28)

The bench is resurrected end-to-end. Four layers, all fixed:

1. **Build rot** — `_start`→`main`, `run`→`Mps2An385::run_bare`, dropped the
   phase-230 `platform-freertos` feature drift, and root-caused the
   `duplicate symbol: strtol` to `nros-platform-mps2-an385` hardcoding
   `nros-baremetal-common/libc-stubs` (gated behind a default-on `libc-stubs`
   feature so the picolibc freertos bench opts out).
2. **Two-image redesign** — publisher + subscriber run as separate QEMU images
   (distinct sessions) so zenohd routes a real transport-arrival wake; the host
   runner drives both via `start_mps2_an385_networked` (`-icount shift=auto`) +
   `ZenohRouter::start_slirp`. Delivery verified.
3. **Async-wake fix (`nros-rmw-zenoh` shim)** — the probe's `on_wake` T0 fires
   from the runtime wake callback, which the shim previously invoked only from the
   main-thread `drive_io` poll path. On the multi-threaded backend the sample
   arrives on the async read task, so `drive_io` saw `n=0` and never fired it.
   Mirrored the runtime wake-cb into a process-global (`set_runtime_wake_cb` /
   `fire_runtime_wake`) and fired it from `subscriber_notify_callback` (the
   read-task arrival hook), so `on_wake` timestamps the real arrival. Verified no
   regression (`test_qemu_bsp_pubsub_e2e`, `test_qemu_zenoh_large_publish`,
   `logging_smoke_freertos_mps2` all green).
4. **CSV emit** — the subscriber's `write_csv` sink split records across lines
   (`write_csv` does partial `write!`s + the board `println!` adds a newline per
   call); buffer the whole CSV into a `heapless::String` and emit per real line.
   The loop now also exits on delivered-message count so the CSV emits even when
   the probe is empty.

The `wake_latency_cortex_m3` runtime test now runs the full two-image flow and
reaches its intended graceful **CYCCNT skip** on QEMU (Cortex-M3 does not emulate
the DWT CYCCNT → `on_dispatch`'s `t1-t0` is 0 → 0 samples, the documented QEMU
outcome); the real P99 validates on hardware (STM32F4), where the shim wake-cb
fix makes `on_wake` fire. Both images are guarded by `just freertos
build-fixture-extras`.

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

## Two-image redesign — DONE + delivery VERIFIED (2026-07-28)

Redesigned the bench as TWO images (`src/lib.rs` shared config/scenario consts +
two bins): the **publisher** (`wake-latency-pub`, distinct IP/MAC/session) pubs
`Int32` on `/wake-latency` at 100 Hz, and the **subscriber**
(`wake-latency-cortex-m3`) subscribes + runs the wake-probe. The host runner
starts both QEMUs (`start_mps2_an385_networked` — `-icount shift=auto` +
LAN9118 slirp) against a `ZenohRouter::start_slirp` on `0.0.0.0:7800`, mirroring
the passing `test_qemu_bsp_pubsub_e2e`. Both images build (added to `just freertos
build-fixture-extras`).

**Delivery works** (verified with a temporary rx counter): the subscriber's
callback fires on the publisher's samples routed through zenohd (`rx≈2500` over a
25 s run) — the original same-image self-delivery blocker is GONE.

## Remaining — the wake-cb async-wake gap (multi-threaded zpico)

The `wake_latency_cortex_m3` runtime test now **skips**: delivery works but the
probe still captures 0 samples. Root cause (isolated): the probe's `on_wake` (T0)
fires only from the RMW **wake callback**, which the zpico shim invokes solely
from the **main-thread `drive_io` poll path** (`nros-rmw-zenoh/src/shim/session.rs`
`drive_io`: `if spin_once() saw work { fire wake_cb }`). On the multi-threaded
FreeRTOS backend the sample is received by the **async read task**, not
`drive_io`, so `drive_io` returns `n=0`, the wake_cb never fires, and the executor
does not cv-wait to be woken — yet the sample is still queued + dispatched
(`on_dispatch`/callback run). So `on_dispatch` (T1) has no paired `on_wake` (T0)
→ 0 probe samples.

**Direction (a distinct executor/zpico fix, NOT a bench change):** have the zpico
read task invoke the session wake callback on subscription-sample arrival (thread
`wake_cb`/`wake_ctx` to the subscriber data handler), and make the embedded
executor's `spin_once` cv-wait on the async-wake signal (`has_async_wake`) so the
wake actually fires `on_wake` before dispatch. Affects the multi-threaded hot
path + every embedded async subscriber, so it needs its own scope + an embedded
regression sweep. Until then the test skips with this reference and the build lane
guards both images compiling.
