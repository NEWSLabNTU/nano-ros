---
id: 671
title: "`contract_monitor_parity` reports NOTHING on /diagnostics — reproducible solo, and the phase that last touched it recorded 5/5"
status: resolved
type: bug
severity: high
area: testing, diagnostics
related: [phase-359, phase-296, rfc-0050, rfc-0052]
resolved_in: phase-296
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

---

## Fixed 2026-08-18 — a config that does not SPECIFY an epoch is not a target that HAS none

Root cause, in `Executor::open` / `from_session_with_config_in`
(`nros-node/src/executor/spin.rs`), two adjacent lines:

```rust
if let Some(clock) = config.clock_us {      // GUARDED
    executor.clock_us_fn = Some(clock);
    executor.last_spin_end_us = Some(clock());
}
executor.epoch_us_fn = config.epoch_us;     // NOT guarded  <- the bug
```

The constructor installs a platform default (`epoch_us_fn:
Some(default_epoch_us)` for any `rmw-cffi` or `std` build). The unguarded line
then overwrote it with whatever the config held — and `ExecutorConfig::new`,
which is the path `nros::init_with_launch_auto()` + `ctx.config()` takes,
leaves `epoch_us: None`. (`from_env()` and `resolve()`'s hosted arm both set
`Some(default_epoch_us)`; `new()` does not.) So every hosted node built through
`ctx.config()` silently lost its wall clock.

With no epoch, `Node::subscription` never attaches the age cell — it requires
`(<M as RosMessage>::STAMP_OFFSET, epoch_us_fn)` BOTH `Some` — so a baked
`max_age_ms` contract became a **silently-dead monitor**, which is precisely
the outcome RFC-0052's fail-loud contract exists to prevent.

**Why only half the fixture went quiet.** The rate monitor rides
`clock_us_fn`, which is GUARDED, so `rate-hierarchy-runtime` kept firing while
`max-age-runtime` never did. That asymmetry is the whole diagnosis: it ruled
out the router, the wire, the reporter, the diagsink and the message crates,
all of which both rules share.

**The comment above the guarded line records that this identical bug was
already found and fixed for `clock_us`.** The sibling line never got the same
treatment — one of two sites fixed, which is the class CLAUDE.md names.

### Fix

Guard the epoch exactly as the clock is guarded, at BOTH sites (`open` and
`from_session_with_config_in`). A config that specifies an epoch still wins; a
config that says nothing now leaves the platform default intact.

### Verified, in this order

| step | result |
| --- | --- |
| three bins by hand, pre-fix | pub publishes, sub receives, `rate-hierarchy-runtime` fires, **no `max-age-runtime`** |
| probe `executor.epoch_now_us()` in the sub | **`None`**, with `age_table_len=1` — table installed, clock absent |
| same three bins, post-fix | **32x `max-age-runtime`** + `rate-hierarchy-runtime` |
| `contract_monitor_parity` | 2/2 PASS (the violating case 32 s -> **5.2 s**: it now finds the violation instead of waiting out the budget) |
| `diagnostic_verbatim`, `roundtrip_xprocess` | 3/3 PASS (W10's other two witnesses) |

### Corrections to this issue as originally filed

* **The phase-359 W10 lead was wrong.** The manifest rewrite (`56ea492af`) is
  not the cause; `contract-monitor` still enables `std` and the generated
  `Header` still carries `STAMP_OFFSET = Some(4)`. Filing it as a lead rather
  than a diagnosis was right, and it is now closed out as refuted.
* **"Not caused by the commit that found it" stands**, and so does "root cause
  not determined" — it is determined now.
* **The regression WINDOW was never pinned and is not claimed.** Both sides of
  the clobber date to `5cd391466` (the original W3b commit), so this was not
  introduced by any recent change; the most likely reason it surfaced now is
  that the fixture binaries had not been rebuilt since 2026-08-02, and the
  first fresh build in sixteen days exposed a latent defect. That is a
  hypothesis about VISIBILITY, not about the bug, which was always there.

### Blast radius of the fix

Wider than this fixture: every hosted node that gets its config from
`ctx.config()` regains a wall clock, so age monitors that were silently
disabled become live. That is the intended behaviour, and it is why the
verification above ran the sibling diagnostics tests too.
