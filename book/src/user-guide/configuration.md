# Configuration Guide

nano-ros configuration spans four layers, from runtime to compile-time.
This guide explains what each layer controls, how they interact, and
which variables matter for each deployment scenario.

## Configuration Layers

```
┌─────────────────────────────────────────────┐
│  Layer 1: config.toml (per-example)         │  Network, zenoh, scheduling
│  Baked into binary at compile time          │
├─────────────────────────────────────────────┤
│  Layer 2: Environment variables (.env)      │  SDK paths, buffer tuning
│  Read at build time by build.rs / justfile  │
├─────────────────────────────────────────────┤
│  Layer 3: Cargo features                    │  RMW backend, platform, std/alloc
│  Selected in Cargo.toml                     │
├─────────────────────────────────────────────┤
│  Layer 4: Runtime (POSIX only)              │  ExecutorConfig::from_env()
│  Read from environment at process startup   │
└─────────────────────────────────────────────┘
```

On embedded targets, all configuration is resolved at compile time
(layers 1–3). Layer 4 only applies to POSIX (Linux/macOS) where the
process has access to environment variables at runtime.

## Layer 1: config.toml

Each example includes a `config.toml` that defines hardware and network
settings. The file is embedded into the binary via `include_str!` (Rust)
or parsed by CMake at configure time (C/C++).

See the config.toml section below for
the full format reference.

### Network Section

```toml
[network]
ip = "192.0.3.10"
mac = "02:00:00:00:00:00"
gateway = "192.0.3.1"
prefix = 24
```

| Field | Used By | Description |
|-------|---------|-------------|
| `ip` | All except DHCP | Static IPv4 address |
| `mac` | Bare-metal, FreeRTOS | Ethernet MAC (colon-separated hex) |
| `gateway` | All with static IP | Default gateway for zenohd routing |
| `prefix` | Bare-metal, NuttX, ESP32 | CIDR subnet prefix length |
| `netmask` | FreeRTOS only | Dotted-quad subnet mask (alternative to `prefix`) |

### WiFi Section (ESP32 only)

```toml
[wifi]
ssid = "MyNetwork"
password = "secret"
```

### Serial Section

```toml
[serial]
baudrate = 115200
```

Used when the `serial` transport feature is enabled instead of `ethernet`.

### Zenoh Section

```toml
[zenoh]
locator = "tcp/192.0.3.1:7447"
domain_id = 0
```

| Field | Description | Default |
|-------|-------------|---------|
| `locator` | zenohd router address (`tcp/host:port` or `serial/device#baudrate=N`) | `tcp/192.0.3.1:7447` |
| `domain_id` | ROS 2 domain ID (0–232) | `0` |

### Scheduling Section (RTOS)

```toml
[scheduling]
app_priority = 12
zenoh_read_priority = 16
zenoh_lease_priority = 16
app_stack_bytes = 65536
```

See the config.toml section below for the full
priority scale and constraints.

### How config.toml is consumed

**Rust:**
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

**C/C++ (CMake):**
```cmake
nano_ros_read_config("${CMAKE_CURRENT_SOURCE_DIR}/config.toml")
# Sets: NROS_CONFIG_IP, NROS_CONFIG_MAC, NROS_CONFIG_GATEWAY, etc.
```

**C/C++ (NuttX Makefile):**
```c
#ifndef APP_ZENOH_LOCATOR
#define APP_ZENOH_LOCATOR "tcp/192.0.3.1:7447"
#endif
```

## Layer 2: Environment Variables (Build-Time)

These are read during `cargo build` (by `build.rs` scripts) or by justfile
recipes. Set them in `.env` at the project root, or export them in your
shell.

### SDK Paths

Auto-resolved by `just setup-*` recipes. Override if SDKs are installed
elsewhere.

| Variable | Default | Description |
|----------|---------|-------------|
| `FREERTOS_DIR` | `third-party/freertos/kernel` | FreeRTOS kernel source |
| `FREERTOS_PORT` | `GCC/ARM_CM3` | FreeRTOS portable layer |
| `LWIP_DIR` | `third-party/freertos/lwip` | lwIP source |
| `FREERTOS_CONFIG_DIR` | Board crate's `config/` | `FreeRTOSConfig.h` location |
| `NUTTX_DIR` | `third-party/nuttx/nuttx` | NuttX RTOS source |
| `NUTTX_APPS_DIR` | `third-party/nuttx/nuttx-apps` | NuttX apps source |
| `THREADX_DIR` | `third-party/threadx/kernel` | ThreadX kernel source |
| `THREADX_CONFIG_DIR` | Board crate's `config/` | ThreadX config (`tx_user.h`) |
| `NETX_DIR` | `third-party/threadx/netxduo` | NetX Duo source |
| `NETX_CONFIG_DIR` | Board crate's `config/` | NetX Duo config (`nx_user.h`) |

### Build Options

