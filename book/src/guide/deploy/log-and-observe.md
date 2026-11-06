# Log and observe a board

<!-- type: how-to · level: 2 -->

**Needs first:** [Run a talker](../../tutorial/run-a-talker.md)

Use this page to log from a node, choose which records are kept, publish them
on `/rosout`, and find out what happened on a board that has no console.

nano-ros logs through `nros-log`, which has the `Logger` and severity surface
of `rclcpp::Logger` and `rcutils_logging`. Every record goes to one sink per
platform: stderr on Linux, Zephyr's log subsystem, NuttX's `syslog`, a UART on
FreeRTOS and ThreadX. The table is in
[Platform differences](../../reference/matrices/platform-differences.md#logging-nros-log).

## Before you start

- A node package in a workspace that runs, for example the talker of
  [Run a talker](../../tutorial/run-a-talker.md).

## Log from a node

<!-- tabs -->

### Rust

```rust
use nros_log::{Logger, Severity};
use nros_log::{log_info, log_warn};

static LOGGER: Logger = Logger::new("my_node");

fn main() {
    nros_log::register_logger(&LOGGER);
    nros_log::init(nros_log::sinks::default());

    log_info!(&LOGGER, "started; domain = {}", 42);
    log_warn!(&LOGGER, "queue depth {} exceeds soft limit", 5);
}
```

Inside a node, prefer the `Node::logger()` accessor — it resolves
to the same intern'd entry when the name matches:

```rust
let mut node = executor.create_node("my_node")?;
log_info!(node.logger(), "subscribed to {}", topic);
```

rclrs's modifiers chain off the logger, and each applies per call site:

```rust
use nros_log::ToLogParams;

log_info!(node.logger().once(), "link up");
log_warn!(node.logger().skip_first(), "late sample {}", seq);
log_debug!(node.logger().only_if(verbose), "state {:?}", state);
// needs the platform clock; a clockless image refuses it at compile time
log_warn!(node.logger().throttle(Duration::from_millis(500)), "lag {} ms", lag);
```

### C

```c
#include <rclc/rclc.h> /* also brings the RCLC_LOG_* macros */

int main(void) {
    rclc_support_t support = rclc_support_get_zero_initialized();
    rcl_allocator_t allocator = rcl_get_default_allocator();
    rclc_support_init(&support, 0, NULL, &allocator); /* locator: build, then NROS_LOCATOR */

    rcl_node_t node = rcl_get_zero_initialized_node();
    rclc_node_init_default(&node, "my_node", "/", &support);

    rclc_logger_t logger = rclc_node_get_logger(&node);
    RCLC_LOG_INFO(logger, "started; domain=%u", 42);
    RCLC_LOG_WARN(logger, "queue depth %u exceeds soft limit", 5);

    rcl_node_fini(&node);
    rclc_support_fini(&support);
    return 0;
}
```

### C++

```cpp
#include <rclcpp/rclcpp.hpp>

int main() {
    rclcpp::init_in("tcp/127.0.0.1:7447", 0);

    rclcpp::Node node;
    rclcpp::create_node(node, "my_node");

    auto logger = node.get_logger();
    RCLC_LOG_INFO(logger, "started; domain=%u", 42);
    RCLC_LOG_WARN(logger, "queue depth %u exceeds soft limit", 5);

    rclcpp::shutdown_in();
    return 0;
}
```

<!-- /tabs -->

The first log call in C or C++ installs the platform sink, so those call sites
need no init step.

## Set the level

1. Set the compile-time ceiling. Records below it are compiled out: a Rust
   call becomes a constant-false branch, and a C or C++ record is refused by
   the same check. State it per image, or per board in its descriptor:

   ```toml
   # system.toml: this image only
   [image.linux.knobs.log]
   max_level = "info"
   ```

   The value is `trace` (the default), `debug`, `info`, `warn`, `error`,
   `fatal` or `off`, or `0`–`6` as Zephyr's `CONFIG_NROS_LOG_MAX_LEVEL` states
   it. `nros config explain` prints which rung decided it.

2. Move a logger's level at run time, above the ceiling:

   ```rust
   let logger = nros_log::get_logger("my_node");
   logger.set_level(nros_log::Severity::Warn);   // silences Trace, Debug, Info
   logger.unset_level();                         // follows the default again
   nros_log::Logger::set_default_level(nros_log::Severity::Debug);
   ```

   A logger with no level of its own follows its nearest dotted ancestor
   (`my_node.planner` follows `my_node`), then the process default, `Info`.
   In C the same verbs are `rclc_logger_set_level`,
   `rclc_log_set_default_level` and `rclc_logger_set_level(logger,
   RCLC_LOG_SEVERITY_UNSET)`.

3. Make a child logger with upstream's spelling:
   `node.logger().create_child("planner")?` in Rust,
   `node->get_logger().get_child("planner")` in C++, and
   `rclc_logger_get_child(node_log, "planner")` in C. Names are capped at 48
   bytes and runtime loggers come from a bounded arena: when a child cannot be
   made, Rust returns an error, C returns `NULL`, and C++ logs through the
   parent and refuses `set_level` on the child.

## Publish records on `/rosout`

1. Declare the capability, `[system] features = ["rosout"]` in `system.toml`
   (or `set(NROS_FEATURES "rosout")` in a bare CMake project).

2. Create the publisher and pump the queue from your spin loop:

<!-- tabs -->

### Rust

```rust
use nros_rcl_interfaces::msg::Log;

let rosout = executor
    .create_node("my_node")?
    .create_publisher_with_qos::<Log>(nros::rosout::TOPIC, nros::rosout::qos_bounded())?;
nros::rosout::enable();
loop {
    executor.spin_once(budget);
    let _ = nros::rosout::pump(&rosout);
}
```

### C

```c
rcl_publisher_t rosout = rcl_get_zero_initialized_publisher();
rclc_rosout_publisher_init(&rosout, &node, NULL);   /* NULL = bounded QoS */
rclc_rosout_enable();
for (;;) {
    rclc_executor_spin_some(&executor, 10000000);
    rclc_rosout_pump(&rosout, NULL);
}
```

### C++

```cpp
rclcpp::rosout::Publisher rosout;
RCLCPP_TRY(rosout.create(node));
RCLCPP_TRY(rclcpp::rosout::enable());
for (;;) {
    rclcpp::spin_once(10);
    rosout.pump();
}
```

<!-- /tabs -->

3. Read them from ROS 2: `ros2 topic echo /rosout`, or `rqt_console`.

Where this differs from ROS 2:

- **It is not automatic.** The publisher is an entity, so you create it, and
  it counts in the image's pools like any other.
- **The default QoS is bounded.** `qos_bounded()` is volatile at the queue
  depth. Upstream's profile (`nros::rosout::qos()`, keep-last 1000,
  transient-local) is expensive on a board; with the bounded one a late
  subscriber misses the boot records.
