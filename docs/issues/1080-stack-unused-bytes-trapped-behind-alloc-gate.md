---
id: 1080
title: "`stack_unused_bytes` allocates nothing but lives in an `alloc`-gated module, so the new stack-headroom rule breaks every `no_std`-without-`alloc` image"
status: open
type: bug
area: platform, core
severity: high
found: 2026-09-05
related: [1075, 0589, phase-359]
---

# A gate that belongs to the neighbours

## Symptom

Every Zephyr cortex-m fixture fails to compile. Found by a tier-2 fixture build
on 2026-09-05, at `build-cortex-m-c-talker-zenoh`:

```
error[E0433]: cannot find `task` in `nros_platform_api`
    --> packages/core/nros-node/src/executor/spin.rs:2415:41
     |
2415 |         let unused = nros_platform_api::task::stack_unused_bytes();
     |                                         ^^^^ could not find `task` in `nros_platform_api`
     |
note: found an item that was configured out
    --> packages/platform/nros-platform-api/src/lib.rs:65:9
     |
  64 | #[cfg(feature = "alloc")]
     |       ----------------- the item is gated behind the `alloc` feature
```

It surfaces through the `nros-c` size probe, which runs a nested cargo build, so
the first line a reader sees is *"size probe could not locate the `nros` rlib"* —
several frames from the cause.

## Cause

`cb2be0ca4` (*"feat(sched): report a stack that has come too close to its end"*)
added `check_stack_headroom_rule` to the executor, calling
`nros_platform_api::task::stack_unused_bytes()` unconditionally.

`pub mod task` is `#[cfg(feature = "alloc")]`
(`nros-platform-api/src/lib.rs:64-65`) — and correctly so for most of it:
`PlatformTask` allocates its own stack and join slot
(`alloc::alloc::alloc`, `task.rs:128-162`).

**But `stack_unused_bytes` allocates nothing.** It is one `extern "C"` call
returning a `usize`:

```rust
pub fn stack_unused_bytes() -> usize {
    unsafe { nros_platform_task_stack_unused_bytes() }
}
```

So the gate is about its NEIGHBOURS, not about it. That cost nothing until
something outside `alloc` wanted the function — and then every `no_std` image
without an allocator stopped compiling.

## Why guarding the CALL SITE would be the wrong fix

The obvious patch is `#[cfg(feature = "alloc")]` on
`check_stack_headroom_rule`'s body. That builds, and it is wrong.

The rule is a SAFETY feature: it reports a spin thread that has come closer to
the end of its stack than its declared minimum. The targets it would be silently
removed from — `no_std`, no allocator, small embedded — are exactly the ones
where a stack overflow is how one component corrupts another's state, and where
nobody is watching a console. **A gate that quietly deletes a safety check on
the platforms it exists for is worse than the build error that revealed it.**

## Fix

`stack_unused_bytes` moves to a new `crate::stack` module that is NOT
alloc-gated; `crate::task` re-exports it, so every existing
`nros_platform_api::task::stack_unused_bytes` caller is unchanged. The function
lands where its own requirements put it rather than where its neighbours did.

## The family

Second regression from the same stack-instrumentation work in one day, and the
two fail at different phases of the build:

| commit | what it added | how it broke |
| --- | --- | --- |
| `411addfd2` | the platform probe | **link** — `undefined reference to z_impl_k_thread_stack_space_get`, issue 1075 |
| `cb2be0ca4` | the executor rule that calls it | **compile** — this issue |

Both are invisible to the compile tiers, which build the host configuration
where `alloc` is on and the Zephyr syscall is irrelevant. Only a fixture build
reaches either.

## Not covered

* `packages/core/nros-node/src/executor/os_priority.rs:61` carries a bare
  `use nros_platform_api::task::PlatformTask;` and its module is gated on
  `#[cfg(any(has_rmw, test))]`, not on `alloc`. That import genuinely needs
  `alloc`, so it is a different question from this one — but whether a
  `has_rmw` build without `alloc` is reachable was not established.
* Whether any other caller reaches an alloc-gated item from a non-alloc path.
  Not swept.
