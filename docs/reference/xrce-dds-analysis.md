# XRCE-DDS Analysis for nros RMW Integration

## Overview

DDS-XRCE (DDS for eXtremely Resource Constrained Environments) is the middleware used by micro-ROS. The client library (Micro-XRCE-DDS-Client) runs on MCUs and communicates with an Agent process on a gateway host. This document analyzes XRCE-DDS as a potential second RMW backend for nros, following the architecture in `docs/design/rmw-layer-design.md`.

## Architecture: Agent-Based Model

```
MCU (XRCE Client)                     Gateway (XRCE Agent)
┌──────────────────┐                   ┌──────────────────────┐
│  nros-qemu       │   XRCE Protocol   │  Micro-XRCE-DDS     │
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

## XRCE-DDS Client API

### Session Management

```c
uxrSession session;
uxr_init_session(&session, &comm, 0xAABBCCDD);  // key identifies client
uxr_create_session(&session);                     // connect to agent
uxr_delete_session(&session);                     // disconnect
```

### Streams (data channels)

```c
// User provides buffers -- no heap allocation
uint8_t output_buf[2048];
uint8_t input_buf[2048];
uxrStreamId reliable_out = uxr_create_output_reliable_stream(&session, output_buf, sizeof(output_buf), 16);
uxrStreamId reliable_in = uxr_create_input_reliable_stream(&session, input_buf, sizeof(input_buf), 16);
uxrStreamId best_effort_in = uxr_create_input_best_effort_stream(&session);
```

### Entity Creation (DDS hierarchy)

Three creation modes: XML (flexible, high memory), binary (moderate), reference (lowest memory, preconfigured on agent).

```c
// Binary mode (recommended for embedded)
uxr_buffer_create_participant_bin(&session, reliable_out, participant_id, 0, "my_node", UXR_REPLACE);
uxr_buffer_create_topic_bin(&session, reliable_out, topic_id, participant_id,
    "rt/chatter", "std_msgs::msg::dds_::Int32_", UXR_REPLACE);
uxr_buffer_create_publisher_bin(&session, reliable_out, publisher_id, participant_id, "", UXR_REPLACE);
uxr_buffer_create_datawriter_bin(&session, reliable_out, datawriter_id, publisher_id, topic_id, qos, UXR_REPLACE);
```

Note the DDS entity hierarchy: Participant > Topic + Publisher > DataWriter. This is more complex than zenoh-pico's flat `declare_publisher(keyexpr)`.

### Data Operations

```c
// Publish: serialize into stream buffer
ucdrBuffer ub;
uxr_prepare_output_stream(&session, reliable_out, datawriter_id, &ub, sizeof(int32_t));
ucdr_serialize_int32_t(&ub, 42);

// Subscribe: request data from agent, receive via callback
uxr_buffer_request_data(&session, reliable_out, datareader_id, reliable_in, NULL);
uxr_set_topic_callback(&session, on_topic, NULL);

// Event loop
uxr_run_session_time(&session, 1000);  // process for 1000ms
```

### Services

```c
// Create requester (client) or replier (server)
uxr_buffer_create_requester_bin(&session, reliable_out, requester_id, participant_id,
    "add_two_ints", "example_interfaces::srv::dds_::AddTwoInts_Request_",
    "example_interfaces::srv::dds_::AddTwoInts_Response_",
    "rq/add_two_intsRequest", "rr/add_two_intsReply", qos, UXR_REPLACE);

