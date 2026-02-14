# XRCE-DDS Analysis for nros RMW Integration

## Overview

DDS-XRCE (DDS for eXtremely Resource Constrained Environments) is the middleware used by micro-ROS. The client library (Micro-XRCE-DDS-Client) runs on MCUs and communicates with an Agent process on a gateway host. This document analyzes XRCE-DDS as a potential second RMW backend for nros, following the architecture in `docs/design/rmw-layer-design.md`.

Source code studied: [Micro-XRCE-DDS-Client](https://github.com/eProsima/Micro-XRCE-DDS-Client) (cloned to `external/Micro-XRCE-DDS-Client/`) and [Micro-CDR](https://github.com/eProsima/Micro-CDR) (cloned to `external/Micro-CDR/`).

## Architecture: Agent-Based Model

```
MCU (XRCE Client)                     Gateway (XRCE Agent)
┌──────────────────┐                   ┌──────────────────────┐
│  nros-mps2-an385       │   XRCE Protocol   │  Micro-XRCE-DDS     │
│  nros-rmw-xrce   │◄─── UDP/Serial ──►│  Agent               │
│  xrce-sys        │                   │  ┌──────────────┐    │
│                  │                   │  │ DDS           │    │
│  3KB RAM         │                   │  │ Participant   │───►│ Full DDS Network
│  75KB Flash      │                   │  └──────────────┘    │
└──────────────────┘                   └──────────────────────┘
```

The agent is **mandatory** -- it creates DDS entities on behalf of the client and bridges XRCE protocol to the full DDS data space. This is fundamentally different from zenoh-pico, where the MCU participates directly in the zenoh network.

| Aspect | zenoh-pico (current) | XRCE-DDS |
|--------|---------------------|----------|
| Bridge process | zenohd (generic router) | Agent (protocol translator) |
| Entity creation | Client creates directly | Client requests, agent creates |
| Discovery | Client participates (liveliness) | Agent handles on behalf |
| Peer-to-peer | Supported (no router needed) | Not possible (agent required) |
| Client RAM | ~16KB+ (heap needed) | ~3KB (fully static) |
| Client Flash | ~100KB+ | ~75KB |
| Failure mode | Router crash = lose routing | Agent crash = lose all connectivity |

## Dependencies

### Micro-CDR v2.0.2

The only dependency. A lightweight CDR serialization library (5 source files, ~300 lines total):

```
Micro-CDR/src/c/
├── common.c              # Buffer init, alignment, remaining
└── types/
    ├── basic.c           # Primitive ser/de (u8, u16, u32, u64, i*, float, double)
    ├── string.c          # String ser/de
    ├── array.c           # Array ser/de
    └── sequence.c        # Sequence ser/de (length-prefixed arrays)
```

Key type:
```c
typedef struct ucdrBuffer {
    uint8_t* init;
    uint8_t* final;
    uint8_t* iterator;
    size_t origin;
    size_t offset;
    ucdrEndianness endianness;
    uint8_t last_data_size;
    bool error;
    OnFullBuffer on_full_buffer;
    void* args;
} ucdrBuffer;
```

Configuration (`config.h.in`) has one setting: `UCDR_MACHINE_ENDIANNESS` (little-endian for ARM/RISC-V).

**Zero heap allocation.** Only uses `memcpy` and `memset` from libc.

### libc Requirements (verified from source)

For a bare-metal build with custom transport only:

| Symbol | Used by | Notes |
|--------|---------|-------|
| `memcpy` | Micro-CDR, XRCE serialization | Bulk data copy |
| `memset` | Session init, stream init | Zero initialization |
| `strlen` | Entity creation (bin/xml/ref) | Measuring name/type strings |
| `strcmp` | Only in shared_memory profile | **Not needed** (profile disabled) |

No `malloc`, `free`, `calloc`, `realloc`, `printf`, `snprintf`, `strtoul`, `strtol`, or `sscanf`.

This is dramatically simpler than zenoh-pico which requires 14+ libc stubs including `snprintf`, `strtoul`, and heap allocation.

## XRCE-DDS Client API

### Session Management

```c
uxrSession session;
uxr_init_session(&session, &comm, 0xAABBCCDD);  // key identifies client
uxr_create_session(&session);                     // connect to agent
uxr_delete_session(&session);                     // disconnect
```

Session run loop variants:
```c
uxr_run_session_time(&session, 1000);             // process for 1000ms
uxr_run_session_until_timeout(&session, 1000);    // block until timeout
uxr_run_session_until_data(&session, 1000);       // block until data or timeout
uxr_run_session_until_confirm_delivery(&session, 1000);  // block until ACK
uxr_run_session_until_all_status(&session, 1000,  // block until all entities confirmed
    request_list, status_list, list_size);
```

### Streams (data channels)

User provides all buffers -- no heap allocation:

```c
// Output (client → agent): user allocates buffer
uint8_t output_buf[2048];
uxrStreamId reliable_out = uxr_create_output_reliable_stream(
    &session, output_buf, sizeof(output_buf), 16);  // history must be power of 2

// Input (agent → client): user allocates buffer for reliable
uint8_t input_buf[2048];
uxrStreamId reliable_in = uxr_create_input_reliable_stream(
    &session, input_buf, sizeof(input_buf), 16);

// Best-effort input uses internal transport buffer (no user allocation)
uxrStreamId best_effort_in = uxr_create_input_best_effort_stream(&session);
```

### Entity Creation (DDS hierarchy)

Three creation modes: XML (flexible, high memory), binary (moderate), reference (lowest memory, preconfigured on agent).

**Binary mode** (recommended for embedded):

```c
// QoS configuration
uxrQoS_t qos = {
    .durability = UXR_DURABILITY_VOLATILE,
    .reliability = UXR_RELIABILITY_RELIABLE,
    .history = UXR_HISTORY_KEEP_LAST,
    .depth = 1,
};

// DDS entity hierarchy: Participant > Topic + Publisher > DataWriter
uxr_buffer_create_participant_bin(&session, reliable_out, participant_id,
    0, "my_node", UXR_REPLACE);
uxr_buffer_create_topic_bin(&session, reliable_out, topic_id, participant_id,
    "rt/chatter", "std_msgs::msg::dds_::Int32_", UXR_REPLACE);
uxr_buffer_create_publisher_bin(&session, reliable_out, publisher_id,
    participant_id, "", UXR_REPLACE);
uxr_buffer_create_datawriter_bin(&session, reliable_out, datawriter_id,
    publisher_id, topic_id, qos, UXR_REPLACE);

// Wait for agent confirmation
uint8_t status[4];
uint16_t requests[4] = { /* request IDs from above */ };
uxr_run_session_until_all_status(&session, 5000, requests, status, 4);
```

Object IDs use type + index encoding:
```c
uxrObjectId participant_id = uxr_object_id(0x01, UXR_PARTICIPANT_ID);  // type=0x01
uxrObjectId topic_id       = uxr_object_id(0x01, UXR_TOPIC_ID);       // type=0x02
uxrObjectId publisher_id   = uxr_object_id(0x01, UXR_PUBLISHER_ID);   // type=0x03
uxrObjectId datawriter_id  = uxr_object_id(0x01, UXR_DATAWRITER_ID);  // type=0x05
```

### Data Operations

```c
// Publish: two approaches

// 1. Pre-serialized (for nros: CDR bytes already serialized by nros-serdes)
uint16_t req = uxr_buffer_topic(&session, reliable_out, datawriter_id,
    cdr_bytes, cdr_len);

// 2. Inline serialization via ucdrBuffer (XRCE-DDS uses Micro-CDR internally)
ucdrBuffer ub;
uxr_prepare_output_stream(&session, reliable_out, datawriter_id, &ub,
    sizeof(int32_t));
ucdr_serialize_int32_t(&ub, 42);

// Subscribe: request data from agent, receive via callback
uxr_buffer_request_data(&session, reliable_out, datareader_id,
    reliable_in, NULL);  // NULL = unlimited delivery
uxr_set_topic_callback(&session, on_topic, NULL);

// Event loop -- data arrives in callback during this call
uxr_run_session_time(&session, 1000);
```

**Note on `uxr_buffer_topic`**: This is the preferred publish path for nros because nros-serdes already serializes to CDR bytes. We pass raw bytes directly instead of using `uxr_prepare_output_stream` + Micro-CDR serialization (which would double-serialize).

### Callback Signatures

```c
// Topic data received (subscription)
typedef void (*uxrOnTopicFunc)(
    uxrSession* session,
    uxrObjectId object_id,
    uint16_t request_id,
    uxrStreamId stream_id,
    ucdrBuffer* ub,           // Micro-CDR buffer pointing to received data
    uint16_t length,
    void* args);

// Entity operation status
typedef void (*uxrOnStatusFunc)(
    uxrSession* session,
    uxrObjectId object_id,
    uint16_t request_id,
    uint8_t status,           // UXR_STATUS_OK, UXR_STATUS_ERR_*, etc.
    void* args);

// Service request received (replier)
typedef void (*uxrOnRequestFunc)(
    uxrSession* session,
    uxrObjectId object_id,
    uint16_t request_id,
    SampleIdentity* sample_id,  // Correlation ID for reply
    ucdrBuffer* ub,
    uint16_t length,
    void* args);

// Service reply received (requester)
typedef void (*uxrOnReplyFunc)(
    uxrSession* session,
    uxrObjectId object_id,
    uint16_t request_id,
    uint16_t reply_id,
    ucdrBuffer* ub,
    uint16_t length,
    void* args);
```

### Services

```c
// Create requester (service client)
uxr_buffer_create_requester_bin(&session, reliable_out, requester_id,
    participant_id,
    "add_two_ints",                                    // service name
    "example_interfaces::srv::dds_::AddTwoInts_Request_",
    "example_interfaces::srv::dds_::AddTwoInts_Response_",
    "rq/add_two_intsRequest", "rr/add_two_intsReply",
    qos, UXR_REPLACE);

// Send request / reply
uxr_buffer_request(&session, reliable_out, requester_id, buf, len);
uxr_buffer_reply(&session, reliable_out, replier_id, sample_id, buf, len);
```

### Status Codes

```c
#define UXR_STATUS_OK                  0x00
#define UXR_STATUS_OK_MATCHED          0x01
#define UXR_STATUS_ERR_DDS_ERROR       0x80
#define UXR_STATUS_ERR_MISMATCH        0x81
#define UXR_STATUS_ERR_ALREADY_EXISTS  0x82
#define UXR_STATUS_ERR_DENIED          0x83
#define UXR_STATUS_ERR_UNKNOWN_REFERENCE 0x84
#define UXR_STATUS_ERR_INVALID_DATA    0x85
#define UXR_STATUS_ERR_INCOMPATIBLE    0x86
#define UXR_STATUS_ERR_RESOURCES       0x87

// Creation modes
#define UXR_REUSE    (0x01 << 1)   // Reuse existing entity if compatible
#define UXR_REPLACE  (0x01 << 2)   // Delete and recreate
```

## Transport Layer

### Custom Transport (bare-metal path)

The custom transport interface provides 4 callbacks:

```c
typedef bool (*open_custom_func)(uxrCustomTransport* transport);
typedef bool (*close_custom_func)(uxrCustomTransport* transport);
typedef size_t (*write_custom_func)(
    uxrCustomTransport* transport,
    const uint8_t* buffer, size_t length, uint8_t* error_code);
typedef size_t (*read_custom_func)(
    uxrCustomTransport* transport,
    uint8_t* buffer, size_t length, int timeout, uint8_t* error_code);
```

The `uxrCustomTransport` struct contains:
- `buffer[UXR_CONFIG_CUSTOM_TRANSPORT_MTU]` — internal receive buffer (default 512 bytes)
- `framing` — enables HDLC frame encapsulation (for serial/stream transports)
- `comm` — `uxrCommunication` struct with function pointers (populated by `uxr_init_custom_transport`)
- `args` — user-provided context pointer

Usage:
```c
uxrCustomTransport transport;
uxr_set_custom_transport_callbacks(&transport,
    false,          // no framing (UDP is datagram-based)
    my_open, my_close, my_write, my_read);
uxr_init_custom_transport(&transport, user_args);
// transport.comm is now ready to pass to uxr_init_session
```

### Internal Communication Abstraction

All transports register into a common `uxrCommunication` struct:

```c
typedef struct uxrCommunication {
    void* instance;                     // transport-specific state
    send_msg_func send_msg;             // internal send callback
    recv_msg_func recv_msg;             // internal recv callback
    comm_error_func comm_error;         // error retrieval
    uint16_t mtu;                       // max transmission unit
} uxrCommunication;
```

This means the session layer is transport-agnostic — it only sees `uxrCommunication`.

### Built-in Transport Options

XRCE-DDS also supports: UDP, TCP (with HDLC framing), serial (HDLC), CAN FD. Each is a CMake-conditional profile. For nros, only custom transport is needed (smoltcp provides the UDP implementation).

**Serial transport** with HDLC framing is valuable for MCUs without networking — a future addition path for nros.

## Build System Analysis

### Source File Inventory

For a **bare-metal custom-transport-only build** (what xrce-sys needs):

**Core session & streams (17 files):**
```
src/c/core/session/session.c
src/c/core/session/session_info.c
src/c/core/session/submessage.c
src/c/core/session/object_id.c
src/c/core/session/read_access.c
src/c/core/session/write_access.c
src/c/core/session/common_create_entities.c
src/c/core/session/create_entities_xml.c
src/c/core/session/create_entities_bin.c
src/c/core/session/create_entities_ref.c
src/c/core/session/stream/stream_storage.c
src/c/core/session/stream/stream_id.c
src/c/core/session/stream/seq_num.c
src/c/core/session/stream/input_best_effort_stream.c
src/c/core/session/stream/input_reliable_stream.c
src/c/core/session/stream/output_best_effort_stream.c
src/c/core/session/stream/output_reliable_stream.c
```

**Serialization (3 files):**
```
src/c/core/serialization/xrce_types.c
src/c/core/serialization/xrce_header.c
src/c/core/serialization/xrce_subheader.c
```

**Utilities (2 files):**
```
src/c/util/time.c        # uxr_millis(), uxr_nanos() — platform-dependent
src/c/util/ping.c        # Heartbeat/ping
```

**Custom transport (1 file):**
```
src/c/profile/transport/custom/custom_transport.c
```

**Micro-CDR (5 files):**
```
Micro-CDR/src/c/common.c
Micro-CDR/src/c/types/basic.c
Micro-CDR/src/c/types/string.c
Micro-CDR/src/c/types/array.c
Micro-CDR/src/c/types/sequence.c
```

**Total: 28 source files** (vs ~100+ for zenoh-pico).

### Configuration (config.h)

Generated from `config.h.in`. For bare-metal custom transport, the minimal config is:

```c
// Profiles
#define UCLIENT_PROFILE_CUSTOM_TRANSPORT    // Only custom transport
// NOT defined: UCLIENT_PROFILE_UDP, _TCP, _SERIAL, _DISCOVERY,
//              _MULTITHREAD, _SHARED_MEMORY, _STREAM_FRAMING, _CAN

// Platform — none defined for bare-metal custom transport
// NOT defined: UCLIENT_PLATFORM_POSIX, _WINDOWS, _ZEPHYR, etc.

// Stream limits
#define UXR_CONFIG_MAX_OUTPUT_BEST_EFFORT_STREAMS   1
#define UXR_CONFIG_MAX_OUTPUT_RELIABLE_STREAMS       1
#define UXR_CONFIG_MAX_INPUT_BEST_EFFORT_STREAMS     1
#define UXR_CONFIG_MAX_INPUT_RELIABLE_STREAMS        1

// Timing
#define UXR_CONFIG_MAX_SESSION_CONNECTION_ATTEMPTS    10
#define UXR_CONFIG_MIN_SESSION_CONNECTION_INTERVAL    1000  // ms
#define UXR_CONFIG_MIN_HEARTBEAT_TIME_INTERVAL        100   // ms

// Transport
#define UXR_CONFIG_CUSTOM_TRANSPORT_MTU               512   // bytes

// Tweaks
#define UCLIENT_TWEAK_XRCE_WRITE_LIMIT               // Allow >64KB WRITE_DATA
```

### Build Strategy: cc::Build (not CMake)

Following the zpico-sys pattern, `xrce-sys/build.rs` will:
1. **Generate `config.h`** in `OUT_DIR` from Cargo feature flags
2. **Generate Micro-CDR `config.h`** with endianness detection
3. **Compile 28 .c files** via `cc::Build` with appropriate include paths
4. **Set target-specific flags** (ARM: `-mcpu=cortex-m3 -mthumb`, RISC-V: `-march=rv32imc`)

This avoids CMake as a build dependency and integrates cleanly with `cargo build`.

### Platform-Specific Code: Only `time.c`

The file `src/c/util/time.c` contains `uxr_nanos()` with platform-specific implementations:

```c
int64_t uxr_nanos(void) {
#ifdef WIN32
    // Windows: FILETIME
#elif defined(UCLIENT_PLATFORM_FREERTOS_PLUS_TCP)
    // FreeRTOS: vTaskSetTimeOutState
#elif defined(UCLIENT_PLATFORM_ZEPHYR)
    // Zephyr: clock_gettime(CLOCK_REALTIME, ...)
#else
    // POSIX: clock_gettime(CLOCK_REALTIME, ...)
#endif
}
```

For bare-metal with no platform defined, this falls into the POSIX branch which calls `clock_gettime()`. The `xrce-platform-qemu` crate provides this symbol using the DWT cycle counter — **the only platform symbol needed**.

This is dramatically simpler than zpico-platform-mps2-an385's 55 FFI symbols (z_malloc, z_clock_now, z_random_*, z_sleep_*, socket stubs, libc stubs, etc.).

## Platform Requirements Comparison

| Requirement | XRCE-DDS Client | zenoh-pico |
|-------------|-----------------|------------|
| Heap allocation | **None** (fully static) | Required (`z_malloc`/`z_free`) |
| Random numbers | Not needed | Required (`z_random_*`, 5 functions) |
| Threading | Optional (`UCLIENT_PROFILE_MULTITHREAD`) | Optional (`Z_FEATURE_MULTI_THREAD`) |
| Mutex/condvar | Only with multithread | 23 symbols (`_z_mutex_*`, `_z_condvar_*`, `_z_task_*`) |
| Clock | `clock_gettime` for `uxr_nanos()` | `z_clock_now` + 6 elapsed/advance functions |
| Sleep | Not needed (event-driven) | `z_sleep_us/ms/s` |
| Network I/O | 4 custom transport callbacks | Full socket API per platform |
| libc stubs | `memcpy`, `memset`, `strlen` only | 14 functions (strlen, memcpy, strtoul, snprintf, ...) |
| **Total platform symbols** | **1** (`clock_gettime`) | **~55** |

## Mapping to nros-rmw Traits

| nros-rmw | XRCE-DDS |
|----------|----------|
| `Rmw::open(config)` | `uxr_set_custom_transport_callbacks` + `uxr_init_custom_transport` + `uxr_init_session` + `uxr_create_session` + create streams |
| `Session::create_publisher(topic, qos)` | `uxr_buffer_create_participant_bin` (once) + `create_topic_bin` + `create_publisher_bin` + `create_datawriter_bin` + `uxr_run_session_until_all_status` |
| `Publisher::publish_raw(data)` | `uxr_buffer_topic(session, stream, datawriter_id, data, len)` |
| `Session::create_subscriber(topic, qos)` | `create_subscriber_bin` + `create_datareader_bin` + `uxr_buffer_request_data` + `uxr_set_topic_callback` |
| `Subscriber::try_recv_raw(buf)` | Read from callback-populated static buffer |
| `Session::spin_once(timeout)` | `uxr_run_session_time(timeout)` |
| `ServiceServer::try_recv_request` | Read from `uxrOnRequestFunc` callback buffer |
| `ServiceServer::send_reply` | `uxr_buffer_reply(session, stream, replier_id, sample_id, data, len)` |
| `ServiceClient::call_raw` | `uxr_buffer_request` + `uxr_run_session_until_data` for reply |

### Key differences in the mapping

1. **Entity creation is multi-step.** zenoh: one call (`declare_publisher(keyexpr)`). XRCE-DDS: 4 calls (participant + topic + publisher + datawriter), then wait for agent confirmation. The `Session::create_publisher` implementation orchestrates this internally.

2. **Participant is shared.** The DDS participant is created once per session, not per publisher/subscriber. `XrceSession` creates it during `Rmw::open()` and reuses the same participant ID for all entities.

3. **Subscribing requires explicit data request.** After creating a datareader, `uxr_buffer_request_data()` must be called to tell the agent to start sending data. For continuous subscription, use `UXR_MAX_SAMPLES_UNLIMITED`.

4. **Publishing uses pre-serialized path.** nros already serializes to CDR via nros-serdes. Use `uxr_buffer_topic()` to pass raw bytes, avoiding double-serialization through Micro-CDR.

5. **Topic naming uses DDS conventions.** zenoh uses keyexprs (`0/chatter/std_msgs::msg::dds_::Int32_/TypeHashNotSupported`). XRCE-DDS uses standard DDS topic names (`rt/chatter`) + type names as separate parameters. `nros-rmw-xrce` formats `TopicInfo` into DDS-style names.

6. **Config model differs.** zenoh: locator string (`tcp/192.168.1.1:7447`). XRCE-DDS: agent IP + port parsed from `RmwConfig.locator` (e.g., `udp/192.168.1.1:2019`). The transport callbacks use IP/port to send UDP datagrams.

## Feasibility Assessment

### What works well

- **Trait signatures are compatible.** The core `Session`, `Publisher`, `Subscriber`, `ServiceServer`, `ServiceClient` trait methods map cleanly to XRCE-DDS operations.
- **Raw bytes interface.** Both zenoh-pico and XRCE-DDS ultimately send/receive CDR-encoded byte buffers. The `publish_raw`/`try_recv_raw` abstraction is correct. The `uxr_buffer_topic()` function accepts pre-serialized data directly.
- **Static memory model.** XRCE-DDS's fully static allocation is even more embedded-friendly than zenoh-pico's heap-based approach. No `z_malloc` needed.
- **Minimal platform porting.** Only 1 platform symbol (`clock_gettime` / `uxr_nanos`) vs 55 for zenoh-pico. New board support is trivial.
- **Serial transport.** Opens up MCUs without Ethernet/WiFi -- a major gap in nros today. HDLC framing is built into the library.
- **No C shim needed.** Unlike zenoh-pico (which requires `zenoh_shim.c` wrapper), XRCE-DDS's C API is clean enough to bind directly from Rust FFI.

### Challenges

- **Agent deployment.** Users must run a separate agent process. This adds operational complexity vs zenoh-pico's direct peer model.
- **DDS entity hierarchy.** The participant > publisher > datawriter hierarchy adds internal complexity to `nros-rmw-xrce`, but this is hidden from users behind `Session::create_publisher()`.
- **Discovery.** XRCE-DDS doesn't do client-side discovery -- the agent handles it. If nros wants to expose `ros2 topic list` visibility, the agent must be configured to advertise.
- **QoS subset.** XRCE-DDS supports a limited QoS subset (reliability, durability, history depth). This aligns with nros's minimal `QosSettings`.
- **Entity ID management.** Object IDs are `(u16 id, u8 type)` tuples. `XrceSession` must track allocated IDs to avoid collisions when creating multiple publishers/subscribers.

## Comparison: zpico-sys vs xrce-sys Build Complexity

| Aspect | zpico-sys | xrce-sys |
|--------|-----------|----------|
| C shim layer | `zenoh_shim.c` (~1200 lines) | **None** — direct FFI binding |
| Submodules | 1 (zenoh-pico) | 2 (XRCE-DDS Client + Micro-CDR) |
| Source files to compile | ~100+ | **28** |
| Platform symbols to provide | 55 | **1** (`clock_gettime`) |
| Transport bridge crate | TCP (8+ callbacks) | UDP (4 callbacks) |
| Heap allocation | Required | **None** |
| Config #defines | 20+ | ~8 |
| libc stubs needed | 14 | 3 (`memcpy`, `memset`, `strlen`) |

## References

- [Micro-XRCE-DDS Client API](https://micro-xrce-dds.docs.eprosima.com/en/latest/client_api.html)
- [Micro-XRCE-DDS Transport](https://micro-xrce-dds.docs.eprosima.com/en/latest/transport.html)
- [rmw_microxrcedds](https://github.com/micro-ROS/rmw_microxrcedds) -- ROS 2 RMW implementation for XRCE-DDS
- [Micro-XRCE-DDS-Client](https://github.com/eProsima/Micro-XRCE-DDS-Client) -- Client library source
- [Micro-CDR](https://github.com/eProsima/Micro-CDR) -- CDR serialization library
- [micro-ROS middleware configuration](https://micro.ros.org/docs/tutorials/advanced/microxrcedds_rmw_configuration/)
