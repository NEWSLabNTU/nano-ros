# Phase 297 — ThreadX multi-tier execution

Implements RFC-0053 (ThreadX multi-tier via codegen static per-tier stacks +
native preemption-threshold). Builds on phase-296 W5.4 (the portable
`ExecutorNodeRuntime::apply_tier_sched_policy` every board shares).

**Status (2026-07-21):** design landed (RFC-0053); implementation pending.

## Goal

Give `nros-board-threadx` the same multi-tier model as freertos/zephyr/nuttx
(one `Executor` per tier over one shared RMW session), with **codegen-baked
static per-tier stacks** (RFC-0053 Option A) and the tier's `preempt_threshold`
applied through ThreadX's **native** `tx_thread_preemption_change` — the one
platform where the six-dim `non_preempt_scope` is a kernel primitive, not
emulated.

## Waves

### W1 — v0 stepping stone: single-executor tier policy

- Thread the boot tier into `run_app_thread`/`run_entry` so the single
  executor calls `apply_tier_sched_policy(class, period_us, budget_us,
  deadline_us, deadline_policy)` (the SchedContext lowering, W5.4) and the app
  thread takes the tier's `priority` + `preempt_threshold`.
- **Done when:** a single-tier ThreadX image lowers a `real_time` tier's
  budget/period to a Sporadic `SchedContext` and applies its priority — same
  observable behavior as the posix/native single-tier path. No new stack
  machinery. Verified on `threadx-linux` (host sim) or `threadx-qemu-riscv64`.

### W2 — C FFI create-task shim

- `nros_threadx_create_task(entry: extern "C" fn(*mut c_void), arg, priority,
  preempt_threshold, stack_ptr, stack_len)` (C, in the board's `c/`): calls
  `tx_thread_create` with the supplied stack, then `tx_thread_preemption_change`
  when `preempt_threshold` is set (bake-validated ThreadX-only). Mirrors the
  FreeRTOS `nros_freertos_create_task` shape.
- **Done when:** a hand-driven test creates two ThreadX threads at distinct
  priorities with distinct preemption thresholds and both run.

### W3 — codegen: static per-tier stacks

- The entry codegen emits one aligned `static mut TIER_STACK_i: [u8;
  stack_bytes_i]` per tier (Cortex-M/R 8-byte alignment; MPU power-of-two
  rounding where the target enables it), and threads each `(ptr, len)` into
  the `TierSpec`/spawn call. A stack too large for the image is a **link**
  error (no runtime alloc). Default size when `stack_bytes` unset mirrors the
  freertos default policy.
- **Done when:** a two-tier bake emits two sized, aligned stack arrays and the
  linker places them; changing a tier's `stack_bytes` changes the emitted
  array size.

### W4 — `run_tiers` multi-tier + native preempt-threshold

- Add `run_tiers(tiers)` to `nros-board-threadx`: boot tier declares FIRST
  (issue #144), then each remaining tier spawns via the W2 shim with its W3
  stack, running one `Executor` + `setup` over the shared RMW session and
  calling `apply_tier_sched_policy` on its executor. `preempt_threshold` →
  `tx_thread_preemption_change` (native `non_preempt_scope`).
- Wire the per-board crates (`nros-board-threadx-linux`,
  `nros-board-threadx-qemu-riscv64`) to route their multi-tier entry through it.
- **Done when:** a multi-tier ThreadX image (e.g. a `real_time` control tier +
  a `best_effort` tier) spawns one thread/executor per tier over one session,
  each executor carries its tier's SchedContext, and the control tier's
  `preempt_threshold` is applied natively — verified on `threadx-linux` and/or
  `threadx-qemu-riscv64` (a two-QEMU or host-sim runtime lane, matching the
  existing threadx e2e fixtures).

## Order and dependencies

W1 (v0, independent — delivers the SchedContext lowering + preempt-threshold on
the single-tier path immediately) → W2 (shim) → W3 (codegen stacks) → W4
(multi-tier `run_tiers`). W3 and W2 can proceed in parallel; W4 needs both.

## Non-goals

- SMP core affinity (the ThreadX boards here are single-core — RFC-0053
  §Non-goals); measured/auto stack sizing; MPU per-tier isolation. The runtime
  `PlatformSched` `set_deadline`/`replenish` (kernel-native EDF/reservation)
  is a separate cross-board follow-up — ThreadX has neither EDF nor a
  reservation server, so the executor's own Sporadic `SchedContext` remains the
  budget mechanism there.
