---
id: 1549
title: "A service client and a service server in one Zephyr image never meet:
  Z_FEATURE_LOCAL_QUERYABLE stayed at zenoh-pico's 0"
status: resolved
type: bug
area: zephyr, rmw, cmake
severity: high
found: 2026-09-28
resolved_in: "feat/zenoh-local-queryable-derived (this PR)"
related: [issue-0096, issue-0135, issue-0317, phase-412]
---

## What happened

Every node of a Zephyr image shares ONE zenoh-pico session, and neither
zenoh-pico nor the router sends a query back to the session it came from. The
topic half of that was fixed long ago: `zephyr/cmake/nros_rmw_zenoh.cmake`
sets `Z_FEATURE_LOCAL_SUBSCRIBER=1` unconditionally. The service half,
`Z_FEATURE_LOCAL_QUERYABLE`, was never set on the west lane, so it stayed at
zenoh-pico's own `#ifndef` default of 0 (`include/zenoh-pico/config.h:164`).
The cargo-built embedded lanes turn it on everywhere (`nros-zpico-build`,
`local_loopback = 1`); Zephyr was the one lane without it.

The effect: a service client calling a server in the same image sends the
query, no server answers, the call times out, and nothing names the cause.
Measured on the Autoware Safety Island (brief B of its RTSS@Work demo, on the
S32K344 over serial, and again in QEMU behind Autoware): the handler detected
the fault and announced `MRM_OPERATING / EMERGENCY_STOP`, its `operate` request
to the emergency-stop operator in the same image was never served, and the
operator's status stayed `AVAILABLE` with its command frozen at cruise. The
island's workaround was a hand-set
`zephyr_compile_definitions(Z_FEATURE_LOCAL_QUERYABLE=1)` in its entry
CMakeLists.

## Resolution

The flag is a DERIVED knob on the Zephyr resolver road, like the session pools:

- `nros ws entity-inventory` counts the queriers (service clients, three per
  action client) and the application queryables (service servers, three per
  action server; the runtime's parameter and lifecycle servers excluded, since
  nothing in an image calls its own) and emits
  `set(NROS_DERIVED_RMW_LOCAL_QUERYABLE 0|1)`: 1 when both are non-zero. The
  fragment's comment line carries both counts.
- `CONFIG_NROS_RMW_LOCAL_QUERYABLE` is an `int` with the `-1` DERIVE
  sentinel; stating 0 or 1 wins, and the environment wins over both.
  `nros_resolve_knobs()` resolves it; where the inventory cannot answer, rung 4
  is zenoh-pico's 0, stated and recorded as `default` (a feature flag that
  gates struct fields cannot be left unresolved the way a size knob is).
- `nros_rmw_zenoh.cmake` emits `Z_FEATURE_LOCAL_QUERYABLE=<resolved>` to every
  TU, always 0 or 1 (the issue-0135 ABI rule), and refuses at configure if the
  knob reached it unresolved.
- `check-knob-delivery` pairs the fact with its resolved knob (`DERIVED_PAIRS`)
  and the resolved knob with the define (`C_DEFINE_KNOBS`), so a dropped
  derivation or two TUs compiled at two values is a finding.
- The rung that decided a knob is now recorded beside it
  (`NROS_KNOB_SOURCE_<knob>`: environment, kconfig, derived, default) and
  forwarded to cargo with the value. The boot record (version 6) appends one
  word, `rmw_local_queryable` = value | source << 8, and
  `read-boot-report.py` prints it with its provenance:

      Z_FEATURE_LOCAL_QUERYABLE     1   (DERIVED from the image's entity inventory)

## Cost

Measured on the island's QEMU image (mps2/an385, qemu-ethernet), the same tree
built twice, the second with `NROS_RMW_LOCAL_QUERYABLE=0` in the environment:
see the PR description for the region reports. Brief B measured the board
image at +504 B of FLASH and no RAM.

## Not done here

The derivation is a presence test, not a match on service names: the inventory
does not pair a client with the server it calls, so an image whose client calls
a remote server while an unrelated local server exists turns the path on. That
errs in the direction that costs flash and no correctness (`_z_query` still
sends the network query and delivers locally in addition).
