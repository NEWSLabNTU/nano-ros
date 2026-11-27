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
