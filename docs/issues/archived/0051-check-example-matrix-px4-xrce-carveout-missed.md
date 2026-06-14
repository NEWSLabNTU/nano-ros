---
id: 51
title: check-example-matrix flags examples/px4/rust/xrce — px4 transport carve-out missed when the XRCE e2e landed
status: resolved
type: tech-debt
area: build
related: [phase-241]
resolved_in: Phase 241 W11 follow-up
---

## Why

`just check` → `scripts/check-example-matrix.sh` fails:

```
Retired examples/<platform>/<language>/<rmw>/ roots found:
  examples/px4/rust/xrce
```

The script rejects any `examples/<plat>/<lang>/<name>/` whose `<name>` matches an
RMW token (`zenoh|xrce|dds|cyclonedds|uorb`) — the **retired** per-RMW layout
(RMW is selected at build time now, not by directory). For PX4 this is a **false
positive**: px4's directory axis is the *transport integration case* (uORB vs
XRCE — PX4's two native messaging surfaces), not the retired RMW axis. The script
already carves out the uORB cases (Phase 118.H):

```
"examples/px4/cpp/uorb"
"examples/px4/rust/uorb"
```

When the PX4 SITL XRCE e2e landed (`1031f07e4` — `px4-probe` + `px4_xrce_e2e`) it
added `examples/px4/rust/xrce/` **without** the matching carve-out line, so the
checker now flags it. The dir is correct; the carve-out list is stale.

This is on `main` (both `examples/px4/rust/xrce` and `examples/px4/rust/uorb`
exist there), so `just check` is red on main independent of any feature branch —
surfaced while running `just check` for the RFC-0042 D3 single-runtime work
(phase-241).

## Fix

Add `examples/px4/rust/xrce` to `allowed_roots` in
`scripts/check-example-matrix.sh`, beside the existing px4 uORB carve-outs. (No
`examples/px4/cpp/xrce` exists yet; add it if/when a C++ XRCE px4 case lands.)

Longer term (out of scope here): the checker could special-case `examples/px4/`
so px4's transport-case dirs don't need per-case carve-out lines — px4 is the one
platform whose sub-dir axis is legitimately a transport case, not an RMW.

## Status

RESOLVED. Two steps: (1) the immediate carve-out line `examples/px4/rust/xrce`
(2026-06-14); (2) the structural fix — `is_allowed()` in
`scripts/check-example-matrix.sh` now exempts the whole `examples/px4/*` platform
(px4's `<lang>/<transport>` sub-dir axis is a legitimate uORB/XRCE integration
case, not the retired per-RMW layout), so the three per-case px4 carve-out lines
are removed and future px4 transport cases need none. `check-example-matrix` green.
