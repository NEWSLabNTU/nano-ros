# First App in Rust

This chapter walks through creating a pub/sub pair: a talker that publishes
`std_msgs/Int32` messages and a listener that receives them.

No git clone is needed — Cargo fetches nano-ros dependencies automatically.

## Prerequisites

- Rust nightly toolchain
- `cargo-nano-ros` installed (see [Installation](installation.md))
- zenohd running

## Talker

### 1. Create the Project

```bash
cargo new my-talker
cd my-talker
```

### 2. Cargo.toml

```toml
[package]
name = "my-talker"
version = "0.1.0"
edition = "2024"

[dependencies]
nros = { version = "0.1", default-features = false,
         features = ["std", "rmw-zenoh", "platform-posix"] }
std_msgs = { version = "*", default-features = false }
log = "0.4"
env_logger = "0.11"

[patch.crates-io]
nros = { git = "https://github.com/jerry73204/nano-ros" }
nros-core = { git = "https://github.com/jerry73204/nano-ros" }
nros-serdes = { git = "https://github.com/jerry73204/nano-ros" }
```

The `[dependencies]` section specifies the version normally. The
`[patch.crates-io]` section redirects Cargo to fetch from the git
repository until the crates are published to crates.io.

### 3. package.xml

nano-ros uses `package.xml` to declare message dependencies for code
generation:

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
cargo nano-ros generate --config --nano-ros-git
```

This creates:

- `generated/std_msgs/` — Rust types for `std_msgs::msg::Int32`, `String`, etc.
- `generated/builtin_interfaces/` — `Time`, `Duration` types
- `.cargo/config.toml` — `[patch.crates-io]` entries for the generated
  message crates (e.g., `std_msgs`, `builtin_interfaces`)

The `--nano-ros-git` flag ensures the generated patches use git references
matching the `[patch.crates-io]` entries in your `Cargo.toml`. The
`--config` flag writes the `.cargo/config.toml` file automatically.

### 5. Write the Publisher

Replace `src/main.rs`:

```rust
use log::info;
use nros::prelude::*;
use std_msgs::msg::Int32;

fn main() {
    env_logger::init();

    let config = ExecutorConfig::from_env().node_name("talker");
    let mut executor: Executor<_> =
        Executor::open(&config).expect("Failed to open session");

    let mut node = executor.create_node("talker").expect("Failed to create node");

    let publisher = node
        .create_publisher::<Int32>("/chatter")
        .expect("Failed to create publisher");

    info!("Publishing Int32 messages on /chatter...");

    let mut count: i32 = 0;
    loop {
        let msg = Int32 { data: count };
        publisher.publish(&msg).expect("Publish failed");
        info!("Published: {}", count);

        count = count.wrapping_add(1);
        executor.spin_once(10);
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
```

Key API elements:

- `ExecutorConfig::from_env()` reads `ZENOH_LOCATOR`, `ROS_DOMAIN_ID`, and
  `ZENOH_MODE` from the environment.
- `Executor<_>` uses env-var-configurable defaults (4 callback slots, 4 KB
  arena). Override with `NROS_EXECUTOR_MAX_CBS` and `NROS_EXECUTOR_ARENA_SIZE`.
- `create_node()` borrows the session from the executor.
- `create_publisher::<Int32>()` creates a typed publisher. The topic name
  follows ROS 2 conventions.
- `spin_once(10)` drives transport I/O with a 10 ms timeout.

### 6. Run

```bash
# Terminal 1: Start the zenoh router
zenohd --listen tcp/127.0.0.1:7447

# Terminal 2: Run the talker
RUST_LOG=info cargo run
```

## Listener

Create a second project alongside the talker.

### Setup

```bash
cargo new my-listener
cd my-listener
```

Use the same `Cargo.toml` dependencies, `package.xml`, and code generation
steps as the talker.

### src/main.rs

```rust
use log::info;
use nros::prelude::*;
use std_msgs::msg::Int32;

fn main() {
    env_logger::init();

    let config = ExecutorConfig::from_env().node_name("listener");
    let mut executor: Executor<_> =
        Executor::open(&config).expect("Failed to open session");

    let mut count: u64 = 0;
    executor
        .add_subscription::<Int32, _>("/chatter", move |msg, _info| {
            count += 1;
            info!("[{}] Received: data={}", count, msg.data);
        })
        .expect("Failed to add subscription");

    info!("Waiting for Int32 messages on /chatter...");

    executor.spin_blocking(SpinOptions::default())
        .expect("Spin error");
}
```

Key differences from the talker:

- `add_subscription()` registers a callback that the executor dispatches
  when data arrives. The closure captures `count` by move.
- `spin_blocking()` loops indefinitely, driving I/O and dispatching
  callbacks. Use `SpinOptions` to set a timeout or maximum callback count.

### Run

```bash
# Terminal 1: zenohd (already running)
# Terminal 2: talker (already running)

# Terminal 3: Run the listener
RUST_LOG=info cargo run
```

You should see the listener printing received messages:

```
[1] Received: data=0
[2] Received: data=1
...
```

## Two API Styles

The examples above show both API styles:

| Style           | Use case                       | Example                                        |
|-----------------|--------------------------------|------------------------------------------------|
| **Manual-poll** | Publisher loops, custom timing | `create_publisher()` + `spin_once()` in a loop |
| **Callback**    | Event-driven subscriptions     | `add_subscription()` + `spin_blocking()`       |

Both styles can be mixed in the same executor. Services and actions follow
the same pattern.

## Next Steps

- [First App in C](first-app-c.md) — the same example using the C API
- [ROS 2 Interop](ros2-interop.md) — verify messages with `ros2 topic echo`
- [Message Generation](../guides/message-generation.md) — use custom message
  types
