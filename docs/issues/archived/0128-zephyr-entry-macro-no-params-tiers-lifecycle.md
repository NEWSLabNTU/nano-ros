---
id: 128
title: "`nros::main!` Zephyr (and Esp32) emit branch wires only register+spin — no param-services / lifecycle / run_tiers, blocking phase-276 W1/W2/W3 on Zephyr"
status: resolved
type: tech-debt
area: core
related: [phase-276, phase-274, rfc-0015, rfc-0032, phase-270]
---

> **Reconciliation context (RFC-0015 Model 1).** RFC-0015 decided **Model 1** — one
> RTOS task per tier, `active_groups` gating — as the single execution model for
> ALL languages. Rust reaches it via `run_tiers` on the **OwnedSpin** boards
> (native/freertos/threadx/nuttx); **phase-274** brings C/C++ to parity (W1/W2
> native landed, W3 = embedded C/C++ `run_tiers` across FreeRTOS/**Zephyr
> `k_thread`**/NuttX/ThreadX — pending). The gap this issue reports is the
> **Rust-Zephyr** sibling: the `Framework::Zephyr` emit branch never got the
> Model-1 (run_tiers) OR the params/lifecycle wiring the OwnedSpin arms have, so
> Zephyr is the one framework off the unified model on the Rust side. Fixing it
> belongs with the phase-274 W3 embedded convergence (they share the Zephyr
> `k_thread` per-tier-task machinery); params/lifecycle are the cheaper half.

