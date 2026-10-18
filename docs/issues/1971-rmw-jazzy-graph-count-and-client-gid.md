---
id: 1971
title: "Jazzy rmw contract adds count_clients/count_services, get_gid_for_client and event_type_is_supported; we answer none"
status: open
type: enhancement
area: rmw
severity: low
found: 2026-10-19
related: [rfc-0054, issue-1972]
---

## What

The Jazzy rmw implementation contract (`docs/reference/editions/jazzy/rmw-implementation-contract.txt`,
95 symbols, derived from the pinned `rmw_implementation` dispatch table) has
seven symbols Humble's 88 do not. Three of them are dynamic typesupport and
were DECLINED (see `rmw-api-map.toml`). These four were ruled ADOPT:

| upstream | shape | why adopt |
| --- | --- | --- |
| `rmw_count_clients` | `(node, service_name, size_t *count)` | the service twin of `count_publishers`, which is a live slot; both graph backends (zenoh graph cache, Cyclone builtin readers) already hold the data |
| `rmw_count_services` | as above | as above |
| `rmw_get_gid_for_client` | `(client, rmw_gid_t *gid)` | twin of the live `get_gid_for_publisher`; Jazzy's service-introspection-free uses (request matching by client GID in tooling) need it |
| `rmw_event_type_is_supported` | `bool (rmw_event_type_t)` | a static per-backend answer the `event_init` slots already imply by returning UNSUPPORTED; asking it up front lets a port fail at configuration rather than at entity creation |

The client-library faces ride along: `rcl_count_clients` / `rcl_count_services`
(`c:count_clients`, `c:count_services`) and `rclcpp::Node::count_clients` /
`count_services` plus their `LifecycleNode` inherits (`cpp:*` rows, all
`"editions": ["jazzy"]` in the api-parity ledger).

## Work

1. Four vtable slots named by the mechanical rule (`count_clients`,
   `count_services`, `get_gid_for_client`, `event_type_is_supported`), NULL =
   UNSUPPORTED like their siblings; `just check rmw-abi-shape --edition jazzy`
   then compares them against the recorded Jazzy signatures.
2. zenoh + Cyclone producers (the graph caches already count per topic).
3. `executor_count_clients`/`executor_count_services` in C and
   `Node::count_clients`/`count_services` in C++, mirroring the
   publisher/subscriber pair.
4. Flip the four `rmw-api-map.toml` rows from `gap` to `vtable`, and delete
   the ledger `gap` rows.

Acceptance: `scripts/rmw-api-parity.py --edition jazzy` reports these four
under `vtable`, and a live graph test counts a ROS 2 service from an image.
