# Platform API and C ABI

<!-- type: reference · level: 4 -->

The platform API is the porting boundary between nano-ros and a
concrete OS / RTOS / bare-metal target. Each platform provides a
clock, optionally a heap, optionally threading, optionally
networking. Platform is **internal** — user applications use the
[Rust](rust-api.md) / [C](c-api.md) / [C++](cpp-api.md) APIs, not
the platform vtable directly.

nano-ros separates the platform layer (clock, sleep, allocator,
threading, critical section, network, timer, …) from the rest of the
code via a **single canonical C ABI** of free `extern "C"` symbols.
Every supported port — POSIX, FreeRTOS, NuttX, ThreadX, Zephyr,
bare-metal Cortex-M, bare-metal RISC-V — implements the
same symbols against its host kernel. RMW backends, codegen output, and the
`nros-node` runtime all link against the ABI; nobody links against a
specific port's Rust crate.

## Canonical reference

The SSoT C headers in
`packages/platform/nros-platform-api/include/nros/` (`platform.h` +
`platform_net.h`, `platform_timer.h`, `platform_zephyr.h`; Rust consumes committed bindgen output from them) are the source of
truth. Every symbol's brief, parameter docs, ownership rules,
blocking / non-blocking classification, and ISR-safe contract live in
the Doxygen compiled from those headers.

| Surface | Link |
|---|---|
| **platform ABI Doxygen** (canonical) | [HTML](../api/platform-cffi/index.html) · [headers](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/platform/nros-platform-api/include/nros) |

