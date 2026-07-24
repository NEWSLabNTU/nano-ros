---
id: 260
title: "Native sched dims (core-pin, sporadic budget) are e2e-verified only on the FALLBACK arm — no fixture exercises the kernel-ACCEPT path"
status: open
type: limitation
area: testing
related: [phase-296, issue-0259]
---

## Finding (phase-296 W5.9–W5.11 placement/budget consumer work, 2026-07-24)

The RFC-0052 `Native`-dim consumers are written with a two-mode fail-loud
contract — a kernel-ACCEPT marker when the kernel honored the policy, or a LOUD
FALLBACK note when it could not — and each has a two-mode e2e
(`nuttx_sporadic_budget_applied`, `zephyr_core_pin_applied`,
`nuttx_core_pin_applied`, `freertos_core_pin_applied`; and W5.5 EDF /
W5.10 preempt-threshold). But EVERY current fixture exercises only the
FALLBACK arm for the SMP/budget-gated dims:

- **core-pin (placement):** every realtime fixture is UNIPROCESSOR —
  zephyr native_sim (no `CONFIG_SCHED_CPU_MASK_PIN_ONLY`), nuttx qemu-arm-virt
  (single core, no `CONFIG_SMP`), freertos mps2-an385 (no
  `configUSE_CORE_AFFINITY`). All three measure the honest fallback; the
  `k_thread_cpu_pin` / `pthread_setaffinity_np` / `vTaskCoreAffinitySet`
  accept path (`#ifdef CONFIG_SMP` etc.) is COMPILE-VERIFIED ONLY (against
  headers), never run.
- **sporadic budget (NuttX, W5.9b):** the arm/riscv defconfigs gained
  `CONFIG_SCHED_SPORADIC=y`, so `nuttx_sporadic_budget_applied` DOES measure
  KERNEL-ACCEPTED — this one is covered. The Zephyr EDF (W5.5,
  `CONFIG_SCHED_DEADLINE`) and ThreadX preempt-threshold (W5.10) are also
  kernel-accepted. So the gap is specifically the SMP core-pin accept arm.

## Why it matters

A typo or ABI mistake in a compile-only `#ifdef CONFIG_SMP` arm (wrong
`cpu_set_t` usage, wrong affinity-mask shift, wrong `pthread_setaffinity_np`
args) would not be caught until someone builds an SMP image — exactly the
`#131`/hand-mirror class of latent break. The fail-loud e2es prove "never
silently dropped" but NOT "correctly applied when the kernel can".

## Direction

Add ONE SMP fixture that flips a core-pin e2e to the ACCEPT arm (the e2es are
already two-mode — they upgrade automatically):
- cheapest candidate: a Zephyr `native_sim` SMP variant
  (`CONFIG_SMP=y` + `CONFIG_MP_MAX_NUM_CPUS=2` + `CONFIG_SCHED_CPU_MASK_PIN_ONLY=y`)
  as a SEPARATE fixture (do NOT flip the shared realtime image — SMP changes
  the scheduler globally and risks the EDF/delivery cells), OR
- a FreeRTOS SMP build (`configNUMBER_OF_CORES > 1` + `configUSE_CORE_AFFINITY`).
Then point a dedicated `*_core_pin_smp` cell at it and assert the ACCEPT marker
exactly. Until then, the accept arms stay header-compile-verified.
