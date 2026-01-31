# Phase 11: C API (rclc-Compatible for Embedded Systems)

**Status: IN PROGRESS** (Core API complete, advanced features pending)

## Executive Summary

Phase 11 implements a C API for nano-ros, providing an rclc-compatible interface for embedded systems.
The API is built on top of the existing Rust implementation via FFI, using zenoh-pico as the transport
layer. This enables C developers to use nano-ros with familiar ROS 2 patterns on resource-constrained
microcontrollers.

**Goals:**
1. Provide an rclc-compatible C API for nano-ros
2. Support static memory allocation (no runtime malloc)
3. Implement a deterministic executor for real-time applications
4. Enable ROS 2 interoperability via rmw_zenoh protocol
5. Target Zephyr RTOS and bare-metal embedded systems

**Non-Goals:**
- Full rclc API compatibility (only core features)
- DDS/XRCE-DDS support (use zenoh-pico instead)
- Dynamic memory allocation at runtime

## Progress Summary

| Task                         | Status          | Description                                 |
|------------------------------|-----------------|---------------------------------------------|
| 11.1 Project Setup           | Complete        | Crate structure, CMake integration          |
| 11.2 Core Types              | Complete        | Support, Node, Error codes, QoS             |
| 11.3 Publisher               | Complete        | Raw publish, QoS support                    |
| 11.4 Subscription            | Complete        | Callback dispatch, executor polling         |
| 11.5 Executor                | Complete        | spin, spin_some, spin_period                |
| 11.6 Timer                   | Complete        | Periodic callbacks, cancel/reset            |
| 11.7 Services                | Complete        | Server and client (sync)                    |
| 11.8 Message Generation      | Complete        | C templates + CDR helpers                   |
| 11.9 Examples                | Complete        | Native C talker/listener                    |
| 11.10 Zephyr Integration     | Partial         | Uses zenoh_shim directly                    |
| 11.11 rclc Header Compat     | Complete        | Modular headers matching rclc structure     |
| 11.12 Clock API              | Complete        | ROS/steady/system time                      |
| 11.13 Parameters             | Complete        | Parameter server with callbacks             |
| 11.14 Actions                | Complete        | Action server/client with goal handling     |
| 11.15 Guard Conditions       | Complete        | Executor wake-up triggers, thread-safe      |
| 11.16 no_std Support         | Not Started     | Full embedded support                       |

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     User Application (C)                        │
├─────────────────────────────────────────────────────────────────┤
│                  nano_ros/ (Modular C Headers)                  │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐           │
│  │ node.h   │ │publisher.│ │subscript.│ │executor.h│           │
│  │ init.h   │ │   h      │ │   h      │ │ timer.h  │           │
│  ├──────────┤ ├──────────┤ ├──────────┤ ├──────────┤           │
│  │ clock.h  │ │parameter.│ │ action.h │ │service.h │           │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘           │
├─────────────────────────────────────────────────────────────────┤
│                    nano_ros_impl.rs (Rust FFI)                  │
│  - Thin wrapper exposing Rust types to C                        │
│  - Manually maintained headers (no cbindgen at build time)      │
├─────────────────────────────────────────────────────────────────┤
│                      nano-ros (Rust core)                       │
│  - nano-ros-node, nano-ros-transport, nano-ros-core             │
├─────────────────────────────────────────────────────────────────┤
│                      zenoh-pico-shim                            │
│  - Safe Rust wrapper for zenoh-pico                             │
├─────────────────────────────────────────────────────────────────┤
│                        zenoh-pico (C)                           │
│  - Lightweight zenoh client for embedded                        │
└─────────────────────────────────────────────────────────────────┘
```

## API Design

### Core Types

```c
// Support context (similar to rclc_support_t)
typedef struct nano_ros_support_t nano_ros_support_t;

// Node handle
typedef struct nano_ros_node_t nano_ros_node_t;

// Publisher/Subscriber handles
typedef struct nano_ros_publisher_t nano_ros_publisher_t;
typedef struct nano_ros_subscription_t nano_ros_subscription_t;

// Service handles
typedef struct nano_ros_client_t nano_ros_client_t;
typedef struct nano_ros_service_t nano_ros_service_t;

// Timer handle
typedef struct nano_ros_timer_t nano_ros_timer_t;

// Executor
typedef struct nano_ros_executor_t nano_ros_executor_t;

// Return codes (compatible with rcl_ret_t)
typedef int32_t nano_ros_ret_t;
#define NANO_ROS_RET_OK 0
#define NANO_ROS_RET_ERROR -1
#define NANO_ROS_RET_TIMEOUT -2
#define NANO_ROS_RET_INVALID_ARGUMENT -3
```

### Initialization API

```c
// Initialize support context
nano_ros_ret_t nano_ros_support_init(
    nano_ros_support_t *support,
    const char *locator,           // e.g., "tcp/192.168.1.1:7447"
    uint8_t domain_id);

// Finalize support context
nano_ros_ret_t nano_ros_support_fini(nano_ros_support_t *support);

// Create node
nano_ros_ret_t nano_ros_node_init(
    nano_ros_node_t *node,
    nano_ros_support_t *support,
    const char *name,
    const char *namespace_);

// Finalize node
nano_ros_ret_t nano_ros_node_fini(nano_ros_node_t *node);
```

### Publisher API

```c
// Message type info (generated per message type)
typedef struct {
    const char *type_name;      // e.g., "std_msgs::msg::dds_::Int32"
    const char *type_hash;      // RIHS hash
    size_t serialized_size_max; // Max serialized size (0 = dynamic)
} nano_ros_message_type_t;

// Create publisher
nano_ros_ret_t nano_ros_publisher_init(
    nano_ros_publisher_t *publisher,
    nano_ros_node_t *node,
    const nano_ros_message_type_t *type,
    const char *topic_name);

