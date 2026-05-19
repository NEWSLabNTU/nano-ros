# Phase 88: Unified Leveled Logging (`nros-log` facade + `nros_platform_log_*` ABI)

**Goal**: Introduce a ROS-style named-logger API (`Severity`, `Logger`,
`nros_info!`, `nros_warn!`, …) backed by a single platform-agnostic
facade crate (`nros-log`) that fans out to per-platform log
implementations through the canonical `nros_platform_*` ABI. Each
`nros-platform-<rtos>` crate carries its own native log backend
(`printk` / `esp_log_write` / `syslog` / UART writer / etc.). `/rosout`
publication is explicitly out of scope for this phase.

**Status**: Not Started

**Priority**: Medium — the project currently has no unified logging
story; board crates use ad-hoc `cortex_m_semihosting::hprintln!`,
`defmt::info!`, and `esp_println` directly. This blocks consistent user
examples, REP-2012-style severity filtering, and any future `/rosout`
integration.

**Depends on**: Phase 79 (`nros-platform` unified abstraction) +
Phase 129 (platform-ABI consolidation pattern).

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

Severity matches REP-2012: `Trace`, `Debug`, `Info`, `Warn`, `Error`,
`Fatal`.

### Architecture (revised 2026-05-19)

```text
┌──────────────────────────────────────────────────────────────────┐
│ user code                                                        │
│   nros_info!(logger, "…")                                        │
└────────────────────────┬─────────────────────────────────────────┘
                         │ formats into heapless::String<N>
                         ▼
┌──────────────────────────────────────────────────────────────────┐
│ nros-log (facade — portable, zero target deps)                   │
│   Severity / Record / Logger / dispatcher / PlatformSink         │
└────────────────────────┬─────────────────────────────────────────┘
                         │ &Record  →  nros_platform_log_write(…)
                         ▼
┌──────────────────────────────────────────────────────────────────┐
│ nros-platform-api (ABI — `nros_platform_log_write` /             │
│                   `nros_platform_log_flush`)                     │
└────────────────────────┬─────────────────────────────────────────┘
                         │ extern "C" — one impl per platform
                         ▼
┌──────────────────────────────────────────────────────────────────┐
│ nros-platform-posix      → fwrite(stderr) + \n                   │
│ nros-platform-zephyr     → log_msg_runtime_create / printk       │
│ nros-platform-esp-idf    → esp_log_write                         │
│ nros-platform-nuttx      → syslog                                │
│ nros-platform-freertos   → board-registered UART writer fn-ptr   │
│ nros-platform-threadx    → board-registered UART writer fn-ptr   │
│ nros-platform-bare-metal → board-registered fn-ptr               │
│                            (semihosting / defmt / RTT / …)       │
└──────────────────────────────────────────────────────────────────┘
```

### Why the platform-ABI route, not per-sink Cargo features

The earlier design (pre-2026-05-19) put per-backend sinks behind
`nros-log` Cargo features (`sink-zephyr` / `sink-esp-idf` / …). That
mirrored `nros-rmw-zenoh`'s pre-Phase-129 pattern. Phase 129 retired
that pattern (`zpico-platform-shim` deleted; every platform feature now
flows through `nros_platform_*`). `nros-log` follows the new precedent:

1. **Symbol locality at link.** RTOS log APIs are native C symbols. Each
   platform crate already brokers its target's toolchain + linker. A
   portable facade reaching them directly would force every consumer to
   juggle a per-target feature set.
2. **Toolchain isolation.** `cortex-m-semihosting`, `defmt`,
   `esp-idf-sys` each need target-specific startup / linker config.
   Pulling them into `nros-log` defeats portability. They stay where
   they belong — inside the per-platform crate.
3. **ISR-safety policy is platform knowledge.** Only `nros-platform-zephyr`
   knows that Zephyr LOG is ISR-safe; only `nros-platform-bare-metal`
   knows semihosting is not. Centralizing the policy in the ABI impl
   beats duplicating it across every consumer.
4. **Board-level override is one fn pointer.** FreeRTOS / ThreadX have
   no native logger — board registers a UART writer fn-ptr with the
   platform crate ONCE. Every nros consumer (`nros-node`, `nros-cpp`,
   future `/rosout`) inherits.
5. **Phase 79/80/129 consistency.** net / mutex / condvar / task /
   clock / random / yield all live in `nros-platform`. Logging is the
   last text-output surface still ad-hoc. Same crate boundary = same
   review pattern = less cognitive overhead.
6. **C/C++ bindings free.** `nros-c` / `nros-cpp` already reach
   `nros-platform` via the cffi vtable. `nros_node_get_logger()`
   becomes a tiny shim — no new vtable entry per RMW backend.
