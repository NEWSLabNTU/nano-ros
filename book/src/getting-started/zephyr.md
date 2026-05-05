# Zephyr

Complete setup procedure for Zephyr `native_sim` testing. Networking uses
**NSOS** (Native Sim Offloaded Sockets) — each socket call is forwarded to
the host kernel, so tests run on `127.0.0.1` without TAP devices, bridges,
or `sudo`.

## Overview

nros uses an **in-tree Zephyr workspace** at `zephyr-workspace/` (gitignored).
Set `$NROS_ZEPHYR_WORKSPACE` to install elsewhere.

```
nros/
├── scripts/zephyr/
│   ├── setup.sh                      # Initialize workspace
│   ├── migrate-workspace.sh          # Move legacy sibling install in-tree
│   ├── downloads/                    # SDK tarball cache (gitignored)
│   └── sdk/                          # Installed Zephyr SDK (gitignored)
├── zephyr/                           # Zephyr module definition
│   ├── Kconfig                       # RMW backend, API selection, tuning
│   ├── CMakeLists.txt                # Transport C sources + nros-c build
│   └── cmake/                        # nros_cargo_build(), nros_generate_interfaces()
├── examples/zephyr/
│   ├── rust/zenoh/                   # Rust + zenoh (talker, listener, ...)
│   ├── rust/xrce/                    # Rust + XRCE-DDS (talker, listener)
│   ├── c/zenoh/                      # C + zenoh (talker, listener)
│   └── c/xrce/                       # C + XRCE-DDS (talker, listener)
├── west.yml                          # West manifest
└── zephyr-workspace/                 # Created by setup.sh (gitignored)
    ├── nros -> ../                   # Symlink back to repo root
    ├── zephyr/                       # Zephyr RTOS v3.7.0
    └── modules/                      # HALs, zephyr-lang-rust
```

### Migrating from the legacy sibling layout

Pre-Phase-113 setups put the workspace at `../nano-ros-workspace/` with an
in-tree symlink. Both layouts work — `just zephyr` recipes auto-detect — but
to consolidate run:

```bash
./scripts/zephyr/migrate-workspace.sh --dry-run     # preview
./scripts/zephyr/migrate-workspace.sh               # execute
```

## Prerequisites

Install system packages (Ubuntu/Debian):
```bash
sudo apt install python3 python3-pip python3-venv cmake ninja-build aria2 git
```

Install Rust (if not already):
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## Step 1: Initialize Workspace (One-Time)

```bash
just zephyr setup
```

This recipe automatically:
- Installs `west` and Python tools
- Downloads Zephyr SDK (~1.5 GB) to `scripts/zephyr/downloads/` using aria2c (parallel, resumable)
- Verifies download with sha256sum
- Installs SDK to `scripts/zephyr/sdk/`
- Creates in-tree workspace at `zephyr-workspace/` (gitignored; auto-detects legacy `../nano-ros-workspace/`)
- Symlinks nros into the workspace
- Fetches Zephyr RTOS and all modules
- Installs Rust embedded targets
- Creates `env.sh` for environment setup

**Options:**
```bash
just zephyr setup --skip-sdk    # Skip SDK download/install
just zephyr setup --force       # Recreate existing workspace
```

## Step 2: Networking

No network setup is required. `native_sim` uses the NSOS offloaded-sockets
driver, enabled by `boards/native_sim_native_64.conf` in each example:

```
CONFIG_ETH_NATIVE_POSIX=n
CONFIG_NET_SOCKETS_OFFLOAD=y
CONFIG_NET_NATIVE_OFFLOADED_SOCKETS=y
```

With NSOS, Zephyr's socket API goes straight to host syscalls. Bind to
`127.0.0.1` and reach `zenohd` / the XRCE Agent on the host loopback just
like any other native test. Multiple `native_sim` processes can coexist
without bridge configuration.

## Step 3: Build and Run Zephyr Examples

