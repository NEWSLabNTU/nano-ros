---
id: 564
title: "`realtime_tiers_e2e` iterates 16 boot-and-observe rows on the DEFAULT 60 s kill, so the sweep was truncated — and two real failures hid behind the truncation"
status: resolved
resolved_in: issue-0564
type: bug
area: testing
related: [issue-0413, issue-0565, issue-0445, phase-329]
---

## Symptom

`just ci` (tier 1), the only red in an otherwise clean run:

```
1400 tests run: 1390 passed, 9 failed, 1 timed out, 73 skipped
  SLOW    [> 30.000s]  nros-tests::realtime_tiers_e2e realtime_tiers
  TIMEOUT [  60.003s]  nros-tests::realtime_tiers_e2e realtime_tiers
```

(The nine "failures" were `nros_tests::skip!` panics, which `just test-all`'s
junit rewrite reclassifies — not regressions.)

It reproduced SOLO, so not load flake. With `--no-capture` it printed nothing,
which reads like a hang before first output — but the test installs an empty
panic hook (`std::panic::set_hook(Box::new(|_| {}))`) so it can classify each
row's panic itself. Silence is by design, not evidence of a hang.

## Cause

Issue 0413's class, one binary further out. phase-329 W1 consolidated 15
per-cell `realtime_tiers_*` files into ONE test iterating every `RealtimeTiers`
cell in a single process. The timeout budget did not follow.

0413 already wrote the general form:

> the SAME consolidation cost, one binary over. These two are each ONE test
> iterating every native example cell … so their wall clock is the sum of nine
> router-start + delivery waits, not one cell's.

and even the trigger:

> They only started running the full nine in the first place when `da26485e9`
> un-carved the rust cyclone/xrce cells, so this budget is that change's tail.

Same here, with the trigger being fixture COVERAGE rather than a code change:
this test's cells `skip!` fast when their fixtures are absent, so on a partial
tree it finished well inside 60 s. After a full `just build-test-fixtures`
(lane=all, all nine platforms green) the zephyr / nuttx-arm / nuttx-riscv /
freertos / threadx images all exist, every row actually BOOTS, and the wall
clock is the sum of sixteen boot-and-observe rows.

`realtime_tiers_e2e` had `[test-groups.*]` entries — those serialise the rows
that share a baked router port — but no `slow-timeout` override at all, so it
ran on the default `period = 30s, terminate-after = 2`.

## Why this was worse than "slow"

Rows are evaluated **in order**, and the kill lands mid-sweep. So the later
cells were not merely slow, they were **never run** — and the verdict printed
was `TIMEOUT`, which says nothing about them.

That is issue 0445's absorbing-verdict shape in a different mechanism: a
truncating outcome replaces whatever the run would have reported. Two genuine
NuttX Rust failures had been sitting behind it (issue **0565**), invisible for
as long as the budget has been wrong.

## Fix

```toml
[[profile.default.overrides]]
filter = "binary(realtime_tiers_e2e)"
slow-timeout = { period = "180s", terminate-after = 3 }
```

Measured on this host across two consecutive full runs: **127 s** and **204 s**.
The variance — this test boots five QEMU images and waits for five 100 ms-tier
deliveries per cell — is why the kill window (540 s) is generous rather than
snug against the larger figure. 0413 pinned its own the same way: measured 93 s,
budgeted 360 s.

## Acceptance

* `cargo nextest run -p nros-tests --test realtime_tiers_e2e` runs to
  COMPLETION rather than being killed, and reports per-row results:
  `realtime_tiers: 2 of 16 row(s) FAILED`.
* Those two rows are issue 0565, filed separately — this issue is the budget,
  not the defect it uncovered.

## For the next reader

A consolidated matrix test's wall clock scales with how many of its cells have
FIXTURES, not with how many it declares. So a timeout that has always passed can
start failing on the day someone finally builds the full matrix, with no code
change anywhere near it. When a consolidation lands (phase-329's shape), the
timeout override is part of the move.