7. **`/rosout` future = ABI consumer, not facade rewrite.** When
   `/rosout` lands, it registers as a fn-pointer slot consumer on top
   of `nros_platform_log_write`. `nros-log` source unchanged.

### Key design decisions

1. **Single portable facade + per-platform ABI impls.** `nros-log` =
   `Severity` + `Record` + `Logger` + macros + dispatcher + `PlatformSink`.
   No backend code. Per-platform deliveries live in
   `nros-platform-<rtos>` extending the existing ABI.
2. **Format at the call site** into a `heapless::String<N>` (default
   N=256, tunable via `buffer-size-<N>` feature). The ABI receives
   `(severity: u8, name_ptr, name_len, msg_ptr, msg_len)` — fully
   formatted text. Forfeits Zephyr LOG's deferred-formatting advantage
   but keeps the ABI uniform across very different backends
   (`/rosout` / ESP-IDF / UART writers all need formed text anyway).
3. **Multi-sink dispatch via `nros-log`'s `&'static [&dyn LogSink]`.**
   `PlatformSink` is just one sink — apps that want fan-out (`stdout +
   /rosout`, etc.) register additional sinks alongside it. Lock-free
   read path; sink list set once at `init`.
4. **Compile-time level ceiling** via `max-level-*` features on
   `nros-log` (same model as the `log` crate). Below-ceiling macros
   expand to `()` so the format call is dead-code eliminated.
5. **Per-logger runtime threshold** stored on the `Logger` itself
   (`Logger { name: &'static str, level: AtomicU8 }`). Small intern
   table (bounded at `MAX_LOGGERS = 32`) so `get_logger("name")`
   resolves to the same instance across call sites that share a name.
6. **Board fn-ptr override for "no native logger" platforms.**
   FreeRTOS, ThreadX, and bare-metal expose a `register_log_writer`
   helper in their `nros-platform-<rtos>` crate. Board crates call it
   once at startup (e.g. `nros-board-mps2-an385-freertos` registers a
   UART writer). Default = null → ABI no-ops.
7. **Bare-metal opt-out is free.** Null writer = 1-instruction return
   from `nros_platform_log_write`. No new `max-level-off` sentinel
   needed at the facade.

### Backend impls (one per `nros-platform-<rtos>`)

| Platform crate            | Native target                                  | ISR-safe |
|---------------------------|------------------------------------------------|----------|
| `nros-platform-posix`     | `fwrite(stderr)` + `\n`                        | N/A      |
| `nros-platform-zephyr`    | `log_msg_runtime_create` (fallback `printk`)   | yes      |
| `nros-platform-esp-idf`   | `esp_log_write(level, tag, "%s", buf)`         | partial  |
| `nros-platform-nuttx`     | `syslog(priority, "%s", buf)`                  | yes      |
| `nros-platform-freertos`  | board-registered UART writer fn-ptr            | caller   |
| `nros-platform-threadx`   | board-registered UART writer fn-ptr            | caller   |
| `nros-platform-bare-metal`| board-registered writer fn-ptr (semihosting / defmt / RTT / …) | caller |

## Work Items

- [ ] 88.1 — Create `packages/core/nros-log/` portable facade:
      `Cargo.toml` (no target deps; only `heapless`), `src/lib.rs`
      with `Severity`, `Record<'a>`, `Logger`, trait `LogSink`,
      lock-free dispatcher, intern table, `nros_log::get_logger(name)`,
      `nros_log::init(&'static [&dyn LogSink])`, `flush()`. Default
      Cargo features pick the buffer size + compile-time ceiling
      only — no sinks here.

- [ ] 88.2 — Macros in `nros-log`: `nros_trace!`, `nros_debug!`,
      `nros_info!`, `nros_warn!`, `nros_error!`, `nros_fatal!`, plus
      `*_throttle!(logger, ms, …)`, `*_once!`, `*_skipfirst!`.
      Formatting uses `heapless::String<N>` with `N` controlled by a
      `buffer-size-<N>` feature family (default 256). Compile-time
      ceiling via `max-level-*` features; macros below the ceiling
      expand to `()`.

- [ ] 88.3 — `nros-platform-api` ABI extension:
      ```c
      void nros_platform_log_write(
          uint8_t  severity,
          const uint8_t *name_ptr, uintptr_t name_len,
          const uint8_t *msg_ptr,  uintptr_t msg_len);
      void nros_platform_log_flush(void);
      ```
      Stable Rust signature on the producer side, `#[unsafe(no_mangle)]
      extern "C"` on each implementor. Severity = `nros_log::Severity::as_u8()`.

- [ ] 88.4 — `PlatformSink` in `nros-log`: thin `LogSink` impl that
      forwards `&Record` to `nros_platform_log_write`. The default sink
      list for `nros_log::init` includes this when the user passes
      `nros_log::sinks::default()`.

- [ ] 88.5 — POSIX impl in `nros-platform-posix`:
      `nros_platform_log_write` → `fwrite(stderr) + \n`. Severity ↦
      `[INFO]` / `[WARN]` / … prefix. `nros_platform_log_flush` →
      `fflush(stderr)`.

- [ ] 88.6 — Zephyr impl in `nros-platform-zephyr` (+ `zephyr/`
      module glue): FFI to `log_msg_runtime_create` (fallback `printk`
      under `CONFIG_LOG=n`). Severity ↦ Zephyr `LOG_LEVEL_*`. Module
      registered as `LOG_MODULE_DECLARE(nros)` so it shows up in
      Zephyr's runtime-filter shell commands.

- [ ] 88.7 — ESP-IDF impl in `nros-platform-esp-idf`: FFI to
      `esp_log_write`. Severity ↦ `ESP_LOG_*`. Uses the logger name as
      the TAG; converts to a null-terminated `CStr` via a small
      `heapless` buffer.

- [ ] 88.8 — NuttX impl in `nros-platform-nuttx`: FFI to
      `syslog(priority, "%s", buf)`. Severity ↦ `LOG_ERR` /
      `LOG_WARNING` / `LOG_INFO` / `LOG_DEBUG`.

- [ ] 88.9 — FreeRTOS + ThreadX + bare-metal:
      - `nros-platform-freertos`: expose
        `register_log_writer(fn(Severity, &str))`. Default = null. Board
        provides the writer (e.g. UART or `configPRINTF`).
      - `nros-platform-threadx`: same shape — board registers writer.
      - `nros-platform-bare-metal`: same shape — board crate registers
        semihosting / defmt / RTT writer.

- [ ] 88.10 — Optional `log-compat` feature on `nros-log`: provide a
      `log::Log` impl that forwards to the same dispatcher, and a
      reverse bridge (`nros_log::LogSink` wrapping a `log::Log`). Lets
      existing ecosystem crates (that use `log::info!`) integrate
      without duplicating output.

- [ ] 88.11 — Board-crate wiring: replace ad-hoc output paths with the
      new platform impls:
      - `nros-board-mps2-an385` → register semihosting writer with
        `nros-platform-bare-metal`; delete the custom `println!` macro
        at `packages/boards/nros-board-mps2-an385/src/lib.rs`.
      - `nros-board-stm32f4` → register defmt writer with
        `nros-platform-bare-metal`; drop direct `defmt::…` call sites
        in `packages/boards/nros-board-stm32f4/src/node.rs`.
      - `nros-board-esp32` / `nros-board-esp32-qemu` → no change
        (impl is in `nros-platform-esp-idf`).
      - `nros-board-mps2-an385-freertos` → register UART writer with
        `nros-platform-freertos`.
      - `nros-board-nuttx-qemu-arm` → no change (impl is in
        `nros-platform-nuttx`).
      - `nros-threadx-*` → register UART writer with
        `nros-platform-threadx`.
      - Zephyr: `zephyr/` module exposes Kconfig
        `CONFIG_NROS_LOG` (already covered by the platform impl when
        enabled).

- [ ] 88.12 — Node integration: `Node::logger() -> &Logger` on the
      Rust API (`nros-node`), `nros_node_get_logger(node)` on the C API
      (`nros-c`), and `Node::get_logger()` on the C++ API (`nros-cpp`).
      Logger name matches the node name (no namespace logic in v1; we
      can add `get_child("subcomponent")` as a follow-up).

- [ ] 88.13 — Examples + docs: one minimal `logging/` example per
      language (Rust, C, C++) that demonstrates per-severity macros and
      runtime threshold adjustment. Extend `book/src/user-guide/` with a
      `logging.md` chapter; extend `book/src/reference/rust-api.md`,
      `c-api.md`, `cpp-api.md` with the `Logger` surface.

- [x] 88.14 — Tests: a `packages/testing/nros-tests/tests/logging.rs`
      verifying compile-time ceiling, per-logger runtime threshold,
      sink fan-out (every installed sink receives every dispatched
      record), and that filtered records reach no sink. Throttle/once
      coverage is deferred along with the macros themselves — the test
      file documents how to extend it when the macros land. RTOS-specific
      UART-capture verification stays best-effort and lives with the
      per-platform smoke tests.

## Design Notes

- **`Logger` shape**:
  ```rust
  pub struct Logger {
      name: &'static str,
      level: AtomicU8,   // Severity threshold
  }
  ```
  Clone-free. `get_logger("x")` returns a `&'static Logger` backed by
  a bounded intern table (`MAX_LOGGERS = 32`). Names not in the table
  resolve to the `DEFAULT_LOGGER ("nros")` catch-all — keeps the API
  total without unbounded `no_std` state.
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
- **Dispatcher**: lock-free read path. Sink list pointer set once at
  `nros_log::init()` via Release store; `dispatch()` does Acquire load
  + iterates. No critical-section needed for hot path. The PlatformSink
  then funnels to `nros_platform_log_write`.
- **Recursion guard**: short-circuit `log()` if called re-entrantly
  from inside a sink (e.g., a UART writer panics and the panic handler
  logs). One `AtomicBool` per thread / per-core is enough on the
  platforms we care about.
- **Compile-time ceiling model**: each `nros_<level>!` macro checks
  `severity_enabled_at_compile_time(Severity::X)`. The function is
  `const`, so the compiler dead-code-eliminates below-ceiling
  expansions. Mirrors `log` crate model — don't invent a new one.
- **Buffer-overflow policy**: if the formatted message exceeds the
  configured `heapless::String<N>`, truncate and append `…` rather
  than dropping the message. The `log()` call never fails; overflow
  increments a per-sink counter accessible via a debug helper.
- **`defmt` interop on bare-metal**: when the board registers a defmt
  writer with `nros-platform-bare-metal`, the writer calls
  `defmt::info!("{=str}", msg)` etc. — interns the format string once,
  sends the message as a `str` payload. Strictly worse for flash
  footprint than native defmt call sites; users who want full defmt
  ergonomics should call `defmt::info!` directly. The platform-ABI
  surface stays uniform.

## Acceptance Criteria

- [ ] `nros-log` is a workspace member under `packages/core/nros-log/`;
      `just ci` passes with no sinks wired (library-only case) and with
      `PlatformSink` wired.
- [ ] `nros_platform_log_write` / `nros_platform_log_flush` ABI lives
      in `nros-platform-api` with `#[unsafe(no_mangle)] extern "C"`
      Rust signatures + the matching C declarations.
- [ ] On native (`platform-posix`), an example calling
      `nros_info!(logger, "hello")` emits a line on stderr with severity
      tag, logger name, and message via `PlatformSink → nros-platform-posix`.
- [ ] On Zephyr QEMU, `nros_info!` output appears in the Zephyr log
      output (visible via `CONFIG_LOG=y` in the board's Kconfig), tagged
      with the `nros` module and the correct severity, via
      `PlatformSink → nros-platform-zephyr`.
- [ ] On ESP32 QEMU, `nros_info!` output is visible via `idf.py monitor`
      equivalent (QEMU UART capture) with the correct TAG and severity,
      via `PlatformSink → nros-platform-esp-idf`.
- [ ] On MPS2-AN385 bare-metal QEMU, `nros_info!` output appears on the
      semihosting console via `PlatformSink → nros-platform-bare-metal →
      board-registered semihosting writer`.
- [ ] Per-logger runtime threshold works: setting
      `logger.set_level(Severity::Warn)` suppresses `Debug` / `Info`
      calls on that logger without affecting other loggers.
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
  `nros-log` has stabilized. Nothing in 88.1–88.14 should preclude it —
  `Record<'a>` stays cheap to clone into a queued owned form when that
  follow-up lands, and `/rosout` becomes just another `LogSink`
  alongside `PlatformSink`.
- **`log` crate interop** (88.10) is optional. We intentionally do NOT
  base the public API on the `log` crate, because ROS-style named
  loggers don't map cleanly to the `log` crate's string-target filter
  model. The bridge is there for ecosystem interop, not as the
  primary surface.
- **Why not `tracing`?** `tracing` is a better fit for rich
  structured/span logging on `std` targets but is overkill here and
  drags in dependencies we don't want on `no_std`. If a future user
  wants span-level instrumentation, we can add a `tracing-compat`
  feature later without changing the core facade.
- **Feature collapse**: `nros-log` `default = ["max-level-trace",
  "buffer-size-256"]`. No sinks default-on; apps register
  `PlatformSink` (or a custom sink) explicitly at `init()`. Matches
  how `nros-rmw-zenoh` makes backend selection explicit.
- **Pre-2026-05-19 design (rejected).** Earlier revision put per-backend
  sinks behind `sink-{stdout,semihosting,defmt,zephyr,esp-idf,nuttx,
  freertos,threadx}` Cargo features inside `nros-log`. That mirrored
  `nros-rmw-zenoh`'s pre-Phase-129 shape. Phase 129 retired that pattern
  (every platform feature now flows through `nros_platform_*`).
  This revision aligns `nros-log` with the post-Phase-129 architecture:
  portable facade + per-platform ABI impls.
- **Follow-ups this phase does not cover**: `/rosout` sink,
  rclcpp-style child loggers (`logger.get_child("sub")`), structured
  key-value logging (ROS 2 doesn't standardize this yet either), and
  per-module compile-time ceilings via the `log` crate's
  `max_level_<level>_feature` convention.
