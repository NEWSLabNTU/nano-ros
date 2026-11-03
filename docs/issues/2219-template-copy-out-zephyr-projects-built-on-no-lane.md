---
id: 2219
title: "template-copy-out's five Zephyr projects are built copied out on no lane, and gate.yml still cites a resolved issue as the reason"
status: open
type: tech-debt
area: [ci, templates, zephyr]
found: 2026-11-03
related: [issue-1453, issue-1782, issue-2140, phase-489]
---

## The gap

Issue 1453 put `just check template-copy-out` on the merge queue so that every
copy-out template is built copied out before it merges. Five of its projects
are never built on any lane:

- `ament-workspace-port/zephyr`
- `cpp-port-minimal-publisher/zephyr`
- `rclcpp-compat-smoke/zephyr`
- `topic-state-monitor-port/zephyr`
- `zephyr-byo/app`

In all 20 successful `merge_group` runs of `gate.yml` on 2026-11-02
(41381715200 … 41496872163), each of the five took about 1 s and was reported
as follows:

```
[SKIPPED] template-copy-out: NOT VERIFIED — <project> (<board>): Zephyr not provisioned — no west workspace resolves (just zephyr setup)
```

The `check-template-copy-out` job provisions the FreeRTOS cross toolchain and
sources, but no west workspace. The other lane that runs the gate is
`host-tests.yml`'s `tier-1 gates` shard (hosted `ubuntu-22.04`, in the CI
image; it ran the gate in run 41381866329, 3445 s). That job has no Zephyr
setup step either. Its parallel runner prints no per-gate output, so the
skip lines do not reach the job log. As far as the workflows show, none of
these five has been built copied out in CI.

It is not silent in the log. Each skip goes through `nros_check_unverified`
into the `nros_check_skip` ledger, and the gate's closing line points at it.
But the lane's green does not mean what issue 1453 says it means, and no lane
claims these five as out of scope.

## Why it reads as deliberate when it is not

`gate.yml`'s comment on `check-template-copy-out` still says that "the Zephyr
ones have no copy-out road the gate can derive (issue 1782)", and the
provisioning step repeats it. Issue 1782 was resolved on 2026-10-29 by giving
the gate exactly that road: `nros configure` + `nros build` in the copied leaf,
against the resolved west workspace. Its acceptance was measured locally with
Zephyr installed. The rationale in the workflow is therefore stale. The only
thing still missing is the precondition.

## Options

1. Provision a Zephyr 3.7 west workspace on a lane that runs the gate (the
   cost of `nros setup zephyr` on a hosted runner: west update, SDK, disk —
   issue 1353), and keep these five NOT VERIFIED on the merge queue as a
   stated scope decision.
2. Provision it in the merge-queue job itself, or in one shard of it
   (phase-489 W8 shards the job), so that issue 1453's guarantee covers Zephyr.
3. Make the scope explicit: declare in the lane that it does not claim the
   Zephyr road, and name the lane that does.

Whichever is chosen, the workflow comment must stop citing 1782 as the reason.

## Acceptance

- Some named lane builds all five Zephyr projects copied out, with the log
  showing `<project>: OK`. Or the merge queue does, and its job log shows them
  OK.
- `gate.yml`'s comment states the real scope.
