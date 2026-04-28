# Phase 88: Unified Leveled Logging (`nros-log` Crate)

**Goal**: Introduce a ROS-style named-logger API (`Severity`, `Logger`,
`nros_info!`, `nros_warn!`, …) with pluggable backends for each target
platform. The facade lives in a single `nros-log` crate; RTOS and host
sinks are feature-gated inside the same crate rather than spawning a
shim crate per backend. `/rosout` publication is explicitly out of scope
for this phase.

**Status**: Not Started
**Priority**: Medium — the project currently has no unified logging
story; board crates use ad-hoc `cortex_m_semihosting::hprintln!`,
`defmt::info!`, and `esp_println` directly. This blocks consistent user
examples, REP-2012-style severity filtering, and any future `/rosout`
integration.
**Depends on**: None (Phase 79 unified platform abstraction is helpful
context but not a blocker).

## Overview

### Status quo

- No logging trait or hook exists in `nros-platform`; every board crate
  chooses its own output path (`nros-board-mps2-an385` → semihosting,
  `nros-board-stm32f4` → `defmt`, `nros-board-esp32` → `esp_println`).
- `nros-node`, `nros-c`, and `nros-cpp` have no `Logger` concept, so
  library code either silently swallows diagnostics or routes them via
  `eprintln!` on `std` targets only.
- `no_std` RTOS targets have native logging facilities
  (Zephyr `LOG_*`, ESP-IDF `ESP_LOG*`, NuttX `syslog`) that we aren't
  forwarding into.

### Target shape (mirrors `rclcpp::Logger` / `rcutils_logging`)

```rust
use nros_log::{Logger, Severity};
use nros_log::{nros_info, nros_warn, nros_info_throttle};

let logger = nros_log::get_logger("my_node");
nros_info!(logger, "started; domain = {}", domain_id);
nros_warn!(logger, "queue depth {} exceeds soft limit", depth);
nros_info_throttle!(logger, 5_000, "alive; msg_count = {}", count);
```

From a `Node`:
```rust
let logger = node.logger();   // borrowed, zero-alloc
nros_info!(logger, "subscribed to {}", topic);
```

Severity matches REP-2012: `Debug`, `Info`, `Warn`, `Error`, `Fatal`.

### Key design decisions

1. **Single crate with feature gates**, not per-backend shim crates. The
   shims are each ~30–80 lines of `extern "C"` + small adapters; no
   build.rs work; final link resolves native symbols. Splitting into
   `nros-log-zephyr`, `nros-log-esp-idf`, … would add Cargo.toml churn
   with no payoff. Matches the precedent set by `nros-rmw-zenoh`, which
   likewise feature-gates per-platform FFI rather than sprouting sibling
   crates.
2. **Format at the call site** into a `heapless::String<N>` (default
   N=256, tunable via feature). Each sink receives `&str`. This forfeits
   Zephyr LOG's deferred-formatting advantage, but it keeps the sink
   interface uniform across very different backends (`/rosout` / ESP-IDF
   / NuttX all need fully-formed text anyway).
3. **Static sink array dispatch**: `static SINKS: &[&dyn LogSink]`
   populated by a board/app-level init function. Works `no_std` with no
   alloc. Multi-sink fan-out drops out for free (useful for
   `stdout + file` on POSIX, or future `native + /rosout`).
4. **Compile-time level ceiling** via `max-level-*` features (same model
   as the `log` crate). Below-ceiling macros expand to a no-op so the
   formatting call is dead-code eliminated.
5. **Per-logger runtime threshold** stored on the `Logger` itself
   (`Logger { name: &'static str, level: AtomicU8 }`). No global logger
   registry in v1 — avoids unbounded state on `no_std`.
6. **ISR-safety documented per sink**, not demanded uniformly. Zephyr
   LOG and NuttX `syslog` are ISR-safe; semihosting, ESP-IDF, and
   stdlib `println!` are not. Users logging from ISRs are responsible
   for picking an appropriate sink.