Regenerating the Doxygen in the tree: see
[Internals → Build Commands](../internals/dev-env/build-commands.html#api-documentation).

This page does **not** duplicate the interface specification — read
the Doxygen for that.

## Surface

Three hand-written headers under
`packages/platform/nros-platform-cffi/include/nros/`:

| Header | Purpose |
|---|---|
| `platform.h` | Core kernel surface: clock, sleep, alloc + heap stats, threading, scheduler, time, yield, random, critical section, opaque wake primitive. |
| `platform_net.h` | Network surface: TCP/UDP/multicast socket helpers, endpoint resolution, IVC. |
| `platform_timer.h` | Periodic timer surface (`nros_platform_timer_*`). |

Every symbol has the prefix `nros_platform_`. A drift gate
(`scripts/check-platform-abi-mirror.sh`) walks each header and asserts
that the `unsafe extern "C" {}` mirror block in
`packages/platform/nros-platform-cffi/src/lib.rs` declares the same set —
no symbol can land in C without its Rust mirror, no Rust mirror can
declare a symbol without a C header decl. CI runs the gate on every
build.

## Capability groups

Within `platform.h`, the symbols are grouped by trait. Each group is
exported by one Rust trait + one C-port section + one drift-gate
match. Adding a capability means adding a row to **every** column.

| Capability | Rust trait | C section | Symbols |
|---|---|---|---|
| Clock | `PlatformClock` | `clock_ns` | `nros_platform_clock_ns` |
| Sleep | `PlatformSleep` | `sleep_ms` | `nros_platform_sleep_ms` |
| Alloc | `PlatformAlloc` | `malloc/realloc/free` | `nros_platform_alloc{,_realloc,_free}` |
| Threading | `PlatformThreading` | mutex/condvar/task | `nros_platform_{mutex,condvar,task}_*` |
| Critical section | `PlatformCriticalSection` | per-CPU interrupt mask | `nros_platform_critical_section_{acquire,release}` |
| Scheduler | `PlatformScheduler` | task hints | `nros_platform_scheduler_*` |
| Time | `PlatformTime` | wall-clock | `nros_platform_time_now_ns` |
| Yield | `PlatformYield` | cooperative yield | `nros_platform_yield_now` |
| Random | `PlatformRandom` | best-effort RNG | `nros_platform_random_*` |
| Wake | `PlatformThreading` (wake methods) | opaque binary-semaphore | `nros_platform_wake_{init,drop,wait_ms,signal,signal_from_isr,storage_size,storage_align}` |

`platform_net.h` covers TCP/UDP/multicast/IVC, and `platform_timer.h`
covers periodic timers. The shapes follow the same pattern: trait →
macro → header → drift gate.

## How a port is built

A port can be written in either of two ways. The choice is per-port
and not visible to consumers — both shapes resolve to the same
symbols.

### Path A — Rust trait + macro export

The Rust crate implements `PlatformClock`, `PlatformAlloc`,
`PlatformThreading`, … on a marker type, then invokes one of the
export macros:

```rust
use nros_platform_api::{
    PlatformClock, PlatformSleep, PlatformAlloc, PlatformThreading,
    PlatformCriticalSection,
    nros_platform_export, nros_platform_export_net, nros_platform_export_timer,
};

pub struct PosixPlatform;
impl PlatformClock for PosixPlatform { /* … */ }
impl PlatformSleep for PosixPlatform { /* … */ }
// … etc.

nros_platform_export!(PosixPlatform);
nros_platform_export_net!(PosixPlatform);
nros_platform_export_timer!(PosixPlatform);
```

The macros expand to the full set of `#[unsafe(no_mangle)] pub
extern "C" fn nros_platform_*` bodies forwarding to the trait calls.
This is what `nros-platform-posix` does today. It is also what the
bare-metal board crates (`nros-platform-mps2-an385`,
`nros-platform-stm32f4`)
use: there is no host kernel to write idiomatic C against, so the
single-task stub impls are written in Rust and exported via the macro.

### Path B — pure C port

The kernel-side ports (FreeRTOS, NuttX, ThreadX, Zephyr) write the
bodies directly in `src/platform.c`, `src/net.c`, `src/timer.c`
inside each `packages/platform/nros-platform-<rtos>/` directory. The Rust
side has no implementation file — the build script (or parent build
system: NuttX make, Zephyr west module) compiles the C
sources against the kernel's headers.

Pure-C bodies are the more natural fit when the kernel already speaks
C and ships C headers (`xSemaphoreCreate*`, `k_sem_*`, `tx_thread_*`):
the impl is one-to-one with the kernel call, no FFI dance through
Rust's calling convention. The shared crate
`nros-platform-critical-section` is the canonical example for a
single-symbol shim: the Rust side just calls the externs and registers
the result with `critical_section::set_impl!`.

## Design

Why the ABI is free symbols rather than a vtable, and why threading and
sockets stay per-RTOS while the scalar services are unified, is in
[Internals → Platform API design](../internals/platform/platform-api-design.html).

## Reference implementations

Each row is a complete worked example. The crate's `README.md`
walks the implementation; the source is the worked solution to
copy.

| Crate | Target | Source |
|---|---|---|
| `nros-platform-posix` | Linux / *BSD | [packages/platform/nros-platform-posix](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/platform/nros-platform-posix) |
| `nros-platform-nuttx` | NuttX RTOS | [packages/platform/nros-platform-nuttx](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/platform/nros-platform-nuttx) |
| `nros-platform-freertos` | FreeRTOS | [packages/platform/nros-platform-freertos](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/platform/nros-platform-freertos) |
| `nros-platform-threadx` | Azure RTOS / ThreadX | [packages/platform/nros-platform-threadx](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/platform/nros-platform-threadx) |
| `nros-platform-zephyr` | Zephyr RTOS | [packages/platform/nros-platform-zephyr](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/platform/nros-platform-zephyr) |
| `nros-platform-mps2-an385` | Cortex-M3 (QEMU) | [packages/platform/nros-platform-mps2-an385](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/platform/nros-platform-mps2-an385) |
| `nros-platform-stm32f4` | STM32F4 | [packages/platform/nros-platform-stm32f4](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/platform/nros-platform-stm32f4) |

The POSIX implementation is the canonical reference port.

## Writing a custom platform

- The task, step by step: [Add a platform](../extending/add-a-platform.md).
- Per-platform behaviour matrix:
  [Platform Differences](matrices/platform-differences.md).

## Design background

- [RFC-0054: C header ABI single source of truth](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0054-c-header-abi-ssot.md) — why the C headers are canonical and Rust uses generated bindings
- [RFC-0034: platform layer split](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0034-platform-layer-split.md) — the scalar / opaque-struct boundary (D2) and the allocation gate (D8)
