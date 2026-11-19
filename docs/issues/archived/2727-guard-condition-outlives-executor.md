---
id: 2727
title: "A guard condition handle could outlive its executor, and `trigger` then used freed memory, from safe code"
status: resolved
type: bug
severity: high
area: [executor, soundness]
found: 2026-11-19
resolved: 2026-11-19
resolved_in: "fix(nros-node, issue 2727): a guard condition borrows its executor's backing and owns its wake state"
related: [2675, phase-573]
---

## What happened

`Executor::register_guard_condition` returned a `GuardCondition` that safe
code could keep for as long as it liked. The handle held two references into
its executor, and the type system knew about neither:

- **The arena flag**, as `&'static AtomicBool`. The flag lives in the
  executor's arena, which is the caller's backing borrowed for the
  executor's `'s`. `GuardCondition::new` asserted `'static` from a raw
  pointer, so a handle could outlive a stack backing.
- **The wake state**, as a raw `*mut c_void` to the executor's `WakeCtx`.
  The executor owns that `WakeCtx`'s only `Arc` and frees it in
  `Executor::drop`. A `trigger` (or `trigger_from_isr`) after `drop(executor)`
  passed the freed pointer to `nros_rmw_runtime_wake_cb`, which read it.

The second one needs no unusual backing: every `Executor::open` image has it.
A handle moved to a worker thread, or kept in a struct that outlives the
executor, is enough.

## Evidence

A probe of the old shape: a handle wired as `register_guard_condition_on`
wires it, the executor's `Arc` dropped, then `trigger()`. Built with
`-Zsanitizer=address` on aarch64 Linux (nightly-2026-04-11):

```
ERROR: AddressSanitizer: heap-use-after-free
READ of size 8 ... in nros_node::executor::spin::nros_rmw_runtime_wake_cb spin.rs:1421
freed by thread T1 here: ... drop<portable_atomic_util::arc::Arc<WakeCtx>>
```

No lane runs AddressSanitizer, so nothing would have reported this.

## Resolution

Both references are now owned or borrowed in the type:

- **`GuardCondition<'s>`.** The handle borrows the executor's backing for the
  executor's own lifetime parameter. `register_guard_condition{,_on}` and
  `Node::create_guard_condition` return `GuardCondition<'s>`. A handle that
  outlives the backing no longer compiles. For an `Executor<'static>`, which
  is every RTOS image and every C and C++ executor, nothing changes.
- **The handle owns an `Arc<WakeCtx>` clone** (with `alloc` + `rmw-cffi`; a
  build with no wake callback holds a zero-size marker). `trigger` after the
  executor is gone reaches live state and wakes nobody. The last handle
  releases it.
  - `Option<Arc<_>>` is one pointer, as the raw pointer was, so the C and C++
    opaque storage sized from this type does not change.
  - Both FFIs already `drop_in_place` the handle on destroy, so the count is
    released there too.

Tests, each failing without its half of the fix:

- `executor::spin::guard_wake_ctx_tests::a_guard_condition_keeps_the_wake_state_alive_past_its_executor`:
  - A `Weak<WakeCtx>` is taken before the executor's `Arc` drops.
  - It must still `upgrade()` afterwards.
  - The trigger must reach the state.
  - The state must be released with the last handle.
  - Mutation: keep the address only, as before. The test fails at "the
    handle holds the wake state past its executor". It runs in `test-unit`.
- **`GuardCondition`'s `compile_fail` doctest**:
  - The doctest is a handle kept past the block that holds its executor's
    backing. Its twin is the same code with the handle used inside the block,
    and it compiles, so the refusal is the lifetime's.
  - Both doctests link the host port (`nros_platform_cffi`). Without it, the
    `compile_fail` passed vacuously on a LINK error.
  - Mutation: `register_guard_condition_on` returning `GuardCondition<'static>`.
    The `compile_fail` doctest then reports "Test compiled successfully".
  - `just check node-std-tests` runs the pair and counts both, because no
    other lane runs a doctest.

AddressSanitizer on the fixed branch, same toolchain: the runtime test is
clean.
