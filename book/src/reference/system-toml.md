# system.toml keys

<!-- type: reference · level: 3 -->

The keys a bringup's `system.toml` takes for an image, the files beside it,
and a bridge binary's `nros-bridge.toml`. The page is hand-kept; a gate holds
the `[image.<id>]` table's key set to the CLI's own schema, both ways, so a key
the CLI accepts cannot be missing here and a key it refuses cannot be listed.
The Kconfig symbols an image can also set are in
[Configuration surface](configuration-surface.md).

## Where each setting lives

| File | Owns | Per |
| --- | --- | --- |
| `system.toml` | The deployment: `[system]` (RMW, domain, capability `features`, `[safety]`, sched tasks), `[[component]]` (what runs), and one `[image.<id>]` per target with its `.run`, `.flash`, `.console` and `.knobs` tables. One schema for Rust, C and C++ | bringup package |
| `Cargo.toml` | Rust toolchain facts only: the crate, its language dependencies, the RMW feature menu. No board, no target triple | Rust node package |
| `CMakeLists.txt` | C and C++ toolchain facts only: targets and language dependencies. No platform, board or RMW `set()` | C or C++ node package |
| `package.xml` | Package identity and message `<depend>`s | every package |
| `build/<coord>/<image>_entry/nros-cargo.toml` | Generated, never edited: target, runner, rustflags, cross compiler, derived pool sizes as `[env]`, the `nros-*` profiles, the `[patch.crates-io]` rows | image |
| `config.toml` | Hand-written `no_std` apps with their own `main()` only: `[node]`, `[<interface>]`, `[network.<name>]` | application |

A `system.toml` beside a `Cargo.toml` or `CMakeLists.txt` is refused.
Precedence, highest first: a command-line or build flag, `[image.<id>]`,
`[system]`, the board descriptor's `[knobs]`, the built-in default.
`nros config show` prints the resolved values with where each came from.

## `[image.<id>]`

| Key | Meaning |
| --- | --- |
| `board` | nano-ros board id, resolved through the board catalog. Never a framework's own board string |
| `launch` | Launch file, relative to the bringup. Absent: `[system] default_launch` |
| `args` | Launch arguments bound at resolve time, such as `{ host = "robot1" }` |
| `rmw` | Backend for this image. Absent: `[system] rmw` |
| `domain_id`, `locator` | Override `[system]`'s. `locator` is what the image dials |
| `os_config` | Layer-1 files, in order: a board network file (`.toml`), Zephyr `prj-*.conf` or `.overlay` fragments. `conf` is read as its alias |
| `rmw_config` | The middleware's own file for this image, or `{ <domain> = "<file>" }` per `[[domain]]` |
| `panic` | Panic policy: `platform`, `halt` or `own` |
| `before_boot` | `"<pkg>::<fn>"` the generated entry calls before the board boots. Rust only |
| `profile` | cargo or CMake build profile name |
| `variant` | Framework build variant (Zephyr `prj_<buildtype>.conf`) |
| `features` | Capability axes for this image, added to `[system]`'s |
| `serdes`, `ros_edition` | Serialization provider and ROS edition. Absent: the `[system]` value |
| `env` | `{ NAME = "value" }` for an application's own build variable. A registry knob here is an error naming its `knobs` spelling |
| `entry` | The workspace package that is this image's application, when it cannot be derived |
| `producers` | `false` builds without the host board's census recorder and profiler |
| `params_files`, `ros_args` | Parameters and ROS arguments baked into the image for `rclcpp::init` |

Retired and refused by name: `ip`, `gateway`, `netmask`, `transport`,
`[image.<id>.net.<if>]`, and the old `[deploy.<id>]` table.

## Folding over `[image_defaults]`

| Kind of key | How a specific block folds over the defaults |
| --- | --- |
| scalars (`board`, `rmw`, `profile`, …) | replaced |
| `args` | merged |
| `os_config`, `features` | concatenated, defaults first |

`[system] default_images = ["<id>", …]` is what a bare `nros build` builds.
Without it, a workspace with more than one image lists them and stops.

## Board network file

Named by `os_config`, beside `system.toml`. One table per interface, and any
network other than the built-in `qemu`:

| Key | Table | Meaning |
| --- | --- | --- |
| `network` | `[<if>]` | The network the interface is on. Default: the board's (`qemu` on QEMU boards) |
| `host` | `[<if>]` | Last octet within the network's subnet |
| `address` | `[<if>]` | Full IPv4, or `"dhcp"` on a network with `dhcp = true` |
| `mac` | `[<if>]` | Optional. Derived from the board's `mac_prefix` and the image's names when absent |
| `subnet`, `gateway` | `[network.<name>]` | The network's addressing. An interface never states them |
| `dhcp` | `[network.<name>]` | The network leases addresses |

The built-in `qemu` network is QEMU user-mode networking, `10.0.2.0/24`, with
gateway and router host `10.0.2.2`. An interface on `qemu` with no address
takes `10.0.2.15`. The resolved answer is written to
`build/nros/net/<image>.toml`.

Refused, naming the image: an address outside its subnet or equal to the
gateway; a network no file defines; two images with one static address or MAC
on a network other than `qemu`; two files defining one network differently; a
board network file on a board whose network belongs to the host OS (`linux`,
`freertos-posix`, `threadx-linux`) or that declares no interfaces (Zephyr
boards take their address from Kconfig).

