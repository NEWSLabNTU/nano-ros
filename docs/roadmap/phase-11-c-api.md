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
| 11.12 Clock API              | Not Started     | ROS/steady/system time                      |
| 11.13 Parameters             | Not Started     | Parameter server                            |
| 11.14 Actions                | Not Started     | Action server/client                        |
| 11.15 Guard Conditions       | Not Started     | Executor wake-up triggers                   |
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

**Status: NOT STARTED**

#### Work Items
- [ ] Implement `nano_ros_clock_t` structure
- [ ] Implement ROS time source
- [ ] Implement steady time source
- [ ] Implement system time source
- [ ] Add clock to support context
- [ ] Support time override for simulation

#### API Design
```c
typedef enum {
    NANO_ROS_CLOCK_ROS_TIME,
    NANO_ROS_CLOCK_SYSTEM_TIME,
    NANO_ROS_CLOCK_STEADY_TIME
} nano_ros_clock_type_t;

typedef struct {
    nano_ros_clock_type_t type;
    // internal state
} nano_ros_clock_t;

nano_ros_ret_t nano_ros_clock_init(
    nano_ros_clock_t *clock,
    nano_ros_clock_type_t type);

nano_ros_ret_t nano_ros_clock_get_now(
    nano_ros_clock_t *clock,
    int64_t *nanoseconds);

nano_ros_ret_t nano_ros_clock_fini(nano_ros_clock_t *clock);
```

### 11.13 Parameters

**Status: NOT STARTED**

#### Work Items
- [ ] Implement `nano_ros_parameter_t` structure
- [ ] Implement parameter types (bool, int, double, string, arrays)
- [ ] Implement parameter server
- [ ] Implement parameter client
- [ ] Add parameter change callbacks
- [ ] Support parameter events topic

#### API Design
```c
typedef enum {
    NANO_ROS_PARAMETER_NOT_SET,
    NANO_ROS_PARAMETER_BOOL,
    NANO_ROS_PARAMETER_INTEGER,
    NANO_ROS_PARAMETER_DOUBLE,
    NANO_ROS_PARAMETER_STRING,
    NANO_ROS_PARAMETER_BYTE_ARRAY,
    NANO_ROS_PARAMETER_BOOL_ARRAY,
    NANO_ROS_PARAMETER_INTEGER_ARRAY,
    NANO_ROS_PARAMETER_DOUBLE_ARRAY,
    NANO_ROS_PARAMETER_STRING_ARRAY
} nano_ros_parameter_type_t;

nano_ros_ret_t nano_ros_node_declare_parameter(
    nano_ros_node_t *node,
    const char *name,
    nano_ros_parameter_type_t type,
    const void *default_value);

nano_ros_ret_t nano_ros_node_get_parameter(
    nano_ros_node_t *node,
    const char *name,
    void *value);

nano_ros_ret_t nano_ros_node_set_parameter(
    nano_ros_node_t *node,
    const char *name,
    const void *value);
```

### 11.14 Actions

**Status: NOT STARTED**

#### Work Items
- [ ] Implement `nano_ros_action_server_t` structure
- [ ] Implement `nano_ros_action_client_t` structure
- [ ] Implement goal handling (send, accept, reject)
- [ ] Implement feedback publishing
- [ ] Implement result handling
- [ ] Implement cancellation
- [ ] Integrate with executor

#### API Design
```c
typedef struct nano_ros_action_server_t nano_ros_action_server_t;
typedef struct nano_ros_action_client_t nano_ros_action_client_t;
typedef struct nano_ros_goal_handle_t nano_ros_goal_handle_t;

// Action server callbacks
typedef nano_ros_goal_response_t (*nano_ros_goal_callback_t)(
    const uint8_t *goal_request,
    size_t goal_len,
    void *context);

typedef void (*nano_ros_cancel_callback_t)(
    nano_ros_goal_handle_t *goal,
    void *context);

typedef void (*nano_ros_accepted_callback_t)(
    nano_ros_goal_handle_t *goal,
    void *context);

// Action server API
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

// Action client API
nano_ros_ret_t nano_ros_action_client_init(
    nano_ros_action_client_t *client,
    nano_ros_node_t *node,
    const char *action_name,
    const nano_ros_action_type_t *type);

nano_ros_ret_t nano_ros_action_send_goal(
    nano_ros_action_client_t *client,
    const uint8_t *goal,
    size_t goal_len,
    nano_ros_goal_handle_t *goal_handle);
```

### 11.15 Guard Conditions

**Status: NOT STARTED**

#### Work Items
- [ ] Implement `nano_ros_guard_condition_t` structure
- [ ] Implement trigger mechanism
- [ ] Integrate with executor for wake-up
- [ ] Support multi-threaded trigger

#### API Design
```c
typedef struct nano_ros_guard_condition_t nano_ros_guard_condition_t;

nano_ros_ret_t nano_ros_guard_condition_init(
    nano_ros_guard_condition_t *guard,
    nano_ros_support_t *support);

nano_ros_ret_t nano_ros_guard_condition_trigger(
    nano_ros_guard_condition_t *guard);

nano_ros_ret_t nano_ros_executor_add_guard_condition(
    nano_ros_executor_t *executor,
    nano_ros_guard_condition_t *guard,
    void (*callback)(void *context),
    void *context);

nano_ros_ret_t nano_ros_guard_condition_fini(
    nano_ros_guard_condition_t *guard);
```

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