### Backend summary

| Sink feature         | Target surface                                   | ISR-safe |
|----------------------|--------------------------------------------------|----------|
| `sink-stdout`        | `std` — `writeln!` on stderr                     | N/A      |
| `sink-semihosting`   | `cortex_m_semihosting::hio::hstderr()`           | no       |
| `sink-defmt`         | `defmt::info!` / `warn!` / … (RTT target)        | yes\*    |
| `sink-zephyr`        | `log_msg_runtime_create` via `extern "C"`        | yes      |
| `sink-esp-idf`       | `esp_log_write(level, tag, "%s", buf)`           | partial  |
| `sink-nuttx`         | `syslog(priority, "%s", buf)`                    | yes      |
| `sink-freertos`      | user-provided `configPRINTF`-equivalent hook     | caller   |
| `sink-threadx`       | user-provided UART writer hook                   | caller   |

\* `defmt` via RTT is ISR-safe with the usual `critical-section` impl.

## Work Items

- [ ] 88.1 — Create `packages/core/nros-log/` skeleton: `Cargo.toml`,
      `src/lib.rs`, `Severity`, `Record<'a>`, `Metadata<'a>`, `Logger`,
      trait `LogSink`, static dispatcher, `nros_log::get_logger(name)`
      free function, init hook `nros_log::init(&'static [&dyn LogSink])`.
      Workspace-visible but behind `default = []` features so pulling
      it in costs nothing until a sink is enabled.
- [ ] 88.2 — Macros: `nros_debug!`, `nros_info!`, `nros_warn!`,
      `nros_error!`, `nros_fatal!`, plus `*_throttle!(logger, ms, …)`,
      `*_once!`, `*_skipfirst!`. Formatting uses `heapless::String<N>`
      with `N` controlled by a `buffer-size-<N>` feature family (default
      256). Compile-time ceiling via `max-level-*` features; macros
      below the ceiling expand to `()`.
- [ ] 88.3 — Host / bare-metal sinks: `sink-stdout` (`std` only),
      `sink-semihosting` (pulls `cortex-m-semihosting`), `sink-defmt`
      (pulls `defmt`, maps severity to the closest defmt level).
      Includes unit tests on native where possible.
- [ ] 88.4 — Zephyr sink (`sink-zephyr`): FFI to
      `log_msg_runtime_create` (or fall back to `printk` if runtime
      message creation is unavailable in the Zephyr release we target).
      Maps `Severity` to Zephyr's `LOG_LEVEL_*`. Register a
      `LOG_MODULE_DECLARE(nros)` from the zephyr module glue in
      `zephyr/cmake/` so the module shows up in Zephyr's runtime-filter
      shell commands.
- [ ] 88.5 — ESP-IDF sink (`sink-esp-idf`): FFI to `esp_log_write`.
      Maps severity to `ESP_LOG_*`. Uses the logger name as the TAG;
      converts to a null-terminated `CStr` via a small `heapless`
      buffer. Documents that calling from a flash-cache-disabled path
      requires the caller to use the `esp_rom_printf`-backed sink
      instead (future work if needed).
- [ ] 88.6 — NuttX + FreeRTOS + ThreadX sinks:
      - `sink-nuttx`: FFI to `syslog(priority, "%s", buf)`; map severity
        to `LOG_ERR` / `LOG_WARNING` / `LOG_INFO` / `LOG_DEBUG`.
      - `sink-freertos`: takes a board-provided
        `fn(level: Severity, msg: &str)` writer pointer at
        `nros_log::init` time; default implementation calls the board
        crate's existing UART/`configPRINTF` hook.
      - `sink-threadx`: same pattern as FreeRTOS — ThreadX has no native
        text logging, so we lean on the board's UART.
- [ ] 88.7 — Optional `log-compat` feature: provide a `log::Log` impl
      that forwards to our sinks, and a reverse bridge
      (`nros_log::LogSink` wrapping a `log::Log`). Lets existing
      ecosystem crates (that use `log::info!`) integrate without
      duplicating output.
