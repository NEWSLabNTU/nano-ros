# nros-log

Phase 88 — ROS 2 style leveled logging facade. `no_std` + optional `alloc`
+ optional `std`. Zero target deps; per-platform log delivery flows
through the `nros_platform_log_*` ABI (see `nros-platform-cffi`).

## Quick start

```rust
use nros_log::{Logger, Severity};
use nros_log::{nros_info, nros_warn};

static LOGGER: Logger = Logger::new("my_node");

fn main() {
    nros_log::register_logger(&LOGGER);
    nros_log::init(nros_log::sinks::default());

    nros_info!(&LOGGER, "started; domain = {}", 42);
    nros_warn!(&LOGGER, "queue depth {} exceeds soft limit", 5);
}
```

`sinks::default()` returns a `&'static [&dyn LogSink]` containing one
`PlatformSink` — that's the only sink that calls
`nros_platform_log_write`. Boards / apps can add their own sinks
(e.g. a future `RosoutSink`) by composing a `&'static` slice.

## Macros

- `nros_trace!` / `nros_debug!` / `nros_info!` / `nros_warn!` /
  `nros_error!` / `nros_fatal!`
- All take `(logger, fmt, args…)`.
- Below-ceiling macros expand to `()` (the format call is
  dead-code-eliminated).

Throttle / once / skip-first variants land alongside the base macros
once 88.2 finalizes.

## Compile-time level ceiling

Pick at most one Cargo feature:

| Feature           | Macros above ceiling that emit                  |
|-------------------|-------------------------------------------------|
| `max-level-trace` | trace, debug, info, warn, error, fatal (default)|
| `max-level-debug` | debug, info, warn, error, fatal                 |
| `max-level-info`  | info, warn, error, fatal                        |
| `max-level-warn`  | warn, error, fatal                              |
| `max-level-error` | error, fatal                                    |
| `max-level-off`   | (none)                                          |

## Buffer size

Pick at most one Cargo feature. Default 256.

| Feature              | Per-call-site stack frame for formatting |
|----------------------|------------------------------------------|
| `buffer-size-128`    | 128 B                                    |
| `buffer-size-256`    | 256 B (default)                          |
| `buffer-size-512`    | 512 B                                    |
| `buffer-size-1024`   | 1024 B                                   |

Overflow truncates + appends `…`; `log()` never fails.

## Backend delivery (per platform)

See `docs/roadmap/archived/phase-88-nros-log.md` for the per-platform
impl table. Summary: POSIX → stderr; Zephyr → `LOG_*`; ESP-IDF →
`ESP_LOG_*`; NuttX → `syslog`; FreeRTOS / ThreadX / bare-metal →
board-registered UART / semihosting / defmt writer fn-ptr.

Each impl lives in its `nros-platform-<rtos>` crate, behind the
ABI. To change behavior on a target, change the platform impl, not
this crate.

## Phase status

See `docs/roadmap/archived/phase-88-nros-log.md`. v1 = facade + macros +
ABI + POSIX impl + PlatformSink + the Rust API. C/C++ bindings,
per-RTOS impls, examples, and tests land incrementally.
