# Phase 19: Transport Session Configuration

## Overview

Extend `TransportConfig` and the zenoh C shim to support backend-specific session configuration (scouting, listen endpoints, timestamps, TLS). The design keeps the ROS-level API transport-agnostic to allow future DDS or other backends.

**Status**: In Progress (Phase 19a complete)

## Problem Statement

`zenoh_shim_init()` currently accepts only a locator string and hardcodes all other session options. This causes issues:

1. **Multicast scouting** is enabled by default in zenoh-pico. Clients discover and connect to unintended routers, breaking parallel test isolation and causing problems on networks without multicast support.
2. **No listen endpoint** — peer-mode nodes can't accept incoming connections.
3. **No scouting control** — timeout, discovery targets, and multicast address are all defaults.
4. **No TLS/auth** — zenoh-pico supports TLS (compile-time) and user/password (runtime), but neither is exposed.

Users on embedded systems need explicit control over these options since there are no environment variables available.

## Design

### Principle

The ROS node API (`InitOptions`, `Context`) stays transport-agnostic. Backend-specific configuration is passed through generic key-value properties that each transport implementation interprets.

```
┌─────────────────────────────────────────────────┐
│  Node API (transport-agnostic)                  │
│  InitOptions: locator, mode, domain_id          │
│               + .property(key, value)            │
├─────────────────────────────────────────────────┤
│  TransportConfig (transport-agnostic)           │
│  locator, mode, properties: &[(&str, &str)]     │
├──────────────────┬──────────────────────────────┤
│  Zenoh backend   │  Future backends             │
│  interprets:     │  interpret their own          │
│  multicast_*     │  property keys                │
│  scouting_*      │                              │
│  listen          │                              │
│  add_timestamp   │                              │
└──────────────────┴──────────────────────────────┘
```

### Layer 1: TransportConfig (no_std, all platforms)

Add a generic properties slice to the existing struct:

```rust
// packages/core/nano-ros-transport/src/traits.rs

pub struct TransportConfig<'a> {
    pub locator: Option<&'a str>,
    pub mode: SessionMode,
    /// Backend-specific key-value properties.
    /// Interpreted by the transport implementation.
    pub properties: &'a [(&'a str, &'a str)],
}
```

### Layer 2: InitOptions (node API)

Add a property pass-through to the builder:

```rust
// packages/core/nano-ros-node/src/context.rs

impl InitOptions {
    // Existing (transport-agnostic):
    pub fn locator(self, locator: &'static str) -> Self
    pub fn session_mode(self, mode: SessionMode) -> Self
    pub fn domain_id(self, id: u32) -> Self

    // New (backend-specific pass-through):
    pub fn property(self, key: &'static str, value: &'static str) -> Self
}
```

Usage:

```rust
let ctx = Context::new(
    InitOptions::new()
        .locator("tcp/192.168.1.1:7447")
        .property("multicast_scouting", "false")
        .property("scouting_timeout_ms", "5000")
)?;
```

### Layer 3: Environment variables (std platforms)

#### ROS-standard variables (transport-agnostic)

