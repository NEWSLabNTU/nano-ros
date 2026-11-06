# Add a platform

<!-- type: how-to · level: 4 -->

**Needs first:** [How a build becomes an image](../background/how-a-build-becomes-an-image.md)

You have an RTOS or a bare-metal environment that nano-ros has no platform
for. A platform is the layer under the RMW backend: the clock, sleep, heap,
threads and sockets, exposed as free C functions named `nros_platform_*`. By
the end of this page your port implements those functions, a board names the
platform, and an image runs on it.

Today a platform is registered inside nano-ros itself, so the last part of
this task is a contribution to nano-ros, not a package you keep on your own.
Step 5 says what that contribution touches.

## Before you start

- Check that you need a platform. If your RTOS is one nano-ros supports
  (FreeRTOS, Zephyr, NuttX, ThreadX, bare metal, POSIX), you need a board
  instead: [Add a board](add-a-board.md).
- Know what your RMW backend asks of the platform. XRCE-DDS needs a clock and
  its transport. zenoh-pico needs the whole surface: a heap of about 64 KB,
  threads (or single-thread stubs), sockets, a random source and wall time.
  Cyclone DDS binds the platform's own socket stack rather than these
  functions.
- Have the headers that specify the ABI at hand. They live in
  `packages/platform/nros-platform-api/include/nros/` of nano-ros:
  `platform.h` (clock, sleep, heap, threads, random, time, critical section),
  `platform_net.h` (sockets) and `platform_timer.h` (periodic timers). The
  [Platform API and C ABI](../reference/platform-api.md) reference links
  their rendered documentation.

## Steps

1. Scaffold the platform package:

   ```sh
   nros new platform myos
   ```

   ```text
     created .../nros-platform-myos/Cargo.toml
     created .../nros-platform-myos/src/lib.rs
     created .../nros-platform-myos/nros-platform.toml
   platform package `myos` scaffolded — next: fill in clock/sleep, then a board package: `nros new board <board> --for-platform myos`
   ```

   `src/lib.rs` holds stubs for the first three functions a port needs, a
   monotonic `clock_ns`, its `clock_resolution_ns`, and `sleep_ms`.

2. Choose how you implement the ABI. Both ways produce the same symbols, and
   the code above the platform cannot tell them apart.

   - **In C.** Write the bodies against your kernel's own API, as the
     FreeRTOS, NuttX, ThreadX, Zephyr and POSIX ports do, in `src/platform.c`,
     `src/net.c` and `src/timer.c`. This is the natural fit when the kernel
     ships C headers and you build with its C toolchain.
   - **In Rust.** Implement the traits of `nros-platform-api`
     (`PlatformClock`, `PlatformAlloc`, `PlatformSleep`,
     `PlatformThreading`, …) on a marker type, then export them with the
     macros of `nros-platform-cffi`:
     `nros_platform_export!(MyOsPlatform)`, and
     `nros_platform_export_net!` and `nros_platform_export_timer!` for the
     network and timer headers. The bare-metal ports do this, because there is
     no kernel to write C against.

3. Implement every function the three headers declare. A port that leaves one
   out fails to link. When your kernel lacks a primitive, return the value the
   header documents for it: `nros_platform_task_init` returns
   `NROS_PLATFORM_RET_UNSUPPORTED` on a port with no tasks, and
   `nros_platform_epoch_us` returns `0` with no wall clock. Four rules catch
   most ports:

   - The clock reads a hardware timer or the kernel tick, never a counter
     that only moves when something polls it.
   - Both mutex kinds inherit priority, as every supported port's do. The
     recursive kind lets the same thread lock again: zenoh-pico needs it.
   - The random source is seeded from something that differs between two
     boards (entropy, an address, the wall clock), not from a constant.
   - Nothing panics or throws across the C boundary. Return the error codes
     the header names.

4. State the platform's facts in `nros-platform.toml`. `[capabilities]` holds
   what the software stack can do (`ip_stack`, `serial`, `threads`, …): a
   transport that needs a capability the platform declares `false` is refused
   when the image is configured. Settings any C build on the platform needs
   go at the top of the file, before any table: `arch_profiles`, `compile`
   and `required_env`. Leave the `[knobs.*]` defaults alone until you have
   measured a reason to change one. An empty file is valid.