// Send request / reply
uxr_buffer_request(&session, reliable_out, requester_id, buf, len);
uxr_buffer_reply(&session, reliable_out, replier_id, buf, len);
```

## Platform Requirements Comparison

| Requirement | XRCE-DDS Client | zenoh-pico |
|-------------|-----------------|------------|
| Heap allocation | **None** (fully static) | Required (`z_malloc`/`z_free`) |
| Random numbers | Not needed | Required (`z_random_*`, 5 functions) |
| Threading | Optional (`UCLIENT_PROFILE_MULTITHREAD`) | Optional (`Z_FEATURE_MULTI_THREAD`) |
| Mutex/condvar | Only with multithread | 23 symbols (`_z_mutex_*`, `_z_condvar_*`, `_z_task_*`) |
| Clock | `clock_gettime` for sync | `z_clock_now` + 6 elapsed/advance functions |
| Sleep | Not needed (event-driven) | `z_sleep_us/ms/s` |
| Network I/O | 4 callbacks (open/close/read/write) | Full socket API per platform |
| libc stubs | Minimal | 14 functions (strlen, memcpy, strtoul, snprintf, ...) |
| **Total platform symbols** | **~6** | **~55** |

XRCE-DDS's zero-allocation model means `zpico-platform-*` crates (727 lines, 55 FFI symbols each) have no equivalent. An `xrce-platform-*` crate would be much smaller -- only transport I/O callbacks and a clock function.

## Transport Layer

XRCE-DDS supports: UDP, TCP (with HDLC framing), serial (HDLC), CAN FD, and custom transports.

**Custom transport interface** (4 callbacks):
```c
typedef bool (*open_func)(uxrCustomTransport* transport);
typedef bool (*close_func)(uxrCustomTransport* transport);
typedef size_t (*write_func)(uxrCustomTransport* transport, const uint8_t* buf, size_t len, uint8_t* err);
typedef size_t (*read_func)(uxrCustomTransport* transport, uint8_t* buf, size_t len, int timeout, uint8_t* err);
```

For bare-metal with smoltcp, an `xrce-smoltcp` crate would implement these 4 callbacks using smoltcp UDP sockets. This is simpler than `zpico-smoltcp` which implements 8+ TCP socket management functions.

**Serial transport** is valuable for MCUs without networking. zenoh-pico also supports serial but nros hasn't implemented it. XRCE-DDS's serial transport with HDLC framing is well-tested and widely used in micro-ROS deployments.

## Mapping to nros-rmw Traits

The proposed `nros-rmw` traits map to XRCE-DDS operations:

| nros-rmw | XRCE-DDS |
|----------|----------|
| `Rmw::open(config)` | `uxr_init_*_transport` + `uxr_create_session` + create streams |
| `Session::create_publisher(topic, qos)` | `uxr_buffer_create_participant_bin` + `create_topic_bin` + `create_publisher_bin` + `create_datawriter_bin` |
| `Publisher::publish_raw(data)` | `uxr_prepare_output_stream` + write bytes |
| `Session::create_subscriber(topic, qos)` | `create_datareader_bin` + `uxr_buffer_request_data` + set callback |
| `Subscriber::try_recv_raw(buf)` | Read from callback-populated buffer |
| `Session::spin_once(timeout)` | `uxr_run_session_time(timeout)` |
| `ServiceServer::try_recv_request` | Replier callback buffer |
| `ServiceServer::send_reply` | `uxr_buffer_reply` |
| `ServiceClient::call_raw` | `uxr_buffer_request` + wait for reply callback |

**Key differences in the mapping:**

1. **Entity creation is multi-step.** zenoh: one call (`declare_publisher(keyexpr)`). XRCE-DDS: 4 calls (participant + topic + publisher + datawriter). The `Session::create_publisher` implementation must orchestrate this internally.

2. **Subscribing requires explicit data request.** zenoh: declare subscriber with callback, data flows automatically. XRCE-DDS: create datareader, then call `uxr_buffer_request_data` to ask the agent to start sending. Must re-request after processing.

3. **Topic naming differs.** zenoh uses keyexprs (`0/chatter/std_msgs::msg::dds_::Int32_/TypeHashNotSupported`). XRCE-DDS uses standard DDS topic names (`rt/chatter`) + type names as separate parameters. The `TopicInfo` struct provides both pieces; the RMW backend formats them for its middleware.

4. **Config model differs.** zenoh: locator string (`tcp/192.168.1.1:7447`). XRCE-DDS: agent IP + port, or serial device + baudrate. `RmwConfig` must be generic enough for both.

## Feasibility Assessment

### What works well

- **Trait signatures are compatible.** The core `Session`, `Publisher`, `Subscriber`, `ServiceServer`, `ServiceClient` trait methods map cleanly to XRCE-DDS operations.
- **Raw bytes interface.** Both zenoh-pico and XRCE-DDS ultimately send/receive CDR-encoded byte buffers. The `publish_raw`/`try_recv_raw` abstraction is correct.
- **Static memory model.** XRCE-DDS's fully static allocation is even more embedded-friendly than zenoh-pico's heap-based approach. No `z_malloc` needed.
- **Serial transport.** Opens up MCUs without Ethernet/WiFi -- a major gap in nros today.

### Challenges

- **Agent deployment.** Users must run a separate agent process. This adds operational complexity vs zenoh-pico's direct peer model.
- **DDS entity hierarchy.** The participant > publisher > datawriter hierarchy adds internal complexity to `nros-rmw-xrce`, but this is hidden from users.
- **Discovery.** XRCE-DDS doesn't do client-side discovery -- the agent handles it. If nros wants to expose `ros2 topic list` visibility, the agent must be configured to advertise.
- **QoS subset.** XRCE-DDS supports a limited QoS subset (reliability, durability, history). This aligns with nros's minimal `QosSettings`, but may limit advanced use cases.
- **Board crate coupling.** Currently `nros-qemu` calls `zenoh_shim_*` FFI directly (bypassing `nros-rmw` traits). Before XRCE-DDS can work, board crates must be refactored to use abstract RMW traits (Phase 34 work).

### Estimated effort

| Component | Effort |
|-----------|--------|
| `xrce-sys` (FFI bindings to Micro-XRCE-DDS-Client) | 1 week |
| `nros-rmw-xrce` (RMW trait implementation) | 2 weeks |
| `xrce-smoltcp` (UDP transport via smoltcp) | 3 days |
| `xrce-platform-qemu` (clock + transport for bare-metal) | 2 days |
| Integration testing with micro-ROS Agent | 1 week |
| Serial transport support | 1 week |
| **Total** | ~5-6 weeks |

## References

- [Micro-XRCE-DDS Client API](https://micro-xrce-dds.docs.eprosima.com/en/latest/client_api.html)
- [Micro-XRCE-DDS Transport](https://micro-xrce-dds.docs.eprosima.com/en/latest/transport.html)
- [rmw_microxrcedds](https://github.com/micro-ROS/rmw_microxrcedds) -- ROS 2 RMW implementation for XRCE-DDS
- [Micro-XRCE-DDS-Client](https://github.com/eProsima/Micro-XRCE-DDS-Client) -- Client library source
- [micro-ROS middleware configuration](https://micro.ros.org/docs/tutorials/advanced/microxrcedds_rmw_configuration/)
