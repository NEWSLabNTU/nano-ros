---
id: 260
title: "Native sched dims (core-pin, sporadic budget) are e2e-verified only on the FALLBACK arm — no fixture exercises the kernel-ACCEPT path"
status: resolved
type: limitation
area: testing
related: [phase-296, issue-0259]
---

## Update (phase-296 W5.13, 2026-07-24) — placement ACCEPT arm now runtime-proven on POSIX

Added a POSIX core-pin consumer (`nros-board-linux::apply_tier_affinity` via
`sched_setaffinity`) — a Linux host is genuinely multi-core and the call is
unprivileged, so `posix_core_pin_applied_at_runtime` (ws-realtime-rust `high`
tier `posix.core: 0`, native cell) measures **KERNEL-ACCEPTED**. This is the
first runtime accept-arm proof of the placement dim's consumer behavior.

RESIDUAL (issue stays open, narrower): the RTOS-specific accept arms remain
COMPILE-ONLY — each is guarded by its own `#ifdef` (`CONFIG_SCHED_CPU_MASK_
PIN_ONLY` / `CONFIG_SMP` / `configUSE_CORE_AFFINITY` / `TX_THREAD_SMP`) that no
uniprocessor fixture compiles. A typo INSIDE one of those RTOS arms would still
escape until an SMP image builds; the shared consumer PATTERN is now
runtime-exercised on posix, but the per-RTOS SMP branches are not. The sporadic
(NuttX W5.9b) / EDF (zephyr W5.5) / preempt-threshold (threadx W5.10) accept
arms were already kernel-accepted. So the remaining gap is purely the SMP
core-pin branches of the four RTOS boards.

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

## Update (phase-356 W3, 2026-08-17) — the arm is now DECLARED and ASSERTED, not merely tolerated

The `AcceptOrFallback` shape in `sched_dims_applied_e2e.rs` asserted that the
accept marker **or** the fallback note appeared. That passes identically on
either arm — which is the mechanism by which this issue stayed invisible, and
is worse than it sounds: it means a dim whose accept path we *do* exercise
could silently regress to the fallback and stay green.

Each two-mode cell now declares the arm its image is known to take
(`AcceptOrFallback { expect: Arm }`), and a mismatch in EITHER direction fails.
So a fixture that loses a capability is caught, and a fixture that silently
gains one is caught too — the accept path cannot start being exercised without
someone noticing, which is how this issue should eventually be closed.

Every cell also prints its arm, so the landscape is machine-produced rather
than re-derived from `#ifdef`s and defconfigs. Full run, 12/12 cells, no skips:

```
sched-dim arm: [zephyr rust CorePin]              FALLBACK
sched-dim arm: [nuttx rust CorePin]               FALLBACK
sched-dim arm: [threadx-linux rust CorePin]       FALLBACK
sched-dim arm: [freertos cpp CorePin]             FALLBACK
sched-dim arm: [posix rust CorePin]               ACCEPT
sched-dim arm: [zephyr rust EdfDeadline]          ACCEPT
sched-dim arm: [zephyr cpp EdfDeadline]           ACCEPT
sched-dim arm: [zephyr c EdfDeadline]             ACCEPT
sched-dim arm: [nuttx cpp SporadicBudget]         ACCEPT
sched-dim arm: [nuttx rust TierPriority]          2/2 tiers ACCEPT, 0 FALLBACK
sched-dim arm: [threadx-linux rust PreemptThreshold] ACCEPT
sched-dim arm: [threadx-linux rust TimeSlice]     ACCEPT
```

That confirms the residual stated above, by measurement rather than by reading
the consumers: **the only fallback arms in the tree are the four RTOS
core-pins.** Every other Native dim is kernel-accepted somewhere.

The one non-obvious declaration was `SporadicBudget = Accept`. It was
verified by building the fixture and running it, not taken from this issue's
own prose — and it is the case that gained the most: that cell previously
tolerated a regression to the fallback arm while this issue recorded the dim
as covered.

### The obstacle is SMP, not privilege

