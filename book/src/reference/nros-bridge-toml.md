# `nros-bridge.toml` — bridge configuration schema

Consumed by `nros::run_from_config` (Rust) and the future C/C++ mirror
(`nros::run_from_config` in `nros-cpp`).

> **Not the orchestration `nros.toml`.** This is the **bridge** runtime
> config (multi-RMW byte forwarding). It is named `nros-bridge.toml` to
> avoid colliding with the Phase 126 orchestration `nros.toml`
> (component/system build config) — different lifecycle, different
> schema (Phase 172.L).

The file lives next to the binary (or anywhere the program can read
it) and selects:

1. Which RMW backends to open, under what locator / domain.
2. How those backends are wired into bridge entries that forward
   traffic between them.

The binary's `Cargo.toml` (or `target_link_libraries`) still selects
which backends are *linked* — the TOML file selects which of the
linked backends are *used* in this run. Backend names in the file
that don't match a linked backend surface as
`ConfigError::OpenSession`.

## Top-level structure

```toml
# nros-bridge.toml — sibling of the binary
[[node]]
name    = "field"
rmw     = "zenoh"
locator = "tcp/10.0.0.1:7447"

[[node]]
name    = "control"
rmw     = "cyclonedds"
locator = "domain=0"

[[bridge]]
type      = "std_msgs/Int32"
type_hash = "RIHS01_..."
from      = { node = "field",   topic = "/sensor/raw" }
to        = { node = "control", topic = "/sensor/raw" }
```

Run via:

```rust
fn main() -> Result<(), nros_bridge::ConfigError> {
    nros_bridge::run_from_config("nros-bridge.toml")
    // or, via the umbrella feature: nros::run_from_config("nros-bridge.toml")
}
```

## `[[node]]`

One block per backend session. The first `[[node]]` becomes the
primary session (`extra_sessions[0]` in `Executor::open_multi`); the
rest open as extras keyed by `rmw`.

| Key         | Type   | Default | Notes |
|-------------|--------|---------|-------|
| `name`      | string | required | Logical name. Bridge entries reference this. |
| `rmw`       | string | required | Canonical backend name: `"zenoh"`, `"xrce"`, `"cyclonedds"`. Must match a backend the binary linked in. |
| `locator`   | string | `""` | Backend-specific locator. Zenoh uses `tcp/...`, `udp4/...`, `serial/...`; DDS uses `domain=<n>`; XRCE uses `udp4://...:port`. |
| `domain_id` | u32    | `0` | ROS domain id passed to the backend. |
| `namespace` | string | `"/"` | Default namespace applied to handles created on this node. |

### Locator scheme grammar (zenoh)

| Scheme            | Example                          | Meaning |
|-------------------|----------------------------------|---------|
| `tcp/<host>:<p>`  | `tcp/10.0.0.1:7447`              | TCP unicast |
| `udp/<host>:<p>`  | `udp/10.0.0.1:7447`              | UDP unicast |
| `serial/<dev>`    | `serial//dev/ttyUSB0`            | UART (host-side) or board UART (bare-metal) |
| `tls/<host>:<p>`  | `tls/router.example.org:7447`    | TLS over TCP (requires `link-tls` on backend) |

DDS / XRCE follow their native locator conventions; consult each
backend's docs for the exact form.

## `[[bridge]]`

One block per topic forwarded between two `[[node]]`s. Each bridge
creates a `RawSubscription` on the source side and an
`EmbeddedRawPublisher` on the destination side and pumps bytes once
per executor tick.

| Key         | Type      | Default | Notes |
|-------------|-----------|---------|-------|
| `type`      | string    | required | ROS type name (`"std_msgs/Int32"`). Backends use it for liveliness / discovery. |
| `type_hash` | string    | `""` | ROS 2 RIHS type hash for the typed binding. May be left empty if both sides ignore it. |
| `from`      | endpoint  | required | `{ node = "<name>", topic = "<topic>" }`. The source node + topic. |
| `to`        | endpoint  | required | `{ node = "<name>", topic = "<topic>" }`. The destination node + topic. |

Bidirectional bridges are two `[[bridge]]` entries — one in each
direction. The runtime stamps a `bridge_origin` attachment field on
forwarded frames and drops on receive when the origin matches the
local backend, so an echo pair does not loop.

## Error semantics

`run_from_config` returns `ConfigError`:

| Variant | Cause |
|---------|-------|
| `Io`    | File read failed (missing / unreadable). |
| `Parse` | TOML malformed or a required field missing. |
| `UnknownNode` | A `[[bridge]]` references a `node` name no `[[node]]` declared. |
| `OpenSession` | `Executor::open_multi` rejected a spec — usually a backend name no `RMW_INIT_ENTRIES` entry registered under. |
| `BuildNode` | `create_node_on` failed (registry exhausted, name too long, …). |
| `BuildEntity` | Creating the source subscription or destination publisher failed (typically backend rejection of the topic name / type / QoS). |

Any error short-circuits the runtime; bridges that opened cleanly
before the failure are not pumped.

## Linked-backend matrix

| Backend name | Cargo dep that contributes the `RMW_INIT_ENTRIES` entry |
|--------------|---------------------------------------------------------|
| `zenoh`      | `nros-rmw-zenoh = { ... }` |
| `xrce`       | `nros-rmw-xrce-cffi = { ... }` |
| `cyclonedds` | `nros-rmw-cyclonedds` static lib (CMake side, `--whole-archive`) |

Add a backend to your `Cargo.toml` (or `target_link_libraries`) to
make its name available in the TOML; remove it to fence off use even
when the TOML file lists it (yields `OpenSession` at startup).