- [ ] 88.8 — Board-crate wiring: replace ad-hoc output in each board
      crate with a sink init:
      - `nros-board-mps2-an385` → enable `sink-semihosting`, delete the custom
        `println!` macro at `packages/boards/nros-board-mps2-an385/src/lib.rs`.
      - `nros-board-stm32f4` → enable `sink-defmt`, remove direct `defmt::…`
        call sites in `packages/boards/nros-board-stm32f4/src/node.rs`.
      - `nros-board-esp32` / `nros-board-esp32-qemu` → enable `sink-esp-idf`.
      - `nros-board-mps2-an385-freertos` → enable `sink-freertos`, provide the
        board's `configPRINTF` adapter.
      - `nros-board-nuttx-qemu-arm` → enable `sink-nuttx`.
      - `nros-threadx-*` → enable `sink-threadx` plus UART writer.
      - Zephyr: the `zephyr/` module's Kconfig gets a
        `CONFIG_NROS_LOG_SINK_ZEPHYR` option; Rust side enables
        `sink-zephyr`.
- [ ] 88.9 — Node integration: `Node::logger() -> &Logger` on the Rust
      API (`nros-node`), `nros_node_get_logger(node)` on the C API
      (`nros-c`), and `Node::get_logger()` on the C++ API (`nros-cpp`).
      Logger name matches the node name (no namespace logic in v1; we
      can add `get_child("subcomponent")` as a follow-up).
- [ ] 88.10 — Examples + docs: one minimal `logging/` example per
      language (Rust, C, C++) that demonstrates per-severity macros and
      runtime threshold adjustment. Extend `book/src/user-guide/` with a
      `logging.md` chapter; extend `book/src/reference/rust-api.md`,
      `c-api.md`, `cpp-api.md` with the `Logger` surface.
- [ ] 88.11 — Tests: a `packages/testing/nros-tests/tests/logging.rs`
      that verifies severity filtering (compile-time ceiling + per-logger
      runtime threshold), throttle/once semantics, and sink fan-out.
      RTOS-specific sink output verification is best-effort — on QEMU
      platforms that expose the UART, assert the expected line appears
      in the captured output.

## Design Notes

- **`Logger` shape**:
  ```rust
  pub struct Logger {
      name: &'static str,
      level: AtomicU8,   // Severity threshold
  }
  ```
  Clone-free. `get_logger("x")` returns a `&'static Logger` backed by
  a small intern table (const-constructed from a macro) — or, for the
  simpler v1, returns an owned `Logger` value that the caller stores.
  Decide in 88.1; the trade-off is one `AtomicU8` per caller-held copy
  vs. a bounded global table.
- **`Record`**:
  ```rust
  pub struct Record<'a> {
      pub severity: Severity,
      pub logger_name: &'a str,
      pub message: &'a str,           // already formatted
      pub file: &'static str,
      pub line: u32,
      pub timestamp_ns: u64,          // from nros_platform::clock
  }
  ```
  `function` is deliberately omitted in v1 (Rust's `core::panic::Location`
  equivalent for functions requires a proc-macro; punt to `/rosout`
  phase if needed for `rcl_interfaces/msg/Log`).
- **Dispatcher without alloc**: use a
  `static SINKS: Mutex<RefCell<Option<&'static [&'static dyn LogSink]>>>`
  (Zephyr/NuttX/QEMU-bare all have `critical-section`). Set once at
  `nros_log::init()`; `log()` acquires, iterates, drops. No hot-path
  allocation.
- **Recursion guard**: even without `/rosout` we should short-circuit
  `log()` if called re-entrantly from inside a sink (e.g., a UART
  writer panics and the panic handler logs). One `AtomicBool` per
  thread / per-core is enough on the platforms we care about.
