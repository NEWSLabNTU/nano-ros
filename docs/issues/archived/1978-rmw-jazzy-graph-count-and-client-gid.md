---
id: 1978
title: "Jazzy rmw contract adds count_clients/count_services, get_gid_for_client and event_type_is_supported; we answer none"
status: resolved
type: enhancement
area: rmw
severity: low
found: 2026-10-19
related: [rfc-0054, issue-1972]
resolved_in: 2026-10-20
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

## Resolution

Four optional slots appended to `nros_rmw_vtable_t` (`rmw_vtable.h`, slots
73–76), bindgen output regenerated:

| slot | zenoh (Rust adapter) | Cyclone | XRCE / uORB |
| --- | --- | --- | --- |
| `count_clients` | `SC` tokens in the graph cache (`count_entities_on_topic`) | writers on `rq/<name>Request` | NULL → UNSUPPORTED |
| `count_services` | `SS` tokens | readers on `rq/<name>Request` | NULL → UNSUPPORTED |
| `get_gid_for_client` | the gid stamped into the request attachment | the request writer's GUID | NULL → UNSUPPORTED |
| `event_type_is_supported` | `shim::events`, the one table its entities' `supports_event` also read | every known kind (its `*_take_event` bodies serve all five) | NULL → `false` (no event path) |

Rust traits: `Session::count_clients` / `count_services` (default
`Unsupported`), `Session::event_type_is_supported` (static, default `false`),
`ClientTrait::get_gid` (default `Unsupported`). The C-ABI consumer reads
`event_type_is_supported` through one helper, cffi's
`backend_event_type_is_supported`, which `CffiPublisher` / `CffiSubscription`
`supports_event` both ask; a NULL slot falls back to slot granularity.

Client-library faces, under the executor receiver every graph verb uses:
`rclc_executor_count_clients` / `_services` (C),
`nros_cpp_executor_count_*` + `Executor`, `Node` and `LifecycleNode`
`count_clients` / `count_services` (C++), `Executor` and `Node`
`count_clients` / `count_services` (Rust; rclrs has none, so the Node rows are
`extension`).

Ledgers: the four `rmw-api-map.toml` rows are `vtable`
(`rmw-api-parity --edition jazzy`); the `c:` / `cpp:` gap rows became
`divergence` rows, and the api-parity taxonomy files `count_clients` /
`count_services` under `graph` with their siblings. `rmw-abi-shape` pins the
two counts' session-for-node deviation, and now treats a slot named after
another recorded edition's symbol as neither an undeclared extra nor an
orphaned pin in the edition that lacks it.

Tests: cffi `jazzy_graph_client_gid.rs` (fake vtable: NULL → `Unsupported`,
forwarding, the event helper) and the adapter case in `rust_adapter.rs`;
nros-node mock forwarders; zenoh `shim::events` unit test; Cyclone
`graph_query` (one server + one client counted across participants, client
gid, event slot agrees with the take slots). Acceptance: `graph-probe` counts
the stock talker's `/talker/get_parameters` service, asserted by
`graph_interop` on zenoh and Cyclone.

