# Phase 72 — Per-Example Configuration Files

**Goal**: Replace hardcoded network configuration in board crate presets and
examples with per-example `config.toml` files, enabling users to customize
IP addresses, MAC addresses, and zenoh locators without modifying source code.

**Status**: In Progress (72.1–72.5 done)

**Priority**: Medium

**Depends on**: None

## Overview

All embedded examples currently hardcode network settings in board crate
`Config` preset methods (`default()`, `listener()`, `server()`, etc.):

```rust
// Current: hardcoded in board crate
pub fn default() -> Self {
    Self {
        ip: [192, 0, 3, 10],
        mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x00],
        gateway: [192, 0, 3, 1],
        prefix: 24,
        zenoh_locator: "tcp/192.0.3.1:7447",
        domain_id: 0,
    }
}
```

Users porting to real hardware must modify the board crate source or chain
builder methods. Changing IP assignments for a different network topology
requires editing multiple files.

### Target State

Each example directory contains a `config.toml` that is baked into the
binary at compile time:

```toml
# examples/qemu-arm-baremetal/rust/zenoh/talker/config.toml
[network]
ip = "192.0.3.10"
mac = "02:00:00:00:00:00"
gateway = "192.0.3.1"
prefix = 24

[zenoh]
locator = "tcp/192.0.3.1:7447"
domain_id = 0
```

The example source becomes:

```rust
// New: config loaded from file at compile time
const CONFIG_TOML: &str = include_str!("../config.toml");

fn main() {
    let config = Config::from_toml(CONFIG_TOML);
    run(config, |node| { ... });
}
```

### Benefits

- **No source edits for different networks** — change `config.toml`, rebuild
- **Self-documenting** — each example's config.toml shows exactly what network
  settings it uses
- **Diffable** — `git diff` shows config changes clearly
- **Per-role separation** — talker and listener each have their own config
  instead of sharing a board crate preset
- **Portable** — same example code works on QEMU, Docker, and real hardware
  by swapping config.toml

### Config File Format

```toml
# Network stack configuration (for ethernet transport)
[network]
ip = "192.0.3.10"           # IPv4 address
mac = "02:00:00:00:00:00"   # MAC address (colon-separated hex)
gateway = "192.0.3.1"       # Default gateway
prefix = 24                 # Subnet prefix length (CIDR)

# Network stack configuration (for WiFi transport, ESP32)
# [wifi]
# ssid = "MyNetwork"
# password = "secret"
# ip_mode = "dhcp"          # "dhcp" or "static"

# Serial transport configuration
# [serial]
# baudrate = 115200

# Zenoh middleware configuration
[zenoh]
locator = "tcp/192.0.3.1:7447"   # Router address
domain_id = 0                     # ROS 2 domain ID
mode = "client"                   # "client" or "peer"

# ThreadX-specific
# [platform]
# interface = "veth-tx0"    # Linux veth pair name
```

All sections are optional — missing fields use board crate defaults.
Field names match the existing `Config` builder methods.

### String Format Rationale

IP and MAC addresses use human-readable string format instead of byte
arrays for readability and familiarity:

| Format     | TOML                            | Why                                      |
|------------|---------------------------------|------------------------------------------|
| IP address | `"192.0.3.10"`                  | Standard dotted-decimal notation         |
| MAC address| `"02:00:00:00:00:00"`           | Standard colon-separated hex             |
| Gateway    | `"192.0.3.1"`                   | Same as IP                               |
| Locator    | `"tcp/192.0.3.1:7447"`          | Zenoh locator format (already a string)  |
| Prefix     | `24`                            | Integer (CIDR notation)                  |

Parsing functions convert strings to byte arrays at compile time
(const-compatible) or startup time.

## Architecture

### Config Loading

On embedded targets (no filesystem), `config.toml` is embedded via
`include_str!` at compile time. The TOML is parsed at startup into the
board crate's `Config` struct.

```
config.toml → include_str!() → &str → parse at startup → Config struct
```

### TOML Parser

Embedded targets cannot use `toml` crate (requires `std`). Options:

1. **Build-time parsing** — `build.rs` reads `config.toml`, generates a
   Rust source file with the parsed values as constants. No runtime
   parsing needed. Similar to how `nros-node/build.rs` generates config.

