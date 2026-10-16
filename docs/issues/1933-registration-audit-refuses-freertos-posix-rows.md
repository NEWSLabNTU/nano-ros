---
id: 1933
title: "The registration audit now asks every non-linux row for a baked locator, and the two freertos-posix workspace rows bake none, so tier 2 fails on main"
status: open
type: bug
area: [testing, ci]
severity: medium
found: 2026-10-16
related: [1793, 1819, 1581, 1865]
---

## Measured

Tier-2 run **38933437082** (`workflow_dispatch` on main `c43a90314`), job
**117082886790** `tier 2 (1-wise matrix)`, step `just ci matrix`:
`Real failures: 1 / 1 total failures`.

The failure is `nros-tests::rmw_coordinate_truth
a_rows_entry_registers_its_nodes_at_runtime`. The main thread reports
`a scoped thread panicked` at `rmw_coordinate_truth.rs:180`, and two worker
threads panicked at `rmw_coordinate_truth.rs:610`:

```
[UNMET PRECONDITION] workspace-c-freertos-posix: a non-linux image dials its BAKED locator, and this test cannot read it: fixture row `workspace-c-freertos-posix` bakes NO locator — neither its image's system.toml nor the row states one, so the image dials its board's compiled-in default (issue 1581)
[UNMET PRECONDITION] workspace-cpp-freertos-posix: … bakes NO locator …
```

## Cause

`d49b65dbf` ("the registration audit gives a baked image a router on its baked
port") made `baked_row_locator` read a baked locator for every row whose
platform is not `linux`. That fixed the two threadx-linux rows the earlier
runs failed on, where the image dials a baked port instead of `NROS_LOCATOR`.
The freertos-posix rows are also non-linux, but their fixtures bake no
locator, so the lookup returns `Err` and `unmet!` panics. The panic is inside
a scoped worker thread, so the whole test fails rather than recording a
skip for those two rows.

The previous failure mode, with threadx-linux rows reporting "the backend
refused the session", did not recur in this run.

## What this is NOT

- Not the threadx-linux session refusal from runs 38818350050 and 38861323075.
  `d49b65dbf` and `72a14ae500` fixed that.
- Not a runtime fault in the freertos-posix images: they never started.

## What would close it

The rule needs one of these:

- the freertos-posix rows state a locator (system.toml or the fixture row);
- or the audit resolves the board's compiled-in default when nothing is
  baked, the same default the image dials (issue 1581);
- or freertos-posix joins the `linux` arm, if those images read
  `NROS_LOCATOR` at run time. That has to be checked, not assumed.

An `unmet!` inside a scoped thread should also not be able to fail the whole
audit: either the precondition is checked before the threads start, or the
thread's verdict is collected per row.

Acceptance: `a_rows_entry_registers_its_nodes_at_runtime` passes in a tier-2
run on main.

## Fix (2026-10-17)

Checked, not assumed: of the 46 non-`linux` `workspace_fixture` rows, every
row whose backend DIALS a peer (all zenoh ones) states a baked locator in its
`system.toml` or its row. The ones that state none are exactly the DDS rows —
`workspace-{c,cpp}-freertos-posix`, `workspace-cpp-{s32z270,mps3-an536}-freertos`
and `workspace-zephyr-cpp-cyclonedds`, all cyclonedds. A DDS image discovers its
peers and has no locator to bake, so the audit was asking a question that has no
answer for them; the board's compiled-in default is not involved, and the two
freertos-posix rows passed before `d49b65dbf` through the exit-status path.

- `baked_row_locator` answers `None` for a backend that dials no peer, through
  one predicate (`dials_a_peer`) that `peer_locator` shares, so a DDS row runs
  as it did before `d49b65dbf`.
- Every row's baked locator is read ONCE, on the test thread, before any peer
  starts and before the row pool, so an `unmet!` names its row instead of
  surfacing as `a scoped thread panicked`.
- A baked XRCE row now gets an Agent on its baked port, the same as a baked
  zenoh row gets a router (none exists in a lane today; before, it would have
  been handed an Agent on a fresh port its image never dials).

Acceptance is unchanged: a tier-2 run on main.
