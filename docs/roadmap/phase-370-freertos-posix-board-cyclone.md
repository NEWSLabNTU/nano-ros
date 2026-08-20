# Phase 370 — freertos-posix board variant + first live Cyclone-on-FreeRTOS cell

**Status (2026-08-20).** OPEN — filed from the ASI reference-consumer side
(ASI roadmap phase-4 W5.a is the consuming half). Implements the "go,
small" scoping decision recorded in phase-292 W4.a: the FreeRTOS POSIX
simulator is a BOARD-level variant, not a new platform layer, and its
RMW/network half is the existing posix Cyclone path verbatim.

## Why

ASI's `freertos-posix` target still builds a LEGACY vendored CycloneDDS
with hand glue — the last consumer of that fork's POSIX path. Migrating it
onto nano-ros retires the vendored middleware and gives the freertos
family its first non-QEMU, CI-runnable e2e lane. Separately, Cyclone on
EMBEDDED FreeRTOS (ddsrt freertos+lwip port, `WITH_FREERTOS+WITH_LWIP`
cmake block, compat shims in `nros-platform-freertos`) is ~70% plumbed but
has ZERO live cells — the Phase 220.C retirement left it configure-proven,
never pub/sub-proven. This phase closes both.

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