// Create publisher with QoS
nano_ros_ret_t nano_ros_publisher_init_with_qos(
    nano_ros_publisher_t *publisher,
    nano_ros_node_t *node,
    const nano_ros_message_type_t *type,
    const char *topic_name,
    const nano_ros_qos_t *qos);

// Publish raw CDR data
nano_ros_ret_t nano_ros_publish_raw(
    nano_ros_publisher_t *publisher,
    const uint8_t *data,
    size_t len);

// Finalize publisher
nano_ros_ret_t nano_ros_publisher_fini(nano_ros_publisher_t *publisher);
```

### Subscription API

```c
// Subscription callback type
typedef void (*nano_ros_subscription_callback_t)(
    const uint8_t *data,
    size_t len,
    void *context);

// Create subscription
nano_ros_ret_t nano_ros_subscription_init(
    nano_ros_subscription_t *subscription,
    nano_ros_node_t *node,
    const nano_ros_message_type_t *type,
    const char *topic_name,
    nano_ros_subscription_callback_t callback,
    void *context);

// Finalize subscription
nano_ros_ret_t nano_ros_subscription_fini(nano_ros_subscription_t *subscription);
```

### Service API

```c
// Service type info
typedef struct {
    const char *service_name;   // e.g., "example_interfaces::srv::dds_::AddTwoInts"
    const char *type_hash;
    size_t request_size_max;
    size_t response_size_max;
} nano_ros_service_type_t;

// Service callback type
typedef void (*nano_ros_service_callback_t)(
    const uint8_t *request,
    size_t request_len,
    uint8_t *response,
    size_t *response_len,
    void *context);

// Create service server
nano_ros_ret_t nano_ros_service_init(
    nano_ros_service_t *service,
    nano_ros_node_t *node,
    const nano_ros_service_type_t *type,
    const char *service_name,
    nano_ros_service_callback_t callback,
    void *context);

// Create service client
nano_ros_ret_t nano_ros_client_init(
    nano_ros_client_t *client,
    nano_ros_node_t *node,
    const nano_ros_service_type_t *type,
    const char *service_name);

// Call service (blocking)
nano_ros_ret_t nano_ros_client_call(
    nano_ros_client_t *client,
    const uint8_t *request,
    size_t request_len,
    uint8_t *response,
    size_t response_max_len,
    size_t *response_len,
    uint32_t timeout_ms);

// Finalize
nano_ros_ret_t nano_ros_service_fini(nano_ros_service_t *service);
nano_ros_ret_t nano_ros_client_fini(nano_ros_client_t *client);
```

### Timer API

```c
// Timer callback type
typedef void (*nano_ros_timer_callback_t)(nano_ros_timer_t *timer, void *context);

// Create timer
nano_ros_ret_t nano_ros_timer_init(
    nano_ros_timer_t *timer,
    nano_ros_support_t *support,
    uint64_t period_ns,
    nano_ros_timer_callback_t callback,
    void *context);

// Cancel timer
nano_ros_ret_t nano_ros_timer_cancel(nano_ros_timer_t *timer);

// Reset timer
nano_ros_ret_t nano_ros_timer_reset(nano_ros_timer_t *timer);

// Finalize timer
nano_ros_ret_t nano_ros_timer_fini(nano_ros_timer_t *timer);
```

### Executor API

```c
// Callback invocation mode
typedef enum {
    NANO_ROS_ON_NEW_DATA,  // Call only when new data available
    NANO_ROS_ALWAYS        // Always call (even with NULL data)
} nano_ros_executor_invocation_t;

// Initialize executor with fixed handle capacity
nano_ros_ret_t nano_ros_executor_init(
    nano_ros_executor_t *executor,
    nano_ros_support_t *support,
    size_t max_handles);

// Add subscription to executor
nano_ros_ret_t nano_ros_executor_add_subscription(
    nano_ros_executor_t *executor,
    nano_ros_subscription_t *subscription,
    nano_ros_executor_invocation_t invocation);

// Add timer to executor
nano_ros_ret_t nano_ros_executor_add_timer(
    nano_ros_executor_t *executor,
    nano_ros_timer_t *timer);

// Add service to executor
nano_ros_ret_t nano_ros_executor_add_service(
    nano_ros_executor_t *executor,
    nano_ros_service_t *service);

// Spin once (process pending callbacks)
nano_ros_ret_t nano_ros_executor_spin_some(
    nano_ros_executor_t *executor,
    uint64_t timeout_ns);

// Spin forever
nano_ros_ret_t nano_ros_executor_spin(nano_ros_executor_t *executor);

// Spin with period
nano_ros_ret_t nano_ros_executor_spin_period(
    nano_ros_executor_t *executor,
    uint64_t period_ns);

// Finalize executor
nano_ros_ret_t nano_ros_executor_fini(nano_ros_executor_t *executor);
```

### QoS Settings

```c
typedef enum {
    NANO_ROS_QOS_RELIABILITY_BEST_EFFORT,
    NANO_ROS_QOS_RELIABILITY_RELIABLE
} nano_ros_qos_reliability_t;

typedef enum {
    NANO_ROS_QOS_DURABILITY_VOLATILE,
    NANO_ROS_QOS_DURABILITY_TRANSIENT_LOCAL
} nano_ros_qos_durability_t;

typedef enum {
    NANO_ROS_QOS_HISTORY_KEEP_LAST,
    NANO_ROS_QOS_HISTORY_KEEP_ALL
} nano_ros_qos_history_t;

typedef struct {
    nano_ros_qos_reliability_t reliability;
    nano_ros_qos_durability_t durability;
    nano_ros_qos_history_t history;
    size_t depth;
} nano_ros_qos_t;

