# Getting Started with nano-ros

This guide walks you through creating your first nano-ros application in Rust and C.

## Prerequisites

- [Rust](https://rustup.rs/) (nightly toolchain, edition 2024)
- [ROS 2 Humble](https://docs.ros.org/en/humble/Installation.html) (for message generation)
- zenohd 1.6.2 router

### Building zenohd

nano-ros includes zenohd 1.6.2 as a git submodule:

```bash
git clone https://github.com/jerry73204/nano-ros.git
cd nano-ros
just build-zenohd
# Binary at: ./build/zenohd/zenohd
```

Or install zenohd 1.6.2 from [zenoh releases](https://github.com/eclipse-zenoh/zenoh/releases).

## Rust

### 1. Create a New Project

```bash
cargo new my-talker
cd my-talker
```

### 2. Add Dependencies

Edit `Cargo.toml`:

```toml
[package]
name = "my-talker"
version = "0.1.0"
edition = "2024"

[features]
default = []
zenoh = ["nano-ros/zenoh"]

[dependencies]
nano-ros = { git = "https://github.com/jerry73204/nano-ros", default-features = false, features = ["std"] }
std_msgs = { version = "*", default-features = false }
log = "0.4"
env_logger = "0.11"
```

### 3. Create `package.xml`

nano-ros uses `package.xml` to declare which ROS 2 message types you need:

```xml
<?xml version="1.0"?>
<package format="3">
  <name>my_talker</name>
  <version>0.1.0</version>
  <description>My first nano-ros talker</description>
  <maintainer email="you@example.com">Your Name</maintainer>
  <license>MIT</license>
  <depend>std_msgs</depend>
  <export>
    <build_type>ament_cargo</build_type>
  </export>
</package>
```

### 4. Generate Message Bindings

```bash
# Install the binding generator (one-time)
cargo install --git https://github.com/jerry73204/nano-ros --path colcon-nano-ros/packages/cargo-nano-ros

# Source ROS 2 environment
source /opt/ros/humble/setup.bash

# Generate bindings with git-based patches
cargo nano-ros generate --config --nano-ros-git
```

This creates:
- `generated/std_msgs/` - Rust types for `std_msgs::msg::Int32`, `String`, etc.
- `generated/builtin_interfaces/` - Time, Duration types
- `.cargo/config.toml` - Patch entries pointing to generated code and nano-ros git

### 5. Write the Publisher

Replace `src/main.rs`:

```rust
use log::info;
use nano_ros::prelude::*;
use std_msgs::msg::Int32;

fn main() {
    env_logger::init();

    let context = Context::from_env().expect("Failed to create context");
    let mut executor = context.create_basic_executor();

    let node = executor
        .create_node("talker".namespace("/"))
        .expect("Failed to create node");

    let publisher = node
        .create_publisher::<Int32>(PublisherOptions::new("/chatter"))
        .expect("Failed to create publisher");

    info!("Publishing Int32 messages on /chatter...");

    let mut count: i32 = 0;
    loop {
        let msg = Int32 { data: count };
        publisher.publish(&msg).expect("Publish failed");
        info!("Published: {}", count);

        count = count.wrapping_add(1);
        let _ = executor.spin_once(1000);
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
```

### 6. Build and Run

```bash
# Terminal 1: Start zenoh router
./path/to/zenohd --listen tcp/127.0.0.1:7447

# Terminal 2: Run the talker
RUST_LOG=info cargo run --features zenoh
```

You should see messages being published. To verify with ROS 2:

```bash
# Terminal 3: ROS 2 listener
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32 --qos-reliability best_effort
```

## C API

### 1. Build the nano-ros C Library

```bash
cd /path/to/nano-ros
cargo build -p nano-ros-c --release
# Library at: target/release/libnano_ros_c.a
# Headers at: crates/nano-ros-c/include/
```

### 2. Create a CMake Project

Create a directory for your C project:

```bash
mkdir my-c-talker
cd my-c-talker
```

Create `CMakeLists.txt`:

```cmake
cmake_minimum_required(VERSION 3.15)
project(my_c_talker LANGUAGES C)

set(CMAKE_C_STANDARD 11)

# Point to nano-ros repository
list(APPEND CMAKE_MODULE_PATH "/path/to/nano-ros/cmake")
find_package(NanoRos REQUIRED)

add_executable(my_c_talker src/main.c)
target_link_libraries(my_c_talker PRIVATE NanoRos::NanoRos)
```

### 3. Write the Publisher

Create `src/main.c`:

```c
#include <stdio.h>
#include <stdlib.h>
#include <signal.h>

#include <nano_ros/init.h>
#include <nano_ros/node.h>
#include <nano_ros/publisher.h>
#include <nano_ros/timer.h>
#include <nano_ros/executor.h>

// Manual Int32 message definition
// (auto-generated in full projects via CMake nano_ros_generate_interfaces)
typedef struct { int32_t data; } std_msgs_Int32;

static const nano_ros_message_type_t std_msgs_Int32_type = {
    .type_name = "std_msgs::msg::dds_::Int32_",
    .type_hash = "RIHS01_5bf22a2e7c2c8a4ca3f55054648f6d8c7c77cc0ae5695a1ff1df0b7ef8df1f09",
    .serialized_size_max = 8,
};

static int32_t serialize_int32(const std_msgs_Int32* msg, uint8_t* buf, size_t len) {
    if (len < 8) return -1;
    buf[0] = 0x00; buf[1] = 0x01; buf[2] = 0x00; buf[3] = 0x00;
    buf[4] = (uint8_t)(msg->data);
    buf[5] = (uint8_t)(msg->data >> 8);
    buf[6] = (uint8_t)(msg->data >> 16);
    buf[7] = (uint8_t)(msg->data >> 24);
    return 8;
}

static nano_ros_publisher_t* g_pub;
static int g_count = 0;

static void timer_cb(struct nano_ros_timer_t* timer, void* ctx) {
    (void)timer; (void)ctx;
    g_count++;
    std_msgs_Int32 msg = { .data = g_count };
    uint8_t buf[64];
    int32_t len = serialize_int32(&msg, buf, sizeof(buf));
    if (len > 0 && nano_ros_publish_raw(g_pub, buf, (size_t)len) == NANO_ROS_RET_OK) {
        printf("Published: %d\n", g_count);
    }
}

static volatile sig_atomic_t running = 1;
static nano_ros_executor_t* g_exec;

static void on_signal(int sig) {
    (void)sig;
    running = 0;
    if (g_exec) nano_ros_executor_stop(g_exec);
}

int main(void) {
    const char* locator = getenv("ZENOH_LOCATOR");
    if (!locator) locator = "tcp/127.0.0.1:7447";

    nano_ros_support_t support = nano_ros_support_get_zero_initialized();
    if (nano_ros_support_init(&support, locator, 0) != NANO_ROS_RET_OK) return 1;

    nano_ros_node_t node = nano_ros_node_get_zero_initialized();
    nano_ros_node_init(&node, &support, "c_talker", "/");

    nano_ros_publisher_t pub = nano_ros_publisher_get_zero_initialized();
    nano_ros_publisher_init(&pub, &node, &std_msgs_Int32_type, "/chatter");
    g_pub = &pub;

    nano_ros_timer_t timer = nano_ros_timer_get_zero_initialized();
    nano_ros_timer_init(&timer, &support, 1000000000ULL, timer_cb, NULL);

    nano_ros_executor_t exec = nano_ros_executor_get_zero_initialized();
    nano_ros_executor_init(&exec, &support, 4);
    nano_ros_executor_add_timer(&exec, &timer);
    g_exec = &exec;

    signal(SIGINT, on_signal);
    printf("Publishing on /chatter (Ctrl+C to stop)...\n");
    nano_ros_executor_spin_period(&exec, 100000000ULL);

    nano_ros_executor_fini(&exec);
    nano_ros_timer_fini(&timer);
    nano_ros_publisher_fini(&pub);
    nano_ros_node_fini(&node);
    nano_ros_support_fini(&support);
    return 0;
}
```

### 4. Build and Run

```bash
mkdir build && cd build
cmake -DNANO_ROS_ROOT=/path/to/nano-ros ..
make

# Terminal 1: zenohd
/path/to/zenohd --listen tcp/127.0.0.1:7447

# Terminal 2: talker
./my_c_talker
```

## Next Steps

- Browse the [examples/](../examples/) directory for more patterns (services, actions, subscribers)
- See [Message Generation](message-generation.md) for generating bindings for custom message types
- See [ROS 2 Interop](rmw_zenoh_interop.md) for details on the rmw_zenoh protocol
- See [Embedded Integration](embedded-integration.md) for bare-metal and RTOS targets
