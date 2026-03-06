# First App in C

This chapter walks through creating a C publisher and subscriber using the
nano-ros C API, which follows rclc naming conventions.

## Prerequisites

- nano-ros C library installed to a prefix (see [Installation](installation.md))
- zenohd running

The examples below assume you installed to `~/.local/nano-ros`. Adjust the
`CMAKE_PREFIX_PATH` if you used a different prefix.

## Talker

### 1. Project Structure

```
my-c-talker/
  CMakeLists.txt
  src/
    main.c
```

### 2. CMakeLists.txt

```cmake
cmake_minimum_required(VERSION 3.16)
project(my_c_talker LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Find nros C library
find_package(NanoRos REQUIRED CONFIG)

# Generate C bindings for std_msgs/Int32
nano_ros_generate_interfaces(std_msgs
    "msg/Int32.msg"
    SKIP_INSTALL
)

# Build executable
add_executable(my_c_talker src/main.c)
target_link_libraries(my_c_talker
    PRIVATE
        std_msgs__nano_ros_c
        NanoRos::NanoRos
)
```

Key points:

- `find_package(NanoRos REQUIRED CONFIG)` — locates the nros C library
  via `CMAKE_PREFIX_PATH`.
- `nano_ros_generate_interfaces()` — generates C struct definitions and
  CDR serialization functions from `.msg` files. Never hand-write these.
- `std_msgs__nano_ros_c` — the generated library target for std_msgs types.

### 3. src/main.c

```c
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <signal.h>

#include <nros/init.h>
#include <nros/node.h>
#include <nros/publisher.h>
#include <nros/timer.h>
#include <nros/executor.h>

// Generated message bindings
#include "std_msgs.h"

// Application state
typedef struct {
    nros_publisher_t* publisher;
    std_msgs_msg_int32 message;
    int count;
} talker_context_t;

static nros_support_t support;
static nros_node_t node;
static nros_publisher_t publisher;
static nros_timer_t timer;
static nros_executor_t executor;
static talker_context_t ctx;

static volatile sig_atomic_t running = 1;

static void on_signal(int sig) {
    (void)sig;
    running = 0;
    nros_executor_stop(&executor);
}

static void timer_callback(struct nros_timer_t* t, void* context) {
    (void)t;
    talker_context_t* c = (talker_context_t*)context;

    c->count++;
    c->message.data = c->count;

    uint8_t buffer[64];
    size_t serialized_size = 0;
    if (std_msgs_msg_int32_serialize(&c->message, buffer,
            sizeof(buffer), &serialized_size) == 0) {
        if (nros_publish_raw(c->publisher, buffer, serialized_size) == NROS_RET_OK) {
            printf("Published: %d\n", c->message.data);
        }
    }
}

int main(void) {
    const char* locator = getenv("ZENOH_LOCATOR");
    if (!locator) locator = "tcp/127.0.0.1:7447";

    const char* domain_str = getenv("ROS_DOMAIN_ID");
    uint8_t domain_id = domain_str ? (uint8_t)atoi(domain_str) : 0;

    // Initialize support context (connects to zenoh router)
    memset(&support, 0, sizeof(support));
    if (nros_support_init(&support, locator, domain_id) != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize support\n");
        return 1;
    }

    // Create node
    nros_node_init(&node, &support, "c_talker", "/");

    // Create publisher using generated type support
    nros_publisher_init(&publisher, &node,
        std_msgs_msg_int32_get_type_support(), "/chatter");

    // Set up timer context
    ctx = (talker_context_t){ .publisher = &publisher, .message = {0}, .count = 0 };
    std_msgs_msg_int32_init(&ctx.message);

    // Create timer (1 second = 1,000,000,000 ns)
    nros_timer_init(&timer, &support, 1000000000ULL, timer_callback, &ctx);

    // Create executor and add timer
    nros_executor_init(&executor, &support, 4);
    nros_executor_add_timer(&executor, &timer);

    signal(SIGINT, on_signal);
    printf("Publishing on /chatter (Ctrl+C to stop)...\n");

    // Spin with 100ms period
    nros_executor_spin_period(&executor, 100000000ULL);

    // Cleanup (reverse order)
    nros_executor_fini(&executor);
    nros_timer_fini(&timer);
    nros_publisher_fini(&publisher);
    nros_node_fini(&node);
    nros_support_fini(&support);
    return 0;
}
```

