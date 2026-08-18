---
id: 655
title: "The Zephyr core-pin accept arm can never succeed: it pins `k_current_get()`, and Zephyr rejects a cpu mask on a RUNNING thread — and it was gated on a knob no image sets, so nothing ever compiled it"
status: resolved
type: bug
severity: high
area: boards, testing
related: [issue-0260, rfc-0052, phase-296, phase-356]
resolved_in: phase-356
---

## Two defects, and the second is why the first survived

**1. The call cannot succeed.** `nros_zephyr_thread_cpu_pin` pins the CALLING
thread:

```c
int nros_zephyr_thread_cpu_pin(int cpu) {
    return k_thread_cpu_pin(k_current_get(), cpu);
}
```

Zephyr's implementation (`kernel/cpu_mask.c`) refuses that:

```c
static int cpu_mask_mod(k_tid_t thread, uint32_t enable_mask, uint32_t disable_mask)
{
	K_SPINLOCK(&_sched_spinlock) {
		if (z_is_thread_prevented_from_running(thread)) {
			thread->base.cpu_mask |= enable_mask;
			thread->base.cpu_mask  &= ~disable_mask;
		} else {
			ret = -EINVAL;
		}
	}
```

`k_current_get()` is by definition running, so `k_thread_cpu_pin` returns
**-EINVAL every time**, on any image — SMP or not, PIN_ONLY or not. The
placement dim's Zephyr accept arm is unreachable as written. (Under
`CONFIG_SCHED_CPU_MASK_PIN_ONLY` there is additionally an `__ASSERT` that
"Running threads cannot change CPU pin".)

The API is documented for this: the mask is applied to a thread that has been
created but not started.

**2. It was gated on a knob nothing sets, so it never compiled.** The shim read

```c
#ifdef CONFIG_SCHED_CPU_MASK_PIN_ONLY
```

but `k_thread_cpu_pin` is declared under `#ifdef CONFIG_SCHED_CPU_MASK`
(`include/zephyr/kernel.h:886`). `..._PIN_ONLY` is a strictly narrower variant
(`depends on SMP && SCHED_CPU_MASK`). **No config in this tree sets either
knob** — `git grep CPU_MASK_PIN_ONLY` returns only comments and the `#ifdef`
itself, and the one image with `CONFIG_SMP=y` (`fvp-aemv8r-smp`, license-gated
FVP) does not set it.

So the real call was preprocessed out of **every** image nano-ros builds. The
`#else` returned a synthetic `-ENOSYS`, the consumer logged the honest fallback
note, and the e2e cell — which asserted "accept marker OR fallback note" —
passed. Nothing at any layer could observe that the code inside the `#ifdef`
had never been compiled, let alone run.

This is exactly the failure [issue 0260](0260-native-dim-kernel-accept-never-exercised.md)
predicted, and worse than it recorded. #260 says the accept arms are
"COMPILE-VERIFIED ONLY (against headers)". They are not compile-verified at
all. A mistake inside them — this one is an API-contract mistake, not a typo —
could not be caught by any build in the tree.

## Measured

`CONFIG_SCHED_CPU_MASK` does not require SMP; its own Kconfig help says it
"does not technically depend on SMP and is implemented without it for testing
purposes". So the gate could be corrected and the knob enabled on the existing
uniprocessor `native_sim` realtime fixture, which is what produced the first
real return value this call has ever yielded:

```
<wrn> rust: nros_board_zephyr::entry_tiers: nros: core pin FAILED tier=`low`
      cpu=0 rc=-22 (…) — tier runs unpinned
```

**-22 is `-EINVAL`, from the kernel.** Before this change the same line read
`rc=-88` (`-ENOSYS`) — a number the shim invented for a branch that did not
exist. The dim's behaviour did not change; what changed is that the failure is
now the real one.

## Landed with this issue (not the fix — the diagnosis)

* **The gate is now `CONFIG_SCHED_CPU_MASK`**, the knob the API actually needs.
* **`CONFIG_SCHED_CPU_MASK=y` (+ `CONFIG_SCHED_DUMB=y`, its dependency)** on
  `realtime-rust/src/zephyr_entry`, so the call is compiled somewhere for the
  first time. Verified not to disturb the shared image: `realtime_tiers_e2e`
  passes and the EDF cell still reports ACCEPT.
* **The fallback note no longer misnames its own cause.** It said
  "CONFIG_SCHED_CPU_MASK_PIN_ONLY off, or invalid cpu"; neither is why, and a
  note that misdiagnoses sends the next reader to the Kconfig instead of the
  call site. Both the Rust and C arms updated in lockstep.

