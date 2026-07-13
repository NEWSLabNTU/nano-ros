# Configuration Guide

nano-ros config is **language-agnostic** and has **one home per concern** — no
setting lives in two places, and nothing is merged across files. The authored
surfaces are `Cargo.toml` metadata (Rust) or the CMake `nano_ros_*` functions
(C/C++), a universal `system.toml`, `package.xml`, and launch XML — plus
Kconfig for the embedded build (Zephyr). This guide covers what each file
owns, the embedded `deploy` config both languages bake at build time, and the
standalone `config.toml` for hand-written `no_std` apps.

> Two files you may find in old material are **retired**: the `nros.toml`
> per-package/workspace file (rejected by the CLI; see below) and the old
> `config.toml` `[network]`/`[zenoh]`/`[scheduling]` schema. Design of record:
> [RFC-0004](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0004-configuration-and-transports.md).

## One home per concern

| File | Owns | Per |
|------|------|-----|
| `Cargo.toml` | Rust build: crate, language deps, the RMW **feature menu** (`rmw-zenoh`/`rmw-cyclonedds`/`rmw-xrce`); node identity via `[package.metadata.nros.node]`; entry/boot via `[package.metadata.nros.entry]`; **embedded net config via `[package.metadata.nros.deploy.<target>]`**; workspace membership via `[workspace.metadata.nros]` | Rust project |
| `CMakeLists.txt` | C/C++ build: targets, language deps, the `NROS_RMW` option; node/entry registration via `nano_ros_node_register` / `nano_ros_entry`; **per-target deploy config via `nano_ros_deploy(...)`** | C/C++ project |
| `.cargo/config.toml` | **`[patch.crates-io]` dependency injection only** (written by `nros sync`; local crate + generated-msg paths), plus the cargo `[build]`/`[target]`/`[env]` knobs (target triple, runner, rustflags). **No nano-ros runtime config.** | Rust project |
| `package.xml` | ROS package identity + msg `<depend>`s (codegen input for `nros generate`) | all |
| **`system.toml`** | **System topology** — components, deploy targets, domain, RMW, capability axes (`[safety]`, `[param_services]`), tiers. The language-agnostic universal descriptor (same schema for Rust/C/C++). **Optional for single-node** (the toolchain synthesises an implicit 1-component system when absent). | bringup pkg |
| **`config.toml`** (standalone) | **Hand-written `no_std` direct-mode apps only** — `[node]` / `[[transport]]` / `[node.rt]`, compile-baked via `Config::from_toml(include_str!(…))`. Apps that use `nros::main!()` / codegen do **not** have one — they use the `deploy` metadata above. | embedded single-node app (no codegen) |

> **Boundary rule.** If a knob changes *what is compiled/linked*, it lives in the
> build file (`Cargo.toml` feature / `CMakeLists.txt` option). If it changes the
> *system topology* (components, deploy, domain, RMW), it lives in `system.toml`.
> If it is the *physical link + router address a target boots with*, it lives in
> the `deploy` config (`[package.metadata.nros.deploy.<t>]` / `nano_ros_deploy`).

### Config home by language × scale

Mirrors [RFC-0004 §3](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0004-configuration-and-transports.md):

| | Single-node | Workspace |
|---|---|---|
| **Rust** | `Cargo.toml [package.metadata.nros.{node,entry,deploy.<t>}]` (+ `nros::main!`); optional `system.toml` to pin rmw/domain | root `[workspace.metadata.nros]` + node `[package.metadata.nros.node]` + entry `[package.metadata.nros.entry]` + bringup `system.toml` |
| **C / C++** | `CMakeLists.txt` (`NANO_ROS_PLATFORM/RMW`, `nano_ros_deploy`) + `package.xml`; optional `system.toml` | `nano_ros_node_register` / `nano_ros_entry` per pkg + **same `system.toml`** + `package.xml` |

Where a concern has both a native-idiom projection and a `system.toml`, the
resolution is a **fixed precedence ladder**, not a merge: explicit CLI/build
flag (`--rmw` / `-DNANO_ROS_*`) > `system.toml` (`[deploy.<t>]` > `[system]`)
> the per-package projection (`[package.metadata.nros.*]` / CMake) > built-in
default. `nros config show` prints the resolved effective config with
per-value provenance; `nros check` flags values still sourced from legacy
files.

