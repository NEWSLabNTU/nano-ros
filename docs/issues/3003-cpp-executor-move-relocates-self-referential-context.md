---
id: 3003
title: "Moving an rclcpp::Executor byte-copies a self-referential context, so the moved executor reads the zeroed original"
status: open
type: bug
severity: low
area: [cpp, executor]
related: [issue-2845]
found: 2026-11-27
---

Found reviewing PR #4995 (issue 2845). Pre-existing on main; that PR adds
one more field to the set it affects.

## What happens

`rclcpp::Executor` (`packages/api/nros-cpp/include/rclcpp/executor.hpp`)
is declared non-copyable but movable. Its move constructor and move
assignment copy `storage_` byte by byte into the new object and then zero
the source:

```cpp
Executor(Executor&& other) : initialized_(other.initialized_) {
    for (unsigned i = 0; i < sizeof(storage_); ++i) {
        storage_[i] = other.storage_[i];
        other.storage_[i] = 0;
    }
    other.initialized_ = false;
}
```

`storage_` holds the `CppContext` that `nros_cpp_init*` writes in place, and
that struct is self-referential. Its documented contract is "pinned
(caller-owned, initialised in place, only reached through a stable
`*mut CppContext`, never moved)" (`packages/api/nros-cpp/src/lib.rs`, the
`CppContext` doc):

- `executor: &'static mut CppExecutor` points into the same struct's `c`
  (the C executor's inline storage);
- the executor's tables are carved from the struct's own `backing` tail;
- with PR #4995, the executor's owned session lives in the struct's
  `session_slot`, and the executor holds a raw pointer to it.

After a move, every one of those still points into the OLD object, whose
bytes the move just zeroed. The next spin through the moved executor reads
a zeroed executor and a zeroed session: undefined behaviour, in practice a
crash or a silent no-op depending on what the zeroes decode to.

Nothing in the tree moves an `rclcpp::Executor`: `create` and
`create_with_rmw` initialise through an out-parameter, and `rclcpp::init`
uses static storage. But the move operations are public API, and
`std::optional<Executor>`, a `std::vector` growth or returning one by value
would reach them.

`MultiExecutor` (`bridge.hpp`) is not affected: it moves a handle pointer,
and the context behind it stays where it is.

## Fix

Make the type immovable, the way its contract already says it is: delete
the move constructor and move assignment (`= delete`), as the copy
operations already are. A caller that needs to hand an executor around
holds it by pointer or reference, or constructs it in place.

Re-pointing the self-references after a byte move is not a fix: the
executor's arena entries, the C executor's own storage and any session
pointer a sched task has already taken all hold the old addresses.

## Acceptance

- `rclcpp::Executor`'s move constructor and move assignment are deleted.
- A `*-verdict` fixture row (issue 1620's shape) shows that
  `rclcpp::Executor b = std::move(a);` fails to compile.
- Any in-tree code that moved one, if a sweep finds some, holds it by
  reference instead.

## Plan (2026-11-28, for review before code)

**Sweep, done first.** Nothing in the tree needs `rclcpp::Executor` to be
movable:

- `Executor::create` and `create_with_rmw` write through an `Executor&`
  out-parameter, and `rclcpp::init` uses static storage.
- Every C++ TU that names the type (the nros-cpp compile tests, examples,
  templates, the codegen'd entries) constructs it in place.
- No `std::move` of one, no by-value return, and no container or
  `optional` of one.

That matters because the FreeRTOS Cortex-M3, NuttX and ThreadX riscv64
toolchains build with `-std=c++14`. Before C++17, even an elided
`Executor e = f();` needs an accessible move constructor, so deleting it
breaks any such factory. The sweep found none.

**Change.**

1. Declare the move constructor and move assignment `= delete`, beside the
   copy operations. The type becomes pinned, which its contract already
   says it is. Re-pointing the self-references after a byte move is
   rejected, for the reason under Fix above. `MultiExecutor` moves a handle
   pointer and keeps its move operations.
2. Add a negative probe,
   `packages/api/nros-cpp/tests/compile/executor_move_probe.cpp`, which
   writes `rclcpp::Executor b = std::move(a);`. The probe is compiled by
   `just check cpp` at `-std=c++14`, the lowest standard any toolchain
   uses. The check requires the compile to FAIL and requires its stderr to
   name the deleted move constructor. That is the same shape as the
   `ros2_refuse_*` probes, and it means a TU that breaks for some other
   reason proves nothing.

**Diagnostic: plain `= delete`** (review 2026-11-28, option (a) below).

The reviewer's choice was between two diagnostics. The `ros2_refuse_*` probes
carry a "REFUSED by nano-ros" reason through a `static_assert`, and a
deleted function cannot carry one (`= delete("reason")` is C++26). The
two options:

- **(a) plain `= delete`, recommended.** The compiler says "use of deleted
  function `Executor(Executor&&)`", and the doc comment on the deleted
  operations gives the reason. Upstream `rclcpp::Executor` cannot be moved
  either (it holds atomics and mutexes), so a port from upstream never
  relied on a move, and the plain diagnostic is enough.
- **(b) a refusal with a reason.** A constrained template constructor for
  rvalue `Executor`s whose body is `static_assert(detail::refuse<T>::value,
  …)`. Because the deleted, non-template move constructor is the better
  match, (b) also needs the move constructor left undeclared rather than
  deleted. That leaves the type's copy/move story spread across a trick,
  which is a worse trade for a mistake no port makes.

**Acceptance (amended).** Replace "a `*-verdict` fixture row" with the
`just check cpp` negative probe above. nros-cpp's expected-failure compiles
already live there, and a fixture row would build one TU through the whole
fixture stage to assert the same verdict.

**When the probe runs.** `check cpp` is part of `check-build`, which CI
runs on schedule and dispatch only, not on a pull request or the merge
queue. So a change that re-adds a move operation is caught by the nightly
run, not before it merges. That is the same as every other nros-cpp
expected-failure compile, and it is accepted here. Moving the probe to
`check compile-smoke` would make it gate merges, but it would be the only
expected-failure compile there, so it stays with its siblings.
