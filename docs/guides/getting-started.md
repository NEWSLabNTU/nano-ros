# Getting Started with nros

This guide walks you through creating your first nros application in Rust and C.

## Prerequisites

- [Rust](https://rustup.rs/) (nightly toolchain, edition 2024)
- [ROS 2 Humble](https://docs.ros.org/en/humble/Installation.html) (for message generation)
- zenohd 1.6.2 router

### Building zenohd

nros includes zenohd 1.6.2 as a git submodule:

```bash
git clone https://github.com/jerry73204/nano-ros.git
cd nros
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
zenoh = ["nros/zenoh"]

[dependencies]
nros = { git = "https://github.com/jerry73204/nano-ros", default-features = false, features = ["std"] }
std_msgs = { version = "*", default-features = false }
log = "0.4"
env_logger = "0.11"
```

### 3. Create `package.xml`

nros uses `package.xml` to declare which ROS 2 message types you need:

```xml
<?xml version="1.0"?>
<package format="3">
  <name>my_talker</name>
  <version>0.1.0</version>
  <description>My first nros talker</description>
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
cargo install --git https://github.com/jerry73204/nano-ros --path packages/codegen/packages/cargo-nano-ros

# Source ROS 2 environment
source /opt/ros/humble/setup.bash

# Generate bindings with git-based patches
cargo nano-ros generate --config --nano-ros-git
```

This creates:
- `generated/std_msgs/` - Rust types for `std_msgs::msg::Int32`, `String`, etc.
- `generated/builtin_interfaces/` - Time, Duration types
- `.cargo/config.toml` - Patch entries pointing to generated code and nros git

### 5. Write the Publisher

Replace `src/main.rs`:

```rust
use log::info;
use nros::prelude::*;
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

### 1. Build the nros C Library

```bash
cd /path/to/nros
cargo build -p nros-c --release
# Library at: target/release/libnros_c.a
# Headers at: packages/core/nros-c/include/
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

# Point to nros repository
list(APPEND CMAKE_MODULE_PATH "/path/to/nros/cmake")
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

#include <nros/init.h>
#include <nros/node.h>
#include <nros/publisher.h>
#include <nros/timer.h>
#include <nros/executor.h>

// Manual Int32 message definition
// (auto-generated in full projects via CMake nano_ros_generate_interfaces)
typedef struct { int32_t data; } std_msgs_Int32;

static const nros_message_type_t std_msgs_Int32_type = {
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

static nros_publisher_t* g_pub;
static int g_count = 0;

static void timer_cb(struct nros_timer_t* timer, void* ctx) {
    (void)timer; (void)ctx;
    g_count++;
    std_msgs_Int32 msg = { .data = g_count };
    uint8_t buf[64];
    int32_t len = serialize_int32(&msg, buf, sizeof(buf));
    if (len > 0 && nros_publish_raw(g_pub, buf, (size_t)len) == NANO_ROS_RET_OK) {
        printf("Published: %d\n", g_count);
    }
}

static volatile sig_atomic_t running = 1;
static nros_executor_t* g_exec;

static void on_signal(int sig) {
    (void)sig;
    running = 0;
    if (g_exec) nros_executor_stop(g_exec);
}

int main(void) {
    const char* locator = getenv("ZENOH_LOCATOR");
    if (!locator) locator = "tcp/127.0.0.1:7447";

    nros_support_t support = nros_support_get_zero_initialized();
    if (nros_support_init(&support, locator, 0) != NANO_ROS_RET_OK) return 1;

    nros_node_t node = nros_node_get_zero_initialized();
    nros_node_init(&node, &support, "c_talker", "/");

    nros_publisher_t pub = nros_publisher_get_zero_initialized();
    nros_publisher_init(&pub, &node, &std_msgs_Int32_type, "/chatter");
    g_pub = &pub;

    nros_timer_t timer = nros_timer_get_zero_initialized();
    nros_timer_init(&timer, &support, 1000000000ULL, timer_cb, NULL);

    nros_executor_t exec = nros_executor_get_zero_initialized();
    nros_executor_init(&exec, &support, 4);
    nros_executor_add_timer(&exec, &timer);
    g_exec = &exec;

    signal(SIGINT, on_signal);
    printf("Publishing on /chatter (Ctrl+C to stop)...\n");
    nros_executor_spin_period(&exec, 100000000ULL);

    nros_executor_fini(&exec);
    nros_timer_fini(&timer);
    nros_publisher_fini(&pub);
    nros_node_fini(&node);
    nros_support_fini(&support);
    return 0;
}
```

### 4. Build and Run

```bash
mkdir build && cd build
cmake -DNANO_ROS_ROOT=/path/to/nros ..
make

# Terminal 1: zenohd
/path/to/zenohd --listen tcp/127.0.0.1:7447

# Terminal 2: talker
./my_c_talker
```

## Configuration

### Runtime Environment Variables

| Variable        | Description                                   | Default              |
|-----------------|-----------------------------------------------|----------------------|
| `ROS_DOMAIN_ID` | ROS 2 domain ID                               | `0`                  |
| `ZENOH_LOCATOR` | Router address (e.g., `tcp/192.168.1.1:7447`) | `tcp/127.0.0.1:7447` |
| `ZENOH_MODE`    | Session mode: `client` or `peer`              | `client`             |

### Build-Time Buffer Tuning

nros uses platform-appropriate defaults for transport buffer sizes. Desktop
(`platform-posix`) builds use larger defaults; embedded (`platform-bare-metal`,
`platform-zephyr`) builds use smaller defaults to fit in constrained memory.

Override defaults by setting environment variables before `cargo build`:

**Zenoh backend (`rmw-zenoh`):**

| Variable                     | Description                                        | Posix Default | Embedded Default |
|------------------------------|----------------------------------------------------|---------------|------------------|
| `ZPICO_FRAG_MAX_SIZE`        | Max reassembled message size after defragmentation | `65536`       | `2048`           |
| `ZPICO_BATCH_UNICAST_SIZE`   | Max unicast batch size before fragmentation        | `65536`       | `1024`           |
| `ZPICO_BATCH_MULTICAST_SIZE` | Max multicast batch size                           | `8192`        | `1024`           |

**XRCE-DDS backend (`rmw-xrce`):**

| Variable             | Description                                                                                | Posix Default | Embedded Default |
|----------------------|--------------------------------------------------------------------------------------------|---------------|------------------|
| `XRCE_TRANSPORT_MTU` | Custom transport MTU; also sizes reliable stream buffers (4 × MTU) and UDP staging buffers | `4096`        | `512`            |

Example — increase zenoh defrag to 128 KB for large point clouds:

```bash
ZPICO_FRAG_MAX_SIZE=131072 cargo build --features rmw-zenoh,platform-posix
```

## Service Calls with Promise API

nros provides a non-blocking, allocation-free Promise API for service calls.
`client.call(&request)` returns a `Promise<Reply>` immediately — the reply
can be polled with `try_recv()` or `.await`ed in an async context.

### Two Usage Patterns

**Pattern 1: Sync polling (no async runtime needed)**

The simplest approach — call `spin_once()` in a loop to drive I/O while
polling the promise with `try_recv()`:

```rust
use nros::prelude::*;
use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};

fn main() {
    let config = ExecutorConfig::from_env().node_name("client");
    let mut executor = Executor::<_, 4, 4096>::open(&config).unwrap();
    let mut node = executor.create_node("client").unwrap();
    let mut client = node.create_client::<AddTwoInts>("/add_two_ints").unwrap();

    // call() sends the request and returns a Promise immediately
    let mut promise = client.call(&AddTwoIntsRequest { a: 1, b: 2 }).unwrap();

    // Drive I/O and poll for the reply
    let reply = loop {
        executor.spin_once(10);  // drives I/O, dispatches callbacks
        if let Ok(Some(reply)) = promise.try_recv() {
            break reply;
        }
    };
    println!("sum = {}", reply.sum);
}
```

**Pattern 2: Background spin task (recommended for async)**

Spawn `spin_async()` as a background task in your async runtime, then
`.await` promises directly. This works because `ServiceClient` is an owned
type — after creating the client, the executor can be moved to a background task.

*Desktop with tokio:*

```rust
use nros::prelude::*;
use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let config = ExecutorConfig::from_env().node_name("client");
    let mut executor = Executor::<_, 4, 4096>::open(&config).unwrap();

    // Create client (owned — no lifetime tied to node or executor)
    let mut client = {
        let mut node = executor.create_node("client").unwrap();
        node.create_client::<AddTwoInts>("/add_two_ints").unwrap()
    }; // node dropped, executor free to move

    let local = tokio::task::LocalSet::new();
    local.run_until(async move {
        // Background spin (same thread, cooperative)
        tokio::task::spawn_local(async move {
            executor.spin_async().await;
        });

        // Sequential calls — just .await the Promise
        let reply = client.call(&AddTwoIntsRequest { a: 1, b: 2 }).unwrap().await.unwrap();
        println!("sum = {}", reply.sum);
    }).await;
}
```

*Zephyr with Embassy (`executor-zephyr` — kernel-backed waking):*

```rust
type NrosExecutor = nros::Executor<nros::RmwSession, 0, 0>;

#[embassy_executor::task]
async fn spin_task(mut exec: NrosExecutor) -> ! {
    exec.spin_async().await
}

#[embassy_executor::task]
async fn app_main(spawner: embassy_executor::Spawner) {
    let config = nros::ExecutorConfig::new("tcp/192.0.2.2:7447");
    let mut nros_exec = nros::Executor::<_, 0, 0>::open(&config).unwrap();
    let mut client = {
        let mut node = nros_exec.create_node("client").unwrap();
        node.create_client::<AddTwoInts>("/add_two_ints").unwrap()
    };
    spawner.spawn(spin_task(nros_exec)).unwrap();

    let reply = client.call(&AddTwoIntsRequest { a: 1, b: 2 }).unwrap().await.unwrap();
}
```

### Async Dependencies

The Promise and `spin_async()` APIs use only `core::future` — no external
runtime dependency. They work on `no_std`/`no_alloc` targets.

nros does **not** provide an async runtime. Use an external crate:
- **Desktop**: `tokio` (`current_thread` + `spawn_local`)
- **Zephyr**: `zephyr` crate with `executor-zephyr` feature (kernel-backed `k_sem` waking)
- **Bare-metal**: `embassy-executor` with arch-specific feature (`arch-cortex-m`, etc.)

For async combinators (`select`, `join`), add `embassy-futures`:

```toml
[dependencies]
embassy-futures = "0.1"  # no_std, no_alloc, runtime-agnostic
```

See `examples/native/rust/zenoh/async-service/` (tokio) and
`examples/zephyr/rust/zenoh/async-service/` (Embassy) for complete working examples.

## Next Steps

- Browse the [examples/](../examples/) directory for more patterns (services, actions, subscribers)
- See [Message Generation](message-generation.md) for generating bindings for custom message types
- See [ROS 2 Interop](rmw_zenoh_interop.md) for details on the rmw_zenoh protocol
- See [Embedded Integration](embedded-integration.md) for bare-metal and RTOS targets
