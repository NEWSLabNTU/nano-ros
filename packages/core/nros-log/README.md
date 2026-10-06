# nros-log

Phase 88 — ROS 2 style leveled logging facade. `no_std` + optional `alloc`
+ optional `std`. Zero target deps; per-platform log delivery flows
through the `nros_platform_log_*` ABI (see `nros-platform-cffi`).

## Quick start

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

`sinks::default()` returns a `&'static [&dyn LogSink]` containing one
`PlatformSink` — that's the only sink that calls
`nros_platform_log_write`. Boards / apps can add their own sinks by
composing a `&'static` slice, or append one at runtime with `add_sink`.

The `/rosout` bridge is the worked example of the second, and it is no longer
"a future `RosoutSink`" (this line said so until 2026-09-29): see
[`rosout`](src/rosout.rs) here for the queue and `nros_node::rosout` for the
publisher. Note what it is NOT — the sink does not publish. It copies into a
bounded ring and returns, because a sink that enters the transport is
re-entrant by construction.

## Macros

- `log_debug!` / `log_info!` / `log_warn!` / `log_error!` / `log_fatal!` —
  rclrs's spellings. Rust follows rclrs because the three ROS 2 client
  libraries disagree with each other (`log_info!` / `RCLCPP_INFO` /
  `RCUTILS_LOG_INFO_NAMED`) and rclrs is the one a ported Rust node is read
  beside (RFC-0089, settled 2026-09-04).
- `nros_trace!` — kept under OUR prefix: rclrs stops at `debug`, so TRACE has
  no upstream twin and must not borrow a name that implies one.
- All take `(logger, fmt, args…)`.
- Below-ceiling macros (`NROS_LOG_MAX_LEVEL`) expand to a constant-false
  branch (the format call is dead-code-eliminated).

### Throttled variants (phase-417 W4.d)

- `nros_info_throttle!(logger, interval_ms, fmt, args…)` — reads the platform
  clock. **Needs the platform clock** (see "The platform clock" below), and says so with a
  `compile_error!` rather than compiling into a window with no time base:
  without a clock every timestamp is a constant `0`, so the window can never
  elapse and the site would emit its first record and then nothing, while
  reading exactly like a working throttle.
- `nros_info_throttle_at!(logger, now_ns, interval_ms, fmt, args…)` — you supply
  the time. This is `rclcpp`'s shape (`RCLCPP_INFO_THROTTLE(logger, clock, …)`
  names its clock) and the only throttled form available with no platform port.
- One window per CALL SITE, not per logger — each expansion declares its own
  `static ThrottleState`.
- The FIRST record at a site always emits, as in `rclcpp`. The severity
  threshold is tested BEFORE the window, so a record the level filters does not
  consume it.
- The rule itself is `nros_log::throttle_decide`, a pure function over ONE
  state word — bit 0 says "has emitted", the 63 bits above it hold the last
  emitted timestamp; the C API's `nros_log_throttle_admit` calls the same one
  over caller-owned storage. A clock that wraps still measures real elapsed
  time across the wrap; a clock that is STUCK (at any value, `0` included)
  admits once and then never (issue 1152 — the old `0`-sentinel-plus-skew made
  a clock pinned at 0 admit every record after the first).

There is no `*_once` / `*_skip_first` family here. `nros_core::logger::Logger`
has one, on the logger that forwards to the `log` crate rather than to
`nros_platform_log_write`; porting it is not part of W4.d.

## Named loggers and per-logger levels

- `get_logger(name)` — LOOKUP. Answers `DEFAULT_LOGGER` for a name no `'static`
  `Logger` was `register_logger`ed under.
- `get_or_create_logger(name)` — `rclcpp::get_logger`'s shape, and what the C
  and C++ wrappers need. Creates from a bounded static arena sized by the
  `NROS_LOG_DYNAMIC_LOGGERS` knob (phase-479 W5): image env > Kconfig / the
  board's `[board.knobs.log] dynamic_loggers` > 16. Linux host boards state
  32; `0` declines the arena. The `dynamic-loggers-<N>` features are
  deprecated (see "Deprecated features" below).
  Returns `None` rather than a logger under the wrong name: aliasing onto
  `DEFAULT_LOGGER` would make `set_level` on the result move the threshold of
  every other unregistered name in the image.