> **Progress 2026-07-03 — the cheap half LANDED.** The `Framework::Zephyr` (and
> Esp32) arm now emits `param_services_call` + `lifecycle_call` + the deploy-rmw
> `register()` (RFC-0031 C5b amendment), and 276 **W1 (params)** and **W3
> (lifecycle)** are proven on Zephyr e2e (`params_zephyr_entry_e2e`,
> `lifecycle_zephyr_entry_e2e` — autostart reaches `active`, all five REP-2002
> services answer; the service-path blocker was issue #139, resolved). **What
> remains of this issue is exactly the hard half: `ZephyrBoard::run_tiers`**
> (Model-1 per-tier `k_thread` tasks on the Rust side — 276 W2), which belongs
> with the phase-274 W3 embedded convergence machinery.

## Summary

Phase 276 (capability-on-embedded) targets **Zephyr `native_sim`** as the primary
embedded platform for proving lifecycle / parameters / RT-tiers. Investigation
(2026-07-02) found the `nros::main!` **Zephyr** emit branch cannot express any of
these three capabilities — it emits register-only + a plain spin loop, so
**276 W1 (params), W2 (tiers), W3 (lifecycle) are blocked on Zephyr at the macro
level**, not the fixture level.

## Evidence (`packages/core/nros-macros/src/main_macro.rs`)

The macro computes `param_services_call` (line ~783), `lifecycle_call` (~768),
and a multi-tier `run_tiers` `entry_call` (~888). These are emitted **only** in
the `OwnedSpin` framework arms:
- run_tiers closure: `#param_services_call` (908), `#lifecycle_call` (910)
- non-tier `run_with_deploy` closure: `#param_services_call` (933), `#lifecycle_call` (935)

The `Framework::Zephyr` arm (~1134–1218) has its own scaffold that opens one
`Executor`, emits `#( #register_calls )*`, logs "zephyr workspace entry up", and
loops `runtime.runtime.spin_once(10)`. It references **none** of
`param_services_call` / `lifecycle_call` / `run_tiers`. `Framework::Esp32`
(~1232) is the same shape. So on Zephyr/Esp32 a `system.toml` with `[tiers.*]`,
param services, or a managed-node lifecycle is silently ignored — you get plain
pub/sub register+spin.

By contrast `OwnedSpin` (native / freertos / threadx-linux / nuttx / bare-metal
Cortex-M) supports all three, which is why the FreeRTOS tiers demo
(`orchestration_tiers_freertos`) works and the Zephyr equivalent cannot be built
by merely adding a fixture.

## Impact on phase-276

| Cap | native | achievable on embedded now | blocked-on-Zephyr by this issue |
| --- | --- | --- | --- |
| W2 RT-tiers | ✓ | FreeRTOS ✓ (done) | Zephyr — needs a `ZephyrBoard` tier-spin + macro emit |
| W1 parameters | ✓ | FreeRTOS (OwnedSpin emits param-services) | Zephyr — needs `#param_services_call` in the Zephyr arm |
| W3 lifecycle | ✓ | (C++ wrapper, phase-270) | Zephyr rust — needs `#lifecycle_call` in the Zephyr arm |
| W4 safety/CRC | ✓ | **Zephyr — pub/sub, node-level, not macro-gated** | — |
| W5 QoS overrides | ✓ | **Zephyr — pub/sub, node-level** | — |
| W6 multihost | ✓ | **Zephyr — pub/sub** | — |

So phase-276 splits: **W4/W5/W6 are buildable on Zephyr today** (the capability
lives in node code riding the proven register+spin path); **W1/W2/W3 need this
macro work first** (or land on FreeRTOS, where the phase's "richest embedded
target = Zephyr" rationale doesn't hold).

## Fix direction

Extend the `Framework::Zephyr` arm to reach parity with `OwnedSpin`:
1. **params/lifecycle** — emit `#param_services_call` before `#register_calls`
   and `#lifecycle_call` after, inside `__nros_zephyr_entry_run` (the store +
   managed-node hooks are platform-agnostic; the blocker is purely that the arm
   doesn't emit them).
2. **tiers** — the harder half: needs a `ZephyrBoard::run_tiers` that maps
   `[tiers.*.zephyr]` priorities onto Zephyr threads (per-tier `active_groups`
   spin), mirroring the `Mps2An385::run_tiers` OwnedSpin path (RFC-0032 §5 /
   RFC-0015 §4.2). Then emit the multi-tier `entry_call` in the Zephyr arm.

Until then, phase-276 W1/W2/W3-on-Zephyr are parked here; W4/W5/W6-on-Zephyr and
W1-on-FreeRTOS remain the achievable slices.

## Progress (2026-07-03) — half 1 LANDED; Esp32 arm included

The **params/lifecycle emits landed**: both the `Framework::Zephyr` arm
(`#param_services_call` before the registers, `#lifecycle_call` after, inside
`__nros_zephyr_entry_run`) and the `Framework::Esp32` arm's `run_with_deploy`
closure now carry them — inert token streams without the `system.toml`
declarations / cargo features, so plain pub/sub entries are unchanged.
Proven compiling end-to-end by the phase-276 W1 fixture
(`ws-params-rust/src/zephyr_entry`, west lane `build-ws-rs-params-entry-zenoh`):
the launch `<param>` seed is baked into the ELF and `apply_param_services`
compiles on the `no_std` Zephyr target. **Runtime verified (2026-07-03, #129
resolved):** `params_zephyr_entry_e2e` PASSES un-ignored — the launch-baked
param initial (250) is seeded into the Zephyr entry's store, live-read by the
node callback, and observed by a cross-process subscriber. phase-276 W1
(params-on-Zephyr) is DONE.

**Remaining here: half 2 — tiers** (`ZephyrBoard::run_tiers` + the multi-tier
`entry_call` in the Zephyr arm).

## Work items — half 2 (tiers on Rust-Zephyr)

Design facts (2026-07-03 exploration): the macro already resolves
`[tiers.*.zephyr]` (`nros-orchestration-ir` `TierDef.zephyr` + `rtos_spec`,
`derive_target_rtos("zephyr")`) and emits `<board_path>::run_tiers(&DEPLOY,
TIERS, closure)` where `board_path_for("zephyr") =
::nros_board_zephyr::ZephyrBoard` — so tier RESOLUTION needs no work; only the
spawn machinery and the arm's emit are missing. `TierSpec.priority` is the RAW
Zephyr value (i64; negatives = cooperative), so the spawn seam must be
`k_thread_create`, NOT the existing `nros_zephyr_task_create` pthread shim
(POSIX priorities can't express Zephyr coop priorities, and
`nros_platform_task_init` ignores `attr` entirely).

- **T1 — tier-spawn shim** (`zephyr/nros_platform_zephyr_shims.c`):
  `nros_zephyr_tier_task_create(void *(*entry)(void*), void *arg,
  int32_t priority, const char *name)` via `k_thread_create` on a dedicated
  static pool (`NROS_ZEPHYR_MAX_TIERS`, `K_THREAD_STACK_ARRAY_DEFINE`) with the
  raw priority. No POSIX dependency; compiled unconditionally by the module
  CMake (same file as the pthread shim).
- **T2 — `ZephyrBoard::run_tiers`** (`packages/boards/nros-board-zephyr`): add
  an `nros` dep (`default-features = false, features = ["alloc", "rmw-cffi"]`,
  same as nros-board-freertos) and mirror
  `nros_board_freertos::run_tiers_entry` minus network/scheduler bring-up
  (Zephyr owns boot; `rust_main` is already a running thread): open the boot
  `Executor` from the caller-built `ExecutorConfig`, wrap in
  `ExecutorNodeRuntime`, hand each `tiers[1..]` a
  `Executor::session_handle()` + `TierSpec` + the `Copy` setup closure via a
  leaked ctx, spawn through T1, each tier task
  `open_with_session_handle` → `set_active_groups(tier.groups)` → setup →
  `spin_once(period)` loop; tiers[0] runs on the caller thread.
- **T3 — macro emit**: `Framework::Zephyr` arm branches on `multi_tier`
  (same `resolved_tiers` filter the OwnedSpin path uses): keep the arm's
  prelude (wait_network → deploy-rmw register → `BAKED_LOCATOR` config) and
  replace the single-executor register+spin body with
  `::nros_board_zephyr::ZephyrBoard::run_tiers(&config, #tiers_ts, closure)`
  where the closure carries the SAME `#param_services_call` / registers /
  `#lifecycle_call` sequence the OwnedSpin tier closure has (runs once per
  tier; groups filter what registers).
- **T4 — fixture + e2e**: `ws-realtime-rust` gains `[tiers.high.zephyr]` /
  `[tiers.low.zephyr]` priorities (system.toml) + `src/zephyr_entry` (the
  proven W5 recipe: workspace excludes, west leaf on port 17855, ws-sync prep,
  resolver) and `realtime_tiers_zephyr_entry_e2e`: two `int32-sink` observers
  on `/ctrl` (10 ms, high) and `/telem` (100 ms, low) — anchor on 5 telem
  receives, assert ctrl count strictly higher (the native
  `realtime_tiers_e2e` assertion, cross-process against the image).
- **T5 — C/C++ parity note**: T1 is deliberately C-ABI-shaped so the
  phase-274 W3 zephyr half (C/C++ `run_tiers` over `k_thread`) can reuse it;
  wiring `nros_board_zephyr_run_tiers` into `nros/main.hpp` stays with
  phase-274.
- **T0 (pre-req cleanup)** — the 276-W5 `int32-observer` fixture bin
  duplicated phase-277 W4's `int32-sink` (landed concurrently): retire
  `int32-observer`, switch `qos_zephyr_entry_e2e` / `safety_zephyr_entry_e2e`
  to `build_int32_sink` (`NROS_SUB_TOPIC`, ready-pattern "Listener").

## Resolution (2026-07-04) — half 2 LANDED; issue closed

All work items T0–T4 shipped (T5 stays with phase-274 as planned):

- **T0**: `int32-observer` retired; qos/safety e2es ride `int32-sink`.
- **T1**: `nros_zephyr_tier_task_create` + `nros_zephyr_set_current_priority`
  in `zephyr/nros_platform_zephyr_shims.c` — `k_thread_create` on a static
  pool (`NROS_ZEPHYR_MAX_TIERS`=4 × 16 KiB stacks), RAW Zephyr priorities.
- **T2**: `ZephyrBoard::run_tiers` (`nros-board-zephyr`, feature `tiers`) —
  boot executor on the caller thread, `SessionHandle`-shared tier tasks,
  per-tier `active_groups` + spin period; the boot thread adopts `tiers[0]`'s
  declared priority via the shim.
- **T3**: the `Framework::Zephyr` arm branches on `multi_tier`
  (`zephyr_body_tail` in `main_macro.rs`) — multi-tier emits
  `ZephyrBoard::run_tiers(&config, TIERS, closure)` with the same
  param/lifecycle/register closure sequence as OwnedSpin; single-tier stays
  byte-identical. Native realtime e2e re-verified green after the refactor.
- **T4**: `ws-realtime-rust` gained `[tiers.*.zephyr]` priorities +
  `src/zephyr_entry` (leaf `build-ws-rs-realtime-entry-zenoh` on 17855);
  `realtime_tiers_zephyr_entry_e2e` PASSES — two `int32-sink` observers see
  `/ctrl` (10 ms, high tier) outrun `/telem` (100 ms, low tier)
  cross-process.

Two zephyr-lane defects found and fixed en route (both same zsock family as
#139):

1. **Concurrent-declare interest race**: entity declares carry an interest
   handshake; when the boot thread and a spawned tier declared concurrently,
   the losing publisher's zenoh-pico write filter stayed closed and every
   put was silently dropped (`z_publisher_put` fired, `_z_send_n_msg`
   never). Fix: `run_tiers` runs the boot tier's setup BEFORE spawning
   tiers[1..]. Residual: with ≥3 tiers the spawned tiers' setups still race
   each other — acceptable for now, noted in `entry_tiers.rs`.
2. **tx throughput ceiling**: zsock's per-fd mutex caps total tx at ~one
   send per recv window; at the 100 ms default both tiers throttled to
   ~5 msg/s. Fix: `CONFIG_NROS_ZENOH_SOCKET_TIMEOUT_MS` Kconfig (default
   100; maps to `Z_CONFIG_SOCKET_TIMEOUT`); the realtime entry sets 5 ms.