// Predefined QoS profiles
extern const nano_ros_qos_t NANO_ROS_QOS_DEFAULT;
extern const nano_ros_qos_t NANO_ROS_QOS_SENSOR_DATA;
extern const nano_ros_qos_t NANO_ROS_QOS_SERVICES;
```

## Message Type Generation

Messages are generated using the existing `cargo nano-ros generate` tool with C output support.

### Generated Header Example

```c
// std_msgs/msg/Int32.h (generated)
#ifndef STD_MSGS__MSG__INT32_H_
#define STD_MSGS__MSG__INT32_H_

#include <nano_ros/types.h>

typedef struct {
    int32_t data;
} std_msgs__msg__Int32;

// Type info for API
extern const nano_ros_message_type_t std_msgs__msg__Int32__type;

// Serialization functions
size_t std_msgs__msg__Int32__serialize(
    const std_msgs__msg__Int32 *msg,
    uint8_t *buffer,
    size_t buffer_size);

bool std_msgs__msg__Int32__deserialize(
    std_msgs__msg__Int32 *msg,
    const uint8_t *buffer,
    size_t buffer_size);

#endif
```

### Usage Example

```c
#include <nano_ros/init.h>
#include <nano_ros/node.h>
#include <nano_ros/publisher.h>
#include <nano_ros/subscription.h>
#include <nano_ros/executor.h>
#include <std_msgs/msg/Int32.h>

// Callback for subscription
void on_message(const uint8_t *data, size_t len, void *ctx) {
    std_msgs__msg__Int32 msg;
    if (std_msgs__msg__Int32__deserialize(&msg, data, len)) {
        printf("Received: %d\n", msg.data);
    }
}

int main(void) {
    nano_ros_support_t support;
    nano_ros_node_t node;
    nano_ros_publisher_t pub;
    nano_ros_subscription_t sub;
    nano_ros_executor_t executor;

    // Initialize
    nano_ros_support_init(&support, "tcp/127.0.0.1:7447", 0);
    nano_ros_node_init(&node, &support, "my_node", "/");

    // Create publisher
    nano_ros_publisher_init(&pub, &node,
        &std_msgs__msg__Int32__type, "/chatter");

    // Create subscription
    nano_ros_subscription_init(&sub, &node,
        &std_msgs__msg__Int32__type, "/chatter",
        on_message, NULL);

    // Setup executor
    nano_ros_executor_init(&executor, &support, 2);
    nano_ros_executor_add_subscription(&executor, &sub, NANO_ROS_ON_NEW_DATA);

    // Publish a message
    std_msgs__msg__Int32 msg = { .data = 42 };
    uint8_t buffer[64];
    size_t len = std_msgs__msg__Int32__serialize(&msg, buffer, sizeof(buffer));
    nano_ros_publish_raw(&pub, buffer, len);

    // Spin
    nano_ros_executor_spin(&executor);

    // Cleanup
    nano_ros_executor_fini(&executor);
    nano_ros_subscription_fini(&sub);
    nano_ros_publisher_fini(&pub);
    nano_ros_node_fini(&node);
    nano_ros_support_fini(&support);

    return 0;
}
```

## Implementation Phases

### 11.1 Project Setup

**Status: COMPLETE**

#### Work Items
- [x] Create `crates/nano-ros-c/` crate structure
- [x] Create manually maintained C headers (cbindgen removed from build)
- [x] Create CMake build integration
- [x] Setup cross-compilation for ARM targets

#### Deliverables
- `crates/nano-ros-c/Cargo.toml`
- `crates/nano-ros-c/src/lib.rs`
- `crates/nano-ros-c/include/nano_ros/` (modular headers)

### 11.2 Core Types and Initialization

**Status: COMPLETE**

#### Work Items
- [x] Implement `nano_ros_support_t` wrapping zenoh session
- [x] Implement `nano_ros_node_t` wrapping Rust node
- [x] Implement return codes and error handling
- [x] Add QoS types and predefined profiles

#### API Coverage
- `nano_ros_support_init()` / `nano_ros_support_fini()`
- `nano_ros_node_init()` / `nano_ros_node_fini()`

### 11.3 Publisher

**Status: COMPLETE**

#### Work Items
- [x] Implement `nano_ros_publisher_t`
- [x] Implement raw publish function
- [x] Add QoS support
- [x] Add liveliness token for ROS 2 discovery

#### API Coverage
- `nano_ros_publisher_init()` / `nano_ros_publisher_init_with_qos()`
- `nano_ros_publish_raw()`
- `nano_ros_publisher_fini()`

### 11.4 Subscription

**Status: COMPLETE**

#### Work Items
- [x] Implement `nano_ros_subscription_t`
- [x] Implement callback dispatch
- [x] Add QoS support
- [x] Handle rmw_zenoh attachment parsing
- [x] Implement executor polling for message delivery

#### API Coverage
- `nano_ros_subscription_init()`
- `nano_ros_subscription_fini()`

### 11.5 Executor

**Status: COMPLETE**

#### Work Items
- [x] Implement `nano_ros_executor_t` with static handle array
- [x] Implement `spin_some()` with timeout
- [x] Implement `spin()` infinite loop
- [x] Implement `spin_period()` for periodic execution
- [x] Add subscription and timer handle management
- [x] Add service handle management
- [x] Fix state check to allow SPINNING state in spin_some

#### API Coverage
- `nano_ros_executor_init()` / `nano_ros_executor_fini()`
- `nano_ros_executor_add_subscription()`
- `nano_ros_executor_add_timer()`
- `nano_ros_executor_add_service()`
- `nano_ros_executor_spin_some()` / `spin()` / `spin_period()`

### 11.6 Timer

**Status: COMPLETE**

#### Work Items
- [x] Implement `nano_ros_timer_t`
- [x] Integrate with executor
- [x] Support cancel and reset operations

#### API Coverage
- `nano_ros_timer_init()` / `nano_ros_timer_fini()`
- `nano_ros_timer_cancel()` / `nano_ros_timer_reset()`

### 11.7 Services

**Status: COMPLETE**

#### Work Items
- [x] Implement `nano_ros_service_t` (server)
- [x] Implement `nano_ros_client_t` (client)
- [x] Implement blocking service call
- [x] Integrate service server with executor

#### API Coverage
- `nano_ros_service_init()` / `nano_ros_service_fini()`
- `nano_ros_client_init()` / `nano_ros_client_fini()`
- `nano_ros_client_call()`
- `nano_ros_executor_add_service()`

### 11.8 Message Generation

**Status: COMPLETE**

C code generation templates exist in `colcon-nano-ros`, and CDR helper functions are
implemented in nano-ros-c. Generated C code can serialize/deserialize all ROS 2 primitive
types, strings, and nested messages.

#### Work Items
- [x] Extend `cargo nano-ros generate` for C output (templates exist)
- [x] Generate C struct definitions (template: `message_c.h.jinja`)
- [x] Generate serialization/deserialization functions (template: `message_c.c.jinja`)
- [x] Generate type info constants
- [x] Support nested types and arrays
- [x] Service C generation (`service_c.h.jinja`, `service_c.c.jinja`)
- [x] Action C generation (`action_c.h.jinja`, `action_c.c.jinja`)
- [x] Implement CDR helper functions in nano-ros-c

#### CDR Helper Functions
Implemented in `crates/nano-ros-c/src/cdr.rs` and exported in `nano_ros/cdr.h`:
```c
// Write functions (all return 0 on success, -1 on buffer overflow)
int32_t nano_ros_cdr_write_bool(uint8_t** ptr, const uint8_t* end, bool value);
int32_t nano_ros_cdr_write_u8(uint8_t** ptr, const uint8_t* end, uint8_t value);
int32_t nano_ros_cdr_write_i8(uint8_t** ptr, const uint8_t* end, int8_t value);
int32_t nano_ros_cdr_write_u16(uint8_t** ptr, const uint8_t* end, uint16_t value);
int32_t nano_ros_cdr_write_i16(uint8_t** ptr, const uint8_t* end, int16_t value);
int32_t nano_ros_cdr_write_u32(uint8_t** ptr, const uint8_t* end, uint32_t value);
int32_t nano_ros_cdr_write_i32(uint8_t** ptr, const uint8_t* end, int32_t value);
int32_t nano_ros_cdr_write_u64(uint8_t** ptr, const uint8_t* end, uint64_t value);
int32_t nano_ros_cdr_write_i64(uint8_t** ptr, const uint8_t* end, int64_t value);
int32_t nano_ros_cdr_write_f32(uint8_t** ptr, const uint8_t* end, float value);
int32_t nano_ros_cdr_write_f64(uint8_t** ptr, const uint8_t* end, double value);
int32_t nano_ros_cdr_write_string(uint8_t** ptr, const uint8_t* end, const char* value);