[phase-356](../roadmap/phase-356-test-evidence-and-measurement-trust.md) W3
recorded this item as blocked on [phase-162](../roadmap/phase-162-rt-scheduling-harness.md)
because "accepting these dims needs capabilities a normal test host does not
have". That is not what this issue says, and it is not true of what remains:

* the sporadic / EDF / preempt-threshold accept arms were already kernel
  accepted, so no privilege was ever needed for them;
* `sched_setaffinity` is unprivileged, which is exactly why the posix core-pin
  accept arm could be added at all (W5.13);
* what the four remaining arms need is an image with **more than one CPU** —
  `CONFIG_SMP` / `configUSE_CORE_AFFINITY` / `TX_THREAD_SMP` — which is a
  fixture-configuration question, not a capability one.

So the Direction above stands unchanged and is NOT blocked: add one SMP
fixture and point a dedicated cell at it.

### Correction (2026-08-17): the accept arms are not "compile-verified", they are NOT COMPILED

This issue records the RTOS core-pin accept arms as "COMPILE-VERIFIED ONLY
(against headers), never run". That is too generous, and the difference
matters because it is the whole basis of the "Why it matters" section above.

Every one of the four arms is behind a preprocessor gate, and **no config in
this tree sets any of those gates**:

| board | gate | set anywhere? |
| --- | --- | --- |
| Zephyr | `CONFIG_SCHED_CPU_MASK_PIN_ONLY` | **no** — comments and the `#ifdef` only |
| NuttX | `CONFIG_SMP` | **no** NuttX defconfig sets it |
| ThreadX | `TX_THREAD_SMP` | **no** |
| FreeRTOS | `configUSE_CORE_AFFINITY` | **no** |

(The single `CONFIG_SMP=y` in the tree is the license-gated `fvp-aemv8r-smp`
Zephyr board, which does not set the Zephyr pin knob either.)

So the bodies are deleted by the preprocessor in every image. They are not
type-checked, not linked, not run. "A typo would not be caught until someone
builds an SMP image" is right — and nobody ever builds one, on any of the four.

Acting on that immediately found a real defect, which is the argument for the
compile gate being worth having on its own:
**[issue 0655](0655-zephyr-core-pin-cannot-succeed-on-running-thread.md)** —
the Zephyr arm pins `k_current_get()`, and Zephyr's `cpu_mask_mod` rejects a
RUNNING thread, so that arm returns `-EINVAL` unconditionally and could never
have worked even on a correct SMP image. Correcting the gate to the knob the
API actually needs (`CONFIG_SCHED_CPU_MASK`, which does NOT require SMP) and
enabling it on the existing uniprocessor fixture made the call compile for the
first time and produced `rc=-22` where the never-compiled `#else` had been
inventing `-88`.

**This narrows the Direction.** Two separable pieces, and the cheap one is not
the fixture:

1. **Make each arm COMPILE somewhere** — cheap, needs no SMP, and catches the
   API-misuse class this issue exists to worry about. Done for Zephyr; NuttX,
   ThreadX and FreeRTOS still have never-compiled arms and each needs its own
   read (do not assume they share 0655's bug, and do not assume they don't).
2. **Make one arm RUN and be observed accepting** — the SMP fixture. Still
   wanted, still the only thing that proves multi-core behaviour, and now known
   to need a REAL SMP board: Zephyr's `native_sim` cannot do it (the POSIX arch
   has no SMP support at all, so the "cheapest candidate" named in the
   Direction above is not viable). The viable Zephyr targets are
   `qemu_cortex_a53_smp` / `qemu_riscv64_smp` — a new board bring-up, not a
   conf tweak. FreeRTOS SMP needs a multi-core port; `mps2-an385` is a
   single-core Cortex-M3.

### Update (2026-08-18) — the Zephyr arm now ACCEPTS, and it is one arm, not four

[issue 0655](archived/0655-zephyr-core-pin-cannot-succeed-on-running-thread.md)
is fixed: the board pins a spawned tier between `k_thread_create` and
`k_thread_start`, which is the only window Zephyr accepts a cpu mask in, and
the realtime fixture moved its `core` off the boot tier (which has no such
window) onto a spawned one. `sched_dims_applied_e2e` now reports

