# Phase 297 — ThreadX multi-tier execution

Implements RFC-0053 (ThreadX multi-tier via codegen static per-tier stacks +
native preemption-threshold). Builds on phase-296 W5.4 (the portable
`ExecutorNodeRuntime::apply_tier_sched_policy` every board shares).

**Status (2026-07-21):** design landed (RFC-0053). W1 C++ path DONE
(`cf69b09f2` + `650a4d7e9` — the tier→SchedContext lowering is single-sourced
in `SchedContext::from_tier_policy`; `emit_cpp` forwards raw fields to it via
the new `nros_cpp_create_sched_context_from_policy` FFI). W1 Rust/C path
pending — found to be `run_tiers`-shaped (the macro routes any single *named*
tier to `<board>::run_tiers`), so it merges into the W4 `run_tiers` work
starting from the single-tier case.

**Common-backend principle (applies to every wave).** One backend serves all
languages; no logic is re-derived per codegen path. The tier→SchedContext
lowering lives once (`SchedContext::from_tier_policy`) and is reached by C,
C++, and Rust alike (W1, done). By the same rule: the ThreadX `run_tiers`
(W1-Rust / W4) must call `apply_tier_sched_policy` (never re-lower), and the C
`nros_threadx_create_task` shim (W2) is the single thread-creation backend the
Rust `run_tiers` and any C/C++ entry both call — mirroring the FreeRTOS
`nros_freertos_create_task` shape, not a parallel per-language implementation.

## Goal

Give `nros-board-threadx` the same multi-tier model as freertos/zephyr/nuttx
(one `Executor` per tier over one shared RMW session), with **codegen-baked
static per-tier stacks** (RFC-0053 Option A) and the tier's `preempt_threshold`
applied through ThreadX's **native** `tx_thread_preemption_change` — the one
platform where the six-dim `non_preempt_scope` is a kernel primitive, not
emulated.

## Waves

### W1 — v0 stepping stone: single-executor tier policy

The tier's RTOS-agnostic policy (class/budget/period/deadline) must reach the
single ThreadX executor. There are **two** entry paths, and the codegen
routing differs per language — both need the lowering:

- **C++ path — DONE (commits `cf69b09f2` then `650a4d7e9`).** The
  single-executor codegen path (`emit_cpp`, used by ThreadX + group-split plans
  per `ResolvedTierTable::has_group_split_node`) hardcoded `__sc.class_ = Fifo`
  and carried only `os_pri` + the spin cadence, so a `real_time` tier silently
  ran best-effort. **Per the common-backend principle** (one backend for all
  languages), the fix does NOT re-derive the mapping in the codegen. The
  tier→SchedContext lowering is single-sourced in
  `SchedContext::from_tier_policy` (nros-node); `apply_tier_sched_policy` (Rust
  runtime) and a new FFI `nros_cpp_create_sched_context_from_policy` (nros-cpp)
  both call it. `emit_cpp` now emits a `from_policy` call forwarding the **raw**
  tier fields (`class` string / periods / `os_pri`), re-deriving nothing — so a
  `real_time` tier lowers to the identical Sporadic SC on every language and
  the mapping cannot drift. `Fifo` behavior unchanged when no RT `class`.
  Backend tests `from_tier_policy_*` (nros-node); codegen test
  `typed_emit_single_executor_forwards_real_time_tier_to_backend`. Deferred:
  `time_triggered` single-executor (the backend returns the major frame, but
  the codegen would need to also emit the `register_time_triggered_dispatcher`
  call) and `deadline_action`/miss-policy carry across the FFI (the backend
  sets it; the `from_policy` FFI forwards `deadline_policy`, so this is
  actually covered — unlike the retired hand-derived path).

- **Rust-board path — PENDING, and it is `run_tiers`-shaped, not a
  `run_app_thread` tweak.** The `nros::main!` macro routes **any** tier table
  that is not the synthesized single `default` tier (`is_single_tier()`) to
  `<board>::run_tiers(&overlay, &[TierSpec{class, period_us, budget_us,
  deadline_us, preempt_threshold, …}], closure)`. So even a *single named*
  `real_time` tier on ThreadX routes to `run_tiers` — which ThreadX does not
  implement, i.e. it does not compile today. The C path (`emit_c`,
  `native_threadx_entry`) likewise emits `TierSpec` tokens, not
  `create_sched_context`, so it too needs a ThreadX `run_tiers`. Therefore the
  v0 Rust deliverable is a **`run_tiers` that handles the single-tier case**
  (boot tier only: build the executor, `apply_tier_sched_policy(tier[0])`,
  apply the tier's `priority` + native `preempt_threshold` to the app thread,
  spin) and errors clearly on `> 1` tier until W4 adds the per-tier threads +
  stacks. This is the `run_app_thread` boot-tier idea from RFC-0053's v0
  ladder, realized through the entry method the macro actually calls. The
  legacy synthesized single-`default`-tier ThreadX image keeps
  `run_with_deploy` → `run_app_thread` unchanged (no RT policy to apply).

- **Done when:** a single *named* `real_time` tier ThreadX image compiles,
  lowers budget/period to a Sporadic `SchedContext`, and applies its priority
  — same observable behavior as the posix/native single-tier path. No new
  stack machinery. Verified on `threadx-linux` (host sim) or
  `threadx-qemu-riscv64`.

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

- **Extend the W1 single-tier `run_tiers`** on `nros-board-threadx` to the
  multi-tier case: boot tier declares FIRST (issue #144), then each remaining
  tier spawns via the W2 shim with its W3 stack, running one `Executor` +
  `setup` over the shared RMW session and calling `apply_tier_sched_policy` on
  its executor. `preempt_threshold` → `tx_thread_preemption_change` (native
  `non_preempt_scope`). (W1 already establishes the `run_tiers` entry method,
  the macro routing, and the boot-tier policy/priority application for one
  tier; W4 adds the additional per-tier threads + stacks.)
- Wire the per-board crates (`nros-board-threadx-linux`,
  `nros-board-threadx-qemu-riscv64`) to route their multi-tier entry through it.
- **Done when:** a multi-tier ThreadX image (e.g. a `real_time` control tier +
  a `best_effort` tier) spawns one thread/executor per tier over one session,
  each executor carries its tier's SchedContext, and the control tier's
  `preempt_threshold` is applied natively — verified on `threadx-linux` and/or
  `threadx-qemu-riscv64` (a two-QEMU or host-sim runtime lane, matching the
  existing threadx e2e fixtures).

## Order and dependencies

W1 (v0 — delivers the SchedContext lowering on the single-tier path
immediately). W1 has **two independent sub-paths**: the C++ codegen lowering
(DONE, `cf69b09f2`) and the Rust/C `run_tiers` v0 (pending — it establishes the
ThreadX `run_tiers` entry the macro already routes to). → W2 (shim) → W3
(codegen stacks) → W4 (extend the W1 `run_tiers` to multi-tier). W3 and W2 can
proceed in parallel; W4 needs both, plus the W1 Rust `run_tiers` v0.

## Non-goals

- SMP core affinity (the ThreadX boards here are single-core — RFC-0053
  §Non-goals); measured/auto stack sizing; MPU per-tier isolation. The runtime
  `PlatformSched` `set_deadline`/`replenish` (kernel-native EDF/reservation)
  is a separate cross-board follow-up — ThreadX has neither EDF nor a
  reservation server, so the executor's own Sporadic `SchedContext` remains the
  budget mechanism there.
