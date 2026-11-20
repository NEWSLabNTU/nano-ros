---
id: 2728
title: "No merge-gating lane runs AddressSanitizer, so a use-after-free that only ASan sees cannot fail CI"
status: resolved
type: tech-debt
severity: medium
area: [testing, ci, soundness]
found: 2026-11-19
resolved: 2026-11-20
resolved_in: "ci: an AddressSanitizer lane over the executor wake path (issue 2728)"
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

[Issue 2727](2727-guard-condition-outlives-executor.md). A
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

## Resolution (2026-11-20)

`just check address-sanitizer` runs two scopes instrumented with
`-Zsanitizer=address`, on the pinned nightly, an explicit host `--target`
and its own target dir (`target/asan`), with `detect_leaks=0`:

- nros-tests `guard_condition_outlives_executor` (`--features trigger-test`),
  over a stub cffi backend: no router, no fixture;
- nros-node `--lib --features alloc,rmw-cffi`, the scope proposed above:
  361 tests.

A run that tests nothing (a renamed test, a cfg turned off) fails rather than
passing on zero tests.

**Measured.**
- Red: at the commit before the guard fix, with a probe of the old shape (the
  guard registered, the executor dropped, then `trigger`), the lane reports
  `heap-use-after-free`. It is a READ of size 8 in `WakeCtx::signal_own`,
  reached from the probe through `nros_rmw_runtime_wake_cb`; the memory was
  freed by `drop_in_place::<Executor>` dropping its `Arc<WakeCtx>`.
- Green on main: both scopes pass (1 + 361 tests).
- Cost: 28 s for both scopes from an empty `target/asan` on the 32-core dev
  host (16 s + 11 s of build), 2.5 GB of target dir.

**Where it runs.** host-tests.yml's daily `asan` job, beside the Miri job,
on the CI image with the pinned nightly installed. The recipe is tier
`exempt` because no merge-gating lane builds with the nightly. Moving it into
`node-std-tests` would put a nightly into that lane, and that is a separate
decision.

**Absence is caught.** Rule R4 of `check-default-gates-run-somewhere` covers
exempt gates. Mutation-checked: with the workflow's `just check
address-sanitizer` renamed, the gate fails, naming the recipe as in no lane.
