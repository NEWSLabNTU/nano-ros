---
id: 1729
title: "`sched_dims_applied`'s tier-1 red is a REAL Zephyr DerivedTierBelowTransport failure (PublisherCreationFailed), not a skip — and the test launders a stale or missing in-lane fixture into a skip"
status: open
type: bug
area: [testing, runtime]
severity: medium
found: 2026-10-07
related: [0630, 0571, 0445, 0584, 1537, 1684]
---

## Why this was filed, and what the measurement showed instead

The local `just ci tier1 run` that brought issue 1684's MISSING class to zero
(branch of #1728, 2026-10-06) had `sched_dims_applied_e2e sched_dims_applied`
fail 3 of 3 tries at ~35 s. `scripts/test/name-real-failures.py` annotated
it:

    nros-tests::sched_dims_applied_e2e sched_dims_applied  <- a skip marker is NESTED in this failure; the test should classify it with `nros_tests::skip_marker` (issue 0658)

That annotation suggested the cell loop was reporting a skip as a failure.
**It is not.** Re-run SOLO under the same lane, with the Zephyr fixtures rebuilt
first:

```
NROS_FIXTURE_LANE=tier1 NROS_TEST_COORDS=<tier-1 lane-coords file> \
  cargo nextest run -p nros-tests --test sched_dims_applied_e2e --no-capture -j1
```

The tier-1 coordinates are linux × {c,cpp,rust,mixed} × rmw, plus
`threadx-linux,c,zenoh` and `zephyr,rust,zenoh`. Fixture mtimes were checked
against HEAD before blaming code: both `build-ws-rs-realtime-entry-zenoh` and
`build-ws-rs-realtime-derived-entry-zenoh` `zephyr.exe` were built 2026-10-07
08:10 (+0800), and the tree's HEAD is 2026-10-06 09:54 (+0800). The test fails
3/3 (48.6 s, 35.8 s, 35.9 s) with this verdict:

```
sched_dims: 10 cell(s) ran, 6 skipped, 7 out of lane
  ...
thread 'sched_dims_applied' panicked at packages/testing/nros-tests/tests/sched_dims_applied_e2e.rs:600:5:
sched_dims: 1 of 10 cell(s) FAILED:
  DerivedTierBelowTransport/zephyr/rust: [zephyr rust DerivedTierBelowTransport] tier(s) ["high"] never dispatched (no `(tier `<tier>` is dispatching)` line) — the report is judged before any tier runs, so an image that did not reach its tiers says nothing about it. issue 1537 — ...
log:
*** Booting Zephyr OS build v3.7.0 ***
<inf> nros: nros_board_zephyr::entry_tiers: nros: zephyr multi-tier entry up (2 tiers, boot tier `derived-telem_node`)
<inf> nros: ctrl_pkg: Control::register on a tier admitting group `ctrl`
<err> nros: nros: node declaration failed — NodeError::Transport(PublisherCreationFailed)
<err> nros: nros_board_zephyr::entry_tiers: nros: tier `derived-control_node` setup failed: NodeRegister("ctrl_pkg") — 0 downstream tier(s) will NOT start
<inf> nros: telem_pkg: on_telem: first publish OK (tier `low` is dispatching)
```

So there are two findings, neither of which is the one this issue was opened for.

### 1. The red is real: a derived tier's publisher cannot be created

In the `examples/workspaces/realtime-rust` derived-entry image
(`workspace-realtime-derived-entry`, Zephyr native_sim, zenoh), the second
tier (`derived-control_node`) fails `ctrl_pkg`'s node declaration with
`NodeError::Transport(PublisherCreationFailed)`, and the boot tier
(`derived-telem_node`) publishes fine. The `high` tier therefore never
dispatches, and the cell's issue-1537 assertion fires correctly.

Not diagnosed. Candidates:
- a zenoh publisher-table or session slot the image sizes for one tier's
  entities rather than both (the 1015/1033 floor class, or a derived count
  that misses the second tier);
- the sizing descriptor / entity facts this image is built with.

The 2026-10-05 tier-1 verdict (run 37252649866) predates the lane changes
that let this cell run at all on a tier-1 host, so this is not evidence of
when it regressed.

### 2. The annotation that started this is a false lead, and why

The cell loop (`sched_dims_applied_e2e.rs` ~550–600) catches each cell's
panic and sorts it with `nros_tests::skip_marker::is_skip(&msg)`, which
searches the WHOLE message for `[SKIPPED`. That sorting is correct: the one
entry in `failed` above carries no marker. The marker that
`name-real-failures.py` saw is in the test's STDOUT, which goes to the same
junit testcase. Before asserting, the test deliberately prints every
`out_of_lane` and `skipped` note (issue 0571's "say what did NOT run"), and
six of those notes are `[SKIPPED:lane] out of lane: …` lines. So "a skip
marker is NESTED in this failure" is true of the testcase's text and false of
the failure. The tooling cannot tell a printed summary line from the panic.

### 3. The real skip-vs-failure defect runs the OTHER way

`run_cell` (~line 649) resolves the fixture with

```rust
let entry = (ex.resolver)().unwrap_or_else(|e| {
    nros_tests::skip!("{platform} {lang} {:?} realtime fixture unavailable: {e}", cell.dim)
});
```

so ANY resolver error — including a STALE or MISSING fixture for an IN-LANE
coordinate — becomes a `[SKIPPED]` (class `capability`). The loop then files
it under `skipped`, and the test PASSES as long as at least one other cell
ran. Measured in the first solo run of this investigation, before the
fixtures were rebuilt: the in-lane `zephyr,rust,zenoh` cells CorePin,
EdfDeadline and DerivedTierBelowTransport all read

    [SKIPPED] zephyr rust CorePin realtime fixture unavailable: Build failed: Zephyr fixture is STALE — a source is newer than the built binary:

and the test reported **PASS**. That is issue 0584's laundering (an absent or
stale in-lane fixture must fail hard) inside a consolidated test, where neither
`check-skip-budget` nor the junit rewriter can see it, because the outer test
passed. It also hid finding 1 for exactly as long as the fixture stayed stale.

## Acceptance

1. `DerivedTierBelowTransport/zephyr/rust` dispatches its `high` tier (or the
   cell is re-specified with a written reason), and `sched_dims_applied`
   passes on fresh `lane=tier1` fixtures.
2. In `sched_dims_applied` and its sibling consolidated tests (lane_scope's
   `CONSUMERS`: `entry_e2e`, `multihost_e2e`, `realtime_tiers_e2e`,
   `roundtrip_xprocess_e2e`), a resolver error for an in-lane cell is a FAILURE
   (the 0584 rule), not a `skip!`. An out-of-lane cell keeps its
   `[SKIPPED:lane]`.
3. Optional, tooling: `name-real-failures.py` attributes a nested marker only
   when it is inside the failure message, not anywhere in the testcase's
   streams.