2. **Minimal `no_std` TOML parser** — parse at startup. The config format
   is simple enough for a hand-written parser (no nested tables, no arrays
   of tables, only strings and integers).

3. **`toml_edit` with `no_std`** — the `toml_edit` crate has partial
   `no_std` support but requires `alloc`.

**Recommendation**: Option 1 (build-time parsing) for embedded, option 3
or runtime env vars for native/POSIX targets. This matches the existing
pattern where `build.rs` generates config constants.

### Board Crate Changes

Each board crate gains:

```rust
impl Config {
    /// Parse config from TOML string.
    /// Missing fields use board-specific defaults.
    pub fn from_toml(toml: &str) -> Self {
        let mut config = Self::board_defaults();
        // Parse and override fields...
        config
    }

    /// Board-specific defaults (the current hardcoded values).
    fn board_defaults() -> Self { ... }
}
```

The existing `default()` / `listener()` presets remain for backwards
compatibility but delegate to `from_toml()` internally or are deprecated.

## Work Items

- [x] 72.1 — Define config TOML schema and implement parser
- [x] 72.2 — Add `Config::from_toml()` to `nros-mps2-an385` (proof of concept)
- [x] 72.3 — Create config.toml for remaining QEMU ARM bare-metal examples
- [x] 72.4 — Port remaining board crates to config.toml
- [x] 72.5 — Port RTOS examples (FreeRTOS, NuttX, ThreadX)
- [ ] 72.6 — Port ESP32 examples (WiFi + serial config)
- [ ] 72.7 — Port C/C++ examples (read config.toml from CMake)
- [ ] 72.8 — Update test infrastructure to use config.toml
- [ ] 72.9 — Document config.toml format in the book

### 72.1 — Define config TOML schema and implement parser

Create a shared config parsing library (`nros-config` or add to `nros-core`)
that:

1. Defines the TOML schema as Rust types
2. Parses IP addresses (`"192.0.3.10"` → `[192, 0, 3, 10]`)
3. Parses MAC addresses (`"02:00:00:00:00:00"` → `[0x02, 0, 0, 0, 0, 0]`)
4. Works at build time (for `build.rs` code generation) or startup time

For build-time parsing, the `build.rs` reads `config.toml` and generates:

```rust
pub const CONFIG_IP: [u8; 4] = [192, 0, 3, 10];
pub const CONFIG_MAC: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x00];
pub const CONFIG_GATEWAY: [u8; 4] = [192, 0, 3, 1];
pub const CONFIG_PREFIX: u8 = 24;
pub const CONFIG_ZENOH_LOCATOR: &str = "tcp/192.0.3.1:7447";
pub const CONFIG_DOMAIN_ID: u32 = 0;
```

**Files**:
- `packages/core/nros-config/` — new crate (or add to `nros-core`)
- Parser functions: `parse_ipv4()`, `parse_mac()`, `parse_toml_config()`

### 72.2 — Add `Config::from_toml()` to `nros-mps2-an385`

Proof of concept: add config.toml support to the QEMU ARM bare-metal
board crate. Add `build.rs` that reads `config.toml` from the example
directory (passed via env var) and generates config constants.

**Files**:
- `packages/boards/nros-mps2-an385/src/config.rs` — add `from_toml()`
- Example `config.toml` files for talker/listener

### 72.3 — Create config.toml for QEMU ARM bare-metal examples

Create `config.toml` in each QEMU ARM bare-metal example directory
with the current hardcoded values. Update `main.rs` to use
`include_str!` + `from_toml()`.

**Talker config.toml**:
```toml
[network]
ip = "192.0.3.10"
mac = "02:00:00:00:00:00"
gateway = "192.0.3.1"
prefix = 24

[zenoh]
locator = "tcp/192.0.3.1:7447"
domain_id = 0
```

**Listener config.toml**:
```toml
[network]
ip = "192.0.3.11"
mac = "02:00:00:00:00:01"
gateway = "192.0.3.1"
prefix = 24

[zenoh]
locator = "tcp/192.0.3.1:7447"
domain_id = 0
```

**Files**:
- `examples/qemu-arm-baremetal/rust/zenoh/talker/config.toml`
- `examples/qemu-arm-baremetal/rust/zenoh/listener/config.toml`
- Update `src/main.rs` in each