Full design rationale: [RFC-0004 (configuration & transports)](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0004-configuration-and-transports.md);
RMW backend selection & lowering is [RFC-0031](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0031-rmw-selection-and-lowering.md).
For the multi-RMW runtime topic-forwarding bridge (a separate file/feature), see
[`nros-bridge.toml`](../reference/nros-bridge-toml.md).

## Embedded deploy config (codegen apps — the common case)

An embedded app built through `nros::main!()` (Rust) or the CMake entry
codegen (C/C++) declares its network + router config **per deploy target** in
its build manifest. The toolchain bakes it at compile time — there is no
config file on the device and nothing is parsed at runtime.

**Rust** — `[package.metadata.nros.deploy.<target>]` in the app's
`Cargo.toml`. `nros::main!()` bakes the block into a `DeployOverlay` that
`BoardEntry::run_with_deploy` applies onto the board's boot `Config`:

```toml
# Cargo.toml (e.g. examples/stm32f4/rust/talker)
[package.metadata.nros.node]
class = "talker_pkg::Talker"
name  = "talker"

[package.metadata.nros.deploy.stm32f4]
board   = "stm32f4"            # board crate (optional where unambiguous)
rmw     = "zenoh"
locator = "tcp/192.168.1.1:7447"
ip      = "192.168.1.10"
gateway = "192.168.1.1"
netmask = "255.255.255.0"
# domain_id = 0
```

**C/C++** — `nano_ros_deploy()` in the app's `CMakeLists.txt` (one call per
deploy target):

```cmake
# CMakeLists.txt (e.g. examples/qemu-arm-freertos/c/talker)
nano_ros_deploy(
    TARGET     freertos
    RMW        ${NROS_RMW}
    DOMAIN_ID  0
    LOCATOR    "tcp/10.0.2.2:7447")
```

RT/stack/priority for a single-node embedded app comes from **board-crate
Cargo features + Kconfig** (`prj*.conf` on Zephyr), not a config file.
Multi-node RT is declared in `system.toml` (`[tiers.<name>.<rtos>]` +
`[[node_overrides]]`) — see RFC-0015.

## Standalone `config.toml` — hand-written `no_std` apps (no codegen)

A hand-written single-node app that writes its own `main()` and bypasses
`nros::main!()` keeps its net config **in a file, not hardcoded in Rust**: a
sibling `config.toml` carrying `[node]` / `[[transport]]` / `[node.rt]`,
compile-baked with `include_str!` and parsed by the board crate's
`Config::from_toml`.

```toml
# config.toml

[node]
domain_id = 0              # ROS 2 domain ID (0–232)
# namespace = "/"
# rmw = "zenoh"            # ACTIVE backend; must match a LINKED backend
                           # (the build file picks what is linked)

# One transport per session. A single ethernet/wifi/serial entry is the
# common case. In-process topic forwarding is the separate [[bridge]] path.
[[transport]]
kind    = "ethernet"       # ethernet | wifi | serial | can
ip      = "10.0.2.10/24"   # CIDR — the prefix rides the address
mac     = "02:00:00:00:00:00"
gateway = "10.0.2.2"
locator = "tcp/10.0.2.2:7447"

[node.rt]                  # scheduling / real-time (RTOS); omit for defaults
app_priority         = 12
app_stack_bytes      = 65536
zenoh_read_priority  = 16
zenoh_lease_priority = 16
poll_priority        = 16
poll_interval_ms     = 5
```

Per-kind transport fields:

| kind | fields |
|------|--------|
| `ethernet` | `ip` (CIDR), `mac`, `gateway`, `interface` |
| `wifi` | `ssid`, `password`, optional static `ip`/`gateway` |
| `serial` / `can` | `device`, `baudrate` |
| all | `id`, `rmw`, `locator` |

`ip` is CIDR (`10.0.2.10/24`) — the board derives the prefix or netmask from it.
A serial locator carrying `#` (`serial/UART_0#baudrate=115200`) is fine — quote it.

Consumption (compile-baked; no filesystem on the target):

```rust
use nros_board_mps2_an385::{Config, run};

fn main() -> ! {
    run(Config::from_toml(include_str!("../config.toml")), |config| {
        let exec = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id);
        // ...
    })
}
```

Pick by whether the app uses codegen: `nros::main!()` app → `deploy` metadata
(above); hand-written `main()` → `config.toml`. Only the **old**
`config.toml` schema (`[network]`/`[zenoh]`/`[scheduling]`) is retired.

## Retired files

