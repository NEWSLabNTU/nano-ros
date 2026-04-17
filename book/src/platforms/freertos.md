# FreeRTOS

nano-ros runs on FreeRTOS with lwIP networking, targeting QEMU MPS2-AN385
(Cortex-M3 + LAN9118 Ethernet). FreeRTOS + lwIP is the most widely deployed
RTOS + TCP/IP combination in the embedded industry (STM32, NXP, Renesas, TI).

## Overview

The FreeRTOS platform uses:

- **FreeRTOS kernel** -- task scheduling, mutexes, semaphores
- **lwIP** -- TCP/IP stack in threaded mode (`NO_SYS=0`, BSD sockets)
- **zenoh-pico** -- Zenoh transport over lwIP BSD sockets
- **LAN9118** -- MMIO Ethernet controller (QEMU MPS2-AN385)

Board crate: `nros-mps2-an385-freertos` (in `packages/boards/`).

### Why lwIP (Not FreeRTOS+TCP)

lwIP was chosen over FreeRTOS+TCP because zenoh-pico's FreeRTOS+TCP variant
lacks UDP multicast (needed for zenoh scouting) and TCP_NODELAY (needed for
low-latency ROS 2 messaging). lwIP has near-universal vendor adoption
(ESP32, STM32, NXP, TI, Xilinx) and a smaller flash footprint (10--40 KB).

## Setup

Download the FreeRTOS kernel and lwIP sources:

```bash
just freertos setup
```

This places the sources in `third-party/freertos/kernel/` and `third-party/freertos/lwip/`.
Override the paths with environment variables if your sources are elsewhere:

| Variable              | Default                    | Description                        |
|-----------------------|----------------------------|------------------------------------|
| `FREERTOS_DIR`        | `third-party/freertos/kernel` | FreeRTOS kernel source             |
| `FREERTOS_PORT`       | `GCC/ARM_CM3`              | FreeRTOS portable layer            |
| `LWIP_DIR`            | `third-party/freertos/lwip`            | lwIP source                        |
| `FREERTOS_CONFIG_DIR` | Board crate's `config/`    | `FreeRTOSConfig.h` + `lwipopts.h` |

### Prerequisites

- `qemu-system-arm` (for running tests)
- `arm-none-eabi-gcc` (for compiling FreeRTOS + lwIP C code)
- Rust nightly toolchain (`thumbv7m-none-eabi` target)

## Building

```bash
just build-examples-freertos
```

This cross-compiles all FreeRTOS examples for `thumbv7m-none-eabi` using
`cargo build --release`. The board crate's `build.rs` compiles FreeRTOS
kernel, lwIP, and the LAN9118 lwIP netif driver via the `cc` crate.

### Available Examples

**Rust** examples are in `examples/qemu-arm-freertos/rust/zenoh/`:

| Example          | Description                                      |
|------------------|--------------------------------------------------|
| `talker`         | Publishes `std_msgs/Int32` on `/chatter`         |
| `listener`       | Subscribes to `std_msgs/Int32` on `/chatter`     |
| `service-server` | Serves `AddTwoInts` on `/add_two_ints`           |
| `service-client` | Calls `AddTwoInts` on `/add_two_ints`            |
| `action-server`  | Serves `Fibonacci` action on `/fibonacci`        |
| `action-client`  | Sends `Fibonacci` goal on `/fibonacci`           |

**C** examples are in `examples/qemu-arm-freertos/c/zenoh/` (same 6 use cases).

**C++** examples are in `examples/qemu-arm-freertos/cpp/zenoh/`:

| Example          | Description                                      |
|------------------|--------------------------------------------------|
| `talker`         | Publishes `std_msgs/Int32` on `/chatter`         |
| `listener`       | Subscribes to `std_msgs/Int32` on `/chatter`     |
| `service-server` | Serves `AddTwoInts` on `/add_two_ints`           |
| `service-client` | Calls `AddTwoInts` on `/add_two_ints`            |

C++ examples use `nros-cpp` freestanding mode (C++14, no `std`). Action examples
are deferred pending alloc-free action module support.

#### C/C++ Build System

C and C++ examples use CMake with cross-compilation. Shared CMake modules under
`examples/qemu-arm-freertos/cmake/` provide:

- `arm-none-eabi-toolchain.cmake` -- ARM Cortex-M3 toolchain + `Rust_CARGO_TARGET`
- `freertos-platform.cmake` -- compiles FreeRTOS + lwIP + startup, builds nros
  FFI via Corrosion, provides `nano_ros_generate_interfaces()` for message codegen

Build a C++ example:

```bash
cmake -S examples/qemu-arm-freertos/cpp/zenoh/talker \
      -B examples/qemu-arm-freertos/cpp/zenoh/talker/build
cmake --build examples/qemu-arm-freertos/cpp/zenoh/talker/build
```

Requires `FREERTOS_DIR` and `LWIP_DIR` environment variables (set by `just freertos setup`).

## Testing

```bash
just test-freertos
```

Tests run under `qemu-system-arm -M mps2-an385` with TAP networking. Each
QEMU instance connects to the host bridge (`br-qemu`) via TAP devices for
zenohd communication. The test infrastructure builds a FreeRTOS firmware
image with the example app, boots it in QEMU, and verifies message exchange.

### Network Configuration

FreeRTOS QEMU instances use the same IP scheme as other QEMU board crates:

| Role             | IP Address  | TAP Device |
|------------------|-------------|------------|
| Talker/Publisher  | 192.0.3.10  | tap-qemu0  |
| Listener/Sub     | 192.0.3.11  | tap-qemu1  |
| Service Server   | 192.0.3.12  | tap-qemu0  |
| Service Client   | 192.0.3.13  | tap-qemu1  |
| zenohd (host)    | 192.0.3.1   | br-qemu    |

## Architecture

### Board Crate

The `nros-mps2-an385-freertos` board crate follows the standard `Config` / `run()` pattern documented in the [Board Crate Guide](../guides/board-crate.md). It provides network and node configuration presets (`default()`, `listener()`, `server()`, `client()`) and initializes FreeRTOS, lwIP, and LAN9118 before running the user closure as a FreeRTOS task. Output uses ARM semihosting (`SYS_WRITE0`).

Unlike NuttX, FreeRTOS is `no_std` -- examples use `#![no_std]` / `#![no_main]`
entry points with semihosting for output.

### FreeRTOS Configuration

The board configuration in `packages/boards/nros-mps2-an385-freertos/config/`:

- `FreeRTOSConfig.h` -- 25 MHz CPU clock, 256 KB heap, recursive mutexes,
  semihosting-compatible `configASSERT()`
- `lwipopts.h` -- threaded mode (`NO_SYS=0`), BSD sockets, `TCP_NODELAY`,
  16 KB lwIP heap
- `mps2_an385.ld` -- 4 MB SSRAM at 0x21000000

### Task Model

FreeRTOS runs multiple tasks for networking and application logic:

| Priority | Task             | Role                                    |
|----------|------------------|-----------------------------------------|
| 4        | tcpip_thread     | lwIP TCP/IP processing                  |
| 4        | poll task        | LAN9118 RX FIFO → lwIP                  |
| 4        | zenoh read/lease | zenoh-pico background I/O               |
| 3        | app task         | nros Executor + Node                    |
| 0        | idle             | WFI (mandatory for QEMU networking)     |

## Scheduling Configuration

Task priorities and stack sizes are configurable via `config.toml`:

```toml
[scheduling]
app_priority = 12              # 0–31 normalized (12 = FreeRTOS pri 3)
app_stack_bytes = 65536        # 64 KB
zenoh_read_priority = 16       # 16 = FreeRTOS pri 4
zenoh_read_stack_bytes = 5120
zenoh_lease_priority = 16
zenoh_lease_stack_bytes = 5120
poll_priority = 16             # Network poll task
poll_interval_ms = 5           # Poll every 5 ms
```

Omit `[scheduling]` entirely to use the defaults shown above.

The normalized 0–31 scale maps linearly to FreeRTOS priorities 0–7
(`configMAX_PRIORITIES = 8`). The mapping function:
`freertos_pri = normalized * 7 / 31`.

**Constraints** -- keep these for reliable operation:
- `poll_priority ≥ zenoh_read_priority` -- poll task must feed the RX FIFO
- `zenoh_read_priority ≥ app_priority` -- prevents lease timeouts
- `app_stack_bytes ≥ 16384` -- executor arena + zenoh-pico buffers (64 KB for actions)

## Tracing (Tonbandgeraet)