- **Compile-time ceiling model**: each `nros_<level>!` macro checks a
  `cfg!` of the form
  `cfg!(any(feature = "max-level-trace", feature = "max-level-debug", …))`
  so expansion is zero-cost when the feature ceiling is below the call
  site's severity. Mirrors how the `log` crate does it — don't invent a
  new model.
- **Buffer-overflow policy**: if the formatted message exceeds the
  configured `heapless::String<N>`, we truncate and append `…` rather
  than dropping the message. The `log()` call never fails; overflow
  increments a per-sink counter accessible via a debug helper.
- **Interaction with `defmt`**: the `sink-defmt` path cannot carry a
  `&str` verbatim through defmt's wire format (defmt wants format
  strings interned at build time). We compromise: `sink-defmt` calls
  `defmt::info!("{=str}", record.message)` etc., which interns the
  literal format string once and sends the message as a `str` payload.
  This is strictly worse for flash footprint than native defmt
  call-sites, but it's a convenience sink — users who want full defmt
  ergonomics should call `defmt::info!` directly.

## Acceptance Criteria

- [ ] `nros-log` is a workspace member under `packages/core/nros-log/`,
      `just ci` passes with every sink feature built standalone and with
      no sinks enabled (the "library-only" case).
- [ ] On native (`platform-posix`), an example calling
      `nros_info!(logger, "hello")` emits a line on stderr with severity
      tag, logger name, and message.
- [ ] On Zephyr QEMU, `nros_info!` output appears in the Zephyr log
      output (visible via `CONFIG_LOG=y` in the board's Kconfig), tagged
      with the `nros` module and the correct severity.
- [ ] On ESP32 QEMU, `nros_info!` output is visible via `idf.py monitor`
      equivalent (QEMU UART capture) with the correct TAG and severity
      color.
- [ ] On MPS2-AN385 bare-metal QEMU, `nros_info!` output appears on the
      semihosting console.
- [ ] Per-logger runtime threshold works: setting
      `logger.set_level(Severity::Warn)` suppresses `Debug`/`Info` calls
      on that logger without affecting other loggers.
- [ ] Compile-time ceiling works: building with
      `--features nros-log/max-level-warn` makes `nros_debug!` /
      `nros_info!` expand to no-ops, verified by a size check
      (`cargo bloat` or equivalent) on a bare-metal target.
- [ ] `nros-c` and `nros-cpp` expose `nros_node_get_logger` and
      `Node::get_logger`, and each has an example that uses
      `nros_log_info(logger, "…")` / `NROS_INFO(logger, "…")` macros
      equivalent to the Rust surface.
- [ ] No `static mut` introduced; no unbounded heap allocation in the
      log path; no format-arg path that panics on overflow.

## Notes

- **`/rosout` is explicitly out of scope.** The ring-buffer-drained-by-
  executor sink we discussed earlier is a follow-up phase once
  `nros-log` has stabilized. Nothing in 88.1–88.11 should preclude it —
  in particular, `Record<'a>` must stay cheap to clone into a
  queued owned form when that follow-up lands.
- **`log` crate interop** (88.7) is optional. We intentionally do NOT
  base the public API on the `log` crate, because ROS-style named
  loggers don't map cleanly to the `log` crate's string-target filter
  model. The bridge is there for ecosystem interop, not as the
  primary surface.
- **Why not `tracing`?** `tracing` is a better fit for rich
  structured/span logging on `std` targets but is overkill here and
  drags in dependencies we don't want on `no_std`. If a future user
  wants span-level instrumentation, we can add a `tracing-compat`
  feature later without changing the core facade.
- **Feature collapse**: keep `default = []`. Do not default-enable
  any sink. Board crates are responsible for picking a sink. This
  matches how `nros-rmw-zenoh` makes platform selection explicit.
- **Follow-ups this phase does not cover**: `/rosout` sink,
  rclcpp-style child loggers (`logger.get_child("sub")`), structured
  key-value logging (ROS 2 doesn't standardize this yet either), and
  per-module compile-time ceilings via the `log` crate's
  `max_level_<level>_feature` convention.
