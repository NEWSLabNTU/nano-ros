# Porting to a New Platform

This guide explains how to add a new platform to nano-ros. A "platform" means
an RTOS or bare-metal environment that nano-ros runs on.

## How nano-ros accesses the system

nano-ros core (`nros-node`, `nros-core`, `nros-serdes`, etc.) is `#![no_std]`
and **has zero platform calls**. All OS/hardware access is delegated to a
unified platform layer:

```
Application
  └── nros-node            (pure no_std Rust — Executor, Node, pub/sub, actions)
       └── nros-rmw        (trait layer — abstracts transport)
            ├── nros-rmw-zenoh   → zpico-sys → zpico-platform-shim → nros-platform → nros-platform-<name>
            └── nros-rmw-xrce    → xrce-sys  → xrce-platform-shim  → nros-platform → nros-platform-<name>
```

When you "port nano-ros to a new platform", you create an
**`nros-platform-<name>` crate** that implements the platform primitives
(clock, memory, sleep, random, threading). The RMW transport libraries access
these primitives through thin shim layers inside `zpico-sys` and `xrce-sys`
that forward FFI symbols (`z_*`, `uxr_*`) to the `ConcretePlatform` type
alias from `nros-platform`.

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

## Three-crate pattern

Every embedded platform uses a three-layer split:

| Crate | Purpose | Dependencies |
|-------|---------|--------------|
| `nros-platform-<name>` | Platform primitives (clock, memory, sleep, random, threading) | Zero nros dependencies |
| `zpico-platform-shim` / `xrce-platform-shim` | FFI symbol mapping (`z_*` / `uxr_*` to `ConcretePlatform`) | Inside `zpico-sys` / `xrce-sys` |
| `nros-<name>` | User-facing board crate (`Config`, `run()`, re-exports) | `nros-platform-<name>` + `nros-node` |

The platform crate is RMW-agnostic -- it knows nothing about zenoh-pico or
XRCE-DDS. The shim layers inside the RMW sys crates map transport-specific
FFI symbols to the unified `ConcretePlatform` trait. Board crates depend on
the platform crate for force-linking the symbols into the final binary.

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

## Guides

- [Implementing a Platform](./implementing-a-platform.md) — how to create an `nros-platform-<name>` crate (start here)
- [Zenoh-pico Symbol Reference](./zenoh-pico.md) — the ~55 FFI symbols mapped by `zpico-platform-shim`
- [XRCE-DDS Symbol Reference](./xrce-dds.md) — the 2-3 FFI symbols mapped by `xrce-platform-shim`
