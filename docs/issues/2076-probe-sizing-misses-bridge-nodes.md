---
id: 2076
title: "Probe-derived sizing misses the nodes a `[[bridge]]` creates, so a contract-less bridge image boots `NodeTableFull`"
status: open
type: bug
area: [sizing, orchestration]
severity: high
found: 2026-10-26
related: [rfc-0100, phase-517, issue-1694]
---

# Probe-derived sizing misses the nodes a `[[bridge]]` creates

**Home:** [phase-517](../roadmap/phase-517-user-flow-setup-build-flash.md) W5,
[RFC-0100 Amendment 3](../design/0100-rmw-agnostic-sizing-model.md#amendment-3-2026-10-26--the-probe-census-sizes-a-contract-less-image-proposed-2026-10-25-phase-517-w5).
**Blocks** Amendment 3 from landing.

## Problem

RFC-0100 Amendment 3 lets the fresh probe sidecars of a contract-less image
stand as its declaration. Its evidence run found one class of image where
the probe-derived counts are short and the configured defaults are not: an
image whose bringup declares a `[[bridge]]`.

The probe runs each COMPONENT's constructor and counts what it registers. A
`[[bridge]]` in `system.toml` is not a component. The runtime creates its own
node per bridged domain and its own endpoints per bridged topic, and no sidecar
describes them. So the derived inventory holds only the talker:
`max_nodes = 1`, `max_cbs = 1`. The image needs more.

Measured on `examples/workspaces/bridge-cyclonedds` (fixture row
`workspace-rust-native-bridge`) with the branch build of Amendment 3, against a
private `rmw_zenohd`:

| build | `max_nodes` / `max_cbs` | boot |
| --- | --- | --- |
| default (`origin/main`, configured knobs) | crate defaults | session opens, subscribes `/chatter` and `/header`, runs to completion |
| probe-derived | 1 / 1 | `Error: OpenSession("NodeTableFull")`, exit 1 |

`examples/workspaces/bridge-xrce` (`workspace-rust-native-bridge-xrce`) derives
the same 1 / 1. Its tests fail derived and pass default:

- `rmw_coordinate_truth::a_rows_entry_registers_its_nodes_at_runtime`: both
  bridge entries "did not reach node registration" (the other 82 entries
  register).
- `declarative_bridge_zenoh_to_cyclonedds::*` (2 tests) and
  `declarative_bridge_zenoh_to_xrce::declarative_zenoh_to_xrce_bridge_to_nros_listener`:
  0 bridged samples.

All four reproduce when run alone. All four pass on `origin/main` in the same
`just test linux` lane.

This is not lazy registration. The static search for `create_*` calls reached
from callbacks found none in any model-road component. The gap is entities the
RUNTIME creates on the bringup's behalf, and the probe is blind to them by
construction.

## Fix direction

Count what the runtime creates for a `[[bridge]]` in the model's frame, the way
`InfraServices` counts the parameter and lifecycle servers: the bridge's nodes
and its per-topic endpoints, in `ImageFrame::from_model`. Until it does, the
probe arm should refuse an image whose model declares a bridge. That keeps the
configured defaults, and "nobody described this" must never become a zero.

One open question: whether a CONTRACT image with a bridge has the same short
count. No in-tree bridge image has a contract, so nothing measures it.
