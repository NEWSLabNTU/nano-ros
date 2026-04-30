# nros-platform-nuttx

NuttX platform implementation for nano-ros. NuttX exposes a near-POSIX
syscall surface, so this crate is a thin re-export of POSIX semantics
plus a few NuttX-specific quirks.

## Role

Implements the trait family in
[`nros-platform-api`](../nros-platform-api) on top of NuttX's POSIX
layer: `clock_gettime`, libc `malloc` / `free`, pthreads + POSIX
semaphores. Networking is delegated to zenoh-pico's C `unix/network.c`
since NuttX's BSD socket layer is fully POSIX-compatible.

## Source layout

| File | Role |
|------|------|
| `src/lib.rs` | `NuttXPlatform` zero-sized type + trait impls (mostly POSIX aliases). |
| `src/sync.rs` | NuttX-specific condvar replacement using `sem_timedwait` (pthread condvars hang on NuttX — see Phase 55.12). |

## When to use

- NuttX RTOS-based target, qemu-arm or real hardware.
- POSIX-flavoured embedded development without a full Linux kernel.

## Caveats

- pthread condvars are unreliable on some NuttX revisions — this crate
  uses a `sem_timedwait`-based replacement; do not switch back without
  rerunning the full `just nuttx test` suite.
- `z_open` first-call is sensitive to task stack size and to the order
  of network-stack init (see the NuttX-investigation memory entry).

## See also

- [Custom Platform porting guide](../../../book/src/porting/custom-platform.md)
- Source on GitHub: <https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/core/nros-platform-nuttx>
