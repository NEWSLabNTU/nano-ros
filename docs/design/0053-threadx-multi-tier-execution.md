---
rfc: 0053
title: "ThreadX multi-tier execution — codegen-baked static per-tier stacks + native preemption-threshold"
status: Draft
since: 2026-07
last-reviewed: 2026-07
implements-tracked-by: [phase-297]
supersedes: []
superseded-by: null
---

# RFC-0053 — ThreadX multi-tier execution

## Summary

ThreadX gains **multi-tier execution** — one `Executor` per tier over one
shared RMW session — mirroring the FreeRTOS/Zephyr/NuttX boards. The threads
are created with **codegen-baked static per-tier stacks** (Option A below),
and the tier's `preempt_threshold` is applied through ThreadX's **native**
`tx_thread_preemption_change`. This makes ThreadX the **only** platform where
the six-dim `non_preempt_scope` dimension (RFC-0052) is realized by a kernel
primitive rather than emulated.

## Background

- `nros-board-threadx` today is **single-executor**: `run_entry` /
  `run_app_thread` sets ThreadX config + one app callback, calls
  `tx_kernel_enter()`, and one app thread drives one `Executor` forever. No
  `TierSpec` is threaded through; there is no `run_tiers`.
- The other embedded boards are **multi-thread**: `run_tiers(tiers)` runs one
  `Executor` per tier over one shared session, spawning threads via an OS
  create-shim (FreeRTOS `nros_freertos_create_task`, Zephyr `k_thread` FFI,
  NuttX `pthread`). The boot tier declares first (issue #144 discovery race),
  then the rest spawn.
- **ThreadX has no default heap.** `tx_thread_create` requires a
  caller-provided stack buffer — unlike FreeRTOS's `xTaskCreate` (heap) or
  NuttX's `pthread`. So ThreadX multi-tier needs an explicit stack strategy.
- W5.4 already landed the **portable** `ExecutorNodeRuntime::apply_tier_sched_policy`
  (the tier→`SchedContext` lowering) that every board calls; ThreadX will call
  it per tier once it has the per-tier executors.

## Decision — static per-tier stacks (Option A), not a byte pool (Option B)

Two ThreadX-canonical stack idioms were compared:

- **A — codegen-baked static per-tier stack arrays.** The CLI emits `N`
  aligned `static` byte arrays sized to each tier's `stack_bytes`; each is
  passed to `tx_thread_create`. (ThreadX "static stack array" idiom; the same
  shape as Zephyr `K_THREAD_STACK_DEFINE` and FreeRTOS `xTaskCreateStatic`.)
- **B — shim-managed `TX_BYTE_POOL`.** One static pool; the shim
  `tx_byte_allocate`s each stack at spawn. (ThreadX byte-pool idiom.)

**Chosen: A.** It wins on every runtime axis and matches nano-ros's
bake-exact discipline:

| Dimension | A (static per-tier) | B (`TX_BYTE_POOL`) |
|---|---|---|
| RAM | exact `Σ stack_bytes` + align pad; no pool/frag overhead | worst-case pool + per-alloc header + frag headroom |
| Flash/code | BSS arrays, no runtime alloc | pool create + `tx_byte_allocate` calls |
| Determinism/safety | linker-placed; fail-at-**link**; no runtime-alloc failure; MISRA/Ravenscar-friendly | `tx_byte_allocate` can fail at run time; fragmentation |
| Tier count | exact `N`, re-bake to change | fixed pool cap → runtime fail if exceeded |
| Consistency | matches freertos/zephyr per-tier codegen + W2 `stack_bytes` | divergent (runtime carve + magic pool constant) |
| Alignment | codegen bakes it (`aligned`) | free (`tx_byte_allocate` aligns) |

A's only costs are a codegen change and baking the alignment — both one-time.
The cross-RTOS norm for a *fixed* thread set is per-task static stacks
(Zephyr `K_THREAD_STACK_DEFINE`, FreeRTOS `xTaskCreateStatic`, ThreadX static
arrays, RTIC on bare-metal); A is that norm.

### Revision (phase-297 W4, 2026-07-22) — implemented as byte-pool (B)

Implementation revised this to **B (byte pool)**. The table's "Consistency"
row was **wrong**: it claimed A "matches freertos/zephyr per-tier codegen", but
neither board bakes per-tier static stacks — FreeRTOS spawns on its **heap**
(`xTaskCreate`, dynamic) and Zephyr on a **static `k_thread` pool** bounded by
`NROS_ZEPHYR_MAX_TIERS` (a C shim), not codegen-baked arrays. So A was
consistent with nothing that actually ships.

Decisive: nano-ros's own `threadx_hooks.c` **already** allocates the boot app
thread's stack from a 4 MB `TX_BYTE_POOL` via `tx_byte_allocate`. Allocating
each tier's stack the same way is consistent with ThreadX's OWN app thread,
adds **no** new static RAM (a tier executor stack is hundreds of KB — a fixed
static pool would cost `MAX_TIERS ×` that in BSS, and exact codegen arrays are
still `Σ stack_bytes` of BSS), and its one downside — runtime-alloc failure —
is handled (`nros_threadx_create_task` returns `-1` → the tier does not spawn).
The small fixed-size `TX_THREAD` control blocks live in a bounded static array
in the shim, so `sizeof(TX_THREAD)` never crosses the FFI.

Exact per-tier static stacks (A) remain a **future RAM optimization** for
constrained MCU targets; it is not required for the ThreadX-Linux /
ThreadX-QEMU-RISC-V64 boards here (both simulate threads with generous host /
QEMU RAM). The rest of this doc reads "static stack" as "byte-pool stack".

## Architecture

```
codegen (host)                          runtime (ThreadX, no_std + C shim)
──────────────                          ─────────────────────────────────
TierSpec per tier i:                    run_tiers(tiers):
  priority, preempt_threshold,            boot tier declares FIRST (issue #144)
  stack_bytes (0 = overlay default)       for tier in rest:
                                            nros_threadx_create_task(
                                              name, entry, arg, stack_bytes,
                                              priority, preempt_threshold)
                                            └─ C shim: tx_byte_allocate stack
                                               + tx_thread_create(..., priority,
                                                 threshold, ...)
                                          each spawned thread:
                                            Executor over shared session
                                            + apply_tier_sched_policy (W5.4)
```

- **C-side FFI shim** `nros_threadx_create_task(name, entry, arg,
  stack_bytes, priority, preempt_threshold)` (as landed, phase-297 W2/W4) →
  allocates the stack from the shared `TX_BYTE_POOL` (see Revision above),
  then `tx_thread_create` with the threshold passed directly as the create
  argument (`-1` sentinel → threshold = priority, ThreadX's "no threshold"
  state; no separate `tx_thread_preemption_change` needed at creation).
  `TX_THREAD` control blocks live in a bounded static array
  (`NROS_TX_MAX_TASKS`) inside the shim. This is the one new native surface
  (the FreeRTOS board has its analogue; ThreadX did not).
- **Byte-pool stacks (Option B, per the Revision):** `stack_bytes == 0`
  falls back to `nros_board_app_stack_size()`; allocation failure returns
  `-1` and the tier does not spawn. Exact per-tier static stacks (A) remain
  a future RAM optimization for constrained MCU targets.
- **`run_tiers` (Rust):** boot tier declares first, then each remaining tier
  spawns via the shim, running one `Executor` + `setup` over the shared RMW
  session, calling `apply_tier_sched_policy(tier.class, tier.period_us,
  tier.budget_us, tier.deadline_us, tier.deadline_policy)` (W5.4) on its
  executor. Shared-session-across-threads follows the other boards' model.
- **`preempt_threshold` → `tx_thread_preemption_change`:** the native
  realization of `non_preempt_scope`. The realizer's
  `RealizedNode.preempt_threshold` (RFC-0052 W5.2) flows through `TierSpec`
  (bake-validated ThreadX-only) into the shim.

## Migration ladder

1. **v0 (stepping stone):** keep the single executor but pass the boot tier so
   the one executor gets `apply_tier_sched_policy` + the app thread gets the
   tier's priority / `preempt_threshold`. Unlocks the SchedContext lowering +
   native preempt-threshold immediately (the degenerate single-tier case), no
   new stack machinery.
2. **End state (as landed):** multi-tier `run_tiers` with byte-pool stacks
   (Option B — see the Revision above; the original plan here said "A
   (codegen static stacks) + B explicitly not taken", which the phase-297 W4
   implementation reversed).

## Non-goals / open questions

- **SMP core affinity.** The ThreadX boards here (`threadx-linux`,
  `threadx-qemu-riscv64`) are single-core (`todo smp`), so `core` placement is
  N/A for now even though ThreadX-SMP supports `tx_thread_smp_core_exclude`.
  `SchedCaps.affinity=true` reflects the *kernel*; a board-vs-kernel cap
  refinement is future work.
- **Stack sizing accuracy.** `stack_bytes` is integrator-declared; a
  measured/auto-sized value (stack-usage analysis) is out of scope.
- **MPU/user-mode isolation.** Static stacks are placed in BSS; MPU-region
  isolation per tier is future work (mirrors Zephyr's MPU stack objects).

## References

- RFC-0052 (SystemModel → RTOS primitives; the six-dim realizer +
  `apply_tier_sched_policy`), phase-296 W5.
- Work breakdown: phase-297.
- FreeRTOS reference: `nros-board-freertos/c/freertos_run_tiers.c`
  (`xTaskCreate` + `stack_bytes`); NuttX/Zephyr `run_tiers`.
- ThreadX static-stack vs byte-pool idioms; Zephyr `K_THREAD_STACK_DEFINE`;
  FreeRTOS `xTaskCreateStatic` (design exploration 2026-07-21).
