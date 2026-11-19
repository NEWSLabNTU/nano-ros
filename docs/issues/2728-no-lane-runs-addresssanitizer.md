---
id: 2728
title: "No merge-gating lane runs AddressSanitizer, so a use-after-free that only ASan sees cannot fail CI"
status: open
type: tech-debt
severity: medium
area: [testing, ci, soundness]
found: 2026-11-19
related: [2727, phase-573]
---

> nros 133 reserved this id. Its host went down before the file was written,
> so another session wrote it from the agreed content on 2026-11-19.

## What is missing

No `just` recipe or workflow passes `-Zsanitizer=address`. The one
memory-model lane is `just check miri-executor-reentry`, a filtered Miri run
of nros-node's re-entry tests.

So a soundness test whose failure is a read of freed memory, with no
assertion on the result, passes in every lane we have: the freed memory
still holds the old bytes, the read returns them, and the test sees the
expected value.

## The example

[Issue 2727](archived/2727-guard-condition-outlives-executor.md). A
`GuardCondition` triggered after its executor dropped passed a freed
`WakeCtx` pointer to `nros_rmw_runtime_wake_cb`. A probe of that shape
asserts nothing; the defect is the read itself. Under AddressSanitizer
(aarch64 Linux, nightly-2026-04-11):

```
ERROR: AddressSanitizer: heap-use-after-free
READ of size 8 ... in nros_node::executor::spin::nros_rmw_runtime_wake_cb spin.rs:1421
freed by thread T1 here: ... drop<portable_atomic_util::arc::Arc<WakeCtx>>
```

2727's regression test does not depend on ASan: it asserts through a
`Weak<WakeCtx>`, which fails in an ordinary run. That was a choice made
BECAUSE no lane runs ASan. The next fix of this class may not have an
observable like that.

## Proposed lane (smallest that covers the class)

```
RUSTFLAGS=-Zsanitizer=address CARGO_TARGET_DIR=<own dir> \
  cargo +<pinned nightly> test -p nros-node --lib \
  --features alloc,rmw-cffi --target <host triple>
```

- **Scope:** the nros-node lib tests with `rmw-cffi`. That covers the
  executor's FFI wake, guard-condition and entity-wake paths, where the C
  callbacks hold raw context pointers.
- **Toolchain:** the repo's pinned nightly from `tools/rust-toolchain.toml`,
  never an unpinned one.
- **Target:** an explicit host `--target`, so build scripts and proc macros
  are not instrumented.
- **Target dir:** its own, so instrumented artifacts never mix with a gate's.
- **Measured:** the 2727 tests under ASan, on the target dir that build
  created. The cold build of the whole set is not measured yet.

Then decide where it runs:
- in `node-std-tests` (merge-gating) if the cold build is cheap enough;
- otherwise on the nightly.

Acceptance:
- the lane runs on some scheduled or gating event;
- reverting 2727's `Arc<WakeCtx>` ownership while keeping a probe of the old
  shape makes it red;
- its absence from a workflow is caught by `check-default-gates-run-somewhere`.
