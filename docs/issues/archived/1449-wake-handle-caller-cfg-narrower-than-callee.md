---
id: 1449
title: "an exported `extern \"C\"` fn was reachable in a build where the method it
  calls was not — `nros_cpp_executor_wake_handle` is `rmw-cffi`, `wake_raw_ptr` is
  `all(alloc, rmw-cffi)`, and every no-alloc board failed to compile"
status: resolved
type: bug
area: [api, build]
found: 2026-09-22
related: [1387, 1431, 1163, 1260, phase-449]
---

# The export and its callee were not the same predicate

```
error[E0599]: no method named `wake_raw_ptr` found for struct `Executor<'s>` in the current scope
error: could not compile `nros-cpp` (lib) due to 1 previous error
  building .../build-cortex-m-c-talker-zenoh
```

| | cfg |
| --- | --- |
| export `nros_cpp_executor_wake_handle` | `#[cfg(feature = "rmw-cffi")]` |
| callee `Executor::wake_raw_ptr` | `#[cfg(all(feature = "alloc", feature = "rmw-cffi"))]` |

The callee is `alloc`-gated because the `node_wake` it reads is an
`Option<portable_atomic_util::Arc<NodeWake>>` — an `Arc`, which cannot exist
without an allocator. So an `rmw-cffi` build with no `alloc` compiled the export
and not the method it calls.

Found by tier 2 after issues 1387 and 1431 cleared the infrastructure in front
of it — the third real code defect the lane surfaced once it could reach the
fixture build at all.

## Why the fix is the BODY, not the export

Measured, the three candidate remedies are not equivalent.

**Narrowing the export is measurably worse.** `nros_cpp_ffi.h:1622` declares
`void *nros_cpp_executor_wake_handle(void *handle);` with no `#if`, and
`nros-board-zephyr/c/zephyr_run_tiers.c:176` declares and calls it unguarded.
Adding `feature = "alloc"` to the export deletes the symbol from exactly the
no-alloc boards that call it, so the failure moves from a compile error naming
a method to a link error naming a symbol.

**Widening the callee is not available.** `node_wake` is `Arc`-based and its
initialiser is already `cfg(all(alloc, rmw-cffi))`; the field genuinely cannot
exist without `alloc`.

**The null return was already the answer.** `wake_raw_ptr` returns
`null_mut()` when `node_wake` is `None`, and the C caller is written for it:

```c
/* Silent no-op when no wake object exists; that build keeps the millisecond
 * `wake_wait_ms` path it already had. */
void* wake = nros_cpp_executor_wake_handle(executor);
if (wake == NULL) { return; }
```

A build with no `alloc` is a third way to reach "this executor has no wake
object", so it gets the same documented answer rather than a new one. The C
side needs no edit and the ABI does not change.

## The gate, and why it is static

`check-cffi-cfg-implication` (`scripts/check-cffi-cfg-implication.py`, fast
line) requires every exported `extern "C"` fn's feature set to imply the
feature set of each `Executor` method it calls, counting the inner
`#[cfg(...)]` blocks in force at the call.

**A third `check-compile-smoke` arm was the obvious widening and it does not
work.** Measured: a host `cargo check` of `nros-cpp` without `std` walks into
`#[panic_handler] function required`, then — with a backend feature added —
`no global memory allocator found`, because on a real board the platform crate
supplies both. Reproducing the embedded shape needs an embedded target and a
platform crate, which is what `check-c` and `rust-rtos-link-check` already do
on schedule. A host gate inventing a configuration nobody builds would be
checking a shape of its own making. The RELATION needs no build.

It decides a cfg that is a pure conjunction of `feature = "..."` (or absent,
the empty requirement) and REFUSES to guess at `any(...)`, `not(...)` or
non-feature predicates, counting those against a ratchet that may only grow
with a stated reason. On this surface the undecidable set is **empty** — 31
pairs, all decided.

### Its first version reported the fix as the defect

The gate read the cfg on the FUNCTION and ignored inner blocks, so the
sanctioned remedy — keep the export, give the body the narrower cfg — tripped
it. A gate blind to the fix it recommends would have pushed the next person to
narrow the export instead, which is the one remedy this issue rules out. Both
directions are now controls: red on the unfixed body, green on the fixed one.
