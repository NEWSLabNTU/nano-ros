# A Zephyr App You Build with West

<!-- type: how-to · level: 2 -->

> **Support level:** [`native_sim`](../../reference/boards/zephyr-native-sim.md). Each page states what nano-ros can show for the board (where it runs, how often, and its breadth), generated from the board support registry; [Board Support Levels](../../reference/board-support-levels.md) explains the words.

In this chapter you add a ROS 2 talker to a Zephyr application that you
already build with `west build`, run it on `native_sim` against a ROS 2
router, and then flash the same application to a real board. The application
keeps its own `CMakeLists.txt` and `prj.conf`; nano-ros joins it as one more
Zephyr module.

**What this assumes:** a Linux host with ROS 2 Humble and its Zenoh
middleware (`ros-humble-rmw-zenoh-cpp`), the [install](../../start/install.md)
done so `nros` is on your PATH, and some familiarity with west: a manifest,
`west update`, `west build -b <board>`.

**What it leaves for later:** a project with several nodes, several boards, or
a launch file. That is a nano-ros workspace, which the
[Zephyr chapter](zephyr.md) builds.

## When to choose this

There are two ways to put a ROS 2 node on Zephyr.

- **A nano-ros workspace** (`nros new my_ws --board zephyr`). Your code is node
  packages; a bringup package says which board each image runs on.
  `nros build` writes the Zephyr application for you and runs `west build`
  on it, and `nros run` starts it. You write no `CMakeLists.txt` for Zephyr
  and no `prj.conf`. Start there if you are starting fresh, or if the project
  will grow past one node. The [Zephyr chapter](zephyr.md) covers it.
- **Your own west application** (this chapter). The Zephyr application
  already exists, owns its build, and has its own drivers, devicetree and
  `prj.conf`. You want a ROS 2 node inside it, and you want `west build` and
  `west flash` to stay the commands you type.

The second road has no bringup and no `system.toml`. Everything nano-ros needs
to know about the image comes from `prj.conf`, the way any Zephyr module is
configured.

## Set up the host and the west workspace

