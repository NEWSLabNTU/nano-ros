# A Zephyr app you build with west

<!-- type: how-to · level: 2 -->

**Needs first:** [Run a talker on an emulated board](../../tutorial/run-a-talker.md), [What you just built](../../tutorial/what-you-just-built.md), [Get started on Zephyr native_sim](../../starters/zephyr-native-sim.md)

Use this page to add a ROS 2 talker to a Zephyr application you already build
with `west build`, run it on `native_sim` against a ROS 2 router, and build the
same application for a real board. The application keeps its own
`CMakeLists.txt` and `prj.conf`; nano-ros joins it as one more Zephyr module.
The steps run on the `native_sim` board; its status is on
[its support page](../../reference/boards/zephyr-native-sim.md).

## Before you start

Choose this road when the Zephyr application already exists, owns its build,
and has its own drivers, devicetree and `prj.conf`, and you want `west build`
and `west flash` to stay the commands you type. It has no bringup and no
`system.toml`: nano-ros reads everything about the image from `prj.conf`, as
any Zephyr module does. For a new project, or one that will grow past one
node, use a nano-ros workspace instead, where `nros build` writes the Zephyr
application for you: [Zephyr native_sim](zephyr-native-sim.md).

You need a Linux host with ROS 2 and `ros-<distro>-rmw-zenoh-cpp`
([which edition](../../start/install.md#install-ros-2-first)), the
[install](../../start/install.md) done, and Zephyr's host tools, `west` and
the build tools
[Zephyr's Getting Started](https://docs.zephyrproject.org/3.7.0/develop/getting_started/index.html)
lists. On Debian or Ubuntu:

```bash probe=30 track=installed variant=west-app,west-app-cpp,west-app-rust,west-app-flash
sudo apt-get install -y --no-install-recommends git cmake ninja-build gperf \
    device-tree-compiler python3-pip python3-venv xz-utils file wget
pip3 install --user west
export PATH="$HOME/.local/bin:$PATH"
```

## Set up the west workspace

Provision the Zephyr SDK and the zenoh sources nano-ros compiles, and point
Zephyr at that SDK. If your workspace already builds with a Zephyr SDK of its
own, set `ZEPHYR_SDK_INSTALL_DIR` to that one instead:

```bash probe=32 track=installed variant=west-app,west-app-cpp,west-app-rust,west-app-flash
nros setup zephyr --rmw zenoh
export ZEPHYR_SDK_INSTALL_DIR="$(nros locate tool zephyr-sdk)"
```

If you already have a west workspace, skip the next block. This one pins
Zephyr 3.7 and fetches only the modules nano-ros's boards use. Its last
project, `zephyr-lang-rust`, is Zephyr's Rust support; leave it out if you
write no Rust application:

```bash probe=34 track=installed variant=west-app,west-app-cpp,west-app-rust,west-app-flash
mkdir -p my_zephyr_ws/manifest && cd my_zephyr_ws
cat > manifest/west.yml <<'WEST_YML'
manifest:
  remotes:
    - name: zephyrproject-rtos
      url-base: https://github.com/zephyrproject-rtos
  projects:
    - name: zephyr
      remote: zephyrproject-rtos
      revision: v3.7.0
      import:
        name-allowlist: [cmsis, picolibc, mbedtls, tinycrypt, mcuboot, net-tools]
    - name: zephyr-lang-rust
      remote: zephyrproject-rtos
      revision: 404fcefdbab0f2711cd342a1fab9d2eda52a1ba9
      path: modules/lang/rust
  self:
    path: manifest
WEST_YML
west init -l manifest
west update --narrow -o=--depth=1
pip3 install --user -r zephyr/scripts/requirements-base.txt
export ZEPHYR_BASE="$PWD/zephyr"
```

## Add nano-ros to the build

nano-ros's Zephyr module ships inside your nano-ros install, and
`nros locate nano-ros` prints the directory that holds it. Hand that directory
to west as an extra module, on the build line after `--`:
`-DZEPHYR_EXTRA_MODULES="$(nros locate nano-ros)"`, as the
[Build](#build) step does. Nothing is added to `west.yml` and nothing is
cloned. To avoid typing the option, ask for the path once in the application's
`CMakeLists.txt`, before `find_package(Zephyr)`:

```cmake
execute_process(COMMAND nros locate nano-ros
    OUTPUT_VARIABLE NROS_MODULE_DIR
    OUTPUT_STRIP_TRAILING_WHITESPACE
    COMMAND_ERROR_IS_FATAL ANY)
list(APPEND ZEPHYR_EXTRA_MODULES "${NROS_MODULE_DIR}")
```

## Write the application

The application is a stock Zephyr application under `apps/my_app/`: its build
file, `package.xml` (the message packages the node uses), `prj.conf` and the
node source. The talker publishes a counter as `std_msgs/Int32` on `/chatter`
once a second.

In C and C++ the node is a **component**: a struct or class that holds its
state, and a `configure` function that creates its publishers and timers. You
write no `main()`: the module generates the entry point, which opens the RMW
session, creates the node and executor, calls `configure`, and spins. In Rust
the node is a nano-ros Rust component, built by zephyr-lang-rust.

<!-- tabs -->

#### C

```bash probe=40 track=installed variant=west-app,west-app-flash
mkdir -p apps/my_app/src
cat > apps/my_app/src/Talker.c <<'EOF'
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <rclc/rclc.h>
#include <std_msgs/std_msgs.h>

typedef struct {
    rcl_publisher_t pub;
    rcl_timer_t timer;
    int32_t count;
} talker_t;

static void on_tick(rcl_timer_t* timer, int64_t last_call_time) {
    talker_t* self = RCLC_CONTAINER_OF(timer, talker_t, timer);
    (void)last_call_time;
    std_msgs__msg__Int32 msg = {.data = self->count};
    if (rcl_publish(&self->pub, &msg, NULL) == RCL_RET_OK) {
        printf("Published: %d\n", (int)self->count);
    }
    self->count++;
}

static rcl_ret_t talker_configure(rcl_node_t* node, rclc_executor_t* executor,
                                  talker_t* self) {
    setvbuf(stdout, NULL, _IONBF, 0);
    self->count = 0;
    self->pub = rcl_get_zero_initialized_publisher();
    rcl_ret_t rc = rclc_publisher_init_default(
        &self->pub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32), "/chatter");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    self->timer = rcl_get_zero_initialized_timer();
    rc = rclc_timer_init_default(&self->timer, NULL, 1000ULL * 1000000ULL, on_tick);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    return rclc_executor_add_timer(executor, &self->timer);
}

RCLC_C_COMPONENT(talker_t, talker_configure)
EOF
```

`RCLC_C_COMPONENT` registers the struct and its `configure` function with the
generated entry point; the rest is the ROS 2 `rclc` API. The build file:

```bash probe=42 track=installed variant=west-app,west-app-flash
cat > apps/my_app/CMakeLists.txt <<'EOF'
cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(my_app)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_auto_add_library(talker_lib STATIC src/Talker.c)

nros_components_register_node(talker_lib
    PLUGIN my_app::Talker
    EXECUTABLE talker
    SHAPE configure
    TYPED
    DEPLOY zephyr)
EOF
```

`find_package(nano_ros)` and `find_package(std_msgs)` come from the module.
`nros_components_register_node(... DEPLOY zephyr)` registers the component,
and makes the module generate the entry point into Zephyr's `app` target.
Your application's other sources still go into `app` with `target_sources`.

#### C++

```bash probe=41 track=installed variant=west-app-cpp
mkdir -p apps/my_app/src
cat > apps/my_app/src/Talker.hpp <<'EOF'
#pragma once
#include <rclcpp/rclcpp.hpp>
#include "std_msgs.hpp"

namespace my_app {
class Talker {
    ::rclcpp::Publisher<std_msgs::msg::Int32> pub_;
    ::rclcpp::Timer timer_;
    int count_ = 0;
    void on_tick();

  public:
    ::rclcpp::Result configure(::rclcpp::Node& node);
};
}  // namespace my_app
EOF
```

```bash probe=43 track=installed variant=west-app-cpp
cat > apps/my_app/src/Talker.cpp <<'EOF'
#include "Talker.hpp"
#include <cstdio>

namespace my_app {
void Talker::on_tick() {
    std_msgs::msg::Int32 m;
    m.data = count_;
    if (pub_.publish(m).ok()) {
        std::printf("Published: %d\n", count_);
    }
    ++count_;
}

::rclcpp::Result Talker::configure(::rclcpp::Node& node) {
    ::setvbuf(stdout, nullptr, _IONBF, 0);
    ::rclcpp::Result r = node.create_publisher(pub_, "/chatter");
    if (!r.ok()) return r;
    return node.create_wall_timer<Talker, &Talker::on_tick>(timer_, 1000, this);
}
}  // namespace my_app
EOF
```

`::setvbuf` turns off stdout's buffering; it is the global one because
Zephyr's C++ library declares it only there. The build file declares C++,
names the class's header, and registers the component with
`nros_components_register_node(... DEPLOY zephyr)`, which makes the module
generate the entry point into Zephyr's `app` target:

```bash probe=45 track=installed variant=west-app-cpp
cat > apps/my_app/CMakeLists.txt <<'EOF'
cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(my_app LANGUAGES C CXX)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_auto_add_library(talker_lib STATIC src/Talker.cpp)

nros_components_register_node(talker_lib
    HEADER Talker.hpp
    PLUGIN my_app::Talker
    EXECUTABLE talker
    SHAPE configure
    TYPED
    DEPLOY zephyr)
EOF
```

#### Rust

zephyr-lang-rust builds the application: its `CMakeLists.txt` calls
`rust_cargo_application()`, which builds the crate with cargo and links it.
It needs the Rust target the board implies, rustup's `clippy`, and clang with
libclang. `nros setup board` adds the target and applies nano-ros's changes
to the west workspace. From the top of the workspace:

```bash probe=37 track=installed variant=west-app-rust
. "$HOME/.cargo/env"     # the cargo the install set up
sudo apt-get install -y --no-install-recommends clang libclang-dev
rustup component add clippy
```

The crate's library must be named `rustapp` and built as a static library:
`rust_cargo_application()` links `librustapp.a`. Write the crate, its build
script and its node:

```bash probe=39 track=installed variant=west-app-rust
mkdir -p apps/my_app/src
cat > apps/my_app/CMakeLists.txt <<'EOF'
cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(my_app)

rust_cargo_application()
EOF
cat > apps/my_app/Cargo.toml <<'EOF'
[package]
name = "my_app"
version = "0.1.0"
edition = "2021"

[lib]
name = "rustapp"
crate-type = ["staticlib"]

[dependencies]
zephyr = "0.1.0"
log = "0.4"
nros = { version = "*", default-features = false, features = ["alloc", "rmw-cffi", "macros"] }
nros-platform = { version = "*", default-features = false, features = ["platform-zephyr"] }
nros-rmw-zenoh = { version = "*", default-features = false, features = ["platform-zephyr"] }
std_msgs = { version = "*", default-features = false }

[build-dependencies]
zephyr-build = "0.1.0"
nros-zephyr-build = "*"

[profile.release]
opt-level = "s"
lto = true
debug = false
EOF
cat > apps/my_app/build.rs <<'EOF'
fn main() {
    zephyr_build::export_kconfig_bool_options();
    nros_zephyr_build::bake_nros_config();
}
EOF
cat > apps/my_app/src/lib.rs <<'EOF'
#![no_std]

extern crate zephyr;

use nros::{
    Callback, CallbackCtx, Component, DeclarativeNode, ExecutableNode, NodeContext, NodeOptions,
    NodeResult, TimerDuration,
};
use std_msgs::msg::Int32;

pub struct Talker;

impl Component for Talker {
    const NAME: &'static str = "talker";
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(1, 0, 0, 0, 0);

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("talker"))?;
        let chatter = node.create_publisher_for_topic::<Int32>("/chatter")?;
        let _timer =
            node.create_timer_for_callback_name("on_tick", TimerDuration::from_millis(1000))?;
        node.callback_for_name("on_tick").publishes_entity(&chatter)?;
        Ok(())
    }
}

impl ExecutableNode for Talker {
    type State = i32;
    const HAS_TICK: bool = false;

    fn init() -> Self::State {
        0
    }

    fn on_callback(count: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_tick" {
            let msg = Int32 { data: *count };
            if ctx.publish_to_topic::<Int32, 16>("/chatter", &msg).is_ok() {
                log::info!("Published: {}", *count);
            }
            *count += 1;
        }
    }
}

nros::node!(Talker);

// Keeps the zenoh backend in the static library, then generates the
// application's entry point around the component.
nros::force_link_backend!(nros_rmw_zenoh);
nros::zephyr_component_main!(crate::Talker);
EOF
```

<!-- /tabs -->

## Configure

nano-ros reads its settings from Kconfig, so they go in `prj.conf` beside
Zephyr's own; `west build -t menuconfig` shows them under
**nros ROS 2 client library**:

```bash probe=44 track=installed variant=west-app,west-app-cpp,west-app-rust,west-app-flash
cat > apps/my_app/prj.conf <<'EOF'
# Zephyr: console and networking
CONFIG_LOG=y
CONFIG_PRINTK=y
CONFIG_NETWORKING=y
CONFIG_NET_IPV4=y
CONFIG_NET_UDP=y
CONFIG_NET_TCP=y
CONFIG_NET_SOCKETS=y
CONFIG_NET_L2_ETHERNET=y
CONFIG_NET_MAX_CONTEXTS=16
CONFIG_DNS_RESOLVER=n
CONFIG_SYS_CLOCK_TICKS_PER_SEC=1000

# zenoh-pico runs on POSIX threads and wants room
CONFIG_POSIX_API=y
CONFIG_MAX_PTHREAD_MUTEX_COUNT=64
CONFIG_MAX_PTHREAD_COND_COUNT=16
CONFIG_POSIX_THREAD_THREADS_MAX=16
CONFIG_MAIN_STACK_SIZE=16384
CONFIG_HEAP_MEM_POOL_SIZE=65536
CONFIG_SYSTEM_WORKQUEUE_STACK_SIZE=4096
CONFIG_NET_PKT_RX_COUNT=32
CONFIG_NET_PKT_TX_COUNT=32
CONFIG_NET_BUF_RX_COUNT=64
CONFIG_NET_BUF_TX_COUNT=64

# nano-ros
CONFIG_NROS=y
CONFIG_NROS_CPP_API=y
CONFIG_STD_CPP17=y
CONFIG_NROS_RMW_ZENOH=y
CONFIG_NROS_ZENOH_LOCATOR="tcp/127.0.0.1:7447"
CONFIG_NROS_DOMAIN_ID=0
EOF
```

| Setting | What it chooses |
|---|---|
| `CONFIG_NROS=y` | Turns the module on. |
| `CONFIG_NROS_CPP_API=y` | The library the generated entry point links. The entry point is C++, so a C node sets it too. |
| `CONFIG_STD_CPP17=y` | The C++ standard the entry point compiles with. |
| `CONFIG_NROS_RMW_ZENOH=y` | The middleware: `_ZENOH`, `_XRCE` or `_CYCLONEDDS`. |
| `CONFIG_NROS_ZENOH_LOCATOR` | Where the router is. The default, `tcp/127.0.0.1:7447`, suits `native_sim`. |
| `CONFIG_NROS_DOMAIN_ID` | The ROS 2 domain, matching your peers' `ROS_DOMAIN_ID`. |

`CONFIG_NROS_C_API` is for an application that calls the C API from its own
`main()` and registers no component. For XRCE-DDS, set
`CONFIG_NROS_RMW_XRCE=y` and name the agent with `CONFIG_NROS_XRCE_AGENT_ADDR`
and `CONFIG_NROS_XRCE_AGENT_PORT`. Each middleware's own Zephyr settings are
listed in [One image per RMW](zephyr-native-sim.md#one-image-per-rmw). Every
other nano-ros setting is a Kconfig symbol too, listed in the module's
[`Kconfig`](https://github.com/NEWSLabNTU/nano-ros/blob/main/zephyr/Kconfig).
On Zephyr 4.x, the module's snippets (`-S nros-zenoh`, `-S nros-xrce`,
`-S nros-cyclonedds`) select the middleware on the build line instead.

## Messages

You run no generator. The build generates the types for every message package
`package.xml` lists, the same file a ROS 2 package uses:

```bash probe=46 track=installed variant=west-app,west-app-cpp,west-app-rust,west-app-flash
cat > apps/my_app/package.xml <<'EOF'
<?xml version="1.0"?>
<package format="3">
  <name>my_app</name>
  <version>0.1.0</version>
  <description>A Zephyr application with a ROS 2 talker</description>
  <maintainer email="you@example.com">You</maintainer>
  <license>Apache-2.0</license>

  <depend>std_msgs</depend>

  <export>
    <build_type>nros_cmake</build_type>
  </export>
</package>
EOF
```

Each `<depend>` also gets a `find_package(<pkg> REQUIRED)` line in
`CMakeLists.txt`. The definitions come from the first place that has the
package: the directories in `NROS_INTERFACE_SEARCH_PATH`, then a sourced ROS 2
install (`AMENT_PREFIX_PATH`). Sourcing ROS 2 is required: if neither names a
package, the configure stops and says so, as `nros configure` does. nano-ros's
own copies of a few standard packages are read only if you add the directory
the refusal names to `NROS_INTERFACE_SEARCH_PATH`, and ROS 2 still comes
first. For your own messages, put their interface package anywhere on disk
and add its parent directory to `NROS_INTERFACE_SEARCH_PATH`.

### Finish a Rust application

A Rust application writes `package.xml` and `prj.conf` as above, then swaps
the C++ API for the Rust one in `prj.conf`. `build.rs` hands the Kconfig
values, among them the locator and the domain, to the crate:

```bash probe=47 track=installed variant=west-app-rust
sed -i -e '/^CONFIG_NROS_CPP_API=y$/d' -e '/^CONFIG_STD_CPP17=y$/d' apps/my_app/prj.conf
cat >> apps/my_app/prj.conf <<'EOF'
CONFIG_RUST=y
CONFIG_NROS_RUST_API=y
EOF
```

`nros configure`, run in the application directory, generates the Rust types
for the message packages `package.xml` lists under `generated/`, and writes
`.cargo/config.toml` pointing the nano-ros crates at your install. Run it
before the first `west build`, and again when `package.xml` changes:

```bash probe=49 track=installed variant=west-app-rust
source /opt/ros/humble/setup.bash
cd apps/my_app && nros configure && cd ../..
```

The application then builds and runs as the C one does. Its log line reads
`<inf> nros: rustapp: Published: 1`.

## Build

From the top of the west workspace, build for `native_sim`:

```bash probe=50 track=installed variant=west-app,west-app-cpp,west-app-rust
west build -b native_sim/native/64 -S nros-nsos apps/my_app -- \
    -DZEPHYR_EXTRA_MODULES="$(nros locate nano-ros)"
```

`native_sim` builds the application as a program for your machine. The
`nros-nsos` snippet routes its sockets to the host's (Zephyr's NSOS); without
it, `native_sim` tries to create a TAP interface, which needs privileges an
ordinary user or a container lacks.

## Run on native_sim

Start the router ROS 2 provides, on the port the talker dials:

```bash probe=60 probe-run=60 probe-keep track=installed variant=west-app,west-app-cpp,west-app-rust
source /opt/ros/humble/setup.bash
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false' \
    ros2 run rmw_zenoh_cpp rmw_zenohd
```

```text probe-expect=60 variant=west-app,west-app-cpp,west-app-rust
Started Zenoh router with id
```

In another terminal, start the talker (`west build -t run` does the same):

```bash probe=70 probe-run=120 track=installed variant=west-app,west-app-cpp,west-app-rust
./build/zephyr/zephyr.exe
```

```text probe-expect=70 variant=west-app,west-app-cpp,west-app-rust
Published: 1
Published: 2
```

To see the messages from ROS 2, run
`ros2 topic echo /chatter std_msgs/msg/Int32 --qos-reliability best_effort`
in a third terminal, with ROS 2 sourced and `RMW_IMPLEMENTATION=rmw_zenoh_cpp`.
The talker publishes best-effort, and an echo that asks for reliable delivery
receives nothing.

## Build for a real board

The board here is ST's Nucleo-F767ZI (Zephyr's `nucleo_f767zi`). Its ST-LINK
debugger is also a USB serial port, so the talker reaches the router over
that cable with no network on the board: a zenoh serial link. Provision the
board and add its HAL, `hal_stm32`, to the west workspace:

```bash probe=52 track=installed variant=west-app-flash
nros setup nucleo-f767zi-zephyr --rmw zenoh
sed -i 's/net-tools]/net-tools, hal_stm32]/' manifest/west.yml
west update --narrow -o=--depth=1
nros setup board nucleo-f767zi-zephyr --zephyr-workspace "$PWD"
```

Zephyr reads `boards/<board>.conf` and `boards/<board>.overlay` from the
application directory after `prj.conf`, for that board only, so the
`native_sim` build stays as it is. These turn the IP stack off, carry zenoh
over USART3, move the console to USART6 (pins D0 and D1), and size the image
for the board's 384 KiB of RAM:

```bash probe=54 track=installed variant=west-app-flash
mkdir -p apps/my_app/boards
cat > apps/my_app/boards/nucleo_f767zi.conf <<'EOF'
# zenoh over USART3, the ST-LINK USB serial port; no IP stack
CONFIG_NETWORKING=n
CONFIG_NROS_TRANSPORT_SERIAL=y
CONFIG_NROS_ZENOH_LOCATOR="serial/serial@40004800#baudrate=115200"
CONFIG_SERIAL=y
CONFIG_UART_INTERRUPT_DRIVEN=y
CONFIG_RING_BUFFER=y
CONFIG_MAIN_THREAD_PRIORITY=5

# no network driver brings up the RNG, so zenoh's session id uses this one
CONFIG_TEST_RANDOM_GENERATOR=y

# 384 KiB of RAM
CONFIG_HEAP_MEM_POOL_SIZE=16384
CONFIG_NROS_MAX_LARGE_SUBSCRIBERS=0
EOF
cat > apps/my_app/boards/nucleo_f767zi.overlay <<'EOF'
/ {
	chosen {
		zephyr,console = &usart6;
		zephyr,shell-uart = &usart6;
	};
};
EOF
```

Build into the board's own directory:

```bash probe=56 track=installed variant=west-app-flash
west build -p always -b nucleo_f767zi -d build-board apps/my_app -- \
    -DZEPHYR_EXTRA_MODULES="$(nros locate nano-ros)"
```

Zephyr warns that `prj.conf`'s networking settings were assigned `y` but got
`n`. That is expected: the board file turned networking off.

To flash it, start the router on the board's serial port, with
`listen/endpoints=["serial//dev/ttyACM0#baudrate=115200"]` in place of the TCP
endpoint above, then run Zephyr's own `west flash -d build-board`; the runner
and probe are Zephyr's and your bench's.
[Flash a real board](flash-a-real-board.md) covers the board, its port and
its twin. The tracks that replay this page boot `build-board/zephyr/zephyr.elf`
in the board's Renode twin instead, with USART3 on a host serial port. For the
board's ethernet, keep `CONFIG_NETWORKING=y` with its ethernet driver and an
address, and set `CONFIG_NROS_ZENOH_LOCATOR` to the router's TCP address.

## If it doesn't work

- **`Could not find a package configuration file provided by "nano_ros"`:**
  the module is not in the build. Check that the build line passes
  `-DZEPHYR_EXTRA_MODULES` after the `--`.
- **`attempt to assign the value ... to the undefined symbol NROS_...`:** the
  symbol does not exist; search `west build -t menuconfig` with `/`.
  `CONFIG_NROS_ROS_EDITION` is a common one.
- **No `Published:` line within 30 seconds:** check the router listens on the
  locator's port, the build line has `-S nros-nsos`, and `prj.conf` turns on
  `CONFIG_NETWORKING`, `CONFIG_NET_IPV4` and `CONFIG_NET_TCP`.
- **A message with an unbounded `string` or sequence fails to generate:**
  give the field a bound in an `nros-codegen.toml` beside `package.xml`:

  ```toml
  [fields]
  "std_msgs/String.data" = 256
  ```

- **Everything else:** [Troubleshoot](../deploy/troubleshoot.md).

## Design background

An application that owns its build stays outside the workspace model on
purpose: for Zephyr it is a guest that loads nano-ros as a module and states
the middleware, locator and domain in Kconfig, so `prj.conf`, `menuconfig` and
`-DCONFIG_*` are its one source of configuration.

- [RFC-0121: workspace-first project layout](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0121-workspace-first-project-layout.md) (D6)
- [RFC-0085: Zephyr workspace and west handoff](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0085-zephyr-workspace-and-west-handoff.md) (D14)