5. Register the platform in nano-ros. This is the part that changes nano-ros,
   and it is a pull request, built as [Contributing](../internals/orientation/contributing.html)
   describes. It touches:

   - `nros-platform`: a `platform-myos` feature that pulls in your crate, and
     its entry in the list that resolves `ConcretePlatform`. A port that
     ships only C can use the existing `platform-cffi` feature, which takes
     the symbols from whatever library the image links.
   - zenoh-pico, when the platform carries it: an ABI marker feature on
     `zpico-sys` (`myos = []`), and a `[port.myos]` row in the zenoh
     backend's `nros-rmw.toml` that says which zenoh-pico system sources,
     defines and include paths build there. Start from the nearest platform's
     row. XRCE-DDS needs no row: it selects its sources by target.
   - CMake, for C and C++ images: `cmake/platform/nano-ros-myos.cmake`, which
     defines the `NanoRos::Platform` alias, the
     `nros_platform_<plat>_iface` target behind it, the
     `nros_platform_link_app(target)` function for per-image link fixups, and
     the `NROS_PLATFORM_LINK_FEATURES` default.

6. Add a board that names the platform, as [Add a board](add-a-board.md)
   describes, with `platform = "myos"` in its descriptor.

7. Build and run the talker on that board, and check it against a stock ROS 2
   peer:

   ```sh
   nros new sensor_ws --board my-board --template talker
   cd sensor_ws
   nros run sensor_node
   ```

   On hardware, use `nros flash --monitor` instead of `nros run`.

## Example

The first functions of a C port, for a kernel called MyOS. Each body is one
kernel call; the opaque storage the runtime passes in holds the kernel's own
handle:

```c
#include <nros/platform.h>
#include "myos.h"

uint64_t nros_platform_clock_ns(void) {
    return (uint64_t)myos_tick_count() * 1000000u;   /* 1 kHz tick */
}

uint64_t nros_platform_clock_resolution_ns(void) {
    return 1000000u;                                  /* say the tick honestly */
}

uint64_t nros_platform_epoch_us(void) {
    return 0;                                         /* no wall clock */
}

void *nros_platform_alloc(size_t size)              { return myos_malloc(size); }
void *nros_platform_realloc(void *ptr, size_t size) { return myos_realloc(ptr, size); }
void  nros_platform_dealloc(void *ptr)              { myos_free(ptr); }

void nros_platform_sleep_ms(size_t ms) { myos_task_delay(ms); }

int8_t nros_platform_mutex_rec_init(void *m) {
    myos_mutex_t *mx = (myos_mutex_t *)m;
    *mx = myos_mutex_create_recursive();
    return *mx ? NROS_PLATFORM_RET_OK : NROS_PLATFORM_RET_ERROR;
}

/* ... every other function declared in platform.h, platform_net.h and
 *     platform_timer.h ... */
```

The complete ports are the working examples. The POSIX port,
[`nros-platform-posix`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/platform/nros-platform-posix),
is the reference C port. The FreeRTOS port,
[`nros-platform-freertos`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/platform/nros-platform-freertos),
shows tasks, recursive mutexes and condition variables on an RTOS, and
[`nros-platform-mps2-an385`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/platform/nros-platform-mps2-an385)
is a Rust port for bare metal.

## If it doesn't work

- **Timeouts never fire, or fire all at once.** The clock advances only when
  something polls it. Read a free-running hardware timer.
- **The session opens, then deadlocks.** A mutex zenoh-pico takes twice is not
  recursive.
- **Two boards on one network cannot talk.** Both seeded the random source
  with the same constant, so they opened sessions with the same id.
- **Silent memory corruption on an RTOS.** A task stack is too small; an RTOS
  has no guard page. Size it from a measured overflow, not by copying a
  number.
- **Duplicate `z_*` symbols at link.** The RTOS vendor's own zenoh-pico
  system layer already defines them, and so does nano-ros's alias layer.
  Turn the `platform-aliases` feature of `zpico-sys` off for that platform.
- **Under QEMU, timeouts fire before packets arrive.** Run the machine with
  `-icount shift=auto`, so virtual time keeps wall-clock pace while the guest
  sleeps.

Internals keeps the longer list of lessons from the in-tree ports:
[Platform porting pitfalls](../internals/platform/porting-pitfalls.html).

## See also

- [Platform API and C ABI](../reference/platform-api.md): the headers, the
  capability groups and the ports to copy.
- [Platform differences](../reference/matrices/platform-differences.md): the
  clock, allocator, threading and network of each supported platform.
- [Add a transport](add-a-transport.md): for a link that needs no platform
  of its own.
- [Internals → Platform API design](../internals/platform/platform-api-design.html):
  why the ABI is free symbols, and why threads and sockets stay per kernel.

## Design background

- [RFC-0034: platform layer split](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0034-platform-layer-split.md): the scalar and opaque-struct services
- [RFC-0049: hierarchical platform and board config](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0049-hierarchical-platform-board-config.md): `nros-platform.toml` and its resolution ladder
- [RFC-0054: C header ABI single source of truth](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0054-c-header-abi-ssot.md): why the headers are canonical
- [RFC-0073: platform clock in nanoseconds](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0073-platform-clock-nanoseconds-and-resolution.md): `clock_ns` and its resolution