Task scheduling can be visualized using [Tonbandgeraet](https://github.com/schilkp/Tonbandgeraet),
an open-source embedded tracer that outputs to [Perfetto](https://ui.perfetto.dev).

Tracing is opt-in via the `NROS_TRACE=1` environment variable. When enabled,
FreeRTOS trace hooks record task switches, queue operations, and mutex activity
to a 16 KB RAM ring buffer. After the example completes, the buffer is dumped
to `trace.bin` via ARM semihosting and converted to Perfetto format.

```bash
# Capture and convert a trace (builds tband-cli automatically)
just freertos trace talker

# Open the result in your browser
# → https://ui.perfetto.dev → Open trace file → test-logs/freertos-trace/trace.pf
```

Overhead: ~16 KB RAM for the snapshot buffer, ~40 ns timestamp resolution
(SysTick at 25 MHz). No overhead when `NROS_TRACE` is not set.

## Status

FreeRTOS platform support (Phase 54) is complete. Phase 69 added C and C++
examples with CMake cross-compilation, integration tests, and shared CMake
platform modules. C++ action examples are pending alloc-free action module support.

## LAN9118 Networking Debugging Guide

The remainder of this chapter is a reference for debugging LAN9118 Ethernet
on QEMU MPS2-AN385 with FreeRTOS + lwIP.

### Architecture

**Board**: QEMU MPS2-AN385 (ARM Cortex-M3, 25 MHz)
- Flash: 4 MB at `0x0000_0000`
- SRAM: 4 MB at `0x2000_0000`
- LAN9118 Ethernet: MMIO base `0x4020_0000`, IRQ 13

**Software stack**: FreeRTOS kernel + lwIP (threaded mode, `NO_SYS=0`) + zenoh-pico (BSD sockets)

**Task priorities** (highest to lowest):

| Priority | Task             | Role                                                            |
|----------|------------------|-----------------------------------------------------------------|
| 4        | `tcpip_thread`   | lwIP TCP/IP processing (set via `TCPIP_THREAD_PRIO`)            |
| 4        | poll task        | Calls `lan9118_lwip_poll()` to drain RX FIFO                    |
| 4        | zenoh read/lease | zenoh-pico background tasks (default: `configMAX_PRIORITIES/2`) |
| 3        | app task         | zenoh-pico / nros application logic                             |
| 2        | timer task       | FreeRTOS software timer service                                 |
| 0        | idle             | WFI hook (critical for QEMU -- see below)                       |

**Poll task at priority 4**: The poll task must run at the same priority as the zenoh-pico
read task (which uses a 100ms `recv()` timeout loop). At lower priority, the read task
monopolizes CPU time and the poll task can't drain the LAN9118 RX FIFO, causing TCP
keep-alives to be missed and zenoh sessions to expire.

**Data flow**: LAN9118 RX FIFO --> poll task (`lan9118_lwip_poll`) --> `tcpip_input` --> `tcpip_thread` --> socket recv buffers --> zenoh-pico

### LAN9118 Register Map (Key Registers)

Direct registers (base + offset):

| Offset | Name           | Description                                                                  |
|--------|----------------|------------------------------------------------------------------------------|
| `0x00` | `RX_DATA_PORT` | RX FIFO data read (32-bit words)                                             |
| `0x20` | `TX_DATA_PORT` | TX FIFO data write (32-bit words, also TX commands)                          |
| `0x40` | `RX_STAT_PORT` | RX status FIFO (packet length, error flags)                                  |
| `0x50` | `ID_REV`       | Chip ID + revision (upper 16 = `0x9220` or `0x0118`)                         |
| `0x54` | `IRQ_CFG`      | IRQ output configuration (default `0x22000111`)                              |
| `0x58` | `INT_STS`      | Interrupt status (TX/RX activity flags)                                      |
| `0x5C` | `INT_EN`       | Interrupt enable mask                                                        |
| `0x68` | `FIFO_INT`     | FIFO interrupt threshold levels                                              |
| `0x6C` | `RX_CFG`       | RX configuration                                                             |
| `0x70` | `TX_CFG`       | TX configuration (bit 1 = `TX_ON`)                                           |
| `0x74` | `HW_CFG`       | Hardware config (bit 0 = soft reset, bits 19:16 = TX FIFO size)              |
| `0x78` | `RX_DP_CTRL`   | RX data path control (bit 31 = fast-forward/discard)                         |
| `0x7C` | `RX_FIFO_INF`  | RX FIFO info (bits 23:16 = status entries used, bits 15:0 = data bytes used) |
| `0x80` | `TX_FIFO_INF`  | TX FIFO info (bits 15:0 = data free space)                                   |
| `0x88` | `GPIO_CFG`     | GPIO/LED configuration                                                       |
| `0xA4` | `MAC_CSR_CMD`  | MAC CSR indirect access command (bit 31 = busy, bit 30 = read)               |
| `0xA8` | `MAC_CSR_DATA` | MAC CSR indirect access data                                                 |
| `0xAC` | `AFC_CFG`      | Auto flow control configuration                                              |

Indirect MAC CSR registers (accessed via `MAC_CSR_CMD` / `MAC_CSR_DATA`):

| Index | Name       | Key Bits                                                |
|-------|------------|---------------------------------------------------------|
| 1     | `MAC_CR`   | bit 3 = TXEN, bit 2 = RXEN, bit 18 = PRMS (promiscuous) |
| 2     | `ADDRH`    | MAC address bytes 5:4                                   |
| 3     | `ADDRL`    | MAC address bytes 3:0                                   |
| 6     | `MII_ACC`  | PHY register access (bit 0 = busy, bit 1 = write)       |
| 7     | `MII_DATA` | PHY register data                                       |

**MAC CSR access protocol**: Write `MAC_CSR_CMD` with busy + read/write + register index, poll until busy clears, then read/write `MAC_CSR_DATA`. Always check busy before starting.

### QEMU LAN9118 Networking Model

Understanding QEMU's internal networking is essential for debugging. Key behaviors:

#### Hub-based architecture

QEMU uses a legacy VLAN 0 hub that connects the NIC model to the network backend (Slirp user-mode, TAP, socket, etc.). All packets flow through this hub.

```
Guest LAN9118 NIC  <-->  QEMU Hub (VLAN 0)  <-->  Backend (Slirp / TAP / socket)
```

#### Synchronous frame delivery

When the guest transmits a packet via `do_tx_packet()`, QEMU calls `qemu_send_packet()` which delivers the frame through the hub synchronously. If using Slirp (user-mode networking), ARP replies and other responses are generated immediately within the same call chain:

```
Guest TX write to TX_DATA_PORT
  --> do_tx_packet()
    --> qemu_send_packet()
      --> Hub delivery
        --> Slirp processes frame
          --> Slirp generates ARP reply (if applicable)
            --> Hub delivers reply back
              --> lan9118_receive()
                --> Frame lands in RX FIFO
```

This means the RX FIFO may already contain a response **before** the TX register write returns to the guest. This is a useful property for diagnostics.

#### No `can_receive` callback

QEMU's LAN9118 model does not implement a `can_receive` callback. The NIC always accepts frames from the hub (subject to MAC_CR_RXEN, FIFO space, and MAC filter checks in `lan9118_receive()`).

#### No `qemu_flush_queued_packets()` on RXEN transition

When the guest enables `MAC_CR_RXEN` (bit 2 of MAC_CR), QEMU does **not** call `qemu_flush_queued_packets()`. Packets that arrived while RXEN was off are silently dropped. The init sequence must enable RXEN before any traffic is expected.

#### The `delivering` flag

QEMU's net queue has a per-queue `delivering` flag to prevent re-entrant delivery. This is per-direction, so TX-triggered synchronous RX delivery works fine -- the TX and RX paths use separate queues.

#### MAC filter

`lan9118_receive()` checks:
1. `MAC_CR_RXEN` -- if clear, frame is silently dropped
2. Size limits: rejects frames < 14 bytes or > 2048 bytes (with VLAN headroom)
3. RX FIFO must have space
4. MAC address filter: calls `lan9118_filter()` which checks unicast/multicast/broadcast against configured MAC address, or accepts everything if `MAC_CR_PRMS` (promiscuous, bit 18) is set

### Common Pitfalls

#### Diagnostic timing: false "0 RX" readings

**Problem**: Checking RX FIFO status after `vTaskDelay()` shows zero entries even though frames arrived.

**Cause**: The poll task runs during the delay and consumes frames from the RX FIFO before the diagnostic code reads the register.

**Fix**: Check `RX_FIFO_INF` immediately after TX, before yielding to the OS. For diagnostics, use busy-wait with WFI instead of `vTaskDelay()`.

#### Initialization order: scheduler must be running first

**Problem**: `nros_freertos_init_network()` hangs or asserts.

**Cause**: lwIP threaded mode (`NO_SYS=0`) requires FreeRTOS to be running before `tcpip_init()` is called. `tcpip_init()` creates the tcpip_thread, which requires a running scheduler.

**Fix**: Call `vTaskStartScheduler()` first (via `nros_freertos_start_scheduler()`), then call `nros_freertos_init_network()` from within a FreeRTOS task.

#### Vector table: FreeRTOS handlers must go directly

**Problem**: HardFault on first context switch or mysterious crashes.

**Cause**: FreeRTOS Cortex-M3 port validates that `vPortSVCHandler`, `xPortPendSVHandler`, and `xPortSysTickHandler` are installed at the correct vector table positions. Wrapper functions break this validation.

**Fix**: Place the FreeRTOS port functions directly in the vector table:
```c
const vector_fn isr_vector[] = {
    (vector_fn)(uintptr_t)&_estack,  /* MSP */
    Reset_Handler,                    /* 1 */
    ...
    vPortSVCHandler,                  /* 11: SVCall */
    ...
    xPortPendSVHandler,              /* 14: PendSV */
    SysTick_Handler,                 /* 15: SysTick -- thin wrapper OK here */
};
```

The `SysTick_Handler` wrapper is acceptable because it only guards against ticks before the scheduler starts (checking `xTaskGetSchedulerState()`).

#### `LWIP_TCPIP_CORE_LOCKING` must be 0

**Problem**: Assert failure in `netif_add()` or `netif_set_up()` about an uninitialized mutex.

**Cause**: When `LWIP_TCPIP_CORE_LOCKING=1`, lwIP expects all netif operations to be called from `tcpip_thread` or with the core lock held. Our setup calls `netif_add()` from the app task after `tcpip_init()` completes.

**Fix**: Set `LWIP_TCPIP_CORE_LOCKING 0` in `lwipopts.h`. The socket API is already thread-safe without core locking.

#### Missing IRQ_CFG write

**Problem**: Intermittent stalls or missed interrupts (even in polling mode).

**Cause**: The IRQ_CFG register at offset `0x54` controls the interrupt output configuration. If left at its reset default, the interrupt line may be in an unexpected state.

**Fix**: Always write `IRQ_CFG_DEFAULT` (`0x22000111`) during init, even in polling mode:
```c
reg_write(base, 0x54 /* IRQ_CFG */, 0x22000111u);
```

#### CRC handling when reading RX FIFO

**Problem**: Corrupted frames or misaligned reads after receiving a packet.

**Cause**: The packet length in `RX_STAT_PORT` includes the 4-byte FCS/CRC. All 32-bit words must be read from `RX_DATA_PORT` including the CRC bytes, even though the CRC is not passed to lwIP.

**Fix**: Read `(pkt_len + 3) / 4` words from the FIFO (where `pkt_len` includes CRC), but only copy `pkt_len - 4` bytes to the pbuf. The driver already handles this correctly.

#### Deterministic `rand()` causes duplicate Zenoh session IDs

**Problem**: When running two QEMU instances (talker + listener), the second instance's `z_open()` hangs and zenohd closes the TCP connection (visible as `FIN-WAIT-2` in `ss -tnp`).

**Cause**: zenoh-pico generates a 16-byte session ID via `z_random_fill()` -> `LWIP_RAND()` -> `rand()`. On FreeRTOS, `rand()` starts from default seed 1 on every boot. All QEMU instances generate identical random sequences, producing identical Zenoh session IDs. zenohd rejects the duplicate by closing the connection.

**Diagnosis**: Monitor zenohd connections during startup:
```bash
watch -n0.5 'ss -tnp | grep 7447'
```
If both QEMU instances use the same source port or the second connection immediately transitions to `FIN-WAIT-2`, duplicate session IDs are the cause.

**Fix**: Seed `srand()` with a value unique to each node during `nros_freertos_init_network()`. Use the node's IP address (guaranteed unique per node) with a multiplicative hash to spread bits:
```c
uint32_t seed = ((uint32_t)ip[0] << 24) | ((uint32_t)ip[1] << 16)
              | ((uint32_t)ip[2] << 8)  | (uint32_t)ip[3];
seed = seed * 2654435761u;  /* Knuth multiplicative hash */
seed ^= ((uint32_t)mac[4] << 8) | (uint32_t)mac[5];
if (seed == 0) seed = 1;
srand(seed);
```

**Caveat**: Do NOT use a simple XOR of MAC and IP -- common address patterns can cancel out (e.g., MAC `...00` XOR IP `...0A` equals MAC `...01` XOR IP `...0B`).

#### Task stack overflow corrupts lwIP `tcpip_mbox`

**Problem**: Service server or action server crashes with `lwIP ASSERT: Invalid mbox` during network init or shortly after `z_open()`. Simpler examples (pub/sub) work fine.

**Cause**: The `Executor` struct has an inline `arena: [MaybeUninit<u8>; ARENA_SIZE]` array that lives on the FreeRTOS task stack. Service examples use `NROS_EXECUTOR_ARENA_SIZE=4096` (4 KB) and action examples use `NROS_EXECUTOR_ARENA_SIZE=8192` (8 KB). Combined with zenoh-pico's internal stack buffers (transport TX/RX, peer structures) and Rust function frames, the total stack usage exceeds a small task stack.

When the stack overflows, it corrupts adjacent memory including lwIP's global `tcpip_mbox` variable (declared in `tcpip.c`). Any subsequent call to `tcpip_input()`, `tcpip_callback()`, or `sys_mbox_trypost()` triggers the "Invalid mbox" assertion, which enters an infinite `for(;;){}` loop.

**Diagnosis**: If pub/sub examples work but service/action examples crash with "Invalid mbox":
- Compare the arena sizes: talker sets `NROS_EXECUTOR_MAX_CBS=0` (no callbacks), service server uses defaults (4 slots, 4096 arena), action server sets `NROS_EXECUTOR_MAX_CBS=8` and `NROS_EXECUTOR_ARENA_SIZE=8192`
- Larger arena = more stack needed = more likely to overflow

**Fix**: Set `APP_TASK_STACK` large enough for the largest example. 64 KB (16384 words) provides adequate headroom for all example types:
```rust
const APP_TASK_STACK: u32 = 16384; // 64 KB
```

**Note**: The 256 KB FreeRTOS heap (`configTOTAL_HEAP_SIZE`) has plenty of room. The constraint is the per-task stack, not total memory.

#### Manual-polling action server must call `try_handle_get_result()` explicitly

**Problem**: Action client receives `ServiceRequestFailed` on `get_result()`, even though the server completes the goal successfully.

**Cause**: When using `create_action_server()` (manual polling), the action server is NOT registered in the executor arena. `spin_once()` only processes get_result queries for arena-registered servers (i.e., those added via `add_action_server()`). Without explicit `try_handle_get_result()` calls, the server never responds to the client's get_result query, which times out after `SERVICE_DEFAULT_TIMEOUT_MS` (5 seconds).

**Diagnosis**: Server output shows "Goal completed" and "Server shutting down." but client output shows "ServiceRequestFailed" after "Requesting result...".

**Fix**: After `complete_goal()`, call `try_handle_get_result()` in the spin loop:
```rust
server.complete_goal(&goal_id, GoalStatus::Succeeded, result);
// Must explicitly handle get_result queries since we're not arena-registered
for _ in 0..2000 {
    executor.spin_once(10);
    let _ = server.try_handle_get_result();
}
```

#### WFI in idle hook is mandatory for QEMU networking

**Problem**: QEMU never delivers incoming frames (ARP replies, TCP SYN-ACKs) to the guest.

**Cause**: Without WFI, the FreeRTOS idle task busy-loops. QEMU's main event loop never gets CPU time to service the TAP/Slirp file descriptor and deliver inbound packets.

**Fix**: Enable the idle hook and execute WFI:
```c
#define configUSE_IDLE_HOOK 1

void vApplicationIdleHook(void) {
    __asm__ volatile("wfi");
}
```

### Key Insights from QEMU Source Analysis

These findings come from reading QEMU's `hw/net/lan9118.c`:

#### `lan9118_receive()`

Called by the hub when a frame arrives for the NIC. Checks:
1. `MAC_CR_RXEN` -- if clear, frame is silently dropped
2. Size limits: rejects frames < 14 bytes or > 2048 bytes (with VLAN headroom)
3. FIFO space: needs room for status word + aligned data words
4. MAC filter: calls `lan9118_filter()` which checks unicast/multicast/broadcast against configured MAC address, or accepts everything if `MAC_CR_PRMS` (promiscuous, bit 18) is set

#### `do_tx_packet()`

Assembles frame from TX FIFO entries and calls `qemu_send_packet()`. The packet is delivered synchronously through the hub. If the backend generates a response (e.g., Slirp ARP reply), it arrives in the RX FIFO before `do_tx_packet()` returns.

#### `lan9118_filter()`

Evaluates MAC filter rules. For our use case, the simplest approach is to rely on the default filter (which accepts frames addressed to the configured MAC + broadcast). Promiscuous mode (`MAC_CR_PRMS`) can be enabled for debugging but is not needed for normal operation.

### Debugging Tips

#### Use semihosting for printf-style output

ARM semihosting bypasses UART and writes directly to the QEMU console via `bkpt #0xAB`. The startup code provides `semihosting_write0()`:

```c
void semihosting_write0(const char *s) {
    __asm__ volatile("mov r0, #0x04\n"
                     "mov r1, %0\n"
                     "bkpt #0xAB\n"
                     : : "r"(s) : "r0", "r1", "memory");
}
```

Requires QEMU flag: `-semihosting-config enable=on,target=native`

#### Check RX FIFO immediately after TX

Due to synchronous delivery, the most reliable way to verify that the full TX-hub-backend-hub-RX path works is to check `RX_FIFO_INF` immediately after writing a TX frame, before any OS delay or yield. The diagnostic function `nros_freertos_diag_network()` demonstrates this pattern.

#### Use QEMU monitor for hub state inspection

Launch QEMU with a monitor:
```bash
qemu-system-arm ... -monitor telnet:127.0.0.1:4444,server,nowait
```

Useful monitor commands:
- `info network` -- shows NICs, hub connections, and backend info
- `info qtree` -- full device tree (find the LAN9118 instance)

#### WFI triggers QEMU main loop iteration

A `WFI` (Wait For Interrupt) instruction causes the vCPU to halt, returning control to QEMU's main event loop. This is when QEMU services network FDs. Use this to force packet delivery in diagnostic loops:

```c
for (int i = 0; i < 200; i++) {
    __asm__ volatile("wfi");
    if (rx_packets_pending(base) > 0) break;
}
```

#### Diagnostic ARP probing

Send a raw ARP request to a known-responding address (e.g., the QEMU gateway at `10.0.2.2` for Slirp, or `192.0.3.1` for TAP bridge) and check for an immediate RX FIFO entry. If the reply arrives synchronously (before any delay), the NIC TX/RX path, hub routing, and backend are all functioning. If it does not, check:

1. `MAC_CR` has both TXEN and RXEN set
2. MAC address in the ARP frame matches the NIC's configured MAC
3. The QEMU network backend is connected (check `-netdev` and `-nic` flags)

#### Register dump for quick health check

Read these registers to assess NIC state:
- `ID_REV` (`0x50`): should be `0x92200001` or similar -- confirms NIC is present
- `MAC_CR` (indirect index 1): confirm TXEN (bit 3) and RXEN (bit 2) are set
- `TX_FIFO_INF` (`0x80`): free space should be ~4-5 KB after init
- `RX_FIFO_INF` (`0x7C`): status entries (bits 23:16) show pending unread packets
- `INT_STS` (`0x58`): TX/RX interrupt flags show recent activity

### Reference Implementations

These are useful for cross-referencing register sequences and understanding expected behavior:

- **FreeRTOS-Plus-TCP MPS2_AN385 driver** (official FreeRTOS demo):
  `FreeRTOS-Plus/Source/FreeRTOS-Plus-TCP/portable/NetworkInterface/MPS2_AN385/`
  in the [FreeRTOS repository](https://github.com/FreeRTOS/FreeRTOS)

- **Zephyr `eth_smsc911x.c`** (cleanest single-file reference):
  `drivers/ethernet/eth_smsc911x.c`
  in the [Zephyr repository](https://github.com/zephyrproject-rtos/zephyr)

- **ARM CMSIS-Driver ETH_LAN9220**:
  `CMSIS/Driver/DriverTemplates/Driver_ETH_MAC.c` and related files
  in the [ARM-software/CMSIS_5 repository](https://github.com/ARM-software/CMSIS_5)

- **QEMU LAN9118 model source**:
  `hw/net/lan9118.c`
  in the [QEMU repository](https://gitlab.com/qemu-project/qemu)

- **nano-ros Rust LAN9118 driver** (smoltcp variant, same register init sequence):
  `packages/drivers/lan9118-smoltcp/`
