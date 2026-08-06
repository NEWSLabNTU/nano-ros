---
id: 438
title: "native_orchestration_tiers greps a `multi-tier` marker only the NuttX board emits"
status: resolved
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

## Root cause (2026-08-06) — not the marker, and not the board

The filed diagnosis above is **wrong in both directions**, and both errors are
worth keeping visible because each is a way of being misled by a grep.

**The linux board already has the marker.** Both of them, in fact:
`nros-board-linux/src/lib.rs:419` (`multi-tier entry needs a live session —
aborting.`) and `:472` (`multi-tier run — N tier(s) over one session`). The
issue's `git grep "needs a live session" -- packages/` found only NuttX because
the linux string is LINE-BROKEN across a `\` continuation (`needs a live \` +
`session`), so the literal never matches. An issue about grep drift, produced by
a grep artifact.

**The binary never reaches that code.** `strings` on the multi-tier fixture
found ZERO occurrences of `multi-tier`, and `nm` no `run_tiers` symbol at all —
the macro emitted the single-tier `BoardEntry::run` shape. So no marker on any
board could have satisfied the assertion.

**What actually happened.** `resolve_tiers` builds tier membership *only* from
callback-group bindings; a tier table with no members hits its documented
degenerate branch and returns one synthesized `default` tier, which
`is_single_tier()` reports true for, which drops the macro onto the single-tier
path. Instrumented:

```
NROSDBG model_path=…/models/demo_bringup/system_model.yaml tiers_in_model=2
NROSDBG node_groups={} instances=["control_node", "telem_node"] resolved=Some(["default"])
```

Two tiers in the model, both nodes present, and **no bindings**. Phase-273 W2
(RFC-0047) moved group→tier binding out of the node package manifest and into
`[[component]].group_tiers` in `system.toml`. This fixture was never migrated:
its packages still carried `callback_groups = [{ id = "ctrl", tier = "high" }]`,
which nothing reads any more, and its `[[component]]` blocks bound nothing. The
model therefore had `execution.tiers` and no `execution.bindings`, and the two
authored tiers were discarded in silence.

The fixture had been emitting a single-tier binary while claiming in its own
doc-comment to prove the multi-tier emit.

## Fix

1. **Both tier fixtures bind their groups** — `orchestration_tiers_native` and
   `orchestration_tiers_freertos`, which carried the identical latent defect
   (found by sweeping every `system.toml` with a `[tiers.*]` table against
   `group_tiers`; the four realtime workspaces were already correct).
2. **The native fixture moves to the canonical `launch =` arm.** Membership
   comes from the node packages, which only that arm walks; under the deprecated
   `model =` arm `node_groups` is empty *by construction* (the macro says so at
   its declaration), so that arm can never resolve a multi-tier system. The
   SystemModel carries `execution.tiers` but no bindings, so it cannot supply
   them either.
3. **The silent discard is now a compile error.** A system that declares tiers
   none of its groups bind to fails with the tier names, the `group_tiers`
   remedy, and a note that `callback_groups` is the retired form:

```
error: nros::main!: the system declares tier(s) [high, low] but no callback group
       is bound to any of them, so they resolve to a single default tier and this
       entry would silently emit the SINGLE-tier path.
  --> src/demo_entry/src/main.rs:16:1
```

Watched to fire (remove `group_tiers` → the error above; restore → green).

## Verification

`native_orchestration_tiers` 4/4 and `orchestration_tiers_freertos` 2/2 —
**6 passed**. That includes `multi_tier_binary_runs_both_tiers_with_router`,
which the issue expected to need separate zenohd wiring: it did not. The
`Transport(ConnectionFailed)` in the original report was the single-tier
fallback path failing to reach a router it was never given, not a router
problem.

## The shape

Same absorption as issue 0445: a declaration discarded quietly at the bottom of
the stack, surfacing as a missing string three layers up, where the obvious
reading — "the marker is missing, add the marker" — is wrong and locally
plausible. The guard exists so the next one reports itself at the point of loss.
