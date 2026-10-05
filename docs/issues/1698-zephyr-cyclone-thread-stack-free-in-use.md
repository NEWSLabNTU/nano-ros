---
id: 1698
title: "Every Zephyr Cyclone boot logs `tid … is in use!` ×5 — `k_thread_stack_free` refuses stacks whose threads are still live"
status: open
type: bug
area: [zephyr, rmw-cyclonedds]
severity: low
found: 2026-10-05
related: [1674]
---

## Summary

Split from issue 1674 ([archived](archived/1674-zephyr-native-sim-cyclonedds-delivers-nothing-and-floods-select-failed.md)). Every native_sim Cyclone image prints
this five times at boot:

```
<err> os: tid 0x… is in use!
```

The message comes from `z_impl_k_thread_stack_free`
(`zephyr-workspace/zephyr/kernel/dynamic.c`). It returns `-EBUSY` when the
stack belongs to a thread that is neither `_THREAD_DUMMY` nor `_THREAD_DEAD`.
So something frees a dynamic thread stack while that thread is still running
or not yet reaped. The likely candidate is ddsrt's thread create or teardown on
Zephyr freeing before the join has finished. The free is refused, so each one
leaks a stack and corrupts nothing.

It appears with 1674's fix in place, and every Cyclone cell passes (19/19), so
it is not that defect. It is unexplained and costs a stack per occurrence.

## To do

* Find the five call sites. Break on `z_impl_k_thread_stack_free` while `-EBUSY`
  is being returned, and take the backtrace.
* Decide the ordering fix: join before free, or free from the thread's exit path.

## Acceptance

* A Cyclone boot logs no `is in use!`, and a Cyclone e2e cell still passes.
