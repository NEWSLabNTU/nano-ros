---
id: 126
title: "`nros::main!` Zephyr (and Esp32) emit branch wires only register+spin — no param-services / lifecycle / run_tiers, blocking phase-276 W1/W2/W3 on Zephyr"
status: open
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
