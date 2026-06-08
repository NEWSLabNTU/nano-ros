# Canonical Platform C ABI

nano-ros separates the platform layer (clock, sleep, allocator,
threading, critical section, network, timer, …) from the rest of the
code via a **single canonical C ABI** of free `extern "C"` symbols.
Every supported port — POSIX, FreeRTOS, NuttX, ThreadX, Zephyr,
ESP-IDF, bare-metal Cortex-M — implements the same symbols against
its host kernel. RMW backends, codegen output, and the
`nros-node` runtime all link against the ABI; nobody links against a
specific port's Rust crate.

This page is the contract: what the surface looks like, how to add a
port, and why the shape is what it is. It is the implementation
companion to [Platform Model](../concepts/platform-model.md) (user-
facing axis description) and
[`docs/design/0006-portable-rmw-platform-interface.md`](../../docs/design/0006-portable-rmw-platform-interface.md)
(L0/L1/L2 design rationale across RMW + platform).

## Surface

Three hand-written headers under
`packages/core/nros-platform-cffi/include/nros/`:

| Header | Purpose | Symbol count |
|---|---|---|
| `platform.h` | Core kernel surface: clock, sleep, alloc, threading, scheduler, time, yield, random, critical section, opaque wake primitive. | 57 |
| `platform_net.h` | Network surface: TCP/UDP/multicast socket helpers, endpoint resolution, IVC. | 29 |
| `platform_timer.h` | Periodic timer surface (`nros_platform_timer_*`). | 8 |

Every symbol has the prefix `nros_platform_`. A drift gate
(`scripts/check-platform-abi-mirror.sh`) walks each header and asserts
that the `unsafe extern "C" {}` mirror block in
`packages/core/nros-platform-cffi/src/lib.rs` declares the same set —
no symbol can land in C without its Rust mirror, no Rust mirror can
declare a symbol without a C header decl. `just check` runs the gate
on every CI build.

## Why free symbols (not a vtable struct)

The RMW layer uses a `NrosRmwVtable` fn-ptr struct because a single
binary registers multiple backends at runtime (bridge nodes). Platforms
are different: **one platform per binary, resolved at link time**. A
vtable would add an indirection on every clock read, every mutex lock,
every socket send — overhead with no upside, because there is nobody to
swap. Free symbols let the linker resolve calls direct, let LTO inline
across the boundary, and let static analysis treat them like any other
extern.

The full rationale is in
[`docs/design/0006-portable-rmw-platform-interface.md`](../../docs/design/0006-portable-rmw-platform-interface.md)
under "Platform ABI: free symbols (no vtable)".

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
`nros-platform-stm32f4`, `nros-platform-esp32`, `nros-platform-esp32-qemu`)
use: there is no host kernel to write idiomatic C against, so the
single-task stub impls are written in Rust and exported via the macro.

### Path B — pure C port

The kernel-side ports (FreeRTOS, NuttX, ThreadX, Zephyr, ESP-IDF) write
the bodies directly in `src/platform.c`, `src/net.c`, `src/timer.c`
inside each `packages/core/nros-platform-<rtos>/` directory. The Rust
side has no implementation file — the build script (or parent build
system: NuttX make, Zephyr west module, ESP-IDF cmake) compiles the C
sources against the kernel's headers.

Pure-C bodies are the more natural fit when the kernel already speaks
C and ships C headers (`xSemaphoreCreate*`, `k_sem_*`, `tx_thread_*`):
the impl is one-to-one with the kernel call, no FFI dance through
Rust's calling convention. The shared crate
`nros-platform-critical-section` is the canonical example for a
single-symbol shim: the Rust side just calls the externs and registers
the result with `critical_section::set_impl!`.

## Adding a new port — checklist

1. **Decide A vs B.** Greenfield host build with a Rust crate
   available? Path A. Vendor RTOS with a C SDK and no Rust toolchain
   on the build machine? Path B.
2. **Mirror, don't extend.** Implement every symbol in the three
   headers. The drift gate fails the build otherwise. If the kernel
   genuinely cannot provide a primitive, return the documented
   sentinel value (`-1` for `task_init` on single-task RTOS, `0` for
   `mutex_*` on single-core no-preempt hardware) rather than skipping
   the symbol.
3. **Write the smoke test.** Land a `tests/<port>-c-smoke/` mini-app
   that links the new C port against the canonical headers and calls
   a representative symbol from each capability group. Wire it into
   `just <port> test-c-port`. The smoke tests are the runtime
   parity layer that the drift gate cannot enforce.
4. **For platforms that emit `critical_section::Impl`** (i.e. anything
   that consumes a `critical-section`-using crate),
   make sure the binary pulls
   `nros-platform-critical-section` once — it does the
   `critical_section::set_impl!(PlatformCs)` registration against the
   canonical externs. Binaries that don't need the global registration
   don't pay for it.
5. **Update the drift gate's smoke list** (the
   `HEADERS_REQUIRE_MACRO` array in
   `scripts/check-platform-abi-mirror.sh`) if you add a new header.
   Adding a symbol to an existing header needs no script change — the
   grep is generic.

## Capability groups

Within `platform.h`, the symbols are grouped by trait. Each group is
exported by one Rust trait + one C-port section + one drift-gate
match. Adding a capability means adding a row to **every** column.

| Capability | Rust trait | C section | Symbols |
|---|---|---|---|
| Clock | `PlatformClock` | `clock_ms` | `nros_platform_clock_ms` |
| Sleep | `PlatformSleep` | `sleep_ms` | `nros_platform_sleep_ms` |
| Alloc | `PlatformAlloc` | `malloc/realloc/free` | `nros_platform_alloc{,_realloc,_free}` |
| Threading | `PlatformThreading` | mutex/condvar/task | `nros_platform_{mutex,condvar,task}_*` |
| Critical section | `PlatformCriticalSection` | per-CPU interrupt mask | `nros_platform_critical_section_{acquire,release}` |
| Scheduler | `PlatformScheduler` | task hints | `nros_platform_scheduler_*` |
| Time | `PlatformTime` | wall-clock | `nros_platform_time_ns` |
| Yield | `PlatformYield` | cooperative yield | `nros_platform_yield` |
| Random | `PlatformRandom` | best-effort RNG | `nros_platform_random_*` |
| Wake | `PlatformThreading` (wake methods) | opaque binary-semaphore | `nros_platform_wake_{init,drop,wait_ms,signal,signal_from_isr,storage_size,storage_align}` |

`platform_net.h` covers TCP/UDP/multicast/IVC, and `platform_timer.h`
covers periodic timers. The shapes follow the same pattern: trait →
macro → header → drift gate.

## Status and architecture

The platform tier today is:

- One canonical header per capability family (`platform.h`,
  `platform_net.h`, `platform_timer.h`). Every port mirrors them
  exactly; a drift gate fails the build on divergence.
- Pure-C ports under `nros-platform-{posix,freertos,nuttx,threadx,zephyr,esp-idf}`
  — the per-RTOS Rust platform crates were retired in favour of
  one C body per RTOS.
- `critical_section` promoted to a canonical platform capability
  owned by every port's C body; `nros-platform-critical-section`
  is the global-registration shim.
- An opaque-storage wake primitive (`nros_platform_wake_*`) — a
  binary semaphore that lets the executor block on RMW activity
  without burning a thread.

The current canonical surface is 57 + 29 + 8 = **94 symbols** across
three headers, mirrored exactly by the Rust extern block, exported by
six ports plus four bare-metal board crates, and gated by one drift
script. Adding a capability touches all four columns in one PR.
