# Phase 20: Remaining Work (TODO Audit)

## Overview

This phase tracks all remaining TODO items, unimplemented stubs, and future work identified through a codebase-wide audit. Items are grouped by theme and prioritized by impact.

**Status**: Planning

## 1. ~~Async Executor Support~~ — Removed

`spin_async()` has been removed. It spawned an OS thread and required `Send` bounds on zenoh-pico shim types, which are intentionally `!Send` (global C state machine). This design was incompatible with RTOS and bare-metal targets — the primary use case for nano-ros.

**Use `spin_once()` instead.** It works on all platforms and integrates naturally with RTIC async tasks, Embassy tasks, and desktop event loops:

```rust
// RTIC / Embassy / bare-metal
loop {
    executor.spin_once(10); // 10ms timeout
    // yield / delay
}
```

The `async` feature flag and `futures` dependency have also been removed from `nano-ros-node`.

## 2. ~~Parameter Array Types (C API)~~ — Complete

All 5 array parameter types are now supported in the C API using a pointer+length design with caller-owned memory. Added `nano_ros_param_array_t` struct, 15 functions (declare/get/set × 5 types), and 12 unit tests.

## 3. Embassy Integration

**File**: `examples/platform-integration/stm32f4-embassy/src/main.rs:64`

The Embassy example cannot use the full nano-ros executor because zenoh-pico-shim-sys requires a C cross-compilation toolchain visible to `bindgen` at build time.

**Work required**:
- Document the required toolchain setup (arm-none-eabi-gcc in PATH)
- Provide a pre-generated FFI bindings option to avoid runtime bindgen dependency
- Test the full executor integration on STM32F4 with Embassy

**Impact**: Medium — Embassy is increasingly popular for embedded Rust. A working example would demonstrate nano-ros on a major async embedded framework.

## Priority Order

| Priority | Item                     | Effort | Impact                             |
|----------|--------------------------|--------|------------------------------------|
| 1        | Embassy integration (#3) | Low    | Medium — documentation + toolchain |
| —        | ~~Async executor (#1)~~  | —      | Removed — use `spin_once()` instead |
| —        | ~~Parameter arrays (#2)~~| —      | Complete                           |

> **Note**: C API `no_std` backend was moved to [Phase 21](phase-21-c-api-nostd-backend.md).

## Verification

After completing each item:
```bash
just quality               # Core checks
just test-c                # C API tests (item 2)
just test-integration      # Full integration (item 1)
```