- **`nros.toml` — retired in full** (phase-256). A workspace-root `nros.toml`
  is rejected by the CLI (`NrosTomlNotSupported`; migrate with the
  `nros-v0.5.0` tag's `nros migrate workspace` — the one-shot verb is retired
  on newer trees, #186); the legacy per-package overlay is a deprecated fallback that
  `nros check` flags; the embedded-runtime role never shipped (no example
  ever declared one). If a doc tells you to write `nros.toml`, it predates
  the migration — the content belongs in `deploy` metadata or `system.toml`.
- **Old `config.toml` schema** (`[network]`/`[zenoh]`/`[scheduling]`) —
  retired (Phase 172.K.6); superseded by the `deploy` class (net) + board
  features / Kconfig (RT). The direct-mode `config.toml` above is the kept,
  supported shape.

## Build-time environment variables

Read during `cargo build` (by `build.rs`) or by justfile recipes. Set in `.env`
or export in your shell.

### SDK paths

Auto-resolved by `just setup <platform>`; override if SDKs live elsewhere.

| Variable | Default | Description |
|----------|---------|-------------|
| `FREERTOS_DIR` | `third-party/freertos/kernel` | FreeRTOS kernel source |
| `FREERTOS_PORT` | `GCC/ARM_CM3` | FreeRTOS portable layer |
| `LWIP_DIR` | `third-party/freertos/lwip` | lwIP source |
| `FREERTOS_CONFIG_DIR` | Board crate's `config/` | `FreeRTOSConfig.h` |
| `NUTTX_DIR` / `NUTTX_APPS_DIR` | `third-party/nuttx/…` | NuttX RTOS + apps |
| `THREADX_DIR` / `NETX_DIR` | `third-party/threadx/…` | ThreadX + NetX Duo |
| `THREADX_CONFIG_DIR` / `NETX_CONFIG_DIR` | Board crate's `config/` | `tx_user.h` / `nx_user.h` |

Buffer-tuning vars (`ZPICO_*`, `XRCE_*`, `NROS_*`) are optional — see the
[Environment Variables Reference](../reference/environment-variables.md).

### Binary-size knobs (embedded)

On a constrained MCU, two build-time env vars (set in the example's
`.cargo/config.toml` `[env]`, like the other `NROS_*` tuning) shed the parts a
brokered client doesn't need:

| Variable | Default | Effect |
|----------|---------|--------|
| `NROS_LINK_IP` | on | `NROS_LINK_IP=0` on a **serial-only** node drops the IP link layer — zenoh-pico's TCP/UDP link C (`Z_FEATURE_LINK_TCP/UDP=0`) and (with `--gc-sections`) the smoltcp platform impl. Serial link stays. |
| `NROS_SMOLTCP_MAX_SOCKETS` / `NROS_SMOLTCP_MAX_UDP_SOCKETS` | 4 / 2 | Sized for DDS RTPS (3 UDP/participant). A zenoh/XRCE client multiplexes everything over **one** session → set both to `1` to drop the spare socket buffers (≈8 KB each). |
| `NROS_HEAP_SIZE` | per-board (64 KB mps2-an385, 32 KB stm32f4) | Decimal **bytes** for the bare-metal static heap. The defaults are generous; size to the RMW's working set (table below). E.g. `NROS_HEAP_SIZE = "24576"` on a zenoh-pico node cut the mps2-an385 `.data` 66 → 25 KB (−41 KB). |

**Static-heap sizing by backend** (bare-metal `FreeListHeap`, set via
`NROS_HEAP_SIZE`):

| Backend | Peak working set | Recommended heap | Notes |
|---------|------------------|------------------|-------|
| zenoh-pico (TCP) | ~16 KB | **24–32 KB** (≈2× for fragmentation) | peer middleware; `alloc`-based session/buffers |
| zenoh-pico (serial) | lighter than TCP | **16–24 KB** | no TCP link buffers; verified running at 16 KB |
| XRCE (Micro-XRCE-DDS) | ~3 KB (micro-ROS figure) | **~8 KB** | static pools, discovery offloaded to the agent — the RAM-minimal backend; a measured bare-metal XRCE figure is pending an example (no bare-metal XRCE example ships yet — XRCE bare-metal needs a custom-transport injection) |

### Measured footprint

Honest, reproducible numbers per `(platform, transport, backend, profile)` —
built with the in-tree examples, the **release** profile is cargo's default
(opt-3), the **size** profile is the scaffolded `[profile.size]` (opt-`s` + `lto`
+ `strip`, see [Phase 204.3](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/roadmap/phase-204-embedded-binary-size.md#2043--size-tuned-embedded-release-profile)).
RAM = `data + bss`. All cells are after `--gc-sections` + the size knobs above
are applied where noted; the serial cell ships with the recipe below.

| platform | transport | backend | profile | text (flash code) | data | bss | RAM total |
|---|---|---|---|---|---|---|---|
| qemu-arm-baremetal (mps2-an385, cortex-m3) | ethernet (smoltcp) | zenoh-pico | release | 177.4 KB | 67.0 KB | 91.7 KB | **158.7 KB** |
| qemu-arm-baremetal | ethernet | zenoh-pico | size | **158.3 KB** | 67.0 KB | 91.7 KB | 158.7 KB |
| qemu-arm-baremetal | **serial** (no IP stack) | zenoh-pico | release | 128.6 KB | 25.2 KB | 75.8 KB | **101.0 KB** |
| qemu-arm-baremetal | **serial** | zenoh-pico | **size** + recipe | **116.1 KB** | 25.2 KB | 75.8 KB | **101.0 KB** |
| stm32f4 (thumbv7em-eabihf, cortex-m4) | ethernet | zenoh-pico | release | 186.9 KB | 13.7 KB | 123.0 KB | 136.7 KB |
| stm32f4 | ethernet | zenoh-pico | size | **138.1 KB** | 13.7 KB | 123.0 KB | 136.7 KB |
| qemu-arm-freertos (cortex-m3 + lwIP, RTOS-reused stack) | ethernet (lwIP) | zenoh-pico | release | 240.6 KB | 10.7 KB | 3.3 MB | 3.3 MB |
| **qemu-arm-baremetal (Phase 207)** | **serial** (custom XRCE transport) | **XRCE** | **size**, heap 24 KB, tight XRCE pools | **60.3 KB** | 25.2 KB (heap 24 KB) | 8.8 KB | **~34 KB** |
| **micro-ROS reference** (XRCE) | serial | XRCE-DDS Client | -Os | < 75 KB | — | ~3 KB | ~3 KB peak |

The XRCE row uses the Phase 207.6 tight per-session pools — set in the
example's `.cargo/config.toml` `[env]` and read by `nros-rmw-xrce-cffi`'s
`build.rs`: `NROS_XRCE_STREAM_HISTORY=4`,
`NROS_XRCE_CUSTOM_TRANSPORT_MTU=512`, `NROS_XRCE_MAX_SUBSCRIBERS=1`,
`NROS_XRCE_MAX_SERVICE_SERVERS=1`, `NROS_XRCE_MAX_SERVICE_CLIENTS=1`,
`NROS_XRCE_SUBSCRIBER_RING_DEPTH=1`, `NROS_XRCE_BUFFER_SIZE=256`. Vendor
defaults grow `xrce_session_state_t` to ~390 KB (which wouldn't fit a
24 KB heap); these knobs drop it to ~12 KB. Defaults are unchanged for
hosted / non-tight-RAM consumers — the env vars are pure opt-in.

**How to read this:**

- **The size profile (opt-`s`) shrinks `.text` by ~10–26 %** with `.bss`/`.data`
  unchanged (opt-level doesn't touch static buffers — those are the env knobs
  above). `-Oz` is **not** used — on smoltcp examples it grows `.bss` +24 KB by
  defeating opt-3's per-socket dead-buffer DCE (see Phase 204.3).
- **Switching ethernet → serial sheds ~50 KB text + ~42 KB `.data`** (no smoltcp
  stack, no IP link C, tuned heap) — the structural lever.
- **FreeRTOS + lwIP cells `.bss` is dominated by lwIP's heap + FreeRTOS task
  stacks** (3 MB is the configured headroom, not nano-ros overhead).
- **The micro-ROS / XRCE row is a reference**, not a nano-ros measurement — no
  bare-metal XRCE example ships yet (needs a custom-transport injection); the
  path to parity is **XRCE + serial + static pools**.

### Size-minimal recipe

Smallest measured nano-ros configuration today (qemu-arm-baremetal serial
talker, **116 KB text / 101 KB RAM**):

```toml
# Cargo.toml
[profile.size]
inherits = "release"
opt-level = "s"
lto = "fat"
codegen-units = 1
debug = false
strip = true
```

```toml
# .cargo/config.toml — gc + serial knobs
[target.thumbv7m-none-eabi]
rustflags = [
    "-C", "link-arg=--gc-sections",   # 204.8 — strip unreferenced fns/data
    "-C", "link-arg=-Tlink.x",
]

[env]
NROS_LINK_IP        = "0"      # 204.7 — drop zenoh-pico TCP/UDP link C
ZPICO_NO_SMOLTCP    = "1"      # skip smoltcp glue on bare-metal
# Heap floor: the phase-271 per-entry executor backing is a single ~75 KB
# allocation, so a `nros::main!` image needs ≥128 KB (the #176 board
# default) — the pre-271 24 KB "zenoh-pico working set" figure OOMs at
# boot. HEAP is `.bss` (no flash cost); shrink below the default only on
# a non-`nros::main!` direct-mode image with a measured smaller peak.
NROS_HEAP_SIZE      = "131072"
NROS_SMOLTCP_MAX_SOCKETS     = "1"   # 204.2 — brokered client multiplexes
NROS_SMOLTCP_MAX_UDP_SOCKETS = "1"
```

Build with `cargo build --profile size`, or fleet-wide via
`NROS_CARGO_PROFILE=size just <plat> build`. `nros new --platform baremetal`
already scaffolds the `[profile.size]` + the `.cargo/config.toml` shape (Phase
204.7/204.8); uncomment the serial block when you swap to a serial transport.

**The deeper RAM win waits on XRCE on bare-metal** (the ~3 KB-class client +
static pools, with discovery offloaded to the agent) — tracked separately;
zenoh-pico's `SUBSCRIBER_BUFFERS` + alloc-based session are what keep this row's
`.bss` ~76 KB.

## Cargo features (which RMW/platform is *linked*)

Features select the **linked** RMW backend, platform, and ROS edition. The
`deploy` config's `rmw` (or `system.toml [system].rmw`) picks which *linked*
backend is *active* — the two are different layers (link vs run). Matrix +
mutual-exclusion rules: [Platform Model](../concepts/platform-model.md).

```toml
[dependencies]
nros = { path = "…/nros", default-features = false, features = [
    "rmw-cffi",            # generic C-vtable runtime registry
    "platform-bare-metal", # or platform-{freertos,nuttx,threadx,zephyr,posix}
    "ros-humble",          # or ros-iron
    "std", "alloc",        # optional, target-dependent
] }
# Exactly one RMW backend crate; its registration runs before main:
nros-rmw-zenoh = { path = "…/nros-rmw-zenoh", features = ["platform-bare-metal", "link-tcp", "ros-humble"] }
# …or nros-rmw-xrce-cffi / the cyclonedds CMake backend
```

## Runtime environment (POSIX only)

On Linux/*BSD, `ExecutorConfig::from_env()` reads at process start (embedded
targets bake their config at build time instead):

| Variable | Description | Default |
|----------|-------------|---------|
| `ROS_DOMAIN_ID` | ROS 2 domain ID | `0` |
| `NROS_LOCATOR` | Router address (legacy alias `ZENOH_LOCATOR`) | `tcp/127.0.0.1:7447` |
| `NROS_SESSION_MODE` | `client` / `peer` (legacy alias `ZENOH_MODE`) | `client` |
| `ZENOH_TLS_ROOT_CA_CERTIFICATE*` | TLS CA cert (path / base64) | (none) |

## By deployment scenario

| Scenario | Config source | Cargo features | Notes |
|----------|---------------|----------------|-------|
| Desktop (POSIX) | env (`ExecutorConfig::from_env()`) | `rmw-cffi, platform-posix, std` + zenoh dep | run zenohd locally (`just native zenohd`) |
| QEMU bare-metal | `[package.metadata.nros.deploy.<t>]` ip/mac/gateway/locator | `rmw-cffi, platform-bare-metal, ros-humble` + zenoh | TAP/slirp bridge |
| FreeRTOS hardware | `deploy` metadata / `nano_ros_deploy` + board features (RT) | `…, platform-freertos, …` | `FREERTOS_DIR`/`LWIP_DIR` |
| ESP32 WiFi | `deploy` metadata (`ssid`/`password` via build env) | `…, platform-bare-metal, …` | `SSID`/`PASSWORD` build env |
| Zephyr module | Kconfig overlay (`prj-<rmw>.conf`) | (Kconfig → features) | |
| Hand-written `no_std` (no codegen) | standalone `config.toml` → `Config::from_toml` | per-board | net config in a file, not in code |
| Minimal RAM (XRCE serial) | `config.toml [[transport]] kind="serial"` or `deploy` | `…` + xrce dep | `XRCE_*` buffer tuning |

## `.env`

```bash
cp .env.example .env   # uncomment + adjust; gitignored; auto-loaded by just + direnv
```
