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

## What would close it

A `live-peer` run whose host job reaches its cells' verdict with space left.
Two levers to measure first:

- narrow `fixtures-build.sh` to the rows the cells resolve, the way the
  workspace rows already are (`--host-workspace-fixtures`). The script
  already takes `--id <row>`;
- release each build's intermediate output (cargo `deps`, cmake objects)
  once its binaries are placed, keeping only what the cells run.