## `[image.<id>.run]`, `.flash` and `.console`

| Table | Key | Meaning |
| --- | --- | --- |
| `.run` | `nic` | `"slirp"` (default on `qemu`), `{ bridge = "<dev>" }` or `{ tap = "<dev>" }`, for the first interface under QEMU |
| `.flash` | `command` | Replaces the board's flash method; `{hex}` and other placeholders are checked |
| `.flash` | `prepare` | Human steps before flashing: `{ say = "…", wait_for = "<event>" }` |
| `.console` | `baud`, … | Overrides a key of the board's `[board.console]` |

An image on a network other than `qemu` with no `nic` is physical only, and
launching it under QEMU is refused.

## The middleware's own file

| Backend | Default file, beside `system.toml` | Same document ROS 2 reads as |
| --- | --- | --- |
| zenoh | `rmw/zenoh.json5` | `ZENOH_SESSION_CONFIG_URI` |
| Cyclone DDS | `rmw/cyclonedds.xml` | `CYCLONEDDS_URI` |
| XRCE-DDS | none: the endpoint is the image's `locator` | — |

`[system] rmw_config` names another default file; `[image.<id>] rmw_config`
names one image's. A named file that does not exist is refused. An image with
a zenoh file that also states a `locator` is refused. The flat
`rmw/zenoh.conf` is retired.

## `[image.<id>.knobs.<group>]`

The same shape as a board descriptor's `[knobs.<group>]`. The key is the knob
registry row's board key where it has one (`executor.max_cbs`); otherwise the
`NROS_*` name without `NROS_`, split at the first `_`
(`NROS_ZEPHYR_HEAP_SIZE` is `zephyr.heap_size`). An unknown key, a wrong type
or an out-of-range value stops `nros build`, naming the key. The image rung
sits above the board and below an exported variable.

## Standalone `config.toml`

| Table | Keys |
| --- | --- |
| `[node]` | `domain_id`, `locator` |
| `[<interface>]` | as the board network file, plus a board's own facts (`[uart0] baud`, `[eth0] device` on ThreadX Linux) |
| `[network.<name>]` | `subnet`, `gateway` |
| `[node.rt.<rtos>]` | raw task priorities, such as `app_priority`, `zenoh_read_priority` on FreeRTOS |

Read with `Config::from_toml(include_str!("../config.toml"))`. A file in the
retired `[[transport]]` shape panics at boot, naming the new spelling.

## Cargo features of the `nros` crate

| Feature | Selects |
| --- | --- |
| `rmw-cffi` | the C-vtable RMW registry the backend registers into |
| `platform-bare-metal`, `platform-freertos`, `platform-nuttx`, `platform-threadx`, `platform-zephyr`, `platform-posix` | the platform |
| `ros-humble`, `ros-iron` | the ROS edition |
| `std`, `alloc` | optional, by target |

The generated entry selects these from the image. `rmw` in `system.toml`
picks which linked backend is active.

## `nros-bridge.toml`

A bridge binary's own file, beside `system.toml`'s keys because it configures
the same thing, a running image, but read at run time by the program, not by
the build.

Consumed by `nros::run_from_config` (Rust) and the future C/C++ mirror
(`nros::run_from_config` in `nros-cpp`).

> **Not the orchestration `nros.toml`.** This is the **bridge** runtime
> config (multi-RMW byte forwarding). It is named `nros-bridge.toml` to
> avoid colliding with the orchestration `nros.toml`
> (component/system build config) — different lifecycle, different
> schema.

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

### Top-level structure

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

### `[[node]]`

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

#### Locator scheme grammar (zenoh)

| Scheme            | Example                          | Meaning |
|-------------------|----------------------------------|---------|
| `tcp/<host>:<p>`  | `tcp/10.0.0.1:7447`              | TCP unicast |
| `udp/<host>:<p>`  | `udp/10.0.0.1:7447`              | UDP unicast |
| `serial/<dev>`    | `serial//dev/ttyUSB0`            | UART (host-side) or board UART (bare-metal) |
| `tls/<host>:<p>`  | `tls/router.example.org:7447`    | TLS over TCP (requires `link-tls` on backend) |

DDS / XRCE follow their native locator conventions; consult each
backend's docs for the exact form.

### `[[bridge]]`

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

### Error semantics

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

### Linked-backend matrix

| Backend name | Cargo dep that contributes the `RMW_INIT_ENTRIES` entry |
|--------------|---------------------------------------------------------|
| `zenoh`      | `nros-rmw-zenoh = { ... }` |
| `xrce`       | `nros-rmw-xrce-cffi = { ... }` |
| `cyclonedds` | `nros-rmw-cyclonedds` static lib (CMake side, `--whole-archive`) |

Add a backend to your `Cargo.toml` (or `target_link_libraries`) to
make its name available in the TOML; remove it to fence off use even
when the TOML file lists it (yields `OpenSession` at startup).

## Design background

- [RFC-0004: configuration and transports](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0004-configuration-and-transports.md)
- [RFC-0065: colcon-like workspace builder](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0065-colcon-like-workspace-builder.md)
- [RFC-0098: generated leaf build config](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0098-generated-leaf-build-config.md)
- [RFC-0109: network config layers](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0109-network-config-layers.md)
- [RFC-0118: target execution model](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0118-target-execution-model.md)
