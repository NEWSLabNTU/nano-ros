# Getting Started {#getting_started}

## Prerequisites

- A C11 compiler (GCC, Clang, or arm-none-eabi-gcc)
- [CMake](https://cmake.org/) >= 3.15
- [Rust](https://rustup.rs/) nightly toolchain (needed to build `libnros_c.a`)
- zenohd router (build from submodule with `just build-zenohd`,
  or install a matching version from [zenoh releases](https://github.com/eclipse-zenoh/zenoh/releases))

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

set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     zenoh)
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

Generated type info structs (`nros_message_type_t`, `nros_service_type_t`,
`nros_action_type_t`) are defined in `<nros/types.h>`.

## 3. RMW Backend Selection

The `NANO_ROS_RMW` CMake variable selects which RMW staticlib to build
in-tree. Set it in `CMakeLists.txt` (BEFORE `add_subdirectory`) or
override on the command line (`-DNANO_ROS_RMW=xrce`). Valid: `zenoh`
(default), `dds`, `xrce`, `cyclonedds`.

## 4. Write a Publisher

Create `src/main.c`:

```c
#include <stdio.h>
#include <stdlib.h>
#include <signal.h>

#include <nros/init.h>
#include <nros/node.h>
#include <nros/publisher.h>
#include <nros/timer.h>
#include <nros/executor.h>

/* Use generated type info from nano_ros_generate_interfaces().
 * For this example we define a minimal Int32 manually. */
typedef struct { int32_t data; } std_msgs_Int32;

static const nros_message_type_t std_msgs_Int32_type = {
    .type_name = "std_msgs::msg::dds_::Int32_",
    .type_hash = "RIHS01_5bf22a2e7c2c8a4ca3f55054648f6d8c7c77cc0ae5695a1ff1df0b7ef8df1f09",
    .serialized_size_max = 8,
};

static int32_t serialize_int32(const std_msgs_Int32 *msg,
                               uint8_t *buf, size_t len) {
    if (len < 8) return -1;
    /* CDR header (little-endian) */
    buf[0] = 0x00; buf[1] = 0x01; buf[2] = 0x00; buf[3] = 0x00;
    /* int32 payload (little-endian) */
    buf[4] = (uint8_t)(msg->data);
    buf[5] = (uint8_t)(msg->data >> 8);
    buf[6] = (uint8_t)(msg->data >> 16);
    buf[7] = (uint8_t)(msg->data >> 24);
    return 8;
}

static nros_publisher_t *g_pub;
static int g_count = 0;

static void timer_cb(struct nros_timer_t *timer, void *ctx) {
    (void)timer; (void)ctx;
    g_count++;
    std_msgs_Int32 msg = { .data = g_count };
    uint8_t buf[64];
    int32_t len = serialize_int32(&msg, buf, sizeof(buf));
    if (len > 0 && nros_publish_raw(g_pub, buf, (size_t)len) == NROS_RET_OK) {
        printf("Published: %d\n", g_count);
    }
}

static volatile sig_atomic_t running = 1;
static nros_executor_t *g_exec;

static void on_signal(int sig) {
    (void)sig;
    running = 0;
    if (g_exec) nros_executor_stop(g_exec);
}

int main(void) {
    const char *locator = getenv("NROS_LOCATOR");
    if (!locator) locator = "tcp/127.0.0.1:7447";

    /* Initialise transport */
    nros_support_t support = nros_support_get_zero_initialized();
    if (nros_support_init(&support, locator, 0) != NROS_RET_OK) return 1;

    /* Create node */
    nros_node_t node = nros_node_get_zero_initialized();
    nros_node_init(&node, &support, "c_talker", "/");

    /* Create publisher */
    nros_publisher_t pub = nros_publisher_get_zero_initialized();
    nros_publisher_init(&pub, &node, &std_msgs_Int32_type, "/chatter");
    g_pub = &pub;

    /* Create 1 Hz timer */
    nros_timer_t timer = nros_timer_get_zero_initialized();
    nros_timer_init(&timer, &support, 1000000000ULL, timer_cb, NULL);

    /* Create executor and add timer */
    nros_executor_t exec = nros_executor_get_zero_initialized();
    nros_executor_init(&exec, &support, 4);
    nros_executor_add_timer(&exec, &timer);
    g_exec = &exec;

    signal(SIGINT, on_signal);
    printf("Publishing on /chatter (Ctrl+C to stop)...\n");
    nros_executor_spin_period(&exec, 100000000ULL);

    /* Tear down in reverse order */
    nros_executor_fini(&exec);
    nros_timer_fini(&timer);
    nros_publisher_fini(&pub);
    nros_node_fini(&node);
    nros_support_fini(&support);
    return 0;
}
```

## 5. Build and Run

```bash
mkdir build && cd build
cmake ..
make

# Terminal 1: start zenoh router
zenohd --listen tcp/127.0.0.1:7447

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
