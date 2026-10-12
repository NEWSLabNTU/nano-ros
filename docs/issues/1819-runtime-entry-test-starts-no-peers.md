---
id: 1819
title: "The generated-entry runtime test starts no peers of its own, so a row whose peer is absent passes as a printed note"
status: open
type: tech-debt
area: [testing, rmw]
related: [1310, 1758, 1009, phase-445]
---

# What happens

`a_rows_entry_registers_its_nodes_at_runtime` (`rmw_coordinate_truth.rs`,
issue 1310) runs each `[[workspace_fixture]]` row's generated entry. It starts
no router and no XRCE Agent, and it passes no `NROS_LOCATOR`. A row whose
session cannot open is listed under "reported a missing peer" on stderr, and
the test still PASSES as long as one other row registered.

Measured 2026-10-12, all 24 Linux Rust rows built
(`scripts/build/workspace-fixtures-build.sh linux rust`):

| peers running | rows that registered |
| --- | --- |
| none | 1 (the cyclonedds row — DDS needs no peer) |
| `rmw_zenohd` on `127.0.0.1:7447` + `MicroXRCEAgent udp4 -p 8888` | 22 |
| the same, plus `NROS_LOCATOR=udp/127.0.0.1:8888` on the two xrce rows | 24 (xrce checked by hand: `native_xrce_entry` logs the session open) |

So on a host with no peers, 23 of 24 rows were never exercised and the test
was green. That is issue 1758's rule broken from inside one test: a missing
peer is a printed line, not a red.

# Fix

The test owns its peers, the way the interop cells do:

- start `rmw_zenohd` (through `nros_router_exec`, issue 0653) and the XRCE
  Agent on per-test ports, and pass each row the matching `NROS_LOCATOR`;
- a row that still cannot open its session is then `unmet!` (the peer could
  not be started) or a failure (it started and the entry still refused),
  never a stderr note;
- if a peer is out of the lane's scope, the lane deselects the row through
  `lane_skip!`, not a probe.

# Also seen while measuring

The bridge-cyclonedds row logs `[ERROR] liveliness: declare failed (KeyExpr)`
and still registers. Not investigated; it may deserve its own issue.
