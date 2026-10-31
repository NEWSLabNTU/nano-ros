---
id: 2076
title: "Probe-derived sizing misses the nodes a `[[bridge]]` creates, so a contract-less bridge image boots `NodeTableFull`"
status: resolved
resolved: 2026-10-26
type: bug
area: [sizing, orchestration]
severity: high
found: 2026-10-26
related: [rfc-0100, phase-517, issue-1694]
---

# Probe-derived sizing misses the nodes a `[[bridge]]` creates

**Home:** [phase-517](../../roadmap/archived/phase-517-user-flow-setup-build-flash.md) W5,
[RFC-0100 Amendment 3](../../design/0100-rmw-agnostic-sizing-model.md#amendment-3-2026-10-26--the-probe-census-sizes-a-contract-less-image-proposed-2026-10-25-phase-517-w5).
Blocked Amendment 3 from landing until it was fixed.

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

## Resolution

The maintainer chose to count what a `[[bridge]]` creates in the image's
inventory. The class sweep found the bridge was the only source the probe
route missed. The other things the runtime creates from config are already
counted: the parameter services, the parameter store, the lifecycle servers and
`~/transition_event`, `/parameter_events`, the scheduling contexts of a tier, an
action's servers and `/status`, and a contracted image's monitors and
`/diagnostics` reporter. They ride `ImageFrame` / `InfraServices`, or the
derivation itself, and so reach a probe-stated inventory. The fix has three
parts.

1. **The overlay carries the bridge** (`nros.toml`, version 2). `[[bridge]]`
   has `name`, `topics`, `bidirectional` and `rmws`, the backends its two
   endpoints bind. A bridge is a nano-ros build fact, so it goes beside the
   model rather than in it.
2. **The inventory expands it from the runtime's own config**
   (`EntityInventory::with_bridge_rows`). This is the bringup's generated
   `nros-bridge.toml`, the file `nros-bridge` reads. Per `[[node]]` session it
   adds one node row. Per `[[bridge]]` row it adds a raw subscription on the
   `from` node and a raw publisher on the `to` node. The first cut instead
   expanded over the topics the image's own rows name, and it was one
   subscription short. A bridge with no `topics` forwards every topic the
   workspace's packages DECLARE. `talker_pkg` declares `/header` and creates
   only `/chatter`, so the entry failed `sub on s0` at boot. Two cases have no
   runtime config. A bridge that names its `topics` is charged an upper bound.
   A forward-all bridge refuses the derivation, so the configured pools stand.
   `contract_inventory` expands the rows, so the knobs, the descriptor rows and
   the fragments all read them.
3. **`backend_count` counts the bridged backends.** The descriptor stated `1`
   for any image naming one RMW, on the premise that a bridge image gets no
   descriptor. With a count of 1, the executor carves no extra session, and
   `open_multi` reported `NodeTableFull` after `max_nodes` was already 3.

Tests:

- `a_bridge_images_derived_pools_include_the_bridge` and
  `a_bridge_without_its_runtime_config_is_bounded_or_refused` cover the
  inventory.
- `a_bridge_counts_the_backends_it_binds` covers the descriptor.
- `the_inventory_reads_the_runtime_config_this_writes` checks the bridge
  config's writer and reader agree.
- `a_bridge_reaches_the_overlay` covers the overlay.

Each of the first three fails with its part of the fix removed (mutation
checked). Measured on `bridge-cyclonedds`: derived `max_nodes 3`, `max_cbs 3`,
`backend_count 2`. The entry runs, and both `declarative_bridge_*` tests and
`rmw_coordinate_truth` pass. The linux lane shows 0 real failures on the
branch, against 0 on its base.
