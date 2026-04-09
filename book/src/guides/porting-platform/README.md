# Porting to a New Platform

This guide explains how to add a new platform to nano-ros. A "platform" means
an RTOS or bare-metal environment that nano-ros runs on.

## How nano-ros accesses the system

nano-ros core (`nros-node`, `nros-core`, `nros-serdes`, etc.) is `#![no_std]`
and **has zero platform calls**. All OS/hardware access is delegated to the
RMW transport backend:

```
Application
  └── nros-node            (pure no_std Rust — Executor, Node, pub/sub, actions)
       └── nros-rmw        (trait layer — abstracts transport)
            ├── nros-rmw-zenoh   → zpico-platform-*   (clock, memory, threading, sockets, ...)
            └── nros-rmw-xrce    → xrce-platform-*    (clock, transport callbacks)
```

When you "port nano-ros to a new platform", you are really porting the
**transport middleware's platform layer**. The set of required symbols depends
entirely on which RMW backend you use.

## Comparison of RMW platform requirements

| Capability | Zenoh-pico (rmw-zenoh) | XRCE-DDS (rmw-xrce) |
|------------|----------------------|----------------------|
| Clock | 7 symbols (`z_clock_*`) | 2 symbols (`uxr_millis`, `uxr_nanos`) |
| Memory | 3 symbols (`z_malloc`, `z_realloc`, `z_free`) | None (heap-less) |
| Sleep | 3 symbols (`z_sleep_*`) | None |
| Random | 5 symbols (`z_random_*`) | None |
| Time (wall clock) | 5 symbols (`z_time_*`) | None |
| Threading | 19 symbols (tasks, mutexes, condvars) | None |
| Sockets | 4 symbols (if using smoltcp) | Custom transport callbacks |
| libc stubs | ~14 (bare-metal only) | None |
| **Total (bare-metal)** | **~55 symbols** | **2-3 symbols** |

XRCE-DDS is dramatically simpler to port because it is single-threaded, uses
no heap, and delegates networking to user-provided transport callbacks rather
than a BSD socket API.

## Two-crate pattern

Both backends use the same crate split:

| Crate | Purpose | Dependencies |
|-------|---------|--------------|
| `<rmw>-platform-<name>` | `#[unsafe(no_mangle)] extern "C"` FFI symbols | Zero nros dependencies |
| `nros-<name>` | User-facing board crate (`Config`, `run()`, re-exports) | Platform crate + `nros-node` |

This keeps the FFI layer reusable across RMW backends and prevents circular
dependencies.

## Shared steps (both backends)

After implementing the RMW-specific platform crate, the remaining steps are
the same regardless of backend:

1. **Create the board crate** — see [Board Crate Implementation](../board-crate.md)
2. **Add the platform feature** — add `platform-<name>` to the `nros` facade
   crate with mutual exclusivity enforcement
3. **Write an example** — see [Creating Examples](../creating-examples.md)
4. **Add test infrastructure** — add `just test-<name>` recipe and nextest
   group; see [Platform Porting Pitfalls](../../advanced/platform-porting-pitfalls.md)
   for QEMU-specific rules

## Backend-specific guides

- [Zenoh-pico (rmw-zenoh)](./zenoh-pico.md) — full platform abstraction layer (~55 symbols)
- [XRCE-DDS (rmw-xrce)](./xrce-dds.md) — minimal clock + transport callbacks (2-3 symbols)
