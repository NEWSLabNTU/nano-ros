# Phase 370 — freertos-posix board variant + first live Cyclone-on-FreeRTOS cell

**Status (2026-08-20).** W1–W4 LANDED. The embedded Cyclone × FreeRTOS cell
builds, boots, and creates writers and readers — the stretch item's goal.
W5 (the ASI consumer switch) is open. Implements the "go, small"
scoping decision recorded in phase-292 W4.a: the FreeRTOS POSIX simulator
is a BOARD-level variant, not a new platform layer, and its RMW/network
half is the existing posix Cyclone path verbatim.

**Landed:** `nros-board-freertos-posix` (declarative — no Rust crate, the
cells are C/C++), `cmake/board/nano-ros-board-freertos-posix.cmake`, a
`FreertosPosix` matrix witness at index 10, C and C++ `cyclonedds`
workspace rows, and `tests/freertos_posix.rs`. Both cells build on a plain
Linux host and deliver `/chatter` end to end — the freertos family's first
runtime lane with no emulator behind it.

**What the bring-up actually cost.** Seven defects, every one of them a
seam that had never been reached rather than anything specific to this
board. Recorded here because the phase's premise — "the RMW/network half
is the existing posix path verbatim, zero new RMW work" — was true of the
DESIGN and not of the code:

1. `network_glue.c` held three KERNEL-only helpers
   (`nros_freertos_start_scheduler`, `_create_task`,
   `_set_current_task_priority`) behind unconditional `lwip/*.h` includes,
   so a board with host sockets could not reach them. Split into
   `freertos_task_glue.c`; both lanes compile it.
2. `freertos_run_tiers.c` — the "board-agnostic tier runner" — called
   `semihosting_write0`, one board family's ARM debug transport. Now
   `nros_board_freertos_console_write`, one strong definition per board.
3. The platform shim's heap stats called `xPortGetFreeHeapSize`, which
   heap_3 does not define. Its own comment already said "heap_4/heap_5";
   no heap_3 board had existed to make it a link error.
4. `cyclonedds_compat.c` supplies `__aeabi_read_tp`/`gethostname`/
   `clock_gettime` for BARE METAL, as its first line says. On a host it
   fails to link (`__tls_base`) and would shadow glibc.
5. The cyclonedds RMW's freertos branch linked the kernel and lwIP but not
   the platform SHIM, so `internal.hpp` could not find `nros/platform.h`.
   The threadx branch beside it had always linked its shim. This is the
   "~70% plumbed, ZERO live cells" note below, measured.
6. `nros_feature_set` gave every FreeRTOS target `alloc` without `std`. On
   a HOST that links the sysroot's unwinding `alloc` and leaves
   `rust_eh_personality` undefined. phase-338 W5.a had already solved this
   for ThreadX by deriving the tier from `_cross` rather than the board
   name; FreeRTOS now takes the same test.
7. The entry emitter did not know the board key, so it fell through to
   `LinuxBoard` and emitted a native `int main` — silently, not as the
   configure error that fallback's comment assumes.

Two more were found beside the path rather than on it, and both were
pre-existing reds nothing had reported: the `c`/`cpp`/`mixed` workspace
roots hardwired `BACKEND zenoh`, so their `*-native-cyclonedds` rows had
been linking ZENOH (phase-368 made that loud; nothing then fixed the
rows); and `CycloneDDS::ddsc` was probed for `IMPORTED_LOCATION_RELEASE`
only, while ROS Humble exports config `NONE` — so the whole-archive flag
named no libddsc and every `dds_*` symbol went undefined. The second was
invisible because the first kept anything from reaching it.

## Why

ASI's `freertos-posix` target still builds a LEGACY vendored CycloneDDS
with hand glue — the last consumer of that fork's POSIX path. Migrating it
onto nano-ros retires the vendored middleware and gives the freertos
family its first non-QEMU, CI-runnable e2e lane. Separately, Cyclone on
EMBEDDED FreeRTOS (ddsrt freertos+lwip port, `WITH_FREERTOS+WITH_LWIP`
cmake block, compat shims in `nros-platform-freertos`) is ~70% plumbed but
has ZERO live cells — the Phase 220.C retirement left it configure-proven,
never pub/sub-proven. This phase closes both.

## W4 — what "~70% plumbed, ZERO live cells" turned out to mean

The build half is landed. `cyclonedds` × `mps2-an385-freertos` went from **does
not compile** to **boots, brings up lwIP, creates a participant**:

```
Network ready
[nros] …/talker/src/main.c:105 nros_publisher_init(…, "/chatter") -> -1
```

Five defects between those two states, none of them about a new board:

1. Three `std::`-qualified C names a cross libc does not alias (`getenv`,
   `strtoull`, `calloc`/`free`). Phase 203 recorded this for ONE symbol on ONE
   libc; newlib on arm-none-eabi aliases a different subset, which is what makes
   it a class.
