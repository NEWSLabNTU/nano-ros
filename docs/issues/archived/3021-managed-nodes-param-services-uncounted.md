---
id: 3021
title: "On the Zephyr west road, XRCE's service-server slots were derived from zenoh's queryable count, so a second managed node came up without its parameter services"
status: resolved
type: bug
severity: medium
area: [xrce, sizing, lifecycle, zephyr]
related: [issue-1033, issue-2904, issue-2780, phase-554, rfc-0128]
found: 2026-11-28
resolved: 2026-11-28
resolved_in: "the change that adds this line"
---

## What happens

features' `zephyr_mps2_c_lifecycle_pair_xrce` (two managed C nodes,
parameter services on, XRCE, mps2/an385 under QEMU) boots both nodes to Active
and publishes, but logs on every spin:

```text
nros_rmw_xrce: no free service server slot for '/talker2/get_parameters': this image was BUILT with NROS_XRCE_MAX_SERVICE_SERVERS=16. ...
nros: parameter services could not be created for node /talker2 at service get_parameters
```

`/talker2` runs without its six parameter services. The cell
(`zephyr_cortex_m_two_managed_xrce_nodes_boot_on_the_derived_heap`) stayed
green, because it asserted only the heap and the publish count.

Reproduced on main `a73e626748` (2026-11-28), so it was there before any open
change. Found while re-running the cell for PR #4808.

## Why

On the west road, `zephyr/cmake/nros_cargo_build.cmake` stated
`NROS_XRCE_MAX_SERVICE_SERVERS` from `NROS_DERIVED_MAX_QUERYABLES` (issue
1033's choice: an XRCE service server is a served endpoint, like a zenoh
queryable). That was right until phase-554 W4 (RFC-0128 D7). Since then, a
managed node's five lifecycle servers share ONE zenoh queryable, so the
queryable count prices a lifecycle machine at one slot. XRCE gives every
server a slot of its own.

The image's inventory read 16 = 12 (parameter services, 2 nodes) + 2 (one
lifecycle queryable per machine) + 2 (transient-local cache queryables, which
serve no request). It needs 22 = 2 × 6 parameter + 2 × 5 lifecycle.

The cargo and CMake roads were not affected. `nros-rmw-xrce-cffi/build.rs`
sums the application's servers and `infra_service_servers_with` (five per
machine, six per parameter node) from the sizing descriptor. The descriptor
here was right as well (`param_service_nodes = 2`, `lifecycle_nodes = 2`).
Only the west road's carrier was the wrong count.

## Workaround (before the fix)

State `[image.<id>.knobs.xrce] max_service_servers` as the application's
servers, plus 3 per action server, 6 per node serving parameters, and 5 per
managed node. For a managed node with parameter services, that is 11 per
node.

## Fix

- The west road no longer states `NROS_XRCE_MAX_SERVICE_SERVERS`. At the -1
  sentinel it forwards nothing, as it already did for the service clients.
  `nros-rmw-xrce-cffi/build.rs` derives the count from the sizing descriptor
  that road names: `[image] service_server_queryables` plus
  `infra_service_servers_with` over `infra_families`, `model_nodes` and
  `lifecycle_nodes`. That is the rule the cargo and CMake roads already used,
  so one rule now prices the XRCE servers on every road.
  `NROS_DERIVED_MAX_QUERYABLES` sizes only zenoh's queryable table.
- Swept, every XRCE pool the west road states from a derived carrier:
  - `NROS_XRCE_MAX_SUBSCRIBERS` keeps `NROS_DERIVED_MAX_SUBSCRIBERS`. Its
    multipliers (action-client feedback, `/clock`) are backend-agnostic,
    held to nros-node's creation sites, and the same on XRCE.
  - The service clients and the three buffer families were already the build
    script's.
  - XRCE has no publisher or action pool knob of its own: an action server is
    three service servers, so this fix covers it.
- The knob's help, `check-knob-delivery`, `check-declared-fact-carriers` and
  the config census now say where the count comes from.
- The cell asserts that no XRCE service-server slot was refused and that no
  node's parameter services were refused. A dropped server goes red.

Acceptance, on host 139 (2026-11-28):

- **Negative control.** The strengthened test against the main build failed
  as intended: `a service server found no XRCE slot ... (issue 3021)`.
- **Fix.** Rebuilt on the fix branch, the west configure forwards nothing.
  `nros-rmw-xrce-cffi/build.rs` compiles both cargo lanes with
  `XRCE_MAX_SERVICE_SERVERS=22`. The cell passes, with no slot refusal and no
  parameter-service refusal in the output. Heap peak 241,816 of 320,000 B
  (main: 214,672 with six servers missing).

Found beside it: both talkers of this image share one C component instance,
so one publishes. That is [issue 3023](3023-c-component-single-instance-per-package.md),
which owns the per-node publish assertion.
