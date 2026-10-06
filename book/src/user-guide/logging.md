# Logging

nano-ros ships a ROS 2 style leveled logging facade
([`nros-log`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/core/nros-log))
with the same `Logger` + severity surface as `rclcpp::Logger` /
`rcutils_logging`. Records flow through a single per-platform sink
(`PlatformSink`) that delegates to whichever
`nros-platform-<rtos>` is linked into the binary.

## Severity ladder

REP-2012 style: rcutils's five levels in rcutils's order, plus `Trace`. The
numbers are NOT `rcutils_log_severity_t`'s (`UNSET=0, DEBUG=10 … FATAL=50`)
and there is no `Unset` level — in Rust a logger's "no level of its own" state
is `Logger::unset_level()`, and `Logger::set_default_level` takes a `Severity`
(pass `Info` where rclrs passes `Unset`). The C surface's `nros_log_severity_t`
does use rcutils's numbers, `UNSET` included.

| Severity | u8 | When to use |
|----------|----|-------------|
| `Trace`  | 0  | Per-instruction granularity; off by default. |
| `Debug`  | 1  | Diagnostic information useful while developing. |
| `Info`   | 2  | Normal operation events worth surfacing once. |
| `Warn`   | 3  | Unexpected but recoverable conditions. |
| `Error`  | 4  | Errors the caller should surface; system continues. |
| `Fatal`  | 5  | Unrecoverable; system is about to abort. |

The numeric representation is stable and used by the
`nros_platform_log_write` ABI.

## Quick start

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

### C

```c
#include <nros/init.h>
#include <nros/node.h>
#include <nros/log.h>

int main(void) {
    nros_support_t support = nros_support_get_zero_initialized();
    nros_support_init(&support, "tcp/127.0.0.1:7447", 0);

    nros_node_t node = rcl_get_zero_initialized_node();
    rclc_node_init_default(&node, "my_node", "/", &support);

    nros_logger_t logger = nros_node_get_logger(&node);
    NROS_LOG_INFO(logger, "started; domain=%u", 42);
    NROS_LOG_WARN(logger, "queue depth %u exceeds soft limit", 5);

    rcl_node_fini(&node);
    rclc_support_fini(&support);
    return 0;
}
```

### C++

```cpp
#include <nros/nros.hpp>
#include <nros/log.hpp>

int main() {
    nros::init("tcp/127.0.0.1:7447", 0);

    rclcpp::Node node;
    nros::create_node(node, "my_node");

    auto logger = node.get_logger();
    NROS_LOG_INFO(logger, "started; domain=%u", 42);
    NROS_LOG_WARN(logger, "queue depth %u exceeds soft limit", 5);

    nros::shutdown();
    return 0;
}
```

The first `NROS_LOG_*` emit auto-installs `PlatformSink` so C/C++
call sites work without an explicit init step.

## Per-platform delivery

| Target | Backend |
|--------|---------|
| POSIX | `fprintf(stderr, "[<LEVEL>] <name>: <msg>\n")` |
| Zephyr | `LOG_INF` / `LOG_WRN` / etc. (or `printk` if `CONFIG_LOG=n`); module `nros` |
| NuttX | `syslog(priority, "%s", buf)` |
| FreeRTOS | board-registered UART writer fn-ptr |
| ThreadX | board-registered UART writer fn-ptr |
| Bare-metal (mps2-an385) | QEMU semihosting via `cortex_m_semihosting::hio::hstderr` |
| Bare-metal (stm32f4) | `defmt::info!("{=str}", msg)` |
| Bare-metal (esp32 / esp32-qemu) | board-registered fn-ptr (esp-println / RTT / serial-jtag) |

Boards on platforms with no native logger (FreeRTOS / ThreadX /
bare-metal Rust ESP32) supply their writer fn-ptr at `run()` time:

```rust
// from nros-board-mps2-an385-freertos/src/lib.rs
fn register_log_writer() {
    unsafe extern "C" fn writer(severity: u8, name_ptr: *const u8, name_len: usize,
                                 msg_ptr: *const u8, msg_len: usize) { … }
    unsafe { nros_platform_register_log_writer(Some(writer), None); }
}
```

## Filtering

### Compile-time ceiling

Cargo features on `nros-log` (pick at most one; default
`max-level-trace`):

| Feature | Macros above ceiling that emit |
|---------|--------------------------------|
| `max-level-trace` | trace / debug / info / warn / error / fatal |
| `max-level-debug` | debug / info / warn / error / fatal |
| `max-level-info` | info / warn / error / fatal |
| `max-level-warn` | warn / error / fatal |
| `max-level-error` | error / fatal |
| `max-level-off` | (none) |

Below-ceiling macros expand to `()`; the format call is
dead-code-eliminated.

### Runtime per-logger threshold

```rust
let logger = nros_log::get_logger("my_node");
logger.set_level(nros_log::Severity::Warn);   // silences Trace/Debug/Info
```

