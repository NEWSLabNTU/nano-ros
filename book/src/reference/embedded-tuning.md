# Embedded Transport Tuning

This guide documents compile-time constants for tuning nano-ros transport layers
on embedded targets. All constants are set via environment variables at build time
and control static memory allocation -- no heap is used for transport buffers.

## Quick Start

Set environment variables before building:

```bash
# Example: constrained Cortex-M4 with 256KB RAM
ZPICO_MAX_PUBLISHERS=4 \
ZPICO_MAX_SUBSCRIBERS=4 \
ZPICO_FRAG_MAX_SIZE=1400 \
ZPICO_BATCH_UNICAST_SIZE=1024 \
ZPICO_SUBSCRIBER_BUFFER_SIZE=512 \
cargo build --release
```

For Zephyr builds, use Kconfig instead (see [Zephyr Integration](#zephyr-integration)).

## Zenoh-pico (ZPICO_*)

### Entity Limits

These control the maximum number of concurrent zenoh entities. Each slot is
statically allocated -- unused slots still consume memory.

| Variable | Default | Description |
|----------|---------|-------------|
| `ZPICO_MAX_PUBLISHERS` | 8 | Max concurrent publishers |
| `ZPICO_MAX_SUBSCRIBERS` | 8 | Max concurrent subscribers |
| `ZPICO_MAX_QUERYABLES` | 8 | Max concurrent service servers |
| `ZPICO_MAX_LIVELINESS` | 16 | Max concurrent liveliness tokens |
| `ZPICO_MAX_PENDING_GETS` | 4 | Max concurrent in-flight service calls |

**Sizing rule:** Set each to the exact number your application uses, plus 1-2
spare slots for parameter services (if enabled). Over-provisioning wastes static
memory; under-provisioning causes runtime failures.

### Buffer Sizes

These control message fragmentation and reassembly. They have **platform-aware
defaults** -- embedded targets get smaller defaults than POSIX.

| Variable | POSIX Default | Embedded Default | Description |
|----------|---------------|------------------|-------------|
| `ZPICO_FRAG_MAX_SIZE` | 65535 | 2048 | Max reassembled message size (bytes) |
| `ZPICO_BATCH_UNICAST_SIZE` | 65535 | 1024 | Max unicast batch before fragmentation |
| `ZPICO_BATCH_MULTICAST_SIZE` | 8192 | 1024 | Max multicast batch size |

**Platform classification:**
- **POSIX**: Linux, macOS, NuttX (POSIX-compatible), ThreadX (NetX Duo BSD sockets)
- **Embedded**: bare-metal, Zephyr, FreeRTOS

**Key constraints:**
- `ZPICO_FRAG_MAX_SIZE` limits the largest message your node can receive. Messages
  exceeding this are silently dropped.
- `ZPICO_BATCH_UNICAST_SIZE` limits the largest message your node can send without
  fragmentation. Messages larger than this are split into fragments.
- All batch sizes are capped at 65535 (u16 protocol limit).
- For MTU-constrained networks (e.g., Ethernet 1500, Zephyr default ~1400), set
  batch sizes below the network MTU to avoid IP fragmentation.

### Per-Entity Buffers

Each subscriber and service server has a static receive buffer.

| Variable | Default | Description |
|----------|---------|-------------|
| `ZPICO_SUBSCRIBER_BUFFER_SIZE` | 1024 | Per-subscriber receive buffer (bytes) |
| `ZPICO_SERVICE_BUFFER_SIZE` | 1024 | Per-service-server receive buffer (bytes) |
| `ZPICO_GET_REPLY_BUF_SIZE` | 4096 | Service client reply buffer (bytes) |
| `ZPICO_GET_POLL_INTERVAL_MS` | 10 | Service call polling interval (ms) |

**Sizing rule:** Set buffer sizes to the CDR-serialized size of the largest message
the entity will receive. CDR overhead is typically 4-8 bytes alignment per field.
Use `std::mem::size_of::<YourMessage>()` as a rough upper bound; actual CDR is
often smaller due to variable-length fields.

### smoltcp Network Stack (ZPICO_SMOLTCP_*)

These apply only to bare-metal targets using the smoltcp TCP/IP stack.

| Variable | Default | Description |
|----------|---------|-------------|
| `ZPICO_SMOLTCP_MAX_SOCKETS` | 4 | Max concurrent TCP sockets |
| `ZPICO_SMOLTCP_MAX_UDP_SOCKETS` | 2 | Max concurrent UDP sockets |
| `ZPICO_SMOLTCP_BUFFER_SIZE` | 2048 | Per-socket staging buffer (bytes) |
| `ZPICO_SMOLTCP_CONNECT_TIMEOUT_MS` | 30000 | TCP connection timeout (ms) |
| `ZPICO_SMOLTCP_SOCKET_TIMEOUT_MS` | 10000 | TCP read/write timeout (ms) |

**Note:** Increasing `MAX_SOCKETS` beyond 4 or `MAX_UDP_SOCKETS` beyond 2 requires
adding static buffer declarations in `zpico-smoltcp/src/lib.rs`.

### Hard-Coded Protocol Constants

These are not tunable via environment variables:

| Constant | Value | Description |
|----------|-------|-------------|
| `Z_CONFIG_SOCKET_TIMEOUT` | 100 ms | TCP socket read timeout |
| `Z_TRANSPORT_LEASE` | 10000 ms | Transport lease/heartbeat timeout |
| `Z_TRANSPORT_LEASE_EXPIRE_FACTOR` | 3 | Lease expiry multiplier |

## XRCE-DDS (XRCE_*)

### Transport MTU

| Variable | POSIX Default | Embedded Default | Description |
|----------|---------------|------------------|-------------|
| `XRCE_TRANSPORT_MTU` | 4096 | 512 | Custom transport MTU (bytes) |

The MTU determines stream buffer sizing (4x MTU for reliable streams) and the
opaque transport struct size (`MTU + 256` bytes overhead).

**Platform classification** matches zenoh-pico: NuttX and ThreadX use POSIX
defaults; bare-metal, Zephyr, and FreeRTOS use embedded defaults.

### Entity Limits

| Variable | Default | Description |
|----------|---------|-------------|
| `XRCE_MAX_SUBSCRIBERS` | 8 | Max concurrent subscribers |
| `XRCE_MAX_SERVICE_SERVERS` | 4 | Max concurrent service servers |
| `XRCE_MAX_SERVICE_CLIENTS` | 4 | Max concurrent service clients |
| `XRCE_BUFFER_SIZE` | 1024 | Per-slot receive buffer (bytes) |

### Stream and Timing

| Variable | Default | Description |
|----------|---------|-------------|
| `XRCE_STREAM_HISTORY` | 4 | Reliable stream history depth (must be >= 2) |
| `XRCE_ENTITY_CREATION_TIMEOUT_MS` | 1000 | Entity creation timeout (ms) |
| `XRCE_SERVICE_REPLY_TIMEOUT_MS` | 1000 | Service reply timeout (ms) |
| `XRCE_SERVICE_REPLY_RETRIES` | 5 | Service reply retry count |
| `XRCE_MAX_SESSION_CONNECTION_ATTEMPTS` | 10 | Session connection attempts |
| `XRCE_MIN_SESSION_CONNECTION_INTERVAL` | 25 | Min interval between attempts (ms) |
| `XRCE_MIN_HEARTBEAT_TIME_INTERVAL` | 100 | Session heartbeat interval (ms) |

**Warning:** `XRCE_STREAM_HISTORY` must be >= 2. A value of 1 causes entity
creation timeouts because slots cannot be recycled between separate
`run_session_until_all_status()` calls.

### smoltcp (XRCE_SMOLTCP_*)

| Variable | Default | Description |
|----------|---------|-------------|
| `XRCE_UDP_META_COUNT` | 4 | Packet metadata slots per direction (RX/TX) |

## Core (NROS_*)

| Variable | Default | Description |
|----------|---------|-------------|
| `NROS_MAX_PARAMETERS` | 32 | Max parameters in parameter server |
| `NROS_MAX_PARAM_NAME_LEN` | 64 | Max parameter name length (bytes) |
| `NROS_MAX_STRING_VALUE_LEN` | 256 | Max string parameter value length (bytes) |
| `NROS_MAX_ARRAY_LEN` | 32 | Max parameter array length |
| `NROS_MAX_BYTE_ARRAY_LEN` | 256 | Max byte array parameter length |
| `NROS_EXECUTOR_MAX_HANDLES` | 16 | Max handles in C API executor |
| `NROS_MAX_SUBSCRIPTIONS` | 8 | Max subscriptions in C API executor |
| `NROS_MAX_TIMERS` | 8 | Max timers in C API executor |
| `NROS_MAX_SERVICES` | 4 | Max services in C API executor |
| `NROS_LET_BUFFER_SIZE` | 512 | LET semantics buffer per handle (bytes) |
| `NROS_MESSAGE_BUFFER_SIZE` | 4096 | Max subscription/service data buffer (bytes) |
| `NROS_MAX_CONCURRENT_GOALS` | 4 | Max concurrent goals per action server |

## Memory Budget Estimation

### Per-Entity Overhead (zpico)

Use this formula to estimate total static memory for the zenoh transport layer:

```
Total = Session baseline
      + (MAX_PUBLISHERS * ~64 bytes)
      + (MAX_SUBSCRIBERS * (64 + SUBSCRIBER_BUFFER_SIZE) bytes)
      + (MAX_QUERYABLES * (64 + SERVICE_BUFFER_SIZE) bytes)
      + (MAX_LIVELINESS * ~32 bytes)
      + (MAX_PENDING_GETS * GET_REPLY_BUF_SIZE bytes)
      + (SMOLTCP_MAX_SOCKETS * 2 * SMOLTCP_BUFFER_SIZE bytes)  [bare-metal only]
```

Session baseline (no entities) is approximately 2-4 KB depending on protocol
features enabled.

### Per-Entity Overhead (XRCE)

```
Total = Session blob (512 bytes)
      + Transport blob (MTU + 256 bytes)
      + Stream buffers (4 * MTU * STREAM_HISTORY bytes)
      + (MAX_SUBSCRIBERS * BUFFER_SIZE bytes)
      + (MAX_SERVICE_SERVERS * BUFFER_SIZE bytes)
      + (MAX_SERVICE_CLIENTS * BUFFER_SIZE bytes)
```

## Recommended Configurations

### Minimal (Cortex-M4, 256 KB RAM)

Suitable for simple pub/sub with 1-4 topics. Total transport overhead: ~20 KB.

```bash
# Entity limits
ZPICO_MAX_PUBLISHERS=4
ZPICO_MAX_SUBSCRIBERS=4
ZPICO_MAX_QUERYABLES=2
ZPICO_MAX_LIVELINESS=8
ZPICO_MAX_PENDING_GETS=2

# Small buffers for constrained RAM
ZPICO_FRAG_MAX_SIZE=1400
ZPICO_BATCH_UNICAST_SIZE=1024
ZPICO_BATCH_MULTICAST_SIZE=512
ZPICO_SUBSCRIBER_BUFFER_SIZE=512
ZPICO_SERVICE_BUFFER_SIZE=512
ZPICO_GET_REPLY_BUF_SIZE=1024

# smoltcp (bare-metal)
ZPICO_SMOLTCP_MAX_SOCKETS=2
ZPICO_SMOLTCP_BUFFER_SIZE=1024
```

### Standard (Cortex-M7, 1 MB RAM)

Suitable for moderate applications with parameter services. Total transport
overhead: ~60 KB.

```bash
# Entity limits (defaults are fine)
ZPICO_MAX_PUBLISHERS=8
ZPICO_MAX_SUBSCRIBERS=8
ZPICO_MAX_QUERYABLES=8
ZPICO_MAX_LIVELINESS=16

# Moderate buffers
ZPICO_FRAG_MAX_SIZE=4096
ZPICO_BATCH_UNICAST_SIZE=2048
ZPICO_SUBSCRIBER_BUFFER_SIZE=1024
ZPICO_SERVICE_BUFFER_SIZE=1024
ZPICO_GET_REPLY_BUF_SIZE=4096

# smoltcp
ZPICO_SMOLTCP_MAX_SOCKETS=4
ZPICO_SMOLTCP_BUFFER_SIZE=2048
```

### Large (Cortex-R52, 4+ MB RAM)

Suitable for complex applications like Autoware safety modules with many topics.
Total transport overhead: ~200 KB.

```bash
# High entity counts
ZPICO_MAX_PUBLISHERS=48
ZPICO_MAX_SUBSCRIBERS=48
ZPICO_MAX_QUERYABLES=16
ZPICO_MAX_LIVELINESS=64
ZPICO_MAX_PENDING_GETS=8

# Large buffers for complex messages
ZPICO_FRAG_MAX_SIZE=16384
ZPICO_BATCH_UNICAST_SIZE=8192
ZPICO_SUBSCRIBER_BUFFER_SIZE=4096
ZPICO_SERVICE_BUFFER_SIZE=4096
ZPICO_GET_REPLY_BUF_SIZE=8192

# smoltcp
ZPICO_SMOLTCP_MAX_SOCKETS=4
ZPICO_SMOLTCP_BUFFER_SIZE=4096
```

## Comparison with CycloneDDS

ARM's [actuation_porting](https://github.com/oguzkaganozt/actuation_porting) project
uses CycloneDDS on Zephyr with these transport settings:

| CycloneDDS Setting | Value | zpico Equivalent | Notes |
|--------------------|-------|------------------|-------|
| `ReceiveBufferSize` | 16 KB | N/A | zpico uses per-entity buffers instead of a shared pool |
| `ReceiveBufferChunkSize` | 2 KB | `ZPICO_SUBSCRIBER_BUFFER_SIZE` | Per-entity in zpico vs shared chunks in CycloneDDS |
| `MaxMessageSize` | 1400 B | `ZPICO_BATCH_UNICAST_SIZE` | MTU-aware limit to avoid IP fragmentation |
| `AllowMulticast` | SPDP only | Cargo feature `link-udp-multicast` | Disabled by default in both |
| C library heap | 1 MB | 0 | zpico uses only static allocation |

**Key difference:** CycloneDDS requires a 1 MB heap (`CONFIG_NEWLIB_LIBC_MIN_REQUIRED_HEAP_SIZE`)
for its C++ runtime and internal dynamic allocation. nano-ros with zpico uses
zero heap -- all buffers are statically allocated at compile time. This makes
memory usage fully deterministic and eliminates heap fragmentation issues in
long-running embedded systems.

CycloneDDS Zephyr network stack settings for reference:

| Zephyr Config | Value | Purpose |
|---------------|-------|---------|
| `CONFIG_NET_PKT_RX_COUNT` | 32 | Receive packet buffers |
| `CONFIG_NET_PKT_TX_COUNT` | 32 | Transmit packet buffers |
| `CONFIG_NET_BUF_RX_COUNT` | 256 | RX data buffers |
| `CONFIG_NET_BUF_TX_COUNT` | 256 | TX data buffers |
| `CONFIG_NET_BUF_DATA_SIZE` | 1600 B | Individual buffer size |
| `CONFIG_NET_MAX_CONN` | 200 | Max concurrent connections |

These large buffer pools are needed because CycloneDDS uses the Zephyr BSD socket
API with dynamic allocation. zpico with smoltcp bypasses Zephyr's network stack
entirely, using direct Ethernet frames with statically allocated socket buffers.

## Zephyr Integration

When building as a Zephyr module, use Kconfig options instead of environment
variables. The CMake integration automatically maps Kconfig to the corresponding
`ZPICO_*` / `NROS_*` env vars.

```kconfig
# prj.conf
CONFIG_NROS=y
CONFIG_NROS_ZENOH=y

# Entity limits
CONFIG_NROS_MAX_PUBLISHERS=8
CONFIG_NROS_MAX_SUBSCRIBERS=8
CONFIG_NROS_MAX_QUERYABLES=4
CONFIG_NROS_MAX_LIVELINESS=16

# Buffer tuning
CONFIG_NROS_FRAG_MAX_SIZE=4096
CONFIG_NROS_BATCH_UNICAST_SIZE=2048
CONFIG_NROS_SUBSCRIBER_BUFFER_SIZE=1024
CONFIG_NROS_SERVICE_BUFFER_SIZE=1024

# Link features
CONFIG_NROS_ZENOH_LINK_TCP=y
CONFIG_NROS_ZENOH_LINK_UDP_UNICAST=n
CONFIG_NROS_ZENOH_LINK_UDP_MULTICAST=n
```

See `zephyr/Kconfig` for the full list of available options.

## Troubleshooting

### Messages silently dropped

Increase `ZPICO_FRAG_MAX_SIZE` -- the message exceeds the reassembly limit.

### Subscription receives truncated data

Increase `ZPICO_SUBSCRIBER_BUFFER_SIZE` -- the per-entity buffer is too small for
the serialized message.

### Service calls time out

- Increase `ZPICO_GET_REPLY_BUF_SIZE` if reply messages are large.
- Decrease `ZPICO_GET_POLL_INTERVAL_MS` for lower latency (at higher CPU cost).
- Increase `ZPICO_MAX_PENDING_GETS` if multiple concurrent service calls are needed.

### Entity creation fails at runtime

Increase the corresponding `ZPICO_MAX_*` limit -- all slots are occupied.

### smoltcp connection timeouts

- Increase `ZPICO_SMOLTCP_CONNECT_TIMEOUT_MS` on slow networks.
- Verify the zenohd router is reachable at the configured `ZENOH_LOCATOR`.

### XRCE entity creation timeouts

- Ensure `XRCE_STREAM_HISTORY >= 2`.
- Increase `XRCE_ENTITY_CREATION_TIMEOUT_MS`.
- Increase `XRCE_MAX_SESSION_CONNECTION_ATTEMPTS` if the agent is slow to respond.