These follow the [ROS 2 environment variable conventions](https://docs.ros.org/en/rolling/Tutorials/Beginner-CLI-Tools/Configuring-ROS2-Environment.html) and are read by `Context::from_env()`:

| Variable                        | Description              | Values                                         | Default  | ROS edition |
|---------------------------------|--------------------------|------------------------------------------------|----------|-------------|
| `ROS_DOMAIN_ID`                 | Domain isolation (0–101) | Integer                                        | `0`      | Foxy+       |
| `ROS_LOCALHOST_ONLY`            | Restrict to loopback     | `0`, `1`                                       | `0`      | Foxy+       |
| `ROS_AUTOMATIC_DISCOVERY_RANGE` | Discovery scope          | `SUBNET`, `LOCALHOST`, `OFF`, `SYSTEM_DEFAULT` | `SUBNET` | Iron+       |
| `ROS_STATIC_PEERS`              | Explicit peer addresses  | Semicolon-separated locators                   | none     | Iron+       |

nano-ros will support feature gates for ROS edition compatibility:
- `ros-humble` (default): `ROS_DOMAIN_ID`, `ROS_LOCALHOST_ONLY`
- `ros-iron`: Adds `ROS_AUTOMATIC_DISCOVERY_RANGE`, `ROS_STATIC_PEERS`

When `ROS_AUTOMATIC_DISCOVERY_RANGE` is set, it supersedes `ROS_LOCALHOST_ONLY` (matching Iron+ semantics).

**Mapping to transport config:**

| ROS variable                              | Effect on zenoh backend                                           |
|-------------------------------------------|-------------------------------------------------------------------|
| `ROS_LOCALHOST_ONLY=1`                    | Sets locator to `tcp/127.0.0.1:7447`, disables multicast scouting |
| `ROS_AUTOMATIC_DISCOVERY_RANGE=LOCALHOST` | Same as `ROS_LOCALHOST_ONLY=1` (Iron+)                            |
| `ROS_AUTOMATIC_DISCOVERY_RANGE=OFF`       | Disables multicast scouting, requires explicit locator (Iron+)    |
| `ROS_STATIC_PEERS`                        | Sets connect endpoints (maps to `Z_CONFIG_CONNECT_KEY`) (Iron+)   |

Note: `ROS_AUTOMATIC_DISCOVERY_RANGE` and `ROS_STATIC_PEERS` are [not supported by rmw_zenoh](https://docs.ros.org/en/rolling/Tutorials/Advanced/Improved-Dynamic-Discovery.html) in standard ROS 2, but nano-ros can implement equivalent behavior since it controls the full transport stack.

**Implementation note:** All edition-gated code must include a comment indicating the edition requirement:

```rust
// ROS edition: Iron+
#[cfg(feature = "ros-iron")]
if let Ok(range) = std::env::var("ROS_AUTOMATIC_DISCOVERY_RANGE") {
    // ...
}
```

#### nano-ros transport variables

| Variable        | Description         | Default              |
|-----------------|---------------------|----------------------|
| `ZENOH_LOCATOR` | Connection endpoint | `tcp/127.0.0.1:7447` |
| `ZENOH_MODE`    | `client` or `peer`  | `client`             |

#### Backend-specific variables (read by the zenoh transport layer, not `Context`)

| Variable                   | Property key          | Example         |
|----------------------------|-----------------------|-----------------|
| `ZENOH_MULTICAST_SCOUTING` | `multicast_scouting`  | `false`         |
| `ZENOH_SCOUTING_TIMEOUT`   | `scouting_timeout_ms` | `3000`          |
| `ZENOH_LISTEN`             | `listen`              | `tcp/0.0.0.0:0` |

### Layer 4: C shim API

```c
typedef struct {
    const char *key;
    const char *value;
} zenoh_shim_property_t;

// New: accepts generic properties
int32_t zenoh_shim_init_with_config(
    const char *locator,
    const char *mode,
    const zenoh_shim_property_t *properties,
    size_t num_properties
);

// Existing: backward compat (calls init_with_config internally)
int32_t zenoh_shim_init(const char *locator);
```

The shim iterates properties and calls `zp_config_insert()` for recognized keys.

### Zenoh Property Keys

Properties recognized by the zenoh backend:

| Property key          | zenoh-pico key                    | Values              | Default                  |
|-----------------------|-----------------------------------|---------------------|--------------------------|
| `multicast_scouting`  | `Z_CONFIG_MULTICAST_SCOUTING_KEY` | `"true"`, `"false"` | `"true"`                 |
| `scouting_timeout_ms` | `Z_CONFIG_SCOUTING_TIMEOUT_KEY`   | milliseconds        | `"1000"`                 |
| `multicast_locator`   | `Z_CONFIG_MULTICAST_LOCATOR_KEY`  | `"udp/ip:port"`     | `"udp/224.0.0.224:7446"` |
| `listen`              | `Z_CONFIG_LISTEN_KEY`             | locator string      | none                     |
| `add_timestamp`       | `Z_CONFIG_ADD_TIMESTAMP_KEY`      | `"true"`, `"false"` | `"false"`                |
| `session_zid`         | `Z_CONFIG_SESSION_ZID_KEY`        | hex UUID            | random                   |

Unknown keys are silently ignored.

## Phased Implementation

### Phase 19a: Core properties API

- [x] Add `properties` field to `TransportConfig`
- [x] Add `.property()` to `InitOptions`
- [x] Add `zenoh_shim_init_with_config()` to C shim
- [x] Wire properties through the Rust → C boundary
- [x] Update existing code to pass empty properties

### Phase 19b: ROS-standard env vars

- [ ] Implement `ROS_LOCALHOST_ONLY` in `Context::from_env()` (Humble+, always enabled)
- [ ] Implement `ROS_AUTOMATIC_DISCOVERY_RANGE` behind `#[cfg(feature = "ros-iron")]`
- [ ] Implement `ROS_STATIC_PEERS` behind `#[cfg(feature = "ros-iron")]`
- [ ] Default feature: `ros-humble`; `ros-iron` implies `ros-humble`

### Phase 19c: Zenoh backend env vars

- [ ] Zenoh transport reads `ZENOH_MULTICAST_SCOUTING`, `ZENOH_SCOUTING_TIMEOUT`, `ZENOH_LISTEN` on std platforms
- [ ] Merge env vars with explicit properties (explicit wins)

### Phase 19d: TLS support (future)

- [ ] Add TLS property keys behind `tls` feature gate
- [ ] Maps to `Z_CONFIG_TLS_*` keys (requires zenoh-pico compiled with `Z_FEATURE_LINK_TLS=1`)

### Phase 19e: Auth support (future)

- [ ] Add `user` and `password` property keys
- [ ] Maps to `Z_CONFIG_USER_KEY` / `Z_CONFIG_PASSWORD_KEY`

## Context: ROS 2 Middleware Configuration

### ROS 2 standard environment variables

All ROS 2 client libraries recognize these networking variables:

| Variable                        | Introduced | Scope    | nano-ros feature | Description                                   |
|---------------------------------|------------|----------|------------------|-----------------------------------------------|
| `ROS_DOMAIN_ID`                 | Foxy       | All RMWs | `ros-humble`     | Domain isolation (0–101)                      |
| `ROS_LOCALHOST_ONLY`            | Foxy       | DDS RMWs | `ros-humble`     | Restrict to loopback interface                |
| `ROS_AUTOMATIC_DISCOVERY_RANGE` | Iron       | DDS RMWs | `ros-iron`       | `SUBNET`/`LOCALHOST`/`OFF`/`SYSTEM_DEFAULT`   |
| `ROS_STATIC_PEERS`              | Iron       | DDS RMWs | `ros-iron`       | Explicit peer addresses (semicolon-separated) |

`ROS_AUTOMATIC_DISCOVERY_RANGE` and `ROS_STATIC_PEERS` supersede `ROS_LOCALHOST_ONLY` (Iron+). Note that rmw_zenoh does not currently support these variables — it uses its own config files instead.

nano-ros edition feature gates follow the ROS 2 release timeline:
- `ros-humble`: Baseline. `ROS_DOMAIN_ID` + `ROS_LOCALHOST_ONLY`.
- `ros-iron`: Adds `ROS_AUTOMATIC_DISCOVERY_RANGE` + `ROS_STATIC_PEERS`. Implies `ros-humble`.
- Future editions (Jazzy, Kilted, etc.) can add further feature gates as needed.

### Middleware-specific configuration

| Middleware | Config format   | Env var                    | Approach                       |
|------------|-----------------|----------------------------|--------------------------------|
| CycloneDDS | XML file        | `CYCLONEDDS_URI`           | File path or inline XML        |
| rmw_zenoh  | JSON5 file      | `ZENOH_SESSION_CONFIG_URI` | File path to full zenoh config |
| rmw_zenoh  | Key=value       | `ZENOH_CONFIG_OVERRIDE`    | Semicolon-separated overrides  |
| nano-ros   | Key-value pairs | `ZENOH_*` per-key          | Properties API + env vars      |

nano-ros uses key-value properties instead of file-based config because:
- zenoh-pico can't parse JSON5 (no parser in the library)
- Bare-metal targets have no filesystem
- Key-value pairs are `no_std`-compatible and zero-allocation

### References

- [ROS 2 Configuring Environment](https://docs.ros.org/en/jazzy/Tutorials/Beginner-CLI-Tools/Configuring-ROS2-Environment.html)
- [ROS 2 Improved Dynamic Discovery](https://docs.ros.org/en/rolling/Tutorials/Advanced/Improved-Dynamic-Discovery.html)
- [rmw_zenoh Configuration](https://github.com/ros2/rmw_zenoh)
- [Zenoh Configuration Manual](https://zenoh.io/docs/manual/configuration/)
- [zenoh-pico Configuration](https://github.com/eclipse-zenoh/zenoh-pico)

## Files to Change

| File                                             | Change                                                                                                                                             |
|--------------------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------|
| `packages/core/nano-ros-transport/src/traits.rs`        | Add `properties` to `TransportConfig`                                                                                                              |
| `packages/core/nano-ros-node/src/context.rs`            | Add `.property()` to `InitOptions`, wire to `TransportConfig`; implement `ROS_LOCALHOST_ONLY`, `ROS_AUTOMATIC_DISCOVERY_RANGE`, `ROS_STATIC_PEERS` |
| `packages/transport/zenoh-pico-shim-sys/c/shim/zenoh_shim.c` | Add `zenoh_shim_init_with_config()`                                                                                                                |
| `packages/transport/zenoh-pico-shim-sys/c/shim/zenoh_shim.h` | Declare new types and function                                                                                                                     |
| `packages/transport/zenoh-pico-shim/src/lib.rs`              | FFI binding for new init function                                                                                                                  |
| `packages/core/nano-ros-transport/src/shim.rs`          | Pass properties through to C shim; read backend env vars                                                                                           |