// Read functions (all return 0 on success, -1 on buffer underflow)
int32_t nano_ros_cdr_read_bool(const uint8_t** ptr, const uint8_t* end, bool* value);
int32_t nano_ros_cdr_read_u8(const uint8_t** ptr, const uint8_t* end, uint8_t* value);
int32_t nano_ros_cdr_read_i8(const uint8_t** ptr, const uint8_t* end, int8_t* value);
int32_t nano_ros_cdr_read_u16(const uint8_t** ptr, const uint8_t* end, uint16_t* value);
int32_t nano_ros_cdr_read_i16(const uint8_t** ptr, const uint8_t* end, int16_t* value);
int32_t nano_ros_cdr_read_u32(const uint8_t** ptr, const uint8_t* end, uint32_t* value);
int32_t nano_ros_cdr_read_i32(const uint8_t** ptr, const uint8_t* end, int32_t* value);
int32_t nano_ros_cdr_read_u64(const uint8_t** ptr, const uint8_t* end, uint64_t* value);
int32_t nano_ros_cdr_read_i64(const uint8_t** ptr, const uint8_t* end, int64_t* value);
int32_t nano_ros_cdr_read_f32(const uint8_t** ptr, const uint8_t* end, float* value);
int32_t nano_ros_cdr_read_f64(const uint8_t** ptr, const uint8_t* end, double* value);
int32_t nano_ros_cdr_read_string(const uint8_t** ptr, const uint8_t* end, char* value, size_t max_len);
```

#### Deliverables
- [x] `colcon-nano-ros/packages/rosidl-codegen/templates/message_c.h.jinja`
- [x] `colcon-nano-ros/packages/rosidl-codegen/templates/message_c.c.jinja`
- [x] `colcon-nano-ros/packages/rosidl-codegen/templates/service_c.h.jinja`
- [x] `colcon-nano-ros/packages/rosidl-codegen/templates/service_c.c.jinja`
- [x] `colcon-nano-ros/packages/rosidl-codegen/templates/action_c.h.jinja`
- [x] `colcon-nano-ros/packages/rosidl-codegen/templates/action_c.c.jinja`
- [x] CDR helper implementation in `crates/nano-ros-c/src/cdr.rs`

#### Future Work
- [ ] CLI integration to invoke C generation directly
- [ ] Pre-generated headers for `std_msgs`, `builtin_interfaces`
- [ ] Pre-generated headers for `example_interfaces` services

### 11.9 Examples

**Status: COMPLETE**

#### Work Items
- [x] Create `examples/native-c-talker/` - Publisher example
- [x] Create `examples/native-c-listener/` - Subscriber example
- [ ] Create `examples/native-c-service/` - Service server/client example

#### Deliverables
- Working C examples with CMake build
- Integration tests (`tests/c-tests.sh`)

### 11.10 Zephyr Integration

**Status: PARTIAL**

#### Work Items
- [x] Create Zephyr example structure (`examples/zephyr-c-talker/`, `examples/zephyr-c-listener/`)
- [x] CMake integration for Zephyr builds
- [ ] Full nano-ros-c integration (currently uses zenoh_shim directly)
- [ ] Test on native_sim with full C API
- [ ] Test on real hardware (STM32, nRF, etc.)

#### Deliverables
- Zephyr west module configuration
- Zephyr Kconfig options
- Zephyr sample applications

#### Notes
Current Zephyr examples use `zenoh_shim` directly as an interim solution. Full integration
requires no_std transport support in nano-ros-c.

### 11.11 rclc Header Compatibility

**Status: COMPLETE**

#### Work Items
- [x] Refactor into modular headers matching rclc structure
- [x] Create `nano_ros/init.h` - Initialization functions
- [x] Create `nano_ros/node.h` - Node API
- [x] Create `nano_ros/publisher.h` - Publisher API
- [x] Create `nano_ros/subscription.h` - Subscription API
- [x] Create `nano_ros/service.h` - Service server API
- [x] Create `nano_ros/client.h` - Service client API
- [x] Create `nano_ros/timer.h` - Timer API
- [x] Create `nano_ros/executor.h` - Executor API
- [x] Create `nano_ros/types.h` - Common types and return codes
- [x] Create `nano_ros/visibility.h` - Export macros
- [x] Create `nano_ros/cdr.h` - CDR serialization helpers
- [x] Update examples to use modular includes
- [x] Update message generation templates for modular includes
- [x] Remove cbindgen from build-time (headers are manually maintained)

#### Final Structure
```
crates/nano-ros-c/include/nano_ros/
├── init.h              # nano_ros_support_init/fini
├── node.h              # nano_ros_node_*
├── publisher.h         # nano_ros_publisher_*, nano_ros_publish_*
├── subscription.h      # nano_ros_subscription_*
├── service.h           # nano_ros_service_*
├── client.h            # nano_ros_client_*
├── timer.h             # nano_ros_timer_*
├── executor.h          # nano_ros_executor_*
├── types.h             # Return codes, QoS, message_type_t
├── visibility.h        # NANO_ROS_PUBLIC, NANO_ROS_WARN_UNUSED
└── cdr.h               # CDR read/write helpers
```

#### Usage Pattern (rclc-style)
```c
// Include only what you need
#include <nano_ros/init.h>
#include <nano_ros/node.h>
#include <nano_ros/publisher.h>
#include <nano_ros/executor.h>
```

#### Notes
- **No umbrella header**: Unlike rclc, we don't provide a `nano_ros.h` umbrella header.
  Users must include specific headers they need (matches rclc best practices).
- **Manually maintained**: Headers are manually maintained, not cbindgen-generated.
  This allows for cleaner API design and avoids build-time generation issues.
- **No build-time code generation**: Headers can be installed to system directories
  without any issues since nothing is generated during compilation.

### 11.12 Clock API

**Status: COMPLETE**

#### Work Items
- [x] Implement `nano_ros_clock_t` structure
- [x] Implement ROS time source (currently same as system time)
- [x] Implement steady time source (monotonic)
- [x] Implement system time source (wall clock)
- [x] Implement `nano_ros_time_t` and `nano_ros_duration_t` types
- [x] Implement time utility functions (add, sub, compare)
- [ ] Support time override for simulation (future: /clock topic subscription)

#### Deliverables
- `crates/nano-ros-c/include/nano_ros/clock.h`
- `crates/nano-ros-c/src/clock.rs`

#### API
```c
#include <nano_ros/clock.h>

