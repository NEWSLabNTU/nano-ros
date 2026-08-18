---
id: 671
title: "`contract_monitor_parity` reports NOTHING on /diagnostics — reproducible solo, and the phase that last touched it recorded 5/5"
status: open
type: bug
severity: high
area: testing, diagnostics
related: [phase-359, phase-296, rfc-0050, rfc-0052]
---

## Symptom

`contract_monitor_violations_report_on_diagnostics` fails, and the assertion's
`got:` is EMPTY — no rule id arrived on `/diagnostics` at all:

```
thread 'contract_monitor_violations_report_on_diagnostics' panicked at
packages/testing/nros-tests/tests/contract_monitor_parity.rs:123:5:
expected max-age-runtime on /diagnostics (stale stamp), got:
```

The test waits the full budget first (14 s for `max-age-runtime`, 18 s for
`rate-hierarchy-runtime`) and collects nothing, so this is silence, not a race
that lost: 32 s elapsed, zero output.

## Reproducible, and not a load flake

```
cargo nextest run -p nros-tests --test contract_monitor_parity \
    -E 'test(violations_report_on_diagnostics)'
```

**2/2 failures SOLO**, on an otherwise idle host. That distinguishes it from
the sibling failures in the same sweep (`rust_multi_node_per_node_graph`,
`interop::case_1_zenoh_pubsub_nano_to_ros2` and six others), which are the
documented in-sweep load flakes and DO pass solo — `rust_multi_node_per_node_graph`
was re-run solo and passed.

Environment ruled out at the time of the runs: no stray `zenohd` / `rmw_zenohd`
process, nothing listening on 7447-7500, ROS 2 present, both `ros2` daemons
healthy.

## Not caused by the commit that found it

Found while running tier 1 for
[issue 0655](archived/0655-zephyr-core-pin-cannot-succeed-on-running-thread.md).
That commit touches nine files — four docs, `realtime-rust`'s `system.toml`,
the Zephyr board's C and Rust tier arms, the Zephyr platform shim, and the
sched-dims test. None is in `contract-monitor`'s dependency graph, so the
fixture binary this test drives is byte-identical with or without it.

## The suspicious neighbourhood, stated as a lead and NOT as a cause

`56ea492af` (phase-359 W10, *"message crates stop granting `std`, and the live
emitter stops writing it"*) rewrote exactly this fixture's dependency lines:
four dep-sites in `nros-tests/bins/contract-monitor` moved from
`features = ["std"]` to `default-features = false` on `nros-diagnostics` and
the three generated diag message crates, because the `std` feature was deleted
from those crates in the same commit.

**That is proximity, not a diagnosis.** The obvious mechanism does not survive
a look: `contract-monitor` still names `"std", "alloc", "env"` on its `nros`
dependency, so the hosted flavour IS enabled for the runtime under it. Whatever
silences the reporter is not simply a dropped `std`.

Worth weighing against that lead: **phase-359 W10's own notes record
`contract_monitor_parity` passing 5/5** after the change, alongside
`roundtrip_xprocess` and `diagnostic_verbatim`. So either something LATER
regressed it, or the 5/5 was measured against a fixture built before the
manifest move. Both are checkable and neither has been checked here.

## What has NOT been determined

The root cause. Specifically unexamined:

* whether `contract-monitor-pub` / `-sub` still detect their violations at all
  (the binaries were not run by hand — only through the test);
* whether the violations are detected but the `nros-diagnostics` reporter drops
  them;
* whether the `DiagnosticArray` reaches the wire and only `-diagsink` fails to
  observe it (an ABI split across the three bins would look identical from
  here, and RFC-0033 capacities are exactly the kind of thing a feature move
  can shift);
* whether `diagnostic_verbatim` and `roundtrip_xprocess`, W10's other two
  witnesses, still pass.

Deliberately filed rather than patched: this sits inside an ACTIVE phase
(phase-359), which is the same call [issue 0643](archived/) took, and guessing
at a fix under a live refactor is how a second wrong mechanism gets added
beside the first.

## Blast radius

`just ci` (tier 1) is RED on this. It is one of 8 real failures in the sweep
that produced it; the other 7 pass solo, so this is the one that blocks a green
tier 1 rather than the load.