- `Logger::set_level` / `level` / `is_enabled` — per-logger runtime threshold,
  checked before any sink sees the record.
- `Logger::unset_level` / `Logger::set_default_level` / `Logger::default_level`
  — a logger built with `Logger::new` has NO level of its own and filters on
  the PROCESS DEFAULT (`Severity::Info` until `set_default_level` moves it,
  which is `RCUTILS_DEFAULT_LOGGER_DEFAULT_LEVEL`'s value). `with_level` /
  `set_level` give it one, `unset_level` takes it away again. This is the whole
  of our level hierarchy: rcutils resolves an unset logger by walking a dotted
  ancestry (`x.y.z` → `x.y` → `x`) to its default, and with no dotted name and
  an exact-match intern table that walk degenerates here to its last step.

## Sinks

- `init(&'static [&'static dyn LogSink])` — REPLACES the list. The board's verb:
  it names, once, the whole delivery it chose.
- `add_sink(&'static dyn LogSink)` — APPENDS, up to `MAX_ADDED_SINKS`. The
  consumer's verb: a library teeing records to /rosout must not discard what the
  board installed. Returns `false` when the registry is full — check it.

Records raised before any sink existed are replayed into whichever of the two
calls installs the first one (see the `early` module).

## Configuration — the `[knobs.log]` tenant (issue 1037)

Every size and the ceiling are KNOBS, resolved by `build.rs` on the RFC-0049
ladder — environment / `[image.<id>] env` > Kconfig `CONFIG_<knob>` > the
board's `[board.knobs.log] <key>` (or a platform's `[knobs.log]`) > builtin —
and printed by `nros config explain`:

| Knob                       | Key               | Builtin | Range       | What |
|----------------------------|-------------------|---------|-------------|------|
| `NROS_LOG_MAX_LEVEL`       | `max_level`       | `trace` | trace…fatal, off (0–6) | compile-time ceiling |
| `NROS_LOG_BUFFER_SIZE`     | `buffer_size`     | 256     | 128–4096    | per-call formatting buffer (stack) |
| `NROS_LOG_EARLY_RECORDS`   | `early_records`   | 4       | 0–256       | records held before `init` (0 = drop, counted) |
| `NROS_LOG_ROSOUT_RECORDS`  | `rosout_records`  | 16      | 1–1024      | `/rosout` queue depth (`rosout` feature) |
| `NROS_LOG_DYNAMIC_LOGGERS` | `dynamic_loggers` | 16 (Linux hosts 32) | 0–1024 | runtime-logger arena (0 = lookup only) |

The CEILING is a bound, not the runtime level: a Rust macro below it folds to
nothing (`severity_enabled_at_compile_time` is a `const fn`), and
`Logger::is_enabled` refuses below it too, which is how C and C++ records —
formatted on the C side — fall under the same bound. `set_level` still moves
each logger's threshold above it. rcutils keeps the same pair
(`RCUTILS_LOG_MIN_SEVERITY_*` and `rcutils_logging_set_logger_level`).

The C/C++ `NROS_LOG_*` printf front-end formats into its own 256-byte frame
(`NROS_LOG_FMT_BUFFER_SIZE` in `<nros/log.h>`, compiled in the caller's
translation unit), so a C record is bounded by the smaller of the two.

Overflow truncates + appends `…`; `log()` never fails.

### Deprecated features (one release)

`max-level-*`, `buffer-size-<N>`, `early-records-<N>`, `rosout-records-<N>`
and `dynamic-loggers-<N>` encoded those values as pick-one Cargo features,
which unify across a build with no precedence (RFC-0086 D5). They still work
for one release: honoured with a `cargo:warning` when no knob is stated, a
redundancy warning when they agree with one, a BUILD ERROR when they disagree
with a stated knob or with each other. They left `default` in the same change.

### The platform clock

`Record::timestamp_ns` and the clock-reading `nros_*_throttle!` family need
`nros_platform_clock_ns`. It is compiled in (`cfg(nros_log_clock)`) when the
lane's platform declares `[capabilities] clock = true` — every in-tree port
does — or when the `platform-clock` feature pulls a port in (`nros-c`'s
`platform-*` arms set it). A bare `cargo test -p nros-log` has neither, and
its test binaries link without a port.

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