| Variable | Description | Required |
|----------|-------------|----------|
| `ZENOH_PICO_DIR` | Pre-built zenoh-pico install prefix | Only with `system-zenohpico` feature |
| `SSID` | WiFi SSID for ESP32 examples | `build-examples-esp32` |
| `PASSWORD` | WiFi password for ESP32 examples | `build-examples-esp32` |

### Buffer Tuning

All buffer tuning variables (`ZPICO_*`, `XRCE_*`, `NROS_*`) are optional -- platform-appropriate defaults apply if unset. See the [Environment Variables Reference](../reference/environment-variables.md) for the complete list of all buffer tuning variables. For detailed sizing guidance, memory budget estimation, and recommended configurations by RAM class.

## Layer 3: Cargo Features

Features select the RMW backend, platform, ROS edition, and optional
capabilities. See [Platform Model](../concepts/platform-model.md) for the
full feature matrix.

### Quick reference

```toml
[dependencies]
nros = { default-features = false, features = [
    # RMW backend (pick one)
    "rmw-zenoh",          # or "rmw-xrce"

    # Platform (pick one)
    "platform-bare-metal", # or "platform-freertos", "platform-nuttx",
                           # "platform-threadx", "platform-zephyr", "platform-posix"

    # ROS edition (pick one)
    "ros-humble",          # or "ros-iron"

    # Optional
    "std",                 # std-dependent APIs
    "alloc",               # heap-dependent APIs
    "safety-e2e",          # CRC-32 integrity
    "param-services",      # ROS 2 parameter handlers (implies alloc)
    "ffi-sync",            # critical_section wrapping for RTOS
    "sync-critical-section", # RTIC/Embassy mutex
] }
```

## Layer 4: Runtime Environment (POSIX only)

On Linux/macOS, `ExecutorConfig::from_env()` reads environment variables
at process startup:

| Variable | Description | Default |
|----------|-------------|---------|
| `ROS_DOMAIN_ID` | ROS 2 domain ID | `0` |
| `NROS_LOCATOR` | Router address (legacy alias: `ZENOH_LOCATOR`) | `tcp/127.0.0.1:7447` |
| `NROS_SESSION_MODE` | Session mode, `client` / `peer` (legacy alias: `ZENOH_MODE`) | `client` |
| `ZENOH_TLS_ROOT_CA_CERTIFICATE` | Path to CA certificate (PEM) | (none) |
| `ZENOH_TLS_ROOT_CA_CERTIFICATE_BASE64` | Base64-encoded CA cert | (none) |
| `ZENOH_TLS_VERIFY_NAME_ON_CONNECT` | Verify hostname (`true`/`false`) | (none) |

```rust
// POSIX example — reads from environment at runtime
let config = ExecutorConfig::from_env()
    .node_name("talker");
let mut executor = Executor::open(&config)?;
```

Embedded examples cannot use `from_env()` — they get their configuration
from `config.toml` (layer 1).

## Configuration by Deployment Scenario

### Desktop development (POSIX)

```
Layer 4: NROS_LOCATOR=tcp/127.0.0.1:7447  (shell export or .env)
Layer 3: features = ["rmw-zenoh", "platform-posix", "std"]
```

No config.toml needed. Start zenohd locally and set environment variables.

### QEMU bare-metal testing

```
Layer 1: config.toml with ip/mac/gateway for TAP bridge
Layer 2: (defaults are fine)
Layer 3: features = ["rmw-zenoh", "platform-bare-metal", "ros-humble"]
```

### FreeRTOS on real hardware

```
Layer 1: config.toml with your board's IP, MAC, zenohd address
Layer 2: FREERTOS_DIR, LWIP_DIR (if not using `just freertos setup`)
Layer 3: features = ["rmw-zenoh", "platform-freertos", "ros-humble"]
```

### ESP32 with WiFi

```
Layer 1: config.toml with [wifi] ssid/password and [zenoh] locator
Layer 2: SSID, PASSWORD (for build-time WiFi config)
Layer 3: features = ["rmw-zenoh", "platform-bare-metal", "ros-humble"]
```

### Zephyr module

```
Layer 1: Kconfig (CONFIG_NROS_RMW_ZENOH=y, CONFIG_NROS_CPP_API=y)
Layer 2: (managed by west/cmake)
Layer 3: (managed by Kconfig → Cargo features)
```

### Minimal RAM (XRCE-DDS over serial)

```
Layer 1: config.toml with [serial] baudrate
Layer 2: XRCE_TRANSPORT_MTU=512, XRCE_BUFFER_SIZE=512
Layer 3: features = ["rmw-xrce", "platform-bare-metal", "ros-humble"]
```

## Precedence

When the same setting can be specified at multiple layers:

1. **Shell export** overrides `.env`
2. **`.env`** overrides justfile defaults
3. **config.toml** is the definitive source for embedded hardware settings
4. **Cargo feature defaults** (`default-features = true` gives `std` only)

## `.env` Setup

```bash
cp .env.example .env
# Edit .env — uncomment and adjust values as needed
```

The `.env` file is:
- Auto-loaded by justfile recipes
- Auto-loaded by direnv (via `.envrc`)
- Gitignored — safe for local overrides