// Time types (compatible with builtin_interfaces)
typedef struct nano_ros_time_t {
    int32_t sec;
    uint32_t nanosec;
} nano_ros_time_t;

typedef struct nano_ros_duration_t {
    int32_t sec;
    uint32_t nanosec;
} nano_ros_duration_t;

// Clock types
typedef enum nano_ros_clock_type_t {
    NANO_ROS_CLOCK_UNINITIALIZED = 0,
    NANO_ROS_CLOCK_ROS_TIME = 1,      // Follows /clock if available
    NANO_ROS_CLOCK_SYSTEM_TIME = 2,   // Wall clock time
    NANO_ROS_CLOCK_STEADY_TIME = 3,   // Monotonic clock
} nano_ros_clock_type_t;

// Clock functions
nano_ros_clock_t nano_ros_clock_get_zero_initialized(void);
nano_ros_ret_t nano_ros_clock_init(nano_ros_clock_t *clock, nano_ros_clock_type_t type);
nano_ros_ret_t nano_ros_clock_get_now(const nano_ros_clock_t *clock, nano_ros_time_t *time_out);
nano_ros_ret_t nano_ros_clock_get_now_ns(const nano_ros_clock_t *clock, int64_t *nanoseconds);
bool nano_ros_clock_is_valid(const nano_ros_clock_t *clock);
nano_ros_clock_type_t nano_ros_clock_get_type(const nano_ros_clock_t *clock);
nano_ros_ret_t nano_ros_clock_fini(nano_ros_clock_t *clock);

// Time utilities
nano_ros_time_t nano_ros_time_from_nanoseconds(int64_t nanoseconds);
int64_t nano_ros_time_to_nanoseconds(const nano_ros_time_t *time);
nano_ros_time_t nano_ros_time_add(nano_ros_time_t time, nano_ros_duration_t duration);
nano_ros_time_t nano_ros_time_sub(nano_ros_time_t time, nano_ros_duration_t duration);
int nano_ros_time_compare(nano_ros_time_t a, nano_ros_time_t b);
```

#### Usage Example
```c
#include <nano_ros/clock.h>

