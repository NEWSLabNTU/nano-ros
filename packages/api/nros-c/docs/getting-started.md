# Getting Started {#getting_started}

## Prerequisites

- A C11 compiler (GCC, Clang, or arm-none-eabi-gcc)
- [CMake](https://cmake.org/) >= 3.15
- [Rust](https://rustup.rs/) nightly toolchain (needed to build `libnros_c.a`)
- the zenoh router ROS ships, `rmw_zenohd` (`ros2 run rmw_zenoh_cpp rmw_zenohd`;
  from the nano-ros source tree, `just native zenohd` starts the same router).
  nano-ros vendors none (RFC-0075).

## 1. Create a CMake Project

```bash
mkdir my-c-talker && cd my-c-talker
mkdir src
```

Create `CMakeLists.txt` (Phase 140 — `add_subdirectory` is the only
consumption shape):

```cmake
cmake_minimum_required(VERSION 3.22)
project(my_c_talker LANGUAGES C)

set(CMAKE_C_STANDARD 11)

set(NROS_PLATFORM posix)
set(NROS_RMW     zenoh)
add_subdirectory(/path/to/nano-ros nano_ros)

add_executable(my_c_talker src/main.c)
target_link_libraries(my_c_talker PRIVATE NanoRos::NanoRos)
nros_platform_link_app(my_c_talker)
```

`NanoRos::NanoRos` provides include directories, the static library, and
platform link libraries (pthread, dl, m) automatically via the root CMake's
INTERFACE target. The nros-c static library is built in-tree per-project
via Corrosion.

## 2. Code Generation

Use `nano_ros_generate_interfaces()` to generate C message types. This
CMake function becomes available automatically once `add_subdirectory(nano-ros)`
runs (the root CMake includes `cmake/NanoRosGenerateInterfaces.cmake`).
The codegen tool (`nros-codegen`) is a Corrosion-built target reachable via
`$<TARGET_FILE:nros-codegen>`.

**Never hand-write CDR serialization or struct definitions** — always use
the code generator.

```cmake
# Standard ROS 2 package (resolved from bundled interfaces)
nano_ros_generate_interfaces(std_msgs
    "msg/Int32.msg"
    SKIP_INSTALL
)

# Service definitions
nano_ros_generate_interfaces(example_interfaces
    "srv/AddTwoInts.srv"
    SKIP_INSTALL
)

# Custom project-local interfaces
nano_ros_generate_interfaces(${PROJECT_NAME}
    "msg/Temperature.msg"
    SKIP_INSTALL
)
```

Resolution order for each file:
1. `${CMAKE_CURRENT_SOURCE_DIR}/<file>` (project-local)
2. `${AMENT_PREFIX_PATH}/share/<target>/<file>` (ROS 2 install)
3. `<install_prefix>/share/nano-ros/interfaces/<target>/<file>` (bundled)

Generated type info structs (`rclc_message_type_t`, `rclc_service_type_t`,
`rclc_action_type_t`) are defined in `<nros/types.h>`.

## 3. RMW Backend Selection

The `NROS_RMW` CMake variable selects which RMW staticlib to build
in-tree. Set it in `CMakeLists.txt` (BEFORE `add_subdirectory`) or
override on the command line (`-DNROS_RMW=xrce`). Valid: `zenoh`
(default), `dds`, `xrce`, `cyclonedds`.

## 4. Write a Publisher

Generated C messages take rosidl's names and member shape
(`std_msgs__msg__Int32` in `<std_msgs/msg/int32.h>`; a string member is
`rosidl_runtime_c__String {data, size, capacity}`). The type support is
rosidl's macro, and publishing is `rcl_publish`:

Create `src/main.c`:

```c
#include <stdio.h>
#include <stdlib.h>
#include <signal.h>

#include <rcl/rcl.h>
#include <rclc/rclc.h>
#include <rclc/executor.h>
#include <std_msgs/msg/int32.h>

static rcl_publisher_t g_pub;
static std_msgs__msg__Int32 g_msg;

static void timer_cb(struct rcl_timer_t *timer, void *ctx) {
    (void)timer; (void)ctx;
    g_msg.data++;
    /* Serialised through the descriptor the publisher was created with, into
     * a transmit buffer bounded at build time. Nothing allocates. */
    if (rcl_publish(&g_pub, &g_msg, NULL) == RCL_RET_OK) {
        printf("Published: %d\n", (int)g_msg.data);
    }
}

static volatile sig_atomic_t running = 1;
static rclc_executor_t *g_exec;

static void on_signal(int sig) {
    (void)sig;
    running = 0;
    if (g_exec) rclc_executor_cancel(g_exec);
}

int main(void) {
    const char *locator = getenv("NROS_LOCATOR");
    if (!locator) locator = "tcp/127.0.0.1:7447";

    rclc_support_t support = rclc_support_get_zero_initialized();
    if (rclc_support_init(&support, locator, 0) != RCL_RET_OK) return 1;

    rcl_node_t node = rcl_get_zero_initialized_node();
    if (rclc_node_init_default(&node, "c_talker", "/", &support) != RCL_RET_OK) return 1;

    g_pub = rcl_get_zero_initialized_publisher();
    if (rclc_publisher_init_default(&g_pub, &node,
                                    ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32),
                                    "/chatter") != RCL_RET_OK) return 1;

    rcl_timer_t timer = rcl_get_zero_initialized_timer();
    rcl_timer_init(&timer, &support, 1000000000ULL, timer_cb, NULL);

    rclc_executor_t exec = rclc_executor_get_zero_initialized_executor();
    rclc_executor_init(&exec, &support, 4);
    rclc_executor_add_timer(&exec, &timer);
    g_exec = &exec;

    signal(SIGINT, on_signal);
    printf("Publishing on /chatter (Ctrl+C to stop)...\n");
    rclc_executor_spin_period(&exec, 100000000ULL);

    rclc_executor_fini(&exec);
    rcl_timer_fini(&timer);
    rcl_publisher_fini(&g_pub);
    rcl_node_fini(&node);
    rclc_support_fini(&support);
    return 0;
}
```

A message with strings or sequences needs memory for them, and you own it.
Either point each `data` at your own buffer (the micro-ROS idiom:
`msg.data.data = buf; msg.data.capacity = sizeof buf;`) or wire the whole
message with the generated helper:

```c
static std_msgs__msg__String__Storage storage;
std_msgs__msg__String msg;
std_msgs__msg__String__init_in(&msg, &storage);
```

rosidl's allocating `<Msg>__init` / `__fini` / `__create` / `__destroy` are
not declared. A subscription delivers into storage the same way:
`rclc_executor_add_subscription_with_context(&exec, &sub, &msg, cb, ctx,
RCLC_EXECUTOR_ON_NEW_DATA)` deserialises into `msg` and refuses a sample
that does not fit its capacities.

## 5. Build and Run

```bash
mkdir build && cd build
cmake ..
make

# Terminal 1: start zenoh router
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false' ros2 run rmw_zenoh_cpp rmw_zenohd

# Terminal 2: run the talker
./my_c_talker
```

## System Install

Phase 140 — there is no system install for nano-ros. Your user
project owns whatever install layout it needs for *its* binaries.

## Zephyr Integration

For Zephyr RTOS, enable the C API in `prj.conf`:

```ini
# Enable nros C API
CONFIG_NROS=y
CONFIG_NROS_C_API=y

# Select RMW backend
CONFIG_NROS_RMW_ZENOH=y           # Zenoh (default)
# CONFIG_NROS_RMW_XRCE=y          # Or XRCE-DDS
# CONFIG_NROS_XRCE_AGENT_ADDR="127.0.0.1"
# CONFIG_NROS_XRCE_AGENT_PORT=2018
```

Zenoh requires `CONFIG_POSIX_API=y`. See existing examples in
`examples/zephyr/c/` for complete `prj.conf` templates.