### The process default, and "unset"

A logger built with `Logger::new(name)` has **no level of its own**. It filters
on the process-wide default, which is `Severity::Info` until something moves
it:

```rust
nros_log::Logger::set_default_level(nros_log::Severity::Debug);  // every
                                     // logger that has not been given a level
logger.set_level(nros_log::Severity::Warn);   // this one now states its own
logger.unset_level();                         // ...and follows the default again
```

`set_default_level` is retroactive — a `static LOGGER: Logger =
Logger::new("my_node")` compiled into the image follows it — and it never
overrides a logger that states its own level, so `Logger::with_level(name,
Severity::Info)` keeps `Info` whatever the default becomes.

From C, the same three verbs are `nros_log_set_default_level`,
`nros_log_get_default_level`, and `nros_logger_set_level(logger,
NROS_LOG_SEVERITY_UNSET)` — which UNSETS the level rather than selecting a
value, exactly as `rcutils_logging_set_logger_level(name,
RCUTILS_LOG_SEVERITY_UNSET)` does. C++ spells the last one
`logger.set_level(nros::Logger::Level::Unset)`.

### Child loggers, and what an unset level inherits

As upstream, an unset logger takes the level of its nearest dotted ancestor
(`x.y.z` → `x.y` → `x`) before the process default (RFC-0102). Make children
with the upstream spelling:

```rust
let node_log = node.logger();                       // "my_node"
let planner = node_log.create_child("planner")?;    // "my_node.planner"
node_log.set_level(nros_log::Severity::Debug);      // reaches `planner` too
nros_log::nros_debug!(planner, "replanning");       // emitted
```

```cpp
auto planner = node->get_logger().get_child("planner");   // C++
```

```c
nros_logger_t planner = nros_logger_get_child(node_log, "planner");   /* C */
```

A child with its own level keeps it; `unset_level()` hands it back to the
parent. The same name always answers the same logger. The catch-all's children
are top-level names: `planner`, not `nros.planner`.

**Bounds, if you are porting.** Upstream keys a level by NAME, so an ancestor
can hold one without being a logger; here an ancestor holds one by existing,
and setting a level by name creates it, so in practice the behaviour matches.
Logger names are capped at **48 bytes**, and runtime-created loggers come from
a bounded arena. When a child cannot be made:

- Rust's `create_child` returns `Err(ChildError::{NameTooLong, EmptyName,
  ArenaFull})`, as rclrs's is fallible too;
- C's `nros_logger_get_child` returns `NULL` and warns once;
- C++'s `get_child`, whose upstream shape cannot fail, **emits through the
  parent** (records carry the parent's name and level) and **refuses
  `set_level`**, so it can never move the parent's threshold. `get_name()`
  still answers the name you asked for.

## Buffer size

`nros-log` formats each record into a stack-resident
`heapless::String<N>`. `N` is picked at compile time by the
`buffer-size-<N>` feature family (default 256). Overflow truncates
and appends `…`; the macro never panics on a long format string.

| Feature | Per-call-site stack frame |
|---------|---------------------------|
| `buffer-size-128` | 128 B |
| `buffer-size-256` | 256 B (default) |
| `buffer-size-512` | 512 B |
| `buffer-size-1024` | 1024 B |

## Interop with the `log` crate

Enable `nros-log/log-compat`:

```rust
nros_log::log_compat::install_log_crate_bridge()
    .expect("log crate already initialized");
// Now `log::info!(...)` calls from ecosystem crates flow through
// nros-log's dispatcher.

// Or fan out the other direction — re-emit nros records via `log`:
static SINKS: &[&dyn nros_log::LogSink] = &[
    &nros_log::sinks::PlatformSink,
    &nros_log::log_compat::LogCrateSink,
];
nros_log::init(SINKS);
```

Severity maps Trace↔Trace, Debug↔Debug, Info↔Info, Warn↔Warn,
Error↔Error round-trip. Fatal folds into Error one-way (`log` has
no Fatal).

## Working examples

- Rust: `examples/native/rust/logging/`
- C: `examples/native/c/logging/`
- C++: `examples/native/cpp/logging/`

Each demonstrates per-severity macros + the runtime threshold
filter.

## Reference

- `packages/core/nros-log/` — facade crate.
- `packages/platform/nros-platform-cffi/include/nros/platform.h` —
  `nros_platform_log_write` / `nros_platform_log_flush` /
  `nros_platform_register_log_writer` ABI.
- `packages/api/nros-c/include/nros/log.h` — C API surface
  (`NROS_LOG_*` macros + `nros_log_emit_fmt`).
- `packages/api/nros-cpp/include/nros/log.hpp` — C++ macros (the
  legacy `NROS_INFO` / etc. file:line printf surface stays
  alongside the new `NROS_LOG_*` macros).

## `/rosout`

**It exists now** (phase-467 Q4; this section read "explicitly out of scope
today" until 2026-09-29). Records can be republished as
`rcl_interfaces/msg/Log` on `/rosout`, so `ros2 topic echo /rosout`,
`rqt_console` and launch-side log aggregators see a nano-ros node.

Declare the `rosout` capability — `[system].features = ["rosout"]` in
`system.toml`, or `set(NANO_ROS_FEATURES "rosout")` before
`find_package(nano_ros)` in a bare CMake project (a Rust leaf can also name the
`nros/rosout` feature) — and wire three lines. Rust:

```rust
use nros_rcl_interfaces::msg::Log;

