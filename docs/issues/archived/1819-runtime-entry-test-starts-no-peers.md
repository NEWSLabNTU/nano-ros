---
id: 1819
title: "The generated-entry runtime test starts no peers of its own, so a row whose peer is absent passes as a printed note"
status: resolved
type: tech-debt
area: [testing, rmw]
related: [1310, 1758, 1009, phase-445]
resolved: 2026-10-12
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

## Resolution (2026-10-12)

`a_rows_entry_registers_its_nodes_at_runtime` now owns its peers:

- it starts a leased `ZenohRouter` and/or `XrceAgent` (`start_unique`), only
  for the backends the located rows need. That includes each bridge row's
  endpoints, read from its tracked `nros-bridge.toml`;
- each entry gets `NROS_LOCATOR` for its own backend, and a bridge gets
  `NROS_BRIDGE_<NODE>_LOCATOR` per endpoint (the bridge's existing override);
- a peer that cannot start is `unmet!` (issue 1758). A row that still does not
  register FAILS, naming the row and the reason. The stderr "missing peer" note
  is gone.

Measured, all 24 Linux Rust rows built, no router or Agent running beforehand:
**24 of 24 registered**, PASS. Before this change, the same host gave 1 of 24.
Mutation (the per-row env emptied): rc=100, with the three xrce rows named, for
example `workspace-rust-native-xrce (xrce) — the backend refused the session
although its peer was started for it`.
