---
id: 2130
title: "`live-peer`'s host job runs out of disk on the GitHub-hosted runner: its fixture build takes 106 G free below 49 G before the cells start"
status: open
type: bug
area: [ci, build]
severity: medium
found: 2026-10-29
related: [phase-466, 2100, 1846]
---

**Home: [phase-466](../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)**, since 2026-10-29.

## Measured

`live-peer` run 40631028259 (2026-10-28), job "rows whose board IS this
runner" (GitHub-hosted `ubuntu-22.04`). It ran from 04:12 to 07:08, then the
runner agent died with `No space left on device` during **Run the cells with
a recorded PASS**. The log is cut off at 05:39, because the agent could no
longer write it. That is the same death as on 2026-10-26, when issue 2100
first noted it.

The fixture step prints free space after each build (issue 1846):

| time | after | free |
| --- | --- | --- |
| 04:23 | reclaim | 106 G |
| 04:52 | `just _codegen` + `workspace-features-rust-params` | 71 G |
| 04:58 | 7 more workspace rows (`features-rust-*`, `rust-native*`) | 66 G |
| 05:02 | `fixtures-build.sh linux c cyclonedds` | 65 G |
| 05:30 | `fixtures-build.sh linux c zenoh` | 51 G |
| 05:34 | `fixtures-build.sh linux cpp cyclonedds` | 49 G |
| 05:39 | `fixtures-build.sh linux cpp zenoh` (running when the log stops) | |

So about 35 G went before the first workspace row finished, which is `_codegen`
plus that row's cold dependency build. Each `fixtures-build.sh linux <lang>
<rmw>` builds every manifest row of that pair (the script's own header:
"Unset = build every matching row"), not only the ones the ledger's cells
resolve. The cells then need the rest.

## Not the same as 2100

Issue 2100 pruned `host-tests`' gates. This job runs none of those gates. Its
disk goes to fixtures that the cells still need afterwards, so they cannot
just be released after a verdict.

## Fix: release compile intermediates after each build

`scripts/ci/release-fixture-intermediates.sh` deletes what was only ever an
input to a link: `deps/*.rlib` and `*.rmeta`, `incremental/`, and the `*.o`
files under `CMakeFiles/`. It keeps every binary, `.a`, `.so`, dep-info file
(`.d`, `.o.d`), build-script `output` and `.ninja_deps`, which is all the
cells and their freshness probes read. `live-peer.yml` runs it on `build/`
and the ignored build dirs under `examples/`:

- once after all the workspace rows, which share one cargo target, so a
  release between them would rebuild their deps for every row;
- after each `fixtures-build.sh linux <lang> <rmw>`;
- after each compile-check row.

It never touches `target/`, where the cell step compiles the test binaries.

Measured on this host (2026-10-29), as what a `--dry-run` would release:

| dir | size | intermediates |
| --- | --- | --- |
| `build/corrosion-cargo/native` | 81 G | 46.8 G |
| `build/cargo-fixtures` | 129 G | 59.2 G |
| `build/cmake-fixtures` | 22 G | 6.8 G |

So roughly half of each cargo tree is crate libraries. The cmake leaves hold
almost no objects: their bulk is the Rust image under `corrosion-cargo`.

Checked end to end: `linux c zenoh` built fresh, and
`an_rclc_c_application_writes_a_census_without_a_router` passed. Its leaf and
its cargo group were then released (6.3 G), and the test still resolved the
fixture fresh and passed. The freshness probe reads dep-info only, and a
dep-info path that no longer exists is not an edit event.

Cost: a later build that shares a released target dir recompiles the crates
it needs. That costs time and changes no artifact.

## What would close it

A `live-peer` run whose host job reaches its cells' verdict with space left.
The `release` lines in the fixture step print what each release freed.

The other lever, still unused:

- narrow `fixtures-build.sh` to the rows the cells resolve. Not done, because
  it cannot be derived the way the workspace rows are: the cells' tests name
  only 4 `[[fixture]]` row ids as literals. They reach the rest through
  resolver helpers keyed on a path and an RMW, and most of those rows have
  no `id`.