```bash
# Source environment
source zephyr-workspace/env.sh

# Build Zephyr talker (Rust + zenoh, default backend)
cd zephyr-workspace
west build -b native_sim/native/64 nros/examples/zephyr/rust/zenoh/talker

# Run (no sudo needed)
./build/zephyr/zephyr.exe
```

## RMW Backend Selection

nros supports two RMW backends on Zephyr, selected via `prj.conf`:

### Zenoh (default)

Connects to a zenoh router. Requires POSIX API for zenoh-pico threads.

```ini
CONFIG_NROS=y
# CONFIG_NROS_RMW_ZENOH=y  # default, can be omitted
CONFIG_NROS_ZENOH_LOCATOR="tcp/127.0.0.1:7456"
CONFIG_POSIX_API=y
CONFIG_MAX_PTHREAD_MUTEX_COUNT=32
CONFIG_MAX_PTHREAD_COND_COUNT=16
```

### XRCE-DDS

Connects to a Micro-XRCE-DDS Agent over UDP. Requires BSD sockets.

```ini
CONFIG_NROS=y
CONFIG_NROS_RMW_XRCE=y
CONFIG_NROS_XRCE_AGENT_ADDR="127.0.0.1"
CONFIG_NROS_XRCE_AGENT_PORT=2018
CONFIG_NET_SOCKETS=y
```

## API Selection

Choose between Rust and C APIs via `prj.conf`:

### Rust API (default)

```ini
CONFIG_NROS_RUST_API=y
CONFIG_RUST=y
CONFIG_RUST_ALLOC=y
```

CMakeLists.txt uses `rust_cargo_application()`:
```cmake
cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(my_example)
rust_cargo_application()
```

### C API

```ini
CONFIG_NROS_C_API=y
```

CMakeLists.txt uses `nros_generate_interfaces()`:
```cmake
cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(my_example)
nros_generate_interfaces(std_msgs "msg/Int32.msg")
target_sources(app PRIVATE src/main.c)
```

## Kconfig Reference

All options are under `menuconfig NROS` in `zephyr/Kconfig`.

### Common Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `CONFIG_NROS` | bool | n | Enable nros module |
| `CONFIG_NROS_RUST_API` | bool | y | Use Rust API |
| `CONFIG_NROS_C_API` | bool | n | Use C API |
| `CONFIG_NROS_DOMAIN_ID` | int | 0 | ROS 2 domain ID |
| `CONFIG_NROS_INIT_DELAY_MS` | int | 2000 | Network init wait (ms) |

### Zenoh Options (visible when `CONFIG_NROS_RMW_ZENOH=y`)

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `CONFIG_NROS_ZENOH_LOCATOR` | string | `"tcp/127.0.0.1:7456"` | Router address |
| `CONFIG_NROS_ZENOH_MULTI_THREAD` | bool | y | Zenoh-pico multithreading |
| `CONFIG_NROS_ZENOH_PUBLICATION` | bool | y | Publication support |
| `CONFIG_NROS_ZENOH_SUBSCRIPTION` | bool | y | Subscription support |
| `CONFIG_NROS_ZENOH_QUERY` | bool | y | Service client support |
| `CONFIG_NROS_ZENOH_QUERYABLE` | bool | y | Service server support |
| `CONFIG_NROS_ZENOH_LINK_TCP` | bool | y | TCP transport link |
| `CONFIG_NROS_MAX_PUBLISHERS` | int | 8 | Max concurrent publishers |
| `CONFIG_NROS_MAX_SUBSCRIBERS` | int | 8 | Max concurrent subscribers |
| `CONFIG_NROS_MAX_QUERYABLES` | int | 8 | Max concurrent queryables |
| `CONFIG_NROS_FRAG_MAX_SIZE` | int | 2048 | Max reassembled message size |
| `CONFIG_NROS_BATCH_UNICAST_SIZE` | int | 1024 | Max unicast batch size |
| `CONFIG_NROS_SUBSCRIBER_BUFFER_SIZE` | int | 1024 | Per-subscriber buffer |
| `CONFIG_NROS_SERVICE_BUFFER_SIZE` | int | 1024 | Per-service buffer |