Zephyr's host tools come first: `west` and the build tools
[Zephyr's Getting Started](https://docs.zephyrproject.org/3.7.0/develop/getting_started/index.html)
lists. `nros setup` installs no system packages. On Debian or Ubuntu:

```bash probe=30 track=installed variant=west-app,west-app-cpp,west-app-rust,west-app-flash
sudo apt-get install -y --no-install-recommends git cmake ninja-build gperf \
    device-tree-compiler python3-pip python3-venv xz-utils file wget
pip3 install --user west
export PATH="$HOME/.local/bin:$PATH"
```

Provision the Zephyr SDK and the zenoh transport sources nano-ros compiles,
then point Zephyr at that SDK:

```bash probe=32 track=installed variant=west-app,west-app-cpp,west-app-rust,west-app-flash
nros setup zephyr --rmw zenoh
export ZEPHYR_SDK_INSTALL_DIR="$(nros locate tool zephyr-sdk)"
```

`nros locate tool zephyr-sdk` prints where `nros setup` installed the Zephyr
SDK version this release of nano-ros pins, so the line keeps working when that
version changes. If you already have a Zephyr SDK that your workspace builds
with, keep using it: set `ZEPHYR_SDK_INSTALL_DIR` to that one instead, or
register it with the SDK's own `setup.sh -c`.

If you already have a west workspace, skip the next block. This one pins
Zephyr 3.7 and fetches only the modules nano-ros's boards use:

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

The last project, `zephyr-lang-rust`, is Zephyr's Rust support, at the
revision nano-ros builds with on Zephyr 3.7. Only a
[Rust application](#a-rust-application) uses it; leave it out if you write
none.

nano-ros supports both the Zephyr 3.7 LTS line and the 4.x line; the
[version table](zephyr.md#which-zephyr--37-lts-and-4x-both-supported) lists
what differs between them.

## Add nano-ros to the build

nano-ros's Zephyr module ships inside your nano-ros install. `nros locate
nano-ros` prints the directory that holds it, and you hand that directory to
west as an extra module:

```bash
west build -b native_sim/native/64 apps/my_app -- \
    -DZEPHYR_EXTRA_MODULES="$(nros locate nano-ros)"
```

You do not add nano-ros to `west.yml`, and nothing is cloned. The command
needs no network, so it is also safe inside your own scripts and CI.

If you would rather not type the option on every build, ask for the path once
in the application's `CMakeLists.txt`, before `find_package(Zephyr)`:

```cmake
execute_process(COMMAND nros locate nano-ros
    OUTPUT_VARIABLE NROS_MODULE_DIR
    OUTPUT_STRIP_TRAILING_WHITESPACE
    COMMAND_ERROR_IS_FATAL ANY)
list(APPEND ZEPHYR_EXTRA_MODULES "${NROS_MODULE_DIR}")
```

With either form, a plain `west build` picks the module up.

## Write the application

The application is a stock Zephyr application with four files:

```text
my_zephyr_ws/
├── zephyr/
├── modules/
└── apps/
    └── my_app/
        ├── CMakeLists.txt     # find_package(Zephyr), then the node
        ├── package.xml        # the message packages the node uses
        ├── prj.conf           # Zephyr's settings and nano-ros's
        └── src/
            └── Talker.c       # the node
```

### The node, in C

The node is a **component**: a struct that holds its state and a `configure`
function that creates its publishers, subscriptions and timers. You do not
write `main()`. The module generates the application's entry point, which
initialises the RMW session, creates the node and executor, calls your
`configure`, and then spins the executor.

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

It publishes a counter as `std_msgs/Int32` on `/chatter` once a second.
`RCLC_C_COMPONENT` registers the struct and its `configure` function with the
generated entry point. The rest is the ROS 2 `rclc` API.

### The build file

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

`find_package(nano_ros)` and `find_package(std_msgs)` are provided by the
module. `nano_ros_auto_add_library` compiles your node source.
`nros_components_register_node` registers the component, and `DEPLOY zephyr`
is what makes the module generate the entry point into Zephyr's `app` target.
Your application's other sources still go into `app` with `target_sources`,
as before; nothing else about the file changes.

### The C++ form

The same node in C++ is a class with a `configure` member. In place of
`Talker.c`, write its header and its source:

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

`::setvbuf` turns off stdout's buffering, so each line shows as soon as it is
printed. It is the global one because Zephyr's C++ library declares it only
there.

The build file differs from the C one in two places: the project declares
C++, and the registration names the class's header.

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

Everything after this point is the same for the C and the C++ node.

## Configure

nano-ros reads its settings from Kconfig, so they go in `prj.conf` beside
Zephyr's own, and `west build -t menuconfig` shows them under
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

The nano-ros lines, one by one:

| Setting | What it chooses |
|---|---|
| `CONFIG_NROS=y` | Turns the module on. Without it nano-ros adds nothing to the build. |
| `CONFIG_NROS_CPP_API=y` | The runtime library the generated entry point links. The entry point is C++ even when your node is C, so a C or C++ node both set this. `CONFIG_NROS_RUST_API` is for a Rust application ([below](#a-rust-application)). |
| `CONFIG_STD_CPP17=y` | The C++ standard the entry point compiles with. |
| `CONFIG_NROS_RMW_ZENOH=y` | The middleware: `CONFIG_NROS_RMW_ZENOH`, `CONFIG_NROS_RMW_XRCE` or `CONFIG_NROS_RMW_CYCLONEDDS`. |
| `CONFIG_NROS_ZENOH_LOCATOR` | Where the zenoh router is. `tcp/127.0.0.1:7447` is the default and suits `native_sim`, which uses the host's sockets. |
| `CONFIG_NROS_DOMAIN_ID` | The ROS 2 domain. It must match the `ROS_DOMAIN_ID` of the nodes you talk to. |

`CONFIG_NROS_C_API` is for an application that calls the C API
(`rclc/*.h`) from its own `main` and registers no component. An application
that registers its node with `nros_components_register_node(... DEPLOY zephyr)`
is driven by the generated entry point, which is C++, so it sets
`CONFIG_NROS_CPP_API=y` even when the node is C.

For XRCE-DDS, set `CONFIG_NROS_RMW_XRCE=y` and name the agent with
`CONFIG_NROS_XRCE_AGENT_ADDR` and `CONFIG_NROS_XRCE_AGENT_PORT` instead of a
locator. Each middleware also needs some Zephyr settings of its own;
[What each RMW's Zephyr-native fragment needs](zephyr.md#what-each-rmws-zephyr-native-fragment-needs)
lists them.

Every other nano-ros setting (executor slots, buffer sizes, the zenoh
session's limits) is a Kconfig symbol too, set the same way. The
[Kconfig reference](../../internals/platform/zephyr-in-tree.html#kconfig-reference)
lists them with their defaults. Do not set `CONFIG_NROS_ROS_EDITION`: there
is no such symbol, and Zephyr stops the build on a symbol it does not know.

On Zephyr 4.x you can also select the middleware on the build line with one of
the module's snippets, `-S nros-zenoh`, `-S nros-xrce` or `-S nros-cyclonedds`,
instead of the `CONFIG_NROS_RMW_*` line
([Zephyr 4.x: select the RMW with a snippet](zephyr.md#zephyr-4x-select-the-rmw-with-a-snippet)).

## Messages

You do not run a generator. The build generates the C or C++ types for every
message package the application depends on, and `package.xml` is where you
list those packages, the same file a ROS 2 package uses:

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
`CMakeLists.txt`. The C node then includes `<std_msgs/std_msgs.h>` and the C++
node `"std_msgs.hpp"`.

The message definitions come from the first of these that has the package:
the directories in `NROS_INTERFACE_SEARCH_PATH`, then a sourced ROS 2
install (`AMENT_PREFIX_PATH`), then copies nano-ros carries of a few
standard packages. Those copies are one ROS 2 distro's, so build from a shell
that has sourced your ROS 2 install (`source /opt/ros/humble/setup.bash`):
the types then match your ROS 2 peers. A Rust application's `nros configure`
([below](#a-rust-application)) does not fall back to the copies at all. For
your own messages, keep them in a ROS 2 interface
package (its `package.xml` and `msg/` directory) anywhere on disk, add the
directory that holds it to `NROS_INTERFACE_SEARCH_PATH`, and depend on it like
any other.

## Build

From the top of the west workspace, build for `native_sim`:

```bash probe=50 track=installed variant=west-app,west-app-cpp,west-app-rust
west build -b native_sim/native/64 -S nros-nsos apps/my_app -- \
    -DZEPHYR_EXTRA_MODULES="$(nros locate nano-ros)"
```

`native_sim` builds the application as a program for your machine, so there
is no emulator. The `nros-nsos` snippet routes its sockets to the host's
(Zephyr's NSOS). Without it, `native_sim` tries to create a TAP interface
(`eth_posix: Cannot create zeth`), which needs privileges an ordinary user or
a container does not have, and the talker never reaches the router.

## Run on native_sim

ROS 2 provides the router. Start it on the port the talker dials:

```bash probe=60 probe-run=60 probe-keep track=installed variant=west-app,west-app-cpp,west-app-rust
source /opt/ros/humble/setup.bash
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false' \
    ros2 run rmw_zenoh_cpp rmw_zenohd
```

```text probe-expect=60 variant=west-app,west-app-cpp,west-app-rust
Started Zenoh router with id
```

With the router running, in another terminal, start the talker:

```bash probe=70 probe-run=120 track=installed variant=west-app,west-app-cpp,west-app-rust
./build/zephyr/zephyr.exe
```

```text probe-expect=70 variant=west-app,west-app-cpp,west-app-rust
Published: 1
Published: 2
```

`west build -t run` does the same. To see the messages from ROS 2, in a third
terminal:

```bash
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32 --qos-reliability best_effort
```

The talker publishes best-effort. `ros2 topic echo` asks for reliable delivery
unless told otherwise, and then receives nothing.

## Flash a real board

The board here is ST's Nucleo-F767ZI (Zephyr's `nucleo_f767zi`). Its
ST-LINK debugger is also a USB serial port, so the talker can reach the router
over that cable, with no network on the board at all. Zenoh calls this a
serial link.

Provision the board, and add its HAL, `hal_stm32`, to the west workspace.
`nros setup board` applies nano-ros's changes for the board to the workspace:

```bash probe=52 track=installed variant=west-app-flash
nros setup nucleo-f767zi-zephyr --rmw zenoh
sed -i 's/net-tools]/net-tools, hal_stm32]/' manifest/west.yml
west update --narrow -o=--depth=1
nros setup board nucleo-f767zi-zephyr --zephyr-workspace "$PWD"
```

Zephyr reads `boards/<board>.conf` and `boards/<board>.overlay` from the
application directory after `prj.conf`, for that board only, so the native_sim
build stays as it is. The settings turn the IP stack off, carry zenoh over
USART3 (the ST-LINK port), move the console to USART6 (pins D0 and D1, for a
USB-to-UART adapter), and size the image for the board's 384 KiB of RAM:

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

Build for the board, into its own build directory:

```bash probe=56 track=installed variant=west-app-flash
west build -p always -b nucleo_f767zi -d build-board apps/my_app -- \
    -DZEPHYR_EXTRA_MODULES="$(nros locate nano-ros)"
```

Zephyr warns that `prj.conf`'s networking settings (`NET_IPV4`, `NET_TCP`
and the rest) were assigned `y` but got `n`. That is expected: the board file
turned networking off, and the warnings do not stop the build.

Plug the board in, start the router on its serial port (the ST-LINK port is
usually `/dev/ttyACM0`), and flash it with Zephyr's own runner:

```bash
source /opt/ros/humble/setup.bash
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["serial//dev/ttyACM0#baudrate=115200"];scouting/multicast/enabled=false' \
    ros2 run rmw_zenoh_cpp rmw_zenohd
```

```bash
west flash -d build-board
```

`ros2 topic echo`, as on native_sim, then shows the counter:

```text
data: 1
---
data: 2
---
```

Without the board, the same image runs in the board's emulator: the probe
for this chapter boots `build-board/zephyr/zephyr.elf` in Renode's
`nucleo_f767zi` machine, with USART3 on a host serial port, instead of
flashing it, and checks that `ros2 topic echo` hears it. The flash runner
(OpenOCD, pyOCD, J-Link and so on) and the probe are Zephyr's and your
bench's; nano-ros adds nothing to them.

To use the board's Ethernet instead, leave out the serial lines: keep
`CONFIG_NETWORKING=y` with the board's Ethernet driver and an address, and set
`CONFIG_NROS_ZENOH_LOCATOR` to the router's address on that network, for
example `"tcp/192.0.2.10:7447"`, with the router listening on
`tcp/0.0.0.0:7447`.

## A Rust application

A Rust application is built by Zephyr's Rust support (zephyr-lang-rust,
the last project in the [manifest above](#set-up-the-host-and-the-west-workspace)):
its `CMakeLists.txt` calls `rust_cargo_application()`, which builds the
application's crate with cargo and links it into the image. The node is the
same talker, written as a nano-ros Rust component.

zephyr-lang-rust needs three things a C application does not: the Rust target
the Zephyr board implies (`x86_64-unknown-none` for `native_sim/native/64` on
an x86_64 host), rustup's `clippy` (it lints every Rust application as part of
the build), and clang with libclang, for the bindings it generates from
Zephyr's headers. `nros setup board` adds the target and applies nano-ros's
changes to the west workspace, without which zephyr-lang-rust's cargo command
carries none of nano-ros's settings for the board. From the top of the west
workspace:

```bash probe=37 track=installed variant=west-app-rust
. "$HOME/.cargo/env"     # the cargo the install set up
sudo apt-get install -y --no-install-recommends clang libclang-dev
rustup component add clippy
nros setup board zephyr --zephyr-workspace "$PWD"
```

The application is a crate whose library is named `rustapp` and built as a
static library: `rust_cargo_application()` links `librustapp.a`, whatever the
package is called. In place of the C application's source and build file,
write the crate, its build script and its node:

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

Write `prj.conf` and `package.xml` as for the C application
([Configure](#configure), [Messages](#messages)), then change two lines of
`prj.conf`: a Rust application selects `CONFIG_NROS_RUST_API` (which needs
`CONFIG_RUST`, Zephyr's switch for zephyr-lang-rust) instead of
`CONFIG_NROS_CPP_API`, and needs no C++ standard:

```bash probe=47 track=installed variant=west-app-rust
sed -i -e '/^CONFIG_NROS_CPP_API=y$/d' -e '/^CONFIG_STD_CPP17=y$/d' apps/my_app/prj.conf
cat >> apps/my_app/prj.conf <<'EOF'
CONFIG_RUST=y
CONFIG_NROS_RUST_API=y
EOF
``` The other settings stay as they are. `build.rs` hands the Kconfig
values to the crate, among them the locator and the domain, so they are still
set in `prj.conf` alone.

The crate names the nano-ros crates and `std_msgs` but does not say where they
come from. `nros configure`, run in the application directory, generates the
Rust types for the message packages `package.xml` lists, under `generated/`,
and writes `.cargo/config.toml`, which points the nano-ros crates at your
nano-ros install and the message crates at `generated/`. Run it before the
first `west build`, and again when `package.xml` changes. It generates the
types from your ROS 2 install, so source it first; the configure records the
prefix, and a later `nros configure` of the application needs no sourced
shell:

```bash probe=49 track=installed variant=west-app-rust
source /opt/ros/humble/setup.bash
cd apps/my_app && nros configure && cd ../..
```

From there the application builds and runs as the C one does: the same
[`west build` line](#build), and the same
[run on native_sim](#run-on-native_sim). The log line reads
`<inf> nros: rustapp: Published: 1`, from Zephyr's logger.

## If it doesn't work

**`Could not find a package configuration file provided by "nano_ros"`.**
The module is not in the build. Check that `nros locate nano-ros` prints a
directory, and that the build line passes it as `-DZEPHYR_EXTRA_MODULES`
after the `--`.

**`attempt to assign the value ... to the undefined symbol NROS_...`.** The
symbol is misspelled or does not exist. Search for it in
`west build -t menuconfig` (`/` searches). `CONFIG_NROS_ROS_EDITION` is a
common one: it does not exist.

**No `Published:` line within 30 seconds.**

1. Check that the router is running and listening on the port in
   `CONFIG_NROS_ZENOH_LOCATOR`.
2. On `native_sim`, check that the build line has `-S nros-nsos`. A line
   `eth_posix: Cannot create zeth` in the output means it did not.
3. Check `CONFIG_NETWORKING=y`, `CONFIG_NET_IPV4=y` and `CONFIG_NET_TCP=y` in
   `prj.conf`. Zephyr's networking is opt-in.

**A message with an unbounded `string` or sequence field fails to generate,
asking for a bound.** nano-ros sizes its buffers at build time, so a field
with no stated maximum needs one. Put it in an `nros-codegen.toml` beside
`package.xml`:

```toml
[fields]
"std_msgs/String.data" = 256
```

**The Zephyr 4.x line says `Could NOT find Python3 ... required is at least
"3.12"`.** Zephyr 4.x needs Python 3.12 or newer for west and its build
scripts.

For anything else, [Troubleshooting: First 10 Minutes](../../reference/troubleshooting-index.md)
walks through the common failures.

## Design background

A nano-ros project is normally a workspace, and its bringup's `system.toml`
says what runs where. An application that owns its own build stays outside
that model on purpose (RFC-0121 D6): nano-ros joins the build the way any
library would, and asks the build for nothing back. For Zephyr, that
application is a **guest** (RFC-0085 D14): it loads nano-ros as a module,
states the middleware, locator and domain in Kconfig, and builds with west
alone. With no `system.toml` beside it, the module gives every nano-ros
setting its Kconfig prompt, so `prj.conf`, `menuconfig` and `-DCONFIG_*` are
the one source of its configuration.

- [RFC-0121: workspace-first project layout](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0121-workspace-first-project-layout.md)
- [RFC-0085: Zephyr workspace and west handoff](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0085-zephyr-workspace-and-west-handoff.md)