### 72.4 — Port remaining board crates to config.toml

Add `from_toml()` to all board crates:
- `nros-mps2-an385-freertos`
- `nros-nuttx-qemu-arm`
- `nros-threadx-qemu-riscv64`
- `nros-threadx-linux`
- `nros-esp32-qemu`
- `nros-stm32f4`
- `nros-esp32`

Each board crate provides its own `board_defaults()` with hardware-specific
values (e.g., STM32F4 uses `192.168.1.x`, ESP32 uses WiFi/DHCP).

**Files**:
- `packages/boards/*/src/config.rs`

### 72.5 — Port RTOS examples

Create `config.toml` for FreeRTOS, NuttX, and ThreadX examples.

**Files**:
- `examples/qemu-arm-freertos/rust/zenoh/*/config.toml`
- `examples/qemu-arm-nuttx/rust/zenoh/*/config.toml`
- `examples/qemu-riscv64-threadx/rust/zenoh/*/config.toml`
- `examples/threadx-linux/rust/zenoh/*/config.toml`

### 72.6 — Port ESP32 examples

ESP32 has WiFi-specific config (SSID, password, IP mode). Currently these
are read via `env!("SSID")` at build time. The config.toml replaces this:

```toml
[wifi]
ssid = "MyNetwork"
password = "secret"
ip_mode = "dhcp"

[zenoh]
locator = "tcp/192.168.1.1:7447"
```

**Files**:
- `examples/esp32/rust/zenoh/*/config.toml`
- `examples/qemu-esp32-baremetal/rust/zenoh/*/config.toml`

### 72.7 — Port C/C++ examples

C/C++ examples need the config values as `#define` macros. The `build.rs`
(or CMake) generates a C header from `config.toml`:

```c
// Auto-generated from config.toml
#define APP_IP {192, 0, 3, 10}
#define APP_MAC {0x02, 0x00, 0x00, 0x00, 0x00, 0x00}
#define APP_ZENOH_LOCATOR "tcp/192.0.3.1:7447"
```

Currently FreeRTOS C++ examples pass these via CMake
`target_compile_definitions()`. Config.toml replaces these definitions.

**Files**:
- `examples/qemu-arm-freertos/cpp/zenoh/*/config.toml`
- CMake helper to generate C header from config.toml

### 72.8 — Update test infrastructure

Test scripts and fixtures currently hardcode IPs for TAP bridge setup.
These should read from the example's `config.toml` or use consistent
defaults.

**Files**:
- `tests/run-test.sh` — read IPs from config.toml
- `packages/testing/nros-tests/src/fixtures/` — config-aware fixtures

### 72.9 — Document config.toml format in the book

Add a reference page documenting the config.toml schema, all supported
fields, and per-board defaults.

**Files**:
- `book/src/reference/config-toml.md`
- `book/src/guides/creating-examples.md` — update example creation guide

## Acceptance Criteria

- [ ] Each embedded example has a `config.toml` with its network settings
- [ ] Changing `config.toml` and rebuilding produces a binary with the new settings
- [ ] No IP/MAC/locator values remain hardcoded in example `main.rs` files
- [ ] Board crate preset methods (`default()`, `listener()`) still work for
      backwards compatibility
- [ ] QEMU tests pass with config.toml-based examples
- [ ] Config format is documented in the book

## Notes

- **Backwards compatibility**: The existing `Config::default()` and
  `Config::listener()` presets remain functional. They can be implemented
  as `from_toml()` with built-in default TOML strings, or kept as-is
  alongside the new `from_toml()` path. Examples migrate incrementally.

- **Config inheritance**: For setups where multiple examples share the same
  gateway/prefix/locator, consider a shared `base-config.toml` that
  individual configs override. However, this adds complexity — start with
  standalone per-example configs.

- **Runtime override**: On POSIX/std targets, `config.toml` values can be
  overridden by environment variables (e.g., `ZENOH_LOCATOR` overrides
  `[zenoh] locator`). This matches the native `ExecutorConfig::from_env()`
  pattern.

- **gitignore**: `config.toml` files should be checked into git (they're
  part of the example). Users who customize for their hardware can use
  `config.local.toml` (gitignored) which takes precedence.