### XRCE Options (visible when `CONFIG_NROS_RMW_XRCE=y`)

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `CONFIG_NROS_XRCE_AGENT_ADDR` | string | `"127.0.0.1"` | Agent IP address |
| `CONFIG_NROS_XRCE_AGENT_PORT` | int | 2018 | Agent UDP port |
| `CONFIG_NROS_XRCE_TRANSPORT_MTU` | int | 512 | Transport MTU |
| `CONFIG_NROS_XRCE_MAX_SUBSCRIBERS` | int | 8 | Max concurrent subscribers |
| `CONFIG_NROS_XRCE_MAX_SERVICE_SERVERS` | int | 4 | Max service servers |
| `CONFIG_NROS_XRCE_MAX_SERVICE_CLIENTS` | int | 4 | Max service clients |
| `CONFIG_NROS_XRCE_BUFFER_SIZE` | int | 1024 | Per-slot buffer size |
| `CONFIG_NROS_XRCE_STREAM_HISTORY` | int | 4 | Reliable stream depth (2-16) |

### C API Options (visible when `CONFIG_NROS_C_API=y`)

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `CONFIG_NROS_C_MAX_HANDLES` | int | 16 | Max executor handles |
| `CONFIG_NROS_C_MAX_SUBSCRIPTIONS` | int | 8 | Max subscriptions |
| `CONFIG_NROS_C_MAX_TIMERS` | int | 8 | Max timers |
| `CONFIG_NROS_C_MAX_SERVICES` | int | 4 | Max services |

## E2E Testing

```bash
# Zenoh examples
just zephyr build           # Build Rust zenoh examples
just zephyr build-c         # Build C zenoh examples
just zephyr test            # Run zenoh E2E tests

# XRCE examples
just zephyr build-xrce      # Build all XRCE examples (Rust + C)
just zephyr test-xrce       # Run XRCE E2E tests

# All examples
just zephyr build-all       # Build everything
just zephyr ci              # Doctor + test (CI shortcut)
```

## Troubleshooting

| Issue | Solution |
|-------|----------|
| `west: command not found` | Run `pip3 install --user west` and add `~/.local/bin` to PATH |
| `Connection refused` | Start `zenohd` / `MicroXRCEAgent` on the host loopback (e.g. `tcp/127.0.0.1:7456`) |
| `Build fails` | Source environment: `source zephyr-workspace/env.sh` |
| `XRCE Agent not found` | Install: `just setup` (installs MicroXRCEAgent) |
| Zenoh mutex exhaustion | Increase `CONFIG_MAX_PTHREAD_MUTEX_COUNT` (default 5 is too low) |

## Network Architecture

With NSOS, Zephyr sockets are forwarded to host syscalls — there is no
emulated L2/L3 stack to configure, no static IP, and no bridge.

```
┌─────────────────────────────────────────────────────────────┐
│                      Host (Linux)                            │
│                                                              │
│   ┌────────────────────┐       ┌────────────────────────┐   │
│   │ zephyr.exe talker  │       │ zephyr.exe listener    │   │
│   │ (native_sim+NSOS)  │       │ (native_sim+NSOS)      │   │
│   └─────────┬──────────┘       └──────────┬─────────────┘   │
│             │ host socket() via NSOS       │                │
│             ▼                              ▼                │
│                 127.0.0.1 (loopback)                        │
│             │                              │                │
│             ▼                              ▼                │
│   ┌────────────────────┐       ┌────────────────────────┐   │
│   │ zenohd             │       │ MicroXRCEAgent         │   │
│   │ tcp/127.0.0.1:7456 │       │ udp/127.0.0.1:2018     │   │
│   └────────────────────┘       └────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

## Updating the Workspace

To update Zephyr and modules to latest versions specified in `west.yml`:

```bash
cd zephyr-workspace
west update
```

To completely recreate the workspace:

```bash
just zephyr setup --force
```