// Initialize a system clock
nano_ros_clock_t clock = nano_ros_clock_get_zero_initialized();
nano_ros_clock_init(&clock, NANO_ROS_CLOCK_SYSTEM_TIME);

// Get current time
nano_ros_time_t now;
nano_ros_clock_get_now(&clock, &now);
printf("Time: %d.%09u sec\n", now.sec, now.nanosec);

// Clean up
nano_ros_clock_fini(&clock);
```

#### Notes
- ROS time currently returns system time; full /clock topic support requires subscription integration
- Steady time uses a monotonic source, suitable for measuring elapsed time
- All time types are compatible with `builtin_interfaces/msg/Time` and `Duration`

### 11.13 Parameters

**Status: COMPLETE**

#### Work Items
- [x] Implement `nano_ros_parameter_t` structure
- [x] Implement parameter types (bool, int64, double, string)
- [x] Implement parameter server with static allocation
- [x] Add parameter change callbacks
- [ ] Support parameter events topic (future: ROS 2 network integration)
- [ ] Array parameter types (future)

#### Deliverables
- `crates/nano-ros-c/include/nano_ros/parameter.h`
- `crates/nano-ros-c/src/parameter.rs`

#### API
```c
#include <nano_ros/parameter.h>

// Parameter types (compatible with rcl_interfaces/msg/ParameterType)
typedef enum nano_ros_parameter_type_t {
    NANO_ROS_PARAMETER_NOT_SET = 0,
    NANO_ROS_PARAMETER_BOOL = 1,
    NANO_ROS_PARAMETER_INTEGER = 2,
    NANO_ROS_PARAMETER_DOUBLE = 3,
    NANO_ROS_PARAMETER_STRING = 4,
    // Array types defined but not yet implemented
} nano_ros_parameter_type_t;

// Parameter server (uses static allocation)
nano_ros_param_server_t nano_ros_param_server_get_zero_initialized(void);
nano_ros_ret_t nano_ros_param_server_init(
    nano_ros_param_server_t *server,
    nano_ros_parameter_t *storage,  // User-provided storage array
    size_t capacity);

// Declare parameters with default values
nano_ros_ret_t nano_ros_param_declare_bool(server, name, default_value);
nano_ros_ret_t nano_ros_param_declare_integer(server, name, default_value);
nano_ros_ret_t nano_ros_param_declare_double(server, name, default_value);
nano_ros_ret_t nano_ros_param_declare_string(server, name, default_value);

// Get/set parameter values
nano_ros_ret_t nano_ros_param_get_bool(server, name, &value);
nano_ros_ret_t nano_ros_param_set_bool(server, name, value);
// ... similar for integer, double, string

// Query and callback
bool nano_ros_param_has(server, name);
nano_ros_parameter_type_t nano_ros_param_get_type(server, name);
nano_ros_ret_t nano_ros_param_server_set_callback(server, callback, context);
```

#### Usage Example
```c
#include <nano_ros/parameter.h>

// Allocate storage for parameters
nano_ros_parameter_t param_storage[8];
nano_ros_param_server_t params = nano_ros_param_server_get_zero_initialized();
nano_ros_param_server_init(&params, param_storage, 8);

// Declare parameters with defaults
nano_ros_param_declare_bool(&params, "verbose", false);
nano_ros_param_declare_integer(&params, "rate_hz", 10);
nano_ros_param_declare_double(&params, "gain", 1.5);
nano_ros_param_declare_string(&params, "topic", "/data");

// Read and modify
int64_t rate;
nano_ros_param_get_integer(&params, "rate_hz", &rate);
nano_ros_param_set_integer(&params, "rate_hz", 20);

// Clean up
nano_ros_param_server_fini(&params);
```

#### Design Notes
- **Static allocation**: User provides parameter storage array (no malloc)
- **Type safety**: Separate get/set functions per type with validation
- **Callbacks**: Optional callback invoked before parameter changes (can reject)
- **Embedded-friendly**: Fixed-size name (64 bytes) and string value (128 bytes)

### 11.14 Actions

**Status: COMPLETE**

#### Work Items
- [x] Implement `nano_ros_action_server_t` structure
- [x] Implement `nano_ros_action_client_t` structure
- [x] Implement goal handling (send, accept, reject)
- [x] Implement feedback publishing
- [x] Implement result handling
- [x] Implement cancellation
- [x] Goal state machine (accepted → executing → succeeded/canceled/aborted)

#### Files
- **Header**: `crates/nano-ros-c/include/nano_ros/action.h`
- **Implementation**: `crates/nano-ros-c/src/action.rs`

#### API Overview

```c
#include <nano_ros/action.h>

// Goal status (compatible with action_msgs/msg/GoalStatus)
typedef enum nano_ros_goal_status_t {
    NANO_ROS_GOAL_STATUS_UNKNOWN = 0,
    NANO_ROS_GOAL_STATUS_ACCEPTED = 1,
    NANO_ROS_GOAL_STATUS_EXECUTING = 2,
    NANO_ROS_GOAL_STATUS_CANCELING = 3,
    NANO_ROS_GOAL_STATUS_SUCCEEDED = 4,
    NANO_ROS_GOAL_STATUS_CANCELED = 5,
    NANO_ROS_GOAL_STATUS_ABORTED = 6,
} nano_ros_goal_status_t;

// Goal response codes
typedef enum nano_ros_goal_response_t {
    NANO_ROS_GOAL_REJECT = 0,
    NANO_ROS_GOAL_ACCEPT_AND_EXECUTE = 1,
    NANO_ROS_GOAL_ACCEPT_AND_DEFER = 2,
} nano_ros_goal_response_t;

// Goal UUID (16-byte identifier)
typedef struct nano_ros_goal_uuid_t {
    uint8_t uuid[16];
} nano_ros_goal_uuid_t;

