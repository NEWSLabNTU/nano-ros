# XRCE-DDS Symbol Reference

XRCE-DDS has a minimal platform abstraction layer. It is single-threaded,
heap-less, and delegates networking to user-provided transport callbacks.
The 2-3 required FFI symbols are provided by `xrce-platform-shim` (inside
`xrce-sys`), which forwards `uxr_*` calls to the `ConcretePlatform` type
alias from `nros-platform`. When porting to a new platform, you implement an
`nros-platform-<name>` crate (see [Implementing a Platform](./implementing-a-platform.md))
rather than providing these symbols directly.

## Platform crate structure

The XRCE-DDS clock symbols are a subset of what the platform crate provides.
Your `nros-platform-<name>` crate provides the clock primitives and
`xrce-platform-shim` maps them to the `uxr_*` symbols XRCE-DDS expects.

```
packages/core/nros-platform-<name>/
├── Cargo.toml
└── src/
    └── lib.rs          # clock + other primitives
```

`Cargo.toml` must have **zero** `nros-*` dependencies. It may depend on:
- Hardware HAL crate (e.g., `stm32f4xx-hal`, `esp-hal`)
- `xrce-smoltcp` (if using smoltcp networking)

## Required FFI symbols

### Clock (2 symbols)

XRCE-DDS uses the clock for session timeouts and time synchronization.

```c
// Monotonic millisecond clock
i64 uxr_millis(void);

// Monotonic nanosecond clock
i64 uxr_nanos(void);
```

These must be backed by a hardware timer or OS tick — same rules as the
zenoh-pico clock.

**Reference implementations:**

| Platform | Clock source | File |
|----------|-------------|------|
| MPS2-AN385 | CMSDK APB Timer0 (25 MHz) | `nros-platform-mps2-an385/src/clock.rs` |
| Zephyr | `k_uptime_get()` | `xrce-zephyr/src/xrce_zephyr.c` |
| POSIX | `clock_gettime(CLOCK_MONOTONIC)` | Built-in `time.c` from XRCE library |

### smoltcp clock (1 symbol, if using smoltcp)

If using `xrce-smoltcp` for bare-metal networking:

```c
u64 smoltcp_clock_now_ms(void);
```

This is the same symbol used by `zpico-smoltcp` — shared clock
implementations work for both RMW backends.

## Transport callbacks

Instead of requiring socket FFI symbols, XRCE-DDS uses a custom transport
abstraction. You register four callbacks via
`uxr_set_custom_transport_callbacks()`:

```c
bool   open(uxrCustomTransport *transport);
bool   close(uxrCustomTransport *transport);
size_t write(uxrCustomTransport *transport, const uint8_t *buf, size_t len, uint8_t *errcode);
size_t read(uxrCustomTransport *transport, uint8_t *buf, size_t len, int timeout_ms, uint8_t *errcode);
```

These are typically provided by a transport crate, not the platform crate:

| Transport | Crate | Description |
|-----------|-------|-------------|
| smoltcp UDP | `xrce-smoltcp` | Bare-metal networking via smoltcp |
| Zephyr UDP | `xrce-zephyr` | Zephyr BSD sockets |
| POSIX UDP | Built-in | Uses XRCE library's POSIX transport |

The transport crate registers its callbacks during initialization. Your
platform crate does not need to implement sockets.

## What you do NOT need

Unlike zenoh-pico, XRCE-DDS does **not** require:

- **Memory symbols** — XRCE-DDS is heap-less; all buffers are statically sized
- **Sleep symbols** — polling is driven by `uxr_run_session_time()`
- **Random symbols** — no session ID randomization needed
- **Threading symbols** — single-threaded by design
- **libc stubs** — Micro-XRCE-DDS has minimal C dependencies

This makes XRCE-DDS the easiest RMW backend to port to new platforms.

## Platform-conditional compilation in xrce-sys

The `xrce-sys` crate uses Cargo features to select platform behavior:

| Feature | Effect |
|---------|--------|
| `posix` | Compiles built-in `time.c` (uses `clock_gettime`, BSD sockets) |
| `bare-metal` | Skips `time.c`; expects `uxr_millis()`/`uxr_nanos()` from platform crate |
| `zephyr` | Compiles `xrce_zephyr.c` (Zephyr kernel APIs) |
| `freertos` | RTOS-specific; skips `time.c` |
| `nuttx` | RTOS-specific; skips `time.c` |
| `threadx` | RTOS-specific; skips `time.c` |

When adding a new platform, you may need to add a feature to `xrce-sys` that
skips the default `time.c` compilation and lets your platform crate provide
the clock symbols.

## Step-by-step procedure

1. **Create the platform crate** — `nros-platform-<name>/` (see
   [Implementing a Platform](./implementing-a-platform.md))
2. **Implement the clock primitives** — `clock_ms()` and friends; the
   `xrce-platform-shim` maps these to `uxr_millis()` and `uxr_nanos()`
3. **Implement `smoltcp_clock_now_ms()`** if using smoltcp transport
4. **Add a feature to `xrce-sys`** for the new platform if needed
5. **Choose or implement a transport crate** — reuse `xrce-smoltcp` for
   bare-metal, or implement a new transport if the platform has its own
   networking stack
6. **Create the board crate** — see [Board Crate Implementation](../board-crate.md)
7. **Add the platform feature** to `nros` with mutual exclusivity checks
8. **Write an example and tests**

## Example: bare-metal MPS2-AN385

The simplest reference is `nros-platform-mps2-an385`, which provides clock
primitives that `xrce-platform-shim` maps to three symbols:

```rust
#[unsafe(no_mangle)]
pub extern "C" fn uxr_millis() -> i64 {
    clock_ms() as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn uxr_nanos() -> i64 {
    clock_ms() as i64 * 1_000_000
}

#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_clock_now_ms() -> u64 {
    clock_ms()
}
```

Where `clock_ms()` reads from a hardware timer with wrap detection.

## Common pitfalls

- **Agent connectivity** — XRCE-DDS requires a Micro-XRCE-DDS Agent running
  on the host. Ensure the agent is reachable from the target before debugging
  the platform layer.
- **Reliable stream history** — `STREAM_HISTORY` must be >= 2 (recommend 4).
  History=1 fails to recycle slots between separate
  `uxr_run_session_until_all_status` calls.
- **Flush after `request_data`** — `uxr_buffer_request_data` must be flushed
  with `uxr_run_session_time` immediately after being called. Unflushed
  requests in the reliable output stream cause intermittent timeouts.
