# Phase 67 — Serial Transport & Board Crate Transport Abstraction

**Goal**: Add serial (UART) transport support via a new `zpico-serial` crate and refactor
board crates so the communication interface (Ethernet, serial, etc.) is selected via Cargo
features rather than hardcoded.

**Status**: In Progress (67.1–67.5 done)

**Priority**: Medium

**Depends on**: Phase 53 (UDP + TLS transport — Complete)

## Overview

All board crates currently hardcode Ethernet networking. The `init_hardware()` function
creates an Ethernet device, smoltcp interface, and TCP/UDP sockets. The `Config` struct
contains Ethernet-specific fields (MAC, IP, gateway). This makes it impossible to use
nano-ros on boards where the only available interface is a UART.

zenoh-pico natively supports serial transport using COBS framing over UART. It connects
to a zenohd router via `serial/<device>#<baudrate>` locators (e.g., `serial/UART_0#115200`).
The serial link uses a simple Init/Ack handshake and CRC32-checked frames — no IP stack
required.

This phase has two parts:

1. **`zpico-serial`** — new crate implementing zenoh-pico serial FFI symbols for the
   bare-metal platform (analogous to `zpico-smoltcp` for TCP/UDP)
2. **Board crate transport features** — refactor board crates so `ethernet` and `serial`
   are Cargo features, with `Config` and `init_hardware()` adapting accordingly

### Why Serial Matters

- **Small MCUs** — Cortex-M0/M0+ targets with no Ethernet MAC and insufficient RAM for
  an IP stack can still connect to a zenoh network via UART
- **Point-to-point topology** — UART connects directly to a host running zenohd with a
  serial plugin, no network infrastructure required
- **Debugging-friendly** — Serial output is visible in any terminal, easy to tap/log
- **Existing support** — zenoh-pico already implements the framing protocol; Zephyr,
  ESP-IDF, and Arduino all ship serial backends

### Scope: `zpico-serial` is Only for Bare-Metal

zenoh-pico already ships native serial implementations for every platform except
bare-metal:

| Platform backend         | System layer                | Serial support                     |
|--------------------------|-----------------------------|------------------------------------|
| POSIX (cmake)            | `unix/network.c`            | Built-in — uses `/dev/ttyXXX`      |
| Zephyr                   | `zephyr/network.c`          | Built-in — uses `uart_poll_in/out` |
| FreeRTOS (lwIP)          | `unix/network.c`            | Built-in (same as POSIX)           |
| NuttX                    | `unix/network.c`            | Built-in (POSIX-like)              |
| ThreadX                  | `threadx/stm32/network.c`   | Built-in — uses HAL DMA            |
| **Bare-metal (smoltcp)** | Custom (`zpico-platform-*`) | **Not implemented**                |

On non-bare-metal platforms, serial just works by enabling `Z_FEATURE_LINK_SERIAL=1`
and using a `serial/...` locator. The new `zpico-serial` crate only fills the gap for
the bare-metal backend, where we provide custom FFI symbols instead of zenoh-pico's
built-in system layer.

### Position in Architecture

Serial is a **link transport**, not a platform or RMW backend:

```
RMW Backend (orthogonal)     Link Transport          Platform (orthogonal)
---------------------------  ---------------------   ----------------------
rmw-zenoh                    zpico-smoltcp (TCP/UDP) platform-bare-metal
rmw-xrce                    zpico-serial  (UART) <-- NEW (bare-metal only)
                             zpico-zephyr (native)   platform-zephyr
                                                     platform-freertos
```

One board can support multiple transports via features:
```toml
[features]
default = ["ethernet"]
ethernet = ["dep:zpico-smoltcp", "dep:lan9118-smoltcp"]
serial   = ["dep:zpico-serial"]
```

## Architecture

### zenoh-pico Serial Protocol

The serial link uses **COBS (Consistent Overhead Byte Stuffing)** framing:

