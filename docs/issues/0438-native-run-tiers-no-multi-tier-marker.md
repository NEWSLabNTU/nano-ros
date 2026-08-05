---
id: 438
title: "native_orchestration_tiers greps a `multi-tier` marker only the NuttX board emits"
status: open
type: bug
area: testing
related: [issue-0422, issue-0429, phase-302]
---

## Symptom

Both `native_orchestration_tiers` tests fail on freshly built fixtures:

```
multi_tier_binary_boots_into_run_tiers
  binary did not reach the run_tiers boot path (no multi-tier marker).
  output:
  nros: Executor::open failed (Transport(ConnectionFailed)); proceeding with
        NullNodeRuntime — `run_plan` register calls will fail loud.
  nros: application error: NodeRegister("ctrl_pkg")

multi_tier_binary_runs_both_tiers_with_router
  binary did not enter the per-tier run with a live session.
```

## Cause

The test's own comment states the contract:

> the binary emits a `multi-tier` marker either way — `multi-tier entry needs a
> live session` (no router → abort) or `multi-tier run — N tier(s)` (a router
> was reachable) … The single-tier path emits NEITHER, so the `multi-tier`
> substring is the branch-specific signal.

That marker exists in exactly one place:

```
$ git grep -n "needs a live session" -- packages/
packages/boards/nros-board-nuttx/src/lib.rs:490
packages/testing/nros-tests/tests/native_orchestration_tiers.rs:93
```

The NuttX board, and the test asserting on it. The **native/linux** board's
equivalent path (`nros-board-linux/src/lib.rs:334-341`) prints the generic
fallback instead:

```rust
Err(err) => {
    <Self as BoardPrint>::println(format_args!(
        "nros: Executor::open failed ({err:?}); proceeding with NullNodeRuntime — \
         `run_plan` register calls will fail loud."
    ));
    None
}
```

No `multi-tier` substring on either branch, so a native multi-tier binary can
never satisfy the assertion — regardless of whether a router is up.

The marker was added to the NuttX board in `f28ebc379` (2026-07-08, phase-281 /
#149) and never mirrored to linux. This is the marker-drift class: archived
issues 0157/0164 are the same shape, and CLAUDE.md's rule — greps use
`nros_tests::output::*` constants, never literal strings — exists for exactly
this.

## Which side is wrong

Both readings are defensible and someone who owns the tier work should choose:

1. **The board is wrong.** A multi-tier entry that cannot open a session should
   say so distinctly on every board, not fall through to a generic message that
   loses the branch. Then linux gains the marker and the test is already right.
2. **The test is wrong.** If the generic fallback is the intended native
   behaviour, the test needs a different branch signal — and per CLAUDE.md it
   should assert on a shared constant rather than a literal, so the next
   message edit breaks one definition instead of two tests.

Option 1 is the better default: the boot diagnostic is the only way to tell "no
router" from "not multi-tier at all", and losing that on the platform used for
development is the worse outcome.

## Note on the second failure

`multi_tier_binary_runs_both_tiers_with_router` fails for the same reason plus a
second one: `Transport(ConnectionFailed)` means the router was not reachable
even in the with-router case, so that test also needs its zenohd wiring checked
once the marker question is settled.

## Notes

Found triaging 0422 on freshly rebuilt fixtures (7 of the 8 remaining failures
reproduced; only `test_xrce_e2e_integrity` cleared). Made diagnosable by
upstream's `decl_err_from_node` widening — before it, this surfaced only as an
opaque `NodeRegister("ctrl_pkg")` with no cause attached.