```
sched-dim arm: [zephyr rust CorePin] ACCEPT
```

so the placement dim has a SECOND runtime accept-arm proof beside posix.

**This does not close this issue, and the reason matters.** The image is
uniprocessor: pinning to cpu 0 on a one-CPU system proves the API is CALLED
correctly, not that multi-core placement works. The gap this issue names —
"no fixture exercises the SMP accept arm" — is untouched by it.

What it does change is the shape of the remaining work:

* **Zephyr's arm is now correct AND compiled**, so an SMP fixture built later
  would exercise working code rather than discovering 0655 on arrival.
* **The other three arms are still NEVER COMPILED** (NuttX `CONFIG_SMP`,
  FreeRTOS `configUSE_CORE_AFFINITY`, ThreadX `TX_THREAD_SMP` — set by no
  config here). 0655 was found by making ONE of them compile; the same move on
  the other three is the cheap next step, and whether any shares 0655's bug is
  unread. Their APIs differ, so it must be checked rather than assumed.

## Resolution (2026-08-21) — both pieces of the narrowed Direction are done

### Piece 1 — every arm compiles

`just check-sched-dim-arms` (`scripts/check-sched-dim-arms-compile.sh`,
phase-356 W3) type-checks each RTOS call site against that RTOS's own vendored
headers under a synthetic SMP config, because the arms sit behind macros no
image defines:

```
freertos core-pin arm (vTaskCoreAffinitySet)       OK
nuttx core-pin arm (pthread_setaffinity_np)        OK
threadx core-pin arm (tx_thread_smp_core_exclude)  OK
```

Zephyr's fourth arm compiles for real (issue 0655's fix put it on the
`k_thread_create` → `k_thread_start` window). So the "a typo in these bodies is
invisible" hazard — the one this issue exists to name — is closed on all four.

The justfile comment above that recipe used to say nuttx and threadx were "not
yet" covered and told readers to trust the script's own output. The script has
printed all three sections since it landed; the comment described a state that
never shipped and has been corrected.

### Piece 2 — one arm RUNS on a real SMP image and is observed

The Direction asked for "one SMP fixture and a dedicated cell pointed at it".
Both exist:

* fixture `workspace-zephyr-c-realtime-smp`, board
  `qemu_cortex_a53/qemu_cortex_a53/smp`, **2 CPUs**, `core = 1` on the spawned
  `high` tier;
* matrix cell `sched(CorePinPlacement, ZephyrNativeSim, C, Runtime)`, consumed
  by `sched_dims_applied_e2e`.

Measured 2026-08-21:

```
sched-dim arm: [zephyr c CorePinPlacement] ACCEPT
sched_dims: 14 cell(s) ran, 8 skipped, 0 out of lane
```

Two details make that a real proof rather than a green box. The cell asserts
PLACEMENT, not acceptance — `nros: core pin observed tier=`high` running_on=1`,
i.e. the tier reports the CPU it actually ran on, so a kernel that accepted the
mask and then ignored it still fails. And it matches the EXACT line rather than
the marker prefix, because the prefix also matches `running_on=0`, which is
where an unpinned tier lands anyway. Its shape is `AcceptOnly` with no fallback
arm: on an image that cannot answer, the board prints nothing and the cell fails.

### Residual, and why it is not this issue

NuttX, FreeRTOS and ThreadX core-pin arms still have never RUN — their images
are uniprocessor and the eight cells that need them skip on this host. Making
them run is not a conf tweak but three multi-core board bring-ups (FreeRTOS
needs an SMP port at all; `mps2-an385` is single-core Cortex-M3). That is
board-enablement scope, and each should be a fixture axis on its own board when
that board arrives.

What this issue asserted — "Native sched dims are e2e-verified only on the
FALLBACK arm; no fixture exercises the kernel-ACCEPT path" — is no longer true
in any part: every Native dim has a kernel-accepted arm somewhere, the four
core-pin bodies all compile, and multi-core placement is observed on hardware
that has more than one core. Resolved.