```
┌────────┬────────┬──────────────┬────────┬───────────┐
│ Header │ Length │    Data      │ CRC32  │ Delimiter │
│ 1 byte │ 2 byte │ N bytes     │ 4 byte │  0x00     │
└────────┴────────┴──────────────┴────────┴───────────┘
         └──── COBS-encoded ────────────┘
```

- **Header**: `[x|x|x|x|x|R|A|I]` — Reset, Ack, Init flags for handshake
- **Length**: Little-endian u16
- **MTU**: 1500 bytes (max frame: 1510, max COBS wire: 1516)
- **CRC32**: Integrity check over header + length + data
- **Handshake**: Client sends Init, waits for Ack (or Reset to retry)

The framing logic lives in zenoh-pico's `src/system/common/serial.c` and
`src/protocol/codec/serial.c`. `zpico-serial` only needs to provide the
low-level byte read/write and port open/close — zenoh-pico handles COBS
encoding, CRC, and the Init/Ack handshake internally.

### FFI Symbols Required

zenoh-pico expects these platform symbols when `Z_FEATURE_LINK_SERIAL=1`:

```c
// Port management
z_result_t _z_open_serial_from_pins(sock, txpin, rxpin, baudrate);
z_result_t _z_open_serial_from_dev(sock, dev, baudrate);
z_result_t _z_listen_serial_from_pins(sock, txpin, rxpin, baudrate);
z_result_t _z_listen_serial_from_dev(sock, dev, baudrate);
void       _z_close_serial(sock);

// Byte-level I/O (called by common/serial.c COBS framing)
size_t _z_read_exact_serial(sock, ptr, len);

// Frame-level I/O (COBS + CRC handled internally)
size_t _z_read_serial_internal(sock, header, ptr, len);
size_t _z_send_serial_internal(sock, header, ptr, len);
```

### zpico-serial Crate Design

`zpico-serial` is only used by **bare-metal** board crates. Non-bare-metal platforms
(POSIX, Zephyr, FreeRTOS, NuttX, ThreadX) use zenoh-pico's built-in serial
implementation and do not depend on this crate.

```
packages/zpico/zpico-serial/
├── Cargo.toml
└── src/
    ├── lib.rs          # Public API: SerialPort trait, init(), feature re-exports
    ├── ffi.rs          # FFI symbol implementations (_z_open_serial_*, etc.)
    └── port.rs         # Static port table (analogous to smoltcp socket table)
```

**Key design**: `zpico-serial` defines a `SerialPort` trait that board crates implement
for their specific UART peripheral. The crate stores a trait object (or static dispatch
via generics) in a global port table, and the FFI symbols delegate to it.

```rust
/// Trait for UART peripherals. Board crates implement this.
pub trait SerialPort {
    /// Write bytes to the UART TX FIFO. Returns bytes written.
    fn write(&mut self, data: &[u8]) -> usize;
    /// Read bytes from the UART RX FIFO. Returns bytes read (0 if empty).
    fn read(&mut self, buf: &mut [u8]) -> usize;
    /// Read exactly `len` bytes, blocking until available or timeout.
    fn read_exact(&mut self, buf: &mut [u8], timeout_ms: u32) -> usize;
}
```

