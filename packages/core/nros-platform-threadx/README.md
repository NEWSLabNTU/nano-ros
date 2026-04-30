# nros-platform-threadx

ThreadX platform implementation for nano-ros. Backs the trait family on
Microsoft Azure RTOS ThreadX + NetX Duo. Used by the Linux simulator
build and the QEMU RISC-V build (and any real ThreadX target with the
same API).

## Role

Implements the trait family in
[`nros-platform-api`](../nros-platform-api) on top of ThreadX +
NetX Duo: `tx_time_get` for monotonic time, `tx_byte_allocate` for
heap, ThreadX threads + mutexes + semaphores for threading, NetX Duo
BSD socket shim (`nxd_bsd.c`) for networking.

## Source layout

| File | Role |
|------|------|
| `src/lib.rs` | `ThreadxPlatform` zero-sized type + trait impls. |
| `src/clock.rs` | `tx_time_get` → ms/us. |
| `src/alloc.rs` | `tx_byte_allocate` / `tx_byte_release` shims. |
| `src/thread.rs` | ThreadX thread / mutex / condvar (semaphore-backed). |
| `src/net.rs` | NetX Duo BSD socket bindings. Includes the `set_recv_timeout_ms` helper that wraps the `nx_bsd_timeval` shape mismatch (see CLAUDE.md "NetX Duo BSD Shim Pitfalls"). |

## When to use

- Microsoft Azure RTOS ThreadX target (Linux sim, QEMU RISC-V, real
  Cortex-M / Cortex-R hardware running ThreadX).
- Required: ThreadX kernel + NetX Duo source trees, located via
  `THREADX_DIR` / `NETX_DIR` env vars (defaults to `third-party/threadx/`
  populated by `just threadx_linux setup` / `just threadx_riscv64 setup`).

## Caveats

- `SO_RCVTIMEO` takes `struct nx_bsd_timeval *` (8 bytes on LP64), **not**
  an `INT` ms — passing an INT silently sets `wait_option = NX_WAIT_FOREVER`
  and the recv path deadlocks. Use `set_recv_timeout_ms` from this crate.
- NetX BSD `fcntl(F_SETFL, O_NONBLOCK)` works correctly — preferred for
  cooperative non-blocking sockets where `SO_RCVTIMEO=0` would mean
  "wait forever".
- Linux-sim build requires the NSOS-NetX shim (
  [`packages/drivers/nsos-netx`](../../drivers/nsos-netx))
  to bridge NetX BSD ↔ Linux POSIX.

## See also

- [Custom Platform porting guide](../../../book/src/porting/custom-platform.md)
- CLAUDE.md "NetX Duo BSD Shim Pitfalls" + "ThreadX Linux x86_64 pointer truncation"
- Source on GitHub: <https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/core/nros-platform-threadx>