// Action type information
typedef struct nano_ros_action_type_t {
    const char *type_name;
    const char *type_hash;
    size_t goal_serialized_size_max;
    size_t result_serialized_size_max;
    size_t feedback_serialized_size_max;
} nano_ros_action_type_t;

// Callbacks
typedef nano_ros_goal_response_t (*nano_ros_goal_callback_t)(
    const nano_ros_goal_uuid_t *goal_uuid,
    const uint8_t *goal_request,
    size_t goal_len,
    void *context);

typedef nano_ros_cancel_response_t (*nano_ros_cancel_callback_t)(
    nano_ros_goal_handle_t *goal,
    void *context);

typedef void (*nano_ros_accepted_callback_t)(
    nano_ros_goal_handle_t *goal,
    void *context);

// Action Server API
nano_ros_action_server_t nano_ros_action_server_get_zero_initialized(void);

nano_ros_ret_t nano_ros_action_server_init(
    nano_ros_action_server_t *server,
    nano_ros_node_t *node,
    const char *action_name,
    const nano_ros_action_type_t *type,
    nano_ros_goal_callback_t goal_callback,
    nano_ros_cancel_callback_t cancel_callback,
    nano_ros_accepted_callback_t accepted_callback,
    void *context);

nano_ros_ret_t nano_ros_action_publish_feedback(
    nano_ros_goal_handle_t *goal,
    const uint8_t *feedback,
    size_t feedback_len);

nano_ros_ret_t nano_ros_action_succeed(
    nano_ros_goal_handle_t *goal,
    const uint8_t *result,
    size_t result_len);

nano_ros_ret_t nano_ros_action_abort(
    nano_ros_goal_handle_t *goal,
    const uint8_t *result,
    size_t result_len);

nano_ros_ret_t nano_ros_action_canceled(
    nano_ros_goal_handle_t *goal,
    const uint8_t *result,
    size_t result_len);

nano_ros_ret_t nano_ros_action_execute(nano_ros_goal_handle_t *goal);

nano_ros_ret_t nano_ros_action_server_fini(nano_ros_action_server_t *server);

// Action Client API
nano_ros_action_client_t nano_ros_action_client_get_zero_initialized(void);

nano_ros_ret_t nano_ros_action_client_init(
    nano_ros_action_client_t *client,
    nano_ros_node_t *node,
    const char *action_name,
    const nano_ros_action_type_t *type);

nano_ros_ret_t nano_ros_action_send_goal(
    nano_ros_action_client_t *client,
    const uint8_t *goal,
    size_t goal_len,
    nano_ros_goal_uuid_t *goal_uuid);

nano_ros_ret_t nano_ros_action_cancel_goal(
    nano_ros_action_client_t *client,
    const nano_ros_goal_uuid_t *goal_uuid);

nano_ros_ret_t nano_ros_action_get_result(
    nano_ros_action_client_t *client,
    const nano_ros_goal_uuid_t *goal_uuid,
    nano_ros_goal_status_t *status,
    uint8_t *result,
    size_t result_capacity,
    size_t *result_len);

nano_ros_ret_t nano_ros_action_client_fini(nano_ros_action_client_t *client);

// Utility functions
nano_ros_ret_t nano_ros_goal_uuid_generate(nano_ros_goal_uuid_t *uuid);
bool nano_ros_goal_uuid_equal(const nano_ros_goal_uuid_t *a, const nano_ros_goal_uuid_t *b);
const char *nano_ros_goal_status_to_string(nano_ros_goal_status_t status);
```

#### Design Notes
- **Static allocation**: Supports up to `NANO_ROS_MAX_CONCURRENT_GOALS` (4) concurrent goals per server
- **Goal state machine**: Proper transitions between accepted, executing, canceling, and terminal states
- **UUID generation**: Uses system time + counter for unique goal identifiers
- **Compatible with action_msgs**: Goal status values match ROS 2 standard

### 11.15 Guard Conditions

**Status: COMPLETE**

#### Work Items
- [x] Implement `nano_ros_guard_condition_t` structure
- [x] Implement trigger mechanism (thread-safe)
- [x] Integrate with executor for wake-up
- [x] Support multi-threaded trigger via atomic operations

#### Files
- **Header**: `crates/nano-ros-c/include/nano_ros/guard_condition.h`
- **Implementation**: `crates/nano-ros-c/src/guard_condition.rs`
- **Executor integration**: Updated `executor.rs` and `executor.h`

#### API Overview

```c
#include <nano_ros/guard_condition.h>

// Guard condition structure
typedef struct nano_ros_guard_condition_t {
    nano_ros_guard_condition_state_t state;
    volatile bool triggered;  // Atomic for thread-safety
    nano_ros_guard_condition_callback_t callback;
    void *context;
    void *_support;
} nano_ros_guard_condition_t;

// Callback type
typedef void (*nano_ros_guard_condition_callback_t)(void *context);

// Lifecycle functions
nano_ros_guard_condition_t nano_ros_guard_condition_get_zero_initialized(void);

nano_ros_ret_t nano_ros_guard_condition_init(
    nano_ros_guard_condition_t *guard,
    nano_ros_support_t *support);

nano_ros_ret_t nano_ros_guard_condition_set_callback(
    nano_ros_guard_condition_t *guard,
    nano_ros_guard_condition_callback_t callback,
    void *context);

nano_ros_ret_t nano_ros_guard_condition_fini(
    nano_ros_guard_condition_t *guard);

// Trigger functions (thread-safe)
nano_ros_ret_t nano_ros_guard_condition_trigger(
    nano_ros_guard_condition_t *guard);

bool nano_ros_guard_condition_is_triggered(
    const nano_ros_guard_condition_t *guard);

nano_ros_ret_t nano_ros_guard_condition_clear(
    nano_ros_guard_condition_t *guard);

