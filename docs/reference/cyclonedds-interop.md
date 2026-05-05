# Cyclone DDS Interop (Phase 117)

This note covers the wire-level details of `nros-rmw-cyclonedds` —
how a nano-ros node using the Cyclone backend interoperates with a
stock ROS 2 node using `rmw_cyclonedds_cpp`.

## Pin

| Component                              | Version |
|----------------------------------------|---------|
| Upstream Cyclone DDS submodule         | tag `0.10.5` (`third-party/dds/cyclonedds/`) |
| Matching ROS 2 Humble package          | `ros-humble-cyclonedds` 0.10.5 |
| Matching ROS 2 RMW                     | `ros-humble-rmw-cyclonedds-cpp` 1.3.4 |

The submodule pin and the apt-installed packages must agree on
**0.10.x**. `0.10.5` is the latest patch release of the line ROS 2
Humble shipped. Upgrading the submodule pin requires a matching ROS 2
distribution upgrade — Cyclone DDS does not commit to wire compat
across `0.x` minor releases.

## Topic naming — required for stock-RMW interop

`rmw_cyclonedds_cpp` uses native DDS topic naming with standard
ROS 2 prefixes:

| Entity        | DDS topic name |
|---------------|----------------|
| Publisher     | `rt/<topic>` (e.g. `rt/chatter`) |
| Subscription  | `rt/<topic>` |
| Service request  | `rq/<service>Request` |
| Service reply    | `rr/<service>Reply` |
| Action goal      | `rq/<action>/_action/send_goalRequest` |
| (etc.)        | (see `rmw_cyclonedds_cpp/src/namespace_prefix.cpp`) |

> **Current state:** the in-tree backend uses **raw, unprefixed
> names** (e.g. publishes to `chatter`, not `rt/chatter`; service
> Request topic is `<svc>Request`, not `rq/<svc>Request`). nano-ros
> ↔ nano-ros works because both sides agree; **stock RMW interop
> is broken** until Phase 117.X.2 lands the prefix convention.

> **Contrast:** `rmw_zenoh` rewrites topics into a zenoh keyexpr of
> the form `<domain>/<topic>/<type>/TypeHashNotSupported`. Cyclone
> stays with raw DDS names. No bridge is involved.

## Type identification

Each topic carries a fully-qualified ROS 2 type name (e.g.
`std_msgs::msg::dds_::String_`). Cyclone's matching logic compares
the `dds_topic_descriptor_t::m_typename` field byte-for-byte; the
descriptor must be generated from the same `.idl` as the peer.

> **Current state:** consumers hand-author `.idl` files and run
> them through the `nros_rmw_cyclonedds_idlc_compile()` CMake
> helper. Type names follow whatever convention the IDL author
> picks — **not** automatically the rosidl-shaped
> `<pkg>::msg::dds_::<Type>_` pattern stock RMW expects. **Stock
> ROS 2 type matching is broken** until Phase 117.X.1 lands
> rosidl_adapter integration in `cargo-nano-ros`.

Two viable production paths:

1. **rosidl_adapter codegen (Phase 117.X.1 — pending).** Extend
   `cargo-nano-ros` so `.msg` / `.srv` files map through ROS 2's
   canonical IDL conventions before idlc runs. Output type names
   match stock RMW.
2. **Manual descriptor link (interim).** Pull the descriptor from
   an existing ROS 2 type-support package that ships
   `_cyclone_idl_native.so` / equivalent and link it into the
   consumer.

Phase 117.5 wired the registry; types can be registered today.
117.X.1 fixes the *naming* of the registered descriptors so they
match what stock RMW emits.

## Discovery

Default Cyclone discovery is SPDP multicast on `239.255.0.1:7400+`,
followed by SEDP unicast. Configuration knobs match upstream:

```xml
<CycloneDDS>
  <Domain id="0">
    <General>
      <NetworkInterfaceAddress>auto</NetworkInterfaceAddress>
      <AllowMulticast>spdp</AllowMulticast>
    </General>
    <!-- For unicast-only deployments (RTOS without IGMP, NAT, etc.) -->
    <Discovery>
      <Peers>
        <Peer Address="192.168.1.42"/>
      </Peers>
      <ParticipantIndex>auto</ParticipantIndex>
    </Discovery>
  </Domain>
</CycloneDDS>
```

The autoware-safety-island app uses a static peer list to bypass
multicast on Cortex-R5 / Cortex-A targets where IGMP is fragile.
nano-ros's Cyclone backend will pick this up via the raw
`ddsi_config` path in Phase 117.6 once the backend exposes a config
hook through `Session::open`.

## Domain ID

Cyclone takes the `dds_domainid_t` (uint32) directly. nano-ros's
`session_open(_, _, domain_id, _, _)` passes the runtime-supplied
`domain_id` straight through; this is normally the value of
`ROS_DOMAIN_ID` set by the consumer.

## QoS

Phase 117.6 maps `nros_rmw_qos_t` → `dds_qos_t` via:

| nano-ros field       | Cyclone setter |
|----------------------|----------------|
| `reliability`        | `dds_qset_reliability` |
| `durability`         | `dds_qset_durability` |
| `history` + `depth`  | `dds_qset_history` |
| `deadline_ms`        | `dds_qset_deadline` |
| `lifespan_ms`        | `dds_qset_lifespan` |
| `liveliness_kind` + `liveliness_lease_ms` | `dds_qset_liveliness` |

Cyclone honours every policy in the `nros_rmw_qos_t` struct, so the
runtime's per-policy support mask is "all set" for this backend.
`NROS_RMW_RET_INCOMPATIBLE_QOS` only fires for inter-endpoint
mismatches that Cyclone itself rejects (e.g. reliable publisher +
best-effort subscriber under strict matching).

## Status events (Phase 108)

The vtable's three event slots are NULL in 117.3:

```c
.register_subscriber_event   = NULL,
.register_publisher_event    = NULL,
.assert_publisher_liveliness = NULL,
```

A follow-up phase wires Cyclone's `dds_set_listener` through to
`nros_rmw_event_callback_t`. Until then, the runtime falls back to
`NROS_RMW_RET_OK` for AUTOMATIC / NONE liveliness and
`NROS_RMW_RET_UNSUPPORTED` for any explicit event registration.

## Build pin verification

```bash
$ cd third-party/dds/cyclonedds
$ git describe --tags
0.10.5
$ cd -
$ dpkg -l ros-humble-cyclonedds | tail -1
ii  ros-humble-cyclonedds  0.10.5-2jammy.20260226.013234  amd64  ...
```

The submodule and the apt package must agree. CI smoke for
`just cyclonedds test` validates the link path; full E2E pub/sub
against a stock `rmw_cyclonedds` peer comes in Phase 117.12.