Static port storage (mirrors zpico-smoltcp's socket table pattern):

```rust
const MAX_SERIAL_PORTS: usize = 2;
static mut SERIAL_PORTS: [Option<&'static mut dyn SerialPort>; MAX_SERIAL_PORTS] =
    [None; MAX_SERIAL_PORTS];
```

The board crate registers its UART during `init_hardware()`:

```rust
// In nros-mps2-an385/src/node.rs (when feature = "serial")
fn init_serial(config: &Config) {
    let uart = create_uart(config.serial_baudrate);
    unsafe { UART_DEVICE.write(uart) };
    let uart = unsafe { UART_DEVICE.assume_init_mut() };
    zpico_serial::register_port(0, uart);
}
```

### Board Crate Transport Features

Refactor each board crate to feature-gate the communication interface:

**Before** (hardcoded Ethernet):
```toml
[dependencies]
zpico-smoltcp = { path = "..." }
lan9118-smoltcp = { path = "..." }
```

**After** (feature-gated):
```toml
[dependencies]
zpico-smoltcp = { path = "...", optional = true }
lan9118-smoltcp = { path = "...", optional = true }
zpico-serial = { path = "...", optional = true }

[features]
default = ["ethernet"]
ethernet = ["dep:zpico-smoltcp", "dep:lan9118-smoltcp"]
serial = ["dep:zpico-serial"]
```

**Config struct** adapts per feature:

```rust
pub struct Config {
    // Always present — transport-agnostic
    pub zenoh_locator: &'static str,
    pub domain_id: u32,

    // Ethernet (feature = "ethernet")
    #[cfg(feature = "ethernet")]
    pub mac: [u8; 6],
    #[cfg(feature = "ethernet")]
    pub ip: [u8; 4],
    #[cfg(feature = "ethernet")]
    pub prefix: u8,
    #[cfg(feature = "ethernet")]
    pub gateway: [u8; 4],

    // Serial (feature = "serial")
    #[cfg(feature = "serial")]
    pub baudrate: u32,
}
```

**`init_hardware()`** dispatches per feature:

```rust
pub fn init_hardware(config: &Config) {
    // System intrinsics — always
    CycleCounter::enable();

    #[cfg(feature = "ethernet")]
    init_ethernet(config);

    #[cfg(feature = "serial")]
    init_serial(config);
}
```

### Locator Format

Serial locators follow zenoh-pico convention:

| Format                    | Example                | Use case                          |
|---------------------------|------------------------|-----------------------------------|
| `serial/<dev>#<baud>`     | `serial/UART_0#115200` | Device name (Zephyr, ESP-IDF)     |
| `serial/<tx>.<rx>#<baud>` | `serial/0.1#115200`    | Pin numbers (Arduino, bare-metal) |

Board crate defaults encode the locator:

```rust
impl Config {
    pub fn serial_default() -> Self {
        Self {
            zenoh_locator: "serial/UART_0#115200",
            baudrate: 115200,
            domain_id: 0,
        }
    }
}
```

### Host-Side Setup

The host runs zenohd with the serial plugin to bridge UART to the zenoh network:

```bash
# Connect to MCU via /dev/ttyUSB0
zenohd --cfg='plugins/serial/port:"/dev/ttyUSB0"' \
       --cfg='plugins/serial/baudrate:"115200"'
```

For QEMU testing, QEMU's UART can be exposed as a PTY:

```bash
qemu-system-arm -M mps2-an385 \
    -serial pty \           # UART0 → /dev/pts/N
    -kernel firmware.elf
```

Then connect zenohd to the PTY device.

## Work Items

- [x] 67.1 — Create `zpico-serial` crate with `SerialPort` trait and FFI symbols
- [x] 67.2 — Implement COBS staging buffers and port table in `zpico-serial`
- [x] 67.3 — Wire `link-serial` feature through zpico-sys build.rs for bare-metal
- [x] 67.4 — Add MPS2-AN385 UART driver (CMSDK UART peripheral)
- [x] 67.5 — Feature-gate `nros-mps2-an385`: `ethernet` (default) vs `serial`
- [ ] 67.6 — Refactor `Config` with `#[cfg(feature)]` transport fields
- [ ] 67.7 — QEMU serial example (`examples/qemu-arm-baremetal/rust/zenoh/serial-talker/`)
- [ ] 67.8 — QEMU serial integration test (zenohd serial plugin + QEMU PTY)
- [ ] 67.9 — Feature-gate `nros-stm32f4`: `ethernet` (default) vs `serial`
- [ ] 67.10 — STM32F4 USART driver for `zpico-serial`
- [ ] 67.11 — Feature-gate `nros-esp32`: `wifi` (default) vs `serial`
- [ ] 67.12 — Feature-gate `nros-esp32-qemu`: `ethernet` (default) vs `serial`
- [ ] 67.13 — Documentation: serial transport guide in book
- [ ] 67.14 — Update CLAUDE.md with transport feature conventions

---

### 67.1 — Create `zpico-serial` crate with `SerialPort` trait and FFI symbols

Create `packages/zpico/zpico-serial/` following the `zpico-smoltcp` pattern.

**Cargo.toml**:
```toml
[package]
name = "zpico-serial"
version = "0.1.0"
edition = "2024"
description = "Serial (UART) link layer for nros (bare-metal systems)"

[dependencies]
zpico-sys = { path = "../zpico-sys", features = ["bare-metal", "link-serial"] }
```

**`src/lib.rs`**: Re-exports, `register_port()`, `init()`.

**`src/port.rs`**: `SerialPort` trait definition, static port table
(`MAX_SERIAL_PORTS = 2`), `register_port(index, &'static mut dyn SerialPort)`.

**`src/ffi.rs`**: `#[unsafe(no_mangle)] extern "C"` implementations of
`_z_open_serial_from_dev`, `_z_open_serial_from_pins`, `_z_close_serial`,
`_z_read_exact_serial`, `_z_read_serial_internal`, `_z_send_serial_internal`.

The `_from_dev` variant parses the device name string (e.g., `"UART_0"`) to a
port index. The `_from_pins` variant stores TX/RX pin numbers for the board
driver to interpret.

**Files**:
- `packages/zpico/zpico-serial/Cargo.toml`
- `packages/zpico/zpico-serial/src/lib.rs`
- `packages/zpico/zpico-serial/src/port.rs`
- `packages/zpico/zpico-serial/src/ffi.rs`

### 67.2 — Implement COBS staging buffers and port table in `zpico-serial`

The COBS framing needs scratch buffers for encode/decode. zenoh-pico's
`_z_read_serial_internal` and `_z_send_serial_internal` in `common/serial.c`
handle COBS internally — they call `_z_read_exact_serial` for raw byte I/O.

However, on bare-metal we may need staging because UART reads are non-blocking
(single bytes from FIFO). Add a per-port RX ring buffer:

```rust
const RX_BUF_SIZE: usize = 2048;  // > _Z_SERIAL_MAX_COBS_BUF_SIZE (1516)

struct PortState {
    port: Option<&'static mut dyn SerialPort>,
    rx_buf: [u8; RX_BUF_SIZE],
    rx_head: usize,
    rx_tail: usize,
}
```

The `_z_read_exact_serial` implementation drains the ring buffer first, then
polls the `SerialPort::read()` for remaining bytes with a timeout.

**Files**:
- `packages/zpico/zpico-serial/src/port.rs`

### 67.3 — Wire `link-serial` feature through zpico-sys build.rs for bare-metal

The `link-serial` feature already exists in zpico-sys. Verify that the embedded
build path (`build_embedded_zenoh_pico`) correctly passes
`Z_FEATURE_LINK_SERIAL=1` and includes the serial source files:

- `src/link/unicast/serial.c`
- `src/link/config/serial.c`
- `src/protocol/codec/serial.c`
- `src/system/common/serial.c`

These files are already picked up by `add_c_sources_recursive` for the `link`,
`protocol`, and `system/common` subdirectories. Verify compilation succeeds with
`--features link-serial` on a `thumbv7m-none-eabi` target.

**Files**:
- `packages/zpico/zpico-sys/build.rs` (verify, may need no changes)

### 67.4 — Add MPS2-AN385 UART driver (CMSDK UART peripheral)

The MPS2-AN385 (Cortex-M3 FPGA image) has CMSDK UART peripherals. QEMU
emulates UART0–UART4 with register-compatible I/O at:

| UART | Base address |
|------|-------------|
| UART0 | 0x4000_4000 |
| UART1 | 0x4000_5000 |
| UART2 | 0x4000_6000 |
| UART3 | 0x4000_7000 |
| UART4 | 0x4000_9000 |

Register layout (CMSDK APB UART, 32-bit registers):
- `DATA` (0x00): TX/RX data (bits [7:0])
- `STATE` (0x04): bit 0 = TX full, bit 1 = RX full
- `CTRL` (0x08): bit 0 = TX enable, bit 1 = RX enable
- `BAUDDIV` (0x10): baud rate divisor = SystemCoreClock / baudrate

Create a minimal UART driver that implements `zpico_serial::SerialPort`:

```rust
pub struct CmsdkUart {
    base: usize,
}

impl CmsdkUart {
    pub fn new(base: usize, baudrate: u32, sysclk: u32) -> Self { ... }
}

impl zpico_serial::SerialPort for CmsdkUart {
    fn write(&mut self, data: &[u8]) -> usize { ... }
    fn read(&mut self, buf: &mut [u8]) -> usize { ... }
    fn read_exact(&mut self, buf: &mut [u8], timeout_ms: u32) -> usize { ... }
}
```

This driver can live in `packages/drivers/cmsdk-uart/` (reusable across CMSDK
boards) or inline in the zpico-platform crate.

**Files**:
- `packages/drivers/cmsdk-uart/Cargo.toml`
- `packages/drivers/cmsdk-uart/src/lib.rs`

### 67.5 — Feature-gate `nros-mps2-an385`: `ethernet` (default) vs `serial`

Refactor `Cargo.toml` to make `zpico-smoltcp`, `lan9118-smoltcp`, and `smoltcp`
optional behind an `ethernet` feature. Add `serial` feature gating
`zpico-serial` and `cmsdk-uart`.

Refactor `node.rs`:
- Extract current `init_hardware` body into `init_ethernet()`
- Add `init_serial()` for UART setup
- Gate each with `#[cfg(feature = "...")]`

The static storage (`ETH_DEVICE`, `NET_IFACE`, `NET_SOCKETS`) moves inside
`#[cfg(feature = "ethernet")]` blocks. Serial adds `UART_DEVICE` storage.

Compile-time check: at least one transport feature must be enabled:
```rust
#[cfg(not(any(feature = "ethernet", feature = "serial")))]
compile_error!("Enable at least one transport: `ethernet` or `serial`");
```

**Files**:
- `packages/boards/nros-mps2-an385/Cargo.toml`
- `packages/boards/nros-mps2-an385/src/config.rs`
- `packages/boards/nros-mps2-an385/src/node.rs`

### 67.6 — Refactor `Config` with `#[cfg(feature)]` transport fields

Split `Config` fields into always-present (locator, domain_id) and
feature-gated (MAC/IP/gateway for ethernet, baudrate for serial).

Add transport-specific constructors:
- `Config::default()` — uses default feature (ethernet)
- `Config::serial_default()` — serial with 115200 baud
- Builder methods: `.with_baudrate()`, `.with_mac()`, etc.

Existing `Config::listener()` / `Config::talker()` only available under
`#[cfg(feature = "ethernet")]`.

**Files**:
- `packages/boards/nros-mps2-an385/src/config.rs`
- `packages/boards/nros-stm32f4/src/config.rs`

### 67.7 — QEMU serial example

Create `examples/qemu-arm-baremetal/rust/zenoh/serial-talker/` — a minimal
talker that uses UART instead of Ethernet.

```rust
use nros_mps2_an385::{Config, run};

fn main() -> ! {
    let config = Config::serial_default();
    run(config, |config| {
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id);
        // ... same as Ethernet talker
    })
}
```

The `.cargo/config.toml` uses `-serial pty` instead of `-netdev tap`:
```toml
[target.thumbv7m-none-eabi]
runner = "qemu-system-arm -M mps2-an385 -semihosting -nographic -serial pty -kernel"
```

The example's `Cargo.toml` depends on `nros-mps2-an385` with
`features = ["serial"]` and `default-features = false`.

**Files**:
- `examples/qemu-arm-baremetal/rust/zenoh/serial-talker/Cargo.toml`
- `examples/qemu-arm-baremetal/rust/zenoh/serial-talker/src/main.rs`
- `examples/qemu-arm-baremetal/rust/zenoh/serial-talker/.cargo/config.toml`

### 67.8 — QEMU serial integration test

Add a test to `packages/testing/nros-tests/tests/emulator.rs` that:

1. Builds the serial-talker example
2. Launches QEMU with `-serial pty` (captures PTY path from QEMU stderr)
3. Starts zenohd with serial plugin connecting to the PTY
4. Verifies message delivery through the serial link

This tests the full path: MCU UART -> QEMU PTY -> zenohd serial plugin -> zenoh
network.

Use `QemuProcess` with modified args (no `-netdev`, add `-serial pty`).

**Files**:
- `packages/testing/nros-tests/tests/emulator.rs`
- `packages/testing/nros-tests/src/fixtures/binaries.rs` (add build helper)

### 67.9 — Feature-gate `nros-stm32f4`

Same pattern as 67.5 for STM32F4. The STM32F4 has both Ethernet and USART
peripherals, making it a natural dual-transport board.

Ethernet feature gates `stm32-eth`, `zpico-smoltcp`, `smoltcp`. Serial feature
gates `zpico-serial` and the USART initialization code.

**Files**:
- `packages/boards/nros-stm32f4/Cargo.toml`
- `packages/boards/nros-stm32f4/src/config.rs`
- `packages/boards/nros-stm32f4/src/node.rs`

### 67.10 — STM32F4 USART driver for `zpico-serial`

Implement `SerialPort` for STM32F4 USART using `stm32f4xx-hal`:

```rust
use stm32f4xx_hal::serial::Serial;

impl<USART, PINS> SerialPort for Serial<USART, PINS>
where
    USART: Instance,
{
    fn write(&mut self, data: &[u8]) -> usize { ... }
    fn read(&mut self, buf: &mut [u8]) -> usize { ... }
    fn read_exact(&mut self, buf: &mut [u8], timeout_ms: u32) -> usize { ... }
}
```

This can be a thin wrapper in `nros-stm32f4` or a separate driver crate. The
HAL's `Serial` type already provides `read`/`write` methods; the wrapper adds
the `read_exact` timeout loop using the DWT cycle counter.

**Files**:
- `packages/boards/nros-stm32f4/src/serial.rs` (or `packages/drivers/stm32f4-serial/`)

### 67.11 — Feature-gate `nros-esp32`

The ESP32-C3 board crate currently hardcodes WiFi via esp-radio. Add `serial`
feature for UART transport as an alternative.

ESP32-C3 has USB-JTAG-Serial and two UART controllers. The `serial` feature
would use UART0 or UART1 for zenoh transport.

**Note:** ESP32 uses zenoh-pico's built-in `espidf/network.c` serial
implementation, not `zpico-serial`. The board crate only needs to feature-gate
the WiFi/network init code and set `Z_FEATURE_LINK_SERIAL=1`. No `zpico-serial`
dependency required.

**Files**:
- `packages/boards/nros-esp32/Cargo.toml`
- `packages/boards/nros-esp32/src/lib.rs`

### 67.12 — Feature-gate `nros-esp32-qemu`

Same pattern for the ESP32-C3 QEMU board crate. Currently uses OpenETH.

Like 67.11, this uses zenoh-pico's built-in serial support, not `zpico-serial`.

**Files**:
- `packages/boards/nros-esp32-qemu/Cargo.toml`
- `packages/boards/nros-esp32-qemu/src/lib.rs`

### 67.13 — Documentation: serial transport guide

Add `book/src/guides/serial-transport.md` covering:
- When to use serial vs Ethernet
- Host-side zenohd serial plugin setup
- Board crate feature selection
- QEMU PTY testing workflow
- Baudrate tuning and buffer sizing
- Troubleshooting (framing errors, handshake failures)

**Files**:
- `book/src/guides/serial-transport.md`
- `book/src/SUMMARY.md`

### 67.14 — Update CLAUDE.md with transport feature conventions

Add transport feature conventions to CLAUDE.md:
- Board crates use `ethernet` (default) and `serial` features
- `Config` fields are `#[cfg(feature)]`-gated per transport
- `zpico-smoltcp` for TCP/UDP, `zpico-serial` for UART
- Examples use `default-features = false` when selecting non-default transport

**Files**:
- `CLAUDE.md`

## Acceptance Criteria

- [ ] `zpico-serial` compiles for `thumbv7m-none-eabi` with `link-serial` feature
- [ ] `nros-mps2-an385` compiles with `--features serial --no-default-features`
- [ ] `nros-mps2-an385` compiles with `--features ethernet` (default, no regression)
- [ ] `nros-mps2-an385` compiles with `--features ethernet,serial` (both enabled)
- [ ] QEMU serial example sends messages over UART PTY to zenohd
- [ ] Integration test passes: serial talker -> zenohd serial plugin -> subscriber
- [ ] `nros-stm32f4` compiles with `--features serial --no-default-features`
- [ ] `just quality` passes (no regressions in existing Ethernet tests)
- [ ] Serial transport guide published in book

## Notes

### COBS Framing is Handled by zenoh-pico

The `zpico-serial` crate does **not** need to implement COBS encoding/decoding
or CRC32 computation. zenoh-pico's `src/system/common/serial.c` provides
`_z_read_serial_internal` and `_z_send_serial_internal` which handle framing
internally. They call the platform's `_z_read_exact_serial` for raw byte I/O.

However, the bare-metal platform must provide **all six FFI symbols** because
zenoh-pico's common implementation calls the platform layer. The split is:

- `_z_open_serial_from_dev` / `_z_open_serial_from_pins` — **zpico-serial** (port setup)
- `_z_listen_serial_from_dev` / `_z_listen_serial_from_pins` — **zpico-serial** (same as open for client mode)
- `_z_close_serial` — **zpico-serial** (port teardown)
- `_z_read_exact_serial` — **zpico-serial** (raw byte read with timeout)
- `_z_read_serial_internal` — **zenoh-pico common** (COBS decode + CRC check, calls `_z_read_exact_serial`)
- `_z_send_serial_internal` — **zenoh-pico common** (COBS encode + CRC, calls platform write)

### Socket Type Reuse

The bare-metal `_z_sys_net_socket_t` has a `_handle: i8` field. For serial,
this handle indexes into the `zpico-serial` port table (0 or 1), same as
zpico-smoltcp uses it for TCP socket handles.

### Dual Transport

Enabling both `ethernet` and `serial` features on a board crate is valid. The
user selects the transport at runtime via the locator string in `Config`:
- `"tcp/192.0.3.1:7447"` → uses Ethernet/smoltcp
- `"serial/UART_0#115200"` → uses serial/UART

zenoh-pico's link layer dispatches based on the locator scheme.

### QEMU UART Numbering

QEMU MPS2-AN385 maps `-serial` arguments to UART peripherals:
- First `-serial` → UART0 (0x4000_4000)
- Second `-serial` → UART1 (0x4000_5000)

By default, QEMU connects UART0 to the monitor. Use `-serial pty` to expose it
as a host PTY device (path printed to stderr).

### zenohd Serial Plugin

The zenoh serial plugin (`zenoh-plugin-serial`) is a separate component. For
testing, it can be built from the zenoh-plugin-serial repository. The test
infrastructure should build or locate it automatically, similar to how zenohd
is auto-built from the submodule.

Alternative: use `socat` to bridge a PTY to a TCP socket, allowing standard
zenohd (without serial plugin) to communicate:
```bash
socat PTY,link=/tmp/vserial0 TCP:localhost:7447
```
This may simplify the test setup at the cost of adding a `socat` dependency.