// Executor integration
nano_ros_ret_t nano_ros_executor_add_guard_condition(
    nano_ros_executor_t *executor,
    nano_ros_guard_condition_t *guard);
```

#### Usage Example

```c
#include <nano_ros/guard_condition.h>
#include <nano_ros/executor.h>

// Callback invoked when guard condition is triggered
void shutdown_callback(void *context) {
    bool *running = (bool *)context;
    *running = false;
}

int main(void) {
    static bool running = true;

    // Initialize guard condition
    nano_ros_guard_condition_t guard = nano_ros_guard_condition_get_zero_initialized();
    nano_ros_guard_condition_init(&guard, &support);
    nano_ros_guard_condition_set_callback(&guard, shutdown_callback, &running);

    // Add to executor
    nano_ros_executor_add_guard_condition(&executor, &guard);

    // From another thread: trigger shutdown
    // nano_ros_guard_condition_trigger(&guard);

    // Executor will invoke callback when triggered
    while (running) {
        nano_ros_executor_spin_some(&executor, 100000000);
    }

    nano_ros_guard_condition_fini(&guard);
}
```

#### Design Notes
- **Thread-safe trigger**: Uses atomic operations for cross-thread signaling
- **Callback optional**: Guard condition works without callback (just wake-up)
- **Executor integration**: Processed in each spin cycle like other handles
- **Zero-copy**: No memory allocation, all state in user-provided structure

### 11.16 no_std Support

**Status: NOT STARTED**

#### Work Items
- [ ] Add `no_std` feature flag to nano-ros-c
- [ ] Implement shim-based transport for no_std
- [ ] Remove std dependencies from core paths
- [ ] Implement static allocation for all internal buffers
- [ ] Test with Zephyr using full C API (not zenoh_shim workaround)
- [ ] Test on bare-metal targets

#### Notes
Currently nano-ros-c requires `std` feature. Full no_std support requires:
- Conditional compilation for std vs no_std paths
- Integration with zenoh-pico-shim for transport
- Static buffer allocation throughout

## Memory Model

### Static Allocation

All handles use static storage provided by the user:

```c
// User provides storage
static nano_ros_support_t support;
static nano_ros_node_t node;
static nano_ros_publisher_t pub;
static nano_ros_subscription_t sub;
static nano_ros_executor_t executor;

// Executor handle array (user-provided)
#define MAX_HANDLES 8
static nano_ros_executor_handle_t handles[MAX_HANDLES];
```

### Memory Requirements

| Component | RAM (bytes) | Notes |
|-----------|-------------|-------|
| Support context | ~64 | Zenoh session reference |
| Node | ~128 | Name, namespace, liveliness |
| Publisher | ~64 | Topic, zenoh publisher |
| Subscription | ~80 | Topic, callback, context |
| Timer | ~48 | Period, callback, state |
| Executor | ~32 + 16*N | Base + per-handle overhead |

Estimated total for minimal system (1 node, 2 pub, 2 sub, executor): **~600 bytes RAM**

## Build Integration

### CMake

```cmake
# Find nano-ros-c
find_package(nano_ros_c REQUIRED)

# Create executable
add_executable(my_app main.c)
target_link_libraries(my_app nano_ros_c::nano_ros_c)

# Generate messages
nano_ros_generate_messages(my_app
    PACKAGES std_msgs example_interfaces
    LANGUAGE C)
```

### Zephyr

```cmake
# In CMakeLists.txt
list(APPEND ZEPHYR_EXTRA_MODULES ${CMAKE_CURRENT_SOURCE_DIR}/modules/nano-ros-c)

# In prj.conf
CONFIG_NANO_ROS=y
CONFIG_NANO_ROS_MAX_NODES=1
CONFIG_NANO_ROS_MAX_PUBLISHERS=4
CONFIG_NANO_ROS_MAX_SUBSCRIPTIONS=4
```

## Testing Strategy

### Unit Tests
- C API function tests using Unity framework
- Memory leak detection with sanitizers
- Static analysis with cppcheck

### Integration Tests
- C ↔ Rust interop tests
- C ↔ ROS 2 (rmw_zenoh) interop tests
- Zephyr native_sim tests
- `tests/c-tests.sh` - Automated C pub/sub test

### Hardware Tests
- STM32F4 Discovery board
- Nordic nRF52840 DK
- Raspberry Pi Pico

## Dependencies

### Required
- Rust toolchain (for building nano-ros core)
- CMake 3.16+
- C11 compiler
- zenoh-pico (included via submodule)

### Optional
- Zephyr SDK (for Zephyr targets)
- ARM GCC toolchain (for embedded targets)

## References

- [rclc](https://github.com/ros2/rclc) - Official ROS 2 C API
- [Pico-ROS](https://github.com/Pico-ROS/Pico-ROS-software) - zenoh-pico based ROS client
- [micro-ROS](https://micro.ros.org/) - Embedded ROS 2 framework
- [cbindgen](https://github.com/mozilla/cbindgen) - Rust to C header generator

## Acceptance Criteria

### API Compatibility
- [x] Core API matches rclc patterns
- [x] Easy migration for rclc users
- [x] Full type safety with message generation (CDR helpers + templates)

### Interoperability
- [x] C nano-ros nodes communicate with Rust nano-ros nodes
- [ ] C nano-ros nodes communicate with ROS 2 nodes (via rmw_zenoh) - needs testing
- [x] Works via zenoh transport

### Embedded Support
- [x] Builds for native targets
- [ ] Full no_std support for embedded
- [ ] Runs on real embedded hardware
- [x] Static memory allocation (no malloc at runtime)
- [x] Memory footprint < 2KB RAM for minimal system

### Documentation
- [x] API reference documentation (in header)
- [x] Usage examples
- [ ] Zephyr integration guide