The C API lifecycle:

1. `nros_support_init()` — connect to the zenoh router
2. `nros_node_init()` — create a named node in a namespace
3. `nros_publisher_init()` — create a publisher with generated type support
4. `nros_timer_init()` — create a periodic timer with callback
5. `nros_executor_init()` + `nros_executor_add_timer()` — set up the executor
6. `nros_executor_spin_period()` — run the event loop
7. `nros_*_fini()` — clean up in reverse order

### 4. Build and Run

```bash
cd my-c-talker
cmake -B build -DCMAKE_PREFIX_PATH=~/.local/nano-ros
cmake --build build

# Terminal 1: zenohd
zenohd --listen tcp/127.0.0.1:7447

# Terminal 2: talker
./build/my_c_talker
```

## Listener

### CMakeLists.txt

Same structure — generate `std_msgs` and link `NanoRos::NanoRos`.

### src/main.c

```c
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <signal.h>

#include <nros/init.h>
#include <nros/node.h>
#include <nros/subscription.h>
#include <nros/executor.h>

#include "std_msgs.h"

static nros_support_t support;
static nros_node_t node;
static nros_subscription_t subscription;
static nros_executor_t executor;
static int message_count = 0;

static void on_signal(int sig) {
    (void)sig;
    nros_executor_stop(&executor);
}

static void subscription_callback(const uint8_t* data, size_t len, void* context) {
    (void)context;
    std_msgs_msg_int32 msg;
    std_msgs_msg_int32_init(&msg);

    if (std_msgs_msg_int32_deserialize(&msg, data, len) == 0) {
        message_count++;
        printf("Received [%d]: %d\n", message_count, msg.data);
    }
}

int main(void) {
    const char* locator = getenv("ZENOH_LOCATOR");
    if (!locator) locator = "tcp/127.0.0.1:7447";

    const char* domain_str = getenv("ROS_DOMAIN_ID");
    uint8_t domain_id = domain_str ? (uint8_t)atoi(domain_str) : 0;

    memset(&support, 0, sizeof(support));
    nros_support_init(&support, locator, domain_id);
    nros_node_init(&node, &support, "c_listener", "/");

    nros_subscription_init(&subscription, &node,
        std_msgs_msg_int32_get_type_support(), "/chatter",
        subscription_callback, NULL);

    nros_executor_init(&executor, &support, 4);
    nros_executor_add_subscription(&executor, &subscription,
        NROS_EXECUTOR_ON_NEW_DATA);

    signal(SIGINT, on_signal);
    printf("Waiting for messages on /chatter...\n");

    nros_executor_spin_period(&executor, 100000000ULL);

    printf("Total messages: %d\n", message_count);
    nros_executor_fini(&executor);
    nros_subscription_fini(&subscription);
    nros_node_fini(&node);
    nros_support_fini(&support);
    return 0;
}
```

Key differences from the Rust API:

- All structures are statically allocated (no heap).
- Subscription callbacks receive raw CDR bytes — use generated
  `*_deserialize()` functions.
- `NROS_EXECUTOR_ON_NEW_DATA` tells the executor to only invoke the
  callback when new data arrives.

## RMW Backend Selection

By default, `find_package(NanoRos)` links the zenoh variant. To use
XRCE-DDS instead, set `NANO_ROS_RMW` before configuring:

```bash
cmake -B build -DNANO_ROS_RMW=xrce -DCMAKE_PREFIX_PATH=~/.local/nano-ros
```

No source code changes are needed — the same C API works with both backends.

## Next Steps

- [ROS 2 Interop](ros2-interop.md) — verify with `ros2 topic echo`
- [C API Reference](../reference/c-api.md) — full function listing