## Direction — the actual fix

Pin between creation and start, which is what the API is for. Zephyr's tier
spawn path uses `k_thread_create`; a thread created with a `K_FOREVER` start
delay is "prevented from running", so:

```
k_thread_create(..., K_FOREVER)   →   k_thread_cpu_pin(tid, cpu)   →   k_thread_start(tid)
```

That reaches the accept arm for SPAWNED tiers. The **boot** tier is a harder
case and may be unfixable as posed: it is already running when
`run_tiers` sees it, so its declared `core` cannot be honored by this API at
all. That is a real limitation and should be stated as one (fail loud, saying
why) rather than papered over.

**Do not close on the change alone.** The accept arm still cannot be OBSERVED
succeeding without an SMP image — on a uniprocessor build, pinning to cpu 0 is
accepted by the mask API but proves nothing about multi-core behaviour. #260
owns that fixture; this issue owns the call being correct.

## Scope

Zephyr only. The NuttX / FreeRTOS / ThreadX core-pin arms have the same
"never compiled" property (`CONFIG_SMP`, `configUSE_CORE_AFFINITY`,
`TX_THREAD_SMP` are set by no config here) but their APIs are different and
each needs its own read — do not assume they share this bug, and do not assume
they don't.

---

## Fixed 2026-08-18 — pin in the create→start window, and say so when there isn't one

The API wants a thread that has not started, so the board now creates one that
way. `nros_zephyr_tier_task_create` gained `core_plus1` + `pin_rc`: a tier that
declares a `core` is created with `K_FOREVER`, pinned by tid, then started.
A tier with no `core` keeps `K_NO_WAIT` verbatim, so the common path is
unchanged and nothing pays a start-up round trip for a knob it does not use.

* **`nros_zephyr_thread_cpu_pin_tid(tid, cpu)`** is the new by-tid entry point.
  The CALLING-thread variant stays, deliberately separate: the two have
  genuinely different preconditions, and collapsing them is what hid this bug.
* **The self-pin is gone from both spawned-tier entries** (Rust
  `tier_task_entry`, C `zephyr_tier_task`). It ran on a started thread and
  could only ever log `-EINVAL` — over a pin that had by then already
  succeeded.
* **The boot tier keeps a self-pin and reports the limitation.** Zephyr starts
  it before `run_tiers` exists, so it has no create→start window and its
  `core` cannot be honored by this API at all. `apply_boot_tier_core_pin` /
  `zephyr_apply_boot_core_pin` name that, and name the remedy (declare the
  `core` on a spawned tier) rather than blaming a Kconfig.
* **One marker spelling per arm**, shared by the spawn and boot paths in each
  language (`report_core_pin`, `zephyr_report_core_pin`), still mirroring
  `nros_tests::output::ZEPHYR_CORE_PIN_*`.

### The fixture declared its `core` on the one tier that cannot take it

`[tiers.low.zephyr] core = 0`, and `low` is the BOOT tier — so even a correct
implementation would have produced the fallback. Moved to
`[tiers.high.zephyr]`, which is spawned. That is this issue's own Direction
applied to the fixture, and it is what makes the accept arm reachable.

### Verified on the wire

Before (gate corrected, caller not yet):

```
<wrn> nros: core pin FAILED tier=`low` cpu=0 rc=-22 — this is the BOOT tier …
```

After:

```
<inf> nros: core pin tier=`high` cpu=0
```

No fallback line. `sched_dims_applied_e2e` reports
`sched-dim arm: [zephyr rust CorePin] ACCEPT`, and its cell's declared arm was
flipped `Fallback -> Accept` — the deliberate edit phase-356 W3's `expect:`
field exists to force. The arm changed because the bug was fixed, not because
an assert was loosened.

### What this does NOT prove

The image is still **uniprocessor**. Pinning to cpu 0 on a one-CPU system
exercises the mask API correctly but says nothing about multi-core placement.
[issue 0260](0260-native-dim-kernel-accept-never-exercised.md)'s SMP-fixture
item stands, and now has a working call to point at instead of one that would
have failed on arrival.

The NuttX / FreeRTOS / ThreadX core-pin arms remain NEVER COMPILED (their
`CONFIG_SMP` / `configUSE_CORE_AFFINITY` / `TX_THREAD_SMP` gates are set by no
config here). Whether any of them shares this bug is unread — the APIs differ,
so it must be checked, not assumed. Tracked on #260.
