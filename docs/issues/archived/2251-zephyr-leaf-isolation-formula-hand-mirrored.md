---
id: 2251
title: "The Zephyr west lane recomputed the isolation allocator in bash, and nothing compared the two"
status: resolved
type: tech-debt
severity: medium
area: [testing, zephyr, build]
related: [2221, 1581]
found: 2026-11-04
---

## What

A baked Zephyr image's router port, XRCE Agent port and DDS domain came from two
computations:

- the TEST side: `nros_tests::alloc` (`port_of`, `xrce_agent_port_of`,
  `dds_domain_of`) — the router/Agent a test starts, the domain it expects, and
  the numbers `allocator_injective_over_matrix` proves disjoint;
- the IMAGE side: `scripts/build/zephyr-fixture-leaves.sh`, which re-derived the
  same formulas in bash for the native_sim role leaves
  (`2400 + lang_offset + variant_offset`, `22 + variant_idx * 3 + lang_idx`, and
  issue 2221's `+ 9`).

They agreed because they had been copied by hand. No gate read the emitted
values (`check-zephyr-fixture-rows.py` compares row SETS), and `alloc.rs`'s own
header claimed every bake was "derived from (or verified against)" the
allocator, which for these leaves was neither.

## Why it mattered

A port that drifts is loud: the image dials a router no test started, and the
cell goes red, if under a misleading message. A DOMAIN that drifts is silent:
both images of one cell share the same stale bake, so they still deliver, and
every test stays green while the images sit on domains the gate never proved
disjoint. That is issue 2221 coming back invisibly. `hold_baked_domain` would
also claim a domain the image is not on.

## Fix

The formula lives only in `nros_tests::alloc`; the build reads its values.

- `alloc::isolation_table()` prints every baked cell as
  `<platform token> <lang> <rmw> <Workload> <router port> <agent port> <domain>`
  (`-` where the RMW dials no such thing), over the same `is_baked` predicate the
  injectivity gate uses. The `cell-isolation` bin prints it.
- `nros_cell_isolation_file` (`scripts/build/fixture-lane.sh`) produces the table
  the way `nros_lane_coords_file` produces coordinates: a current prebuilt
  binary, else `cargo run`. `_nros_lane_coords_bin` became
  `_nros_tests_bin <name>` so both reach one lookup.
- `zephyr-fixture-leaves.sh --isolation-table FILE` keys the table on the row's
  coordinate (`row_coord`) plus the workload its role is an image of. The bash
  arithmetic is gone; a role-to-workload map (vocabulary, no numbers) remains.
- A row that states its own values AND is a cell's image is checked against the
  table: a disagreement refuses the emit, naming the row, the cell and both
  values. The idle-wake XRCE row's Agent port and domain were pure copies, so
  they are deleted and derived. The mps2 zenoh rows keep their locators
  (SLIRP host `10.0.2.2`), now verified.
- Without `--isolation-table` the emit is read-only (the inventory gates): the
  values read `UNRESOLVED`, and `zephyr-fixture-run-one.sh` refuses to build such
  a record.

## Verified

- Every emitted record, signatures included, is byte-identical to the bash
  formula's (87 leaves). So no image rebuilds.
- Editing `XRCE_DOMAIN_SHIFT` to 10 moved the Rust XRCE talker's emitted bake
  from 31 to 32. The same edit refused the idle-wake row while it still stated
  218.
- The runner refuses an `UNRESOLVED` record.

## Still hand-spelled

`examples/fixtures.toml` locator columns for other platforms and the threadx
domain bakes (`just/threadx-*.just`) still spell allocator numbers.
`fixtures::baked_locator` verifies the locator rows it names. Moving those
carriers onto `cell-isolation` is the same change on other roads.