let rosout = executor
    .create_node("my_node")?
    .create_publisher_with_qos::<Log>(nros::rosout::TOPIC, nros::rosout::qos_bounded())?;
nros::rosout::enable();                  // records start queueing
loop {
    executor.spin_once(budget);
    let _ = nros::rosout::pump(&rosout); // and reach the wire here
}
```

C (`<nros/rosout.h>`):

```c
nros_publisher_t rosout = rcl_get_zero_initialized_publisher();
nros_rosout_publisher_init(&rosout, &node, NULL);   /* NULL = bounded QoS */
nros_rosout_enable();
for (;;) {
    rclc_executor_spin_some(&executor, 10000000);
    nros_rosout_pump(&rosout, NULL);
}
```

C++ (`<nros/rosout.hpp>`):

```cpp
nros::rosout::Publisher rosout;
NROS_TRY(rosout.create(node));          // or create(node, nros::RosoutQoS())
NROS_TRY(nros::rosout::enable());
for (;;) {
    nros::spin_once(10);
    rosout.pump();
}
```

All three share one encoder, so the records are identical. Without the
capability the C and C++ entry points still link: `nros_logging_rosout_enabled()`
/ `nros::rosout::enabled()` answer `false` and the rest answer UNSUPPORTED.
`rclcpp::NodeOptions::enable_rosout(true)` still refuses to compile; "It is
not automatic" below says why the publisher is yours to create.

Five things to know before you rely on it.

**Which loggers reach it depends on the ROS release** (RFC-0102 D4), because
upstream's does. On Humble, rcl publishes only a NODE's logger: a child or a
free `get_logger("x")` never reaches `/rosout`. Iron and Jazzy also publish a
node's `get_child` descendants. `nros::rosout::enable()` scopes the queue to
the release the image is built for (`ros-humble` / `ros-iron` / `ros-jazzy`;
an image naming none gets Humble's rule). So log through `node.logger()` or
its children, not a free logger, if you want the record on the wire. One
bound: upstream stops publishing a child when its last copy is destroyed;
here it stays published, because runtime loggers are never freed.

**It is not automatic.** Upstream republishes from every rcl node with no user
action. Here you create the publisher yourself, because a publisher is an
ENTITY: it reaches the derived pool counts, the zenoh session's tables and the
sizing descriptor. An entity the runtime conjured below your declaration is how
a queryable table fills up at boot with nothing in your launch file to explain
it.

**The default QoS is not upstream's.** `nros::rosout::qos()` IS
`rcl_qos_profile_rosout_default` — KEEP_LAST(1000), RELIABLE, TRANSIENT_LOCAL,
10 s lifespan — and on an embedded target it is expensive: a transient-local
publisher takes a retention slot AND a cache queryable out of a
`ZPICO_MAX_QUERYABLES` that defaults to 8 and already has eleven claimants if
you run parameter and lifecycle services, and KEEP_LAST(1000) over a ~1 KB
message is a megabyte of history. `qos_bounded()` is VOLATILE at the queue
depth and costs neither. What you give up is that a subscriber which attaches
LATE sees no boot story. A stock `ros2 topic echo` still matches — it requests
VOLATILE and downgrades across a mixed publisher set.

**The stamp is the platform monotonic clock**, not wall time, because an RTOS
image has none to offer: `rqt_console` will display an uptime as a time of day.
Without `nros-log/platform-clock` it is a constant zero.

**It costs `.bss`.** 5 760 bytes on the defaults (16 queued records x 360 B),
down to 1 856 B with `rosout-records-8` + `buffer-size-128` and up to 72 192 B
with `rosout-records-64` + `buffer-size-1024`. An image that does not enable
the feature pays nothing — the module is compiled out whole.

### The sink does not publish, and that is the point

`nros_log`'s `/rosout` sink copies each record into a bounded static ring and
returns. It never enters the transport. A logging sink that publishes, on a
path that can itself log, is unbounded recursion — and on Zephyr native_sim
that shape kills the image by stack exhaustion with no message at all. The
publish happens in `pump()`, on your spin thread, with a flag set that makes
the queue refuse anything logged underneath it; those refusals are counted
(`nros::rosout::suppressed()`) alongside the ones lost to a full ring
(`nros::rosout::dropped()`), and `pump` reports both on `/rosout` itself so an
operator learns about a gap on the channel the gap is in.

C and C++ reach none of this yet — see issue 1589.
