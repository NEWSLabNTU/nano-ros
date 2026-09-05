---
id: 1076
title: "The stack-headroom rule left two reds on main — `check-build` cannot compile no-heap, and `check-api-parity` has an unledgered symbol"
status: resolved
type: bug
area: build, core
severity: medium
found: 2026-09-05
related: [0952, 0996, 1050, 0455, phase-424]
---

# A red nothing merge-gating could see

`just check build` fails two gates on `main` — `check-workspace` (its
`workspace-embedded` arm) and `check-workspace-features` — with one cause:

```
error[E0433]: cannot find `task` in `nros_platform_api`
    --> packages/core/nros-node/src/executor/spin.rs:2415:41
     |
2415 |         let unused = nros_platform_api::task::stack_unused_bytes();
     |
note: found an item that was configured out
    --> packages/platform/nros-platform-api/src/lib.rs:65:9
  64 | #[cfg(feature = "alloc")]
  65 | pub mod task;
```

Reproduce in ~20 s, no fixtures:

```
$ cargo clippy -p nros --no-default-features --features rmw-cffi
```

## Root cause

`cb2be0ca4` (2026-09-04, *"feat(sched): report a stack that has come too close
to its end"*, PR #455) added `Executor::check_stack_headroom_rule`, which calls
`nros_platform_api::task::stack_unused_bytes()` **ungated**. That module is
`#[cfg(feature = "alloc")]` as a whole.

The gate is not wrong about the module — `PlatformTask` owns a heap allocation
sized by the port's own storage probe. It is wrong about `stack_unused_bytes`,
which is a bare `extern "C" fn() -> usize` and allocates nothing. Gating a
number-returning query along with the allocator made a no-heap image unable to
compile a rule that only reads it.

## Why it survived

`check-build` runs on `schedule` and `workflow_dispatch` only — never on
`pull_request` or `merge_group` (CLAUDE.md, and issue 0952's lane note). It is
also the only place the no-default-features workspace combination is compiled.
So the merge queue could not observe this, and the local tier that would have
(`just ci gate`, step 3 of 6) is the one contributors run before pushing rather
than one CI runs per PR.

This is the same shape as issue 0996 one lane over: a lane that cannot see a
class of breakage, and a break in exactly that class.

## The second red, same commit

`just check api-parity` also fails, and it is the same `cb2be0ca4`:

```
+ Executor::set_min_stack_headroom_bytes    UNLEDGERED   ours-only
```

`docs/reference/api-parity-ledger/` is AUTHORED — a symbol we have and rclrs
does not must carry a written verdict, which is the whole mechanism (CLAUDE.md:
"the parity MAP is AUTHORED, so it drifts when slots move"). The new setter had
none.

Ledgered as an `extension` in `exec.json`, beside `rust:Executor::*violation*`,
which is the same family: a monitor rule with no upstream analogue. The reason
it is a SETTER rather than a query is worth recording and is now recorded — only
the party that spawned the thread knows what stack it handed over.

Two gates, one commit, both invisible to the merge queue: `check-api-parity` is
step 4 of `just ci gate` and is not in the required `CI` context either.

## Fix

`pub mod task;` becomes unconditional; the `alloc`-using items inside it —
`PlatformTask`, its `impl`, and its `Drop` — carry the gate instead. Nothing
about the heap path changes: `-p nros-platform-api --features alloc` and
`-p nros-node --features std,alloc --all-targets` are unaffected.

Verified both directions:

```
$ cargo clippy -p nros --no-default-features --features rmw-cffi     # was E0433
$ cargo clippy -p nros-platform-api --features alloc                 # unchanged
$ cargo clippy -p nros-node --features std,alloc --all-targets       # unchanged
```

## Not done

**No new gate.** The rule that would have caught this already exists and already
covers the class — it just runs on an event nobody watches. Adding a second
compile of the same combination somewhere cheaper is a real proposal, and it is
issue 0996's subject, not this one's. Filed here so the next person reading a red
`check-build` does not re-derive the cause.