- **Which loggers publish follows the ROS release.** On Humble only a node's
  logger reaches `/rosout`, not a child or a free `get_logger("x")`; Iron and
  Jazzy also publish a node's children.
- **The stamp is the platform's monotonic clock**, so `rqt_console` shows
  uptime as a time of day.
- **It costs `.bss`**: 5,760 bytes at the defaults, from 16 queued records.
  An image without the feature pays nothing.

The sink never publishes from inside a log call: it copies the record into a
ring, and `pump()` publishes on your spin thread. Records lost to a full ring
or logged during a publish are counted (`nros::rosout::dropped()`,
`suppressed()`) and reported on `/rosout` itself. The buffer sizes are knobs:
see [Logging](../../reference/environment-variables.md#logging).

## Observe a board with no console

Some boards have no console you can read, for example one whose only wired
UART carries the zenoh serial link, where a log line would corrupt the
transport's framing. Reach for these in order. A debug probe is not a passive
instrument there: reading memory halts the core, and a serial zenoh session
misses its keepalives and does not come back until the router restarts.

1. **Read the boot record.** With `CONFIG_NROS_BOOT_REPORT=y`, the image keeps
   one fixed-size record in RAM, the `NROS_BOOT_REPORT` symbol, as it boots.
   It survives the failures that stop a log from ever printing. It holds the
   knobs the image was compiled with, the last boot stage it reached, the
   allocation that did not fit and which allocator refused it, the heap's
   peak and capacity, samples dropped as too large, the domain and the rung
   that decided it, and a count of `ERROR` and `FATAL` records with the
   first one's file and line. After the run, halt the core and save it, with
   the address and size from the image's own symbol table:

   ```sh
   read ADDR LEN <<< "$(arm-none-eabi-nm -S build/zephyr/zephyr.elf \
       | awk '$4=="NROS_BOOT_REPORT"{print "0x"$1, "0x"$2}')"
   pyocd commander -t <target> -c halt -c "savemem $ADDR $LEN report.bin"
   ```

   The `nros` CLI has no command that decodes the bytes yet. The record is
   off by default, and an image without it is byte-identical to one built
   before the feature existed.

2. **Read the fault record.** `CONFIG_NROS_FAULT_LOG=y` replaces Zephyr's
   fatal-error handler with one that writes the reason, the faulting PC, LR
   and PSR, and the thread name into a `.noinit` record that survives a
   functional reset. Set `CONFIG_THREAD_NAME=y` too, or the thread reads `?`.
   `CONFIG_NROS_FAULT_LOG_FLASH=y` also copies it to the storage partition at
   the next boot. A power-on or pin reset may zero that RAM.

3. **Read a live log over RTT, last.** Move the console to an unwired UART in
   a devicetree overlay, and read SEGGER RTT over the SWD link:

   ```sh
   script -qec "pyocd rtt -t <target> -a $(arm-none-eabi-nm build/zephyr/zephyr.elf \
       | awk '$3==\"_SEGGER_RTT\"{print \"0x\"$1}')" /dev/null
   ```

   It needs the `zephyrproject-rtos/segger` module in the workspace, passed
   with `-DZEPHYR_EXTRA_MODULES=<path>`; a TTY, which `script` provides; and
   the control block's address, `-a`. Set
   `CONFIG_SEGGER_RTT_BUFFER_SIZE_UP=16384`, or the default 1 KiB buffer
   wraps before a late reader sees anything. Check anything found this way
   again with the probe detached, using the two records above.

## Example

A Rust node that logs once at start and throttles a warning, in an image
whose ceiling is `info`:

```rust
use nros_log::{log_info, log_warn, ToLogParams};

log_info!(node.logger().once(), "link up");
log_warn!(node.logger().throttle(Duration::from_millis(500)), "lag {} ms", lag);
```

```toml
[image.linux.knobs.log]
max_level = "info"
```

The logging demos, one node package per language, are in
[`examples/workspaces/features`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces/features).

## If it doesn't work

- **A `debug!` record never appears.** The compile-time ceiling is above it,
  or the logger's level is. Lower `max_level`, then the logger's level.
- **Nothing reaches `/rosout`.** The `rosout` feature is not declared, the
  queue is never pumped, or the record came from a child or free logger on
  Humble.
- **A throttled call does not compile.** `throttle` needs the platform clock;
  a clockless image refuses it.

## See also

- [Environment variables: logging](../../reference/environment-variables.md#logging):
  the ceiling and the buffer knobs.
- [Platform differences: logging](../../reference/matrices/platform-differences.md#logging-nros-log):
  where records go, and the severity numbers.
- [Use a serial link](../rmw/use-a-serial-link.md): the two settings a Zephyr
  board joining over a UART needs.

## Design background

- [RFC-0102: logger hierarchy](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0102-logger-hierarchy.md)
