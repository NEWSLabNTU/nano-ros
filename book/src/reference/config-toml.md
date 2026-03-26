# Example Configuration (config.toml)

Each embedded example includes a `config.toml` file that defines network
settings and zenoh middleware parameters. The configuration is baked into
the binary at compile time — no filesystem or runtime parsing is required.

## Format

```toml
# Network stack configuration
[network]
ip = "192.0.3.10"           # IPv4 address
mac = "02:00:00:00:00:00"   # MAC address (colon-separated hex)
gateway = "192.0.3.1"       # Default gateway
prefix = 24                 # Subnet prefix length (CIDR)
netmask = "255.255.255.0"   # Alternative to prefix (FreeRTOS only)

# WiFi configuration (ESP32 only)
[wifi]
ssid = "MyNetwork"          # WiFi network name
password = "secret"          # WiFi password

# Serial transport configuration
[serial]
baudrate = 115200            # UART baud rate

# Zenoh middleware configuration
[zenoh]
locator = "tcp/192.0.3.1:7447"  # Router address
domain_id = 0                     # ROS 2 domain ID (0–232)
```

## Scheduling

The `[scheduling]` section configures RTOS task priorities and stack sizes.
All fields are optional — omit the entire section to use board defaults.

```toml
[scheduling]
# Normalized 0–31 priority scale (higher = more important).
# Board crate maps to platform-native range:
#   FreeRTOS: 0–7, ThreadX: 31–0 (inverted), NuttX: POSIX nice
app_priority = 12              # Application task (executor + callbacks)
zenoh_read_priority = 16       # Zenoh-pico socket read task
zenoh_lease_priority = 16      # Zenoh-pico session keep-alive task
poll_priority = 16             # Network poll task (FreeRTOS only)

# Stack sizes in bytes
app_stack_bytes = 65536        # 64 KB (must fit executor arena)
zenoh_read_stack_bytes = 5120
zenoh_lease_stack_bytes = 5120

# Platform-specific
poll_interval_ms = 5           # Network poll interval (FreeRTOS only)
```

### Priority Scale

| Normalized | Meaning | FreeRTOS (0–7) | ThreadX (0–31) |
|------------|---------|----------------|----------------|
| 0 | Idle | 0 | 31 |
| 8 | Below normal | 2 | 20 |
| 12 | **Normal (default app)** | 3 | 16 |
| 16 | **Above normal (default zenoh)** | 4 | 12 |
| 24 | Very high | 6 | 4 |
| 31 | Critical | 7 | 0 |

### Scheduling Constraints

These should hold for correct operation:

1. **poll ≥ zenoh read** (FreeRTOS): poll task must feed the RX FIFO
2. **zenoh read ≥ app**: prevents lease timeouts from delayed message processing
3. **app stack ≥ 16 KB**: executor arena + zenoh-pico call depth; 64 KB for actions

All sections and fields are optional — missing values use board-specific
defaults.

## Per-Platform Fields

Not all platforms use every field:

| Field | Bare-metal | FreeRTOS | NuttX | ESP32 QEMU | ESP32 |
|-------|-----------|----------|-------|------------|-------|
| `ip` | Yes | Yes | Yes | Yes | Static IP only |
| `mac` | Yes | Yes | — | Yes | — |
| `gateway` | Yes | Yes | Yes | Yes | Static IP only |
| `prefix` | Yes | — | Yes | Yes | Static IP only |
| `netmask` | — | Yes | — | — | — |
| `ssid` | — | — | — | — | Yes |
| `password` | — | — | — | — | Yes |
| `baudrate` | Serial only | — | — | Serial only | Serial only |
| `locator` | Yes | Yes | Yes | Yes | Yes |
| `domain_id` | Yes | Yes | Yes | Yes | Yes |

NuttX doesn't need `mac` because the kernel configures networking.
FreeRTOS uses `netmask` (dotted quad) instead of `prefix` (integer).
ESP32 uses DHCP by default; `[network]` fields enable static IP mode.

`[scheduling]` fields are parsed on all platforms but only FreeRTOS
currently uses them to configure task creation. Other platforms ignore
the values (zenoh-pico read/lease tasks use platform defaults).

## How It Works

### Rust examples

The config file is embedded at compile time via `include_str!` and parsed
at startup by the board crate's `Config::from_toml()` method:

```rust
use nros_mps2_an385::{Config, run};

fn main() -> ! {
    run(Config::from_toml(include_str!("../config.toml")), |config| {
        let exec = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("talker");
        // ...
    })
}
```

### C/C++ examples (CMake)

The `nano_ros_read_config()` CMake function parses config.toml at
configure time and sets compile definitions:

```cmake
include("${PROJECT_ROOT}/cmake/NanoRosConfig.cmake")

nano_ros_read_config("${CMAKE_CURRENT_SOURCE_DIR}/config.toml")

target_compile_definitions(my_example PRIVATE
    "APP_IP={${NROS_CONFIG_IP}}"
    "APP_MAC={${NROS_CONFIG_MAC}}"
    "APP_GATEWAY={${NROS_CONFIG_GATEWAY}}"
    "APP_NETMASK={${NROS_CONFIG_NETMASK}}"
    "APP_ZENOH_LOCATOR=\"${NROS_CONFIG_ZENOH_LOCATOR}\""
    "APP_DOMAIN_ID=${NROS_CONFIG_DOMAIN_ID}"
)
```

C source code uses the macros:

```c
nros_support_init(&support, APP_ZENOH_LOCATOR, APP_DOMAIN_ID);
```

### C/C++ examples (NuttX Makefile)

NuttX C examples that don't use CMake define macros with `#ifndef` guards
and defaults. Override via `-D` flags in the Makefile if needed:

```c
#ifndef APP_ZENOH_LOCATOR
#define APP_ZENOH_LOCATOR "tcp/192.0.3.1:7447"
#endif
#ifndef APP_DOMAIN_ID
#define APP_DOMAIN_ID 0
#endif
```

## Network Topology (QEMU)

The default config.toml values match the QEMU TAP bridge test topology:

```
talker (192.0.3.10, tap-qemu0) ──┐
                                  ├── qemu-br (192.0.3.1) ── zenohd :7447
listener (192.0.3.11, tap-qemu1) ┘
```

- **Talker/server role**: IP `.10`, MAC `:00`
- **Listener/client role**: IP `.11`, MAC `:01`
- **Bridge/gateway**: `192.0.3.1` (zenohd listens here)
- **Subnet**: `192.0.3.0/24` (RFC 5737 TEST-NET-3)

Set up the bridge with: `sudo ./scripts/qemu/setup-network.sh`

## Customization

To customize for your hardware:

1. Edit the example's `config.toml` with your network settings
2. Rebuild: `cargo build --release` (Rust) or `cmake --build build` (C/C++)

The binary will contain the new values — no runtime configuration needed.

For local overrides that shouldn't be committed to git, create a
`config.local.toml` (gitignored) and update your `include_str!` or
CMake path to point to it.