2. Those `calloc`/`free` were on TRANSIENT SAMPLES — the hazard
   `cyclonedds-known-limitations.md` states outright ("never libc — RTOS heap is
   separate"). `subscriber.cpp` already followed the rule; `service.cpp` did not.
3. Under `-ffreestanding`, `getenv` is not declared at all — correctly, since
   the image has no environment. One `env_lookup` now says so for all three
   sites, in its own dependency-free header (putting it in `internal.hpp`
   dragged `dds/dds.h` into test TUs and broke `check-rmw-cyclonedds`).
4. `ddsrt`'s FreeRTOS `thread_start_routine` never gave Cyclone's OWN threads an
   lwIP per-thread netconn semaphore, so the first socket call from one asserted
   `sem != NULL`. Fork commit `99cfac88` (delta §5), pushed to `origin/nano-ros`
   and pinned.

   **A correction belongs here.** This was first reported as "ruled out by
   measurement": the fork file was reverted, the tree rebuilt INCREMENTALLY, and
   the failure looked identical. It was identical because the cyclonedds
   subproject had not recompiled. From CLEAN directories in both directions the
   change is decisive. The same clean re-measurement retired the fix that was
   credited instead — reordering `lwip_socket_thread_init()` after `tcpip_init()`
   in `network_glue.c`, which is neither necessary nor sufficient and whose
   rationale was false (`sys_sem_new` takes FreeRTOS heap, not lwIP memp). It has
   been reverted.

5. The type-descriptor registry was empty, so `nros_publisher_init` returned a
   bare `-1`. Descriptors register from `__attribute__((constructor))` and the
   FreeRTOS linker script had no `.init_array` output section, so the ctor
   BODIES were discarded. Fixed with the #195 pattern
   `nros-board-threadx-qemu-riscv64` has used since that issue: a `.init_array`
   KEEP block plus an idempotent walk in both boot lanes (issue 0733).

   The fix INVERTED after measurement, which is worth recording. The first
   reading said the register objects were never pulled from the archive and
   recommended an aggregate strong registrar. They ARE pulled — the type-support
   archives are whole-archived on bare metal by a branch written for exactly
   that case, and the image carried 9 descriptor symbols. A registrar would have
   been a second registration mechanism beside #195's.

   It also needed `__dso_handle` / `__cxa_atexit` / `_fini` no-ops: keeping
   `.init_array` retains C++ statics WITH DESTRUCTORS, which register through
   `__cxa_atexit(dtor, obj, &__dso_handle)` — symbols a `-nostartfiles` image
   does not link. Caught by the C++ workspace entry, not the C one.

## Work items

* **W1 — `packages/boards/nros-board-freertos-posix/`** (per phase-292
  W4.a's four-item plan): host `FreeRTOSConfig.h`; kernel `GCC/Posix` port
  + `utils/wait_for_event.c` + `heap_3`; pthread link; `main()` scheduler
  glue reusing `nros-board-freertos/c/freertos_run_tiers.c` (the family
  driver's tier runner). `nros-platform-freertos`'s `platform.c`/`timer.c`
  carry over unchanged; `net.c` is not compiled
  (`NROS_PLATFORM_FREERTOS_WITH_NET=OFF`) — sockets are the host's.
* **W2 — `cmake/board/nano-ros-board-freertos-posix.cmake`**: no cross
  toolchain; Cyclone provisioning delegates to the posix branch (the
  Phase-186 block in `nano-ros-posix.cmake`), i.e. host ddsrt, host
  sockets, zero new RMW work. Board key registered in
  `packages/boards/board-support.toml`.
* **W3 — fixtures + e2e lane**: `fixtures.toml` rows for a C and a C++
  cyclonedds cell on `freertos-posix` (workspace shape, Entry pkg), and a
  runtime test in `packages/testing/nros-tests` — the freertos family's
  first non-QEMU e2e. Un-`#[ignore]` nothing yet: these are NEW cells, the
  QEMU ones stay parked.
* **W4 — one embedded Cyclone proving cell (stretch, may split out)**:
  revive a single C cyclonedds×`mps2-an385-freertos` fixture through the
  existing `WITH_FREERTOS+WITH_LWIP` block on QEMU — the cheap proving
  ground for ddsrt-lwip before any hardware target (ASI W5.b) needs it.
  Known unproven spots: lwIP multicast/IGMP (the platform `net.c` join is
  stubbed; same class as the Zephyr multicast saga, issue 0231-class) and
  the fixed-pool heap budget (`kEmbeddedCycloneConfig`, Phase 177.22).
* **W5 — consumer validation**: ASI switches `--platform freertos-posix`
  onto the board variant and retires `build_cyclonedds_host` /
  `build_cyclonedds_target_posix` + the raw `dds.hpp` node for that
  target (tracked as ASI phase-4 W5.a; walls filed here per the
  reference-consumer contract).

## Risks / expected walls

* **The 0715 class** — threadx-linux (the nearest "RTOS threads + host
  Cyclone" precedent) currently SEGVs in `_tx_thread_timeout` seconds
  after boot on every Cyclone image. The freertos GCC/Posix port runs
  tasks as pthreads with signal-driven preemption; Cyclone's own threads
  are plain pthreads outside the kernel's knowledge. Watch the same
  timer/thread-interaction seam; budget debugging time for it.
* FreeRTOS POSIX port tick signals interrupting host syscalls (EINTR
  discipline — ASI's legacy port needed EINTR-safe sleeps; Cyclone's
  ddsrt should be audited for the same).

## Non-goals

* S32Z270 FreeRTOS (NETC/lwIP, Cortex-R52 toolchain, RTU memory map) —
  ASI W5.b's hardware territory; W4's QEMU cell is deliberately the only
  embedded step here.
* zenoh/XRCE on freertos-posix — cyclonedds is the ASI-driving backend;
  other RMWs join later if a consumer asks.

## Acceptance

* `nros-board-freertos-posix` builds a C and a C++ cyclonedds workspace
  cell on a plain Linux host, `just` lane green in CI, pub/sub e2e
  delivering against a host CycloneDDS peer.
* ASI `freertos-posix` builds and passes its smoke with the vendored
  cyclonedds path deleted (consumer-side acceptance lives in ASI
  phase-4 W5.a).
* (Stretch) one QEMU MPS2 cyclonedds cell boots and delivers locally.
