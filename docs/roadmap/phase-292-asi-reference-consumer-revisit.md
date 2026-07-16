# Phase 292 — ASI reference-consumer revisit: FVP entry parity, consumer-wall intake, S32Z board

Status: **Draft — 2026-07-16** · Counterpart of ASI `docs/roadmap/
phase-3-modern-nano-ros-migration.md` (autoware-safety-island, `nano-ros`
branch) · Touches phase-215 (board import), 217 (FVP lane), 236 (ASI is the
named reference consumer), 287 (ament verbs on zephyr).

> **Goal.** Autoware Safety Island is re-adopting nano-ros on a current pin
> after ~6 weeks on `a7b6eac5c`. This phase owns the nano-ros side: prove the
> canonical zephyr Entry shape on the FVP board ASI ships on, absorb the
> consumer-surfaced walls the pin bump will produce, and close the S32Z board
> gap. ASI's Phase-2.D round surfaced 9 real nano-ros bugs; expect this round
> to surface more — that is the reference-consumer contract working.

## Context (ASI review, 2026-07-16)

- ASI consumes nano-ros as a **west Zephyr module** (`import: false`, ASI is
  manifest authority), Cyclone-on-Zephyr RMW, board
  `nano_ros_use_board(fvp-aemv8r-smp)` — all still-current mechanisms.
- ASI's Phase-2.D wall ("component lib doesn't inherit Zephyr's compile
  context") is already fixed by 287-W6 (`find_package(nano_ros)` supplies the
  verbs on zephyr without re-importing the runtime). ASI will migrate to a
  LAUNCH-only Entry pkg (`nano_ros_add_executable(BOARD zephyr LAUNCH …
  TYPED)`, the `ws-realtime-cpp/src/zephyr_entry` shape).
- ASI keeps upstream's FreeRTOS targets on the vendored cyclonedds for now;
  moving them onto nano-ros's freertos platform + cyclone RMW is ASI W5 and
  needs scoping here (W4).

## Waves

### W1 — FVP entry-path parity proof

> As-landed (2026-07-17): both waves green, one configure each, no verb
> changes needed.
- [x] W1.a `examples/workspaces/ws-realtime-cpp-fvp/` (new): the
  `ws-realtime-cpp` two-tier demo re-deployed on the FVP via a
  `src/fvp_entry/` whose CMakeLists is exactly the ASI W2.b shape —
  `nano_ros_use_board(fvp-aemv8r-smp)` BEFORE `find_package(Zephyr)`, then
  `find_package(nano_ros)` + one `nano_ros_add_executable(BOARD zephyr
  LAUNCH "demo_bringup:system.launch.xml" TYPED DEPLOY zephyr)`. Cyclone
  RMW (`rmw = "cyclonedds"` in system.toml + `prj-cyclonedds.conf`),
  typed Int32, `ZephyrBoard::run_tiers`. Lane: `just zephyr
  build-fvp-ws-entry` (in `build-fvp-all`). Findings: (1) the entry
  codegen's workspace-root walk-up requires a root CMakeLists/Cargo.toml
  marker — added a fail-loud west-only root CMakeLists; (2) the board
  crate's base prj.conf bakes `CONFIG_RUST=y` (rust-talker assumption) —
  a C++ consumer must override `CONFIG_RUST=n` in its own fragment (done
  in fvp_entry/prj.conf; ASI must do the same, or the board conf grows a
  prj-rust.conf split later).
- [x] W1.b Proven against ASI's EXACT consumption model with a scratch
  downstream west workspace (`~/repos/nros-downstream-ws`, symlink-backed:
  own manifest repo = authority, nano-ros as `import: false` leaf at
  `modules/nros`, zephyr-lang-rust pinned beside it, module crawler
  auto-discovery). The same `west build …/src/fvp_entry` (NO `-b`)
  produced the ELF unchanged. Composition contract documented: the
  downstream app must `include($ENV{NROS_REPO_DIR}/zephyr/cmake/
  nano_ros_use_board.cmake)` DIRECTLY (the module's own include happens
  during `find_package(Zephyr)` — too late for board resolution), with
  `NROS_REPO_DIR` pointing at the module checkout; the helper fail-louds
  if called after Zephyr.

### W2 — consumer-wall intake (open-ended, ASI-driven)
- [ ] W2.a Standing intake: each wall ASI's pin bump surfaces gets a repro +
  an issue here, fixed on main (the Phase-2.D precedent: 9 gaps in one
  round). Track the list in this doc as they arrive.

  **Intake log:**
  - [x] Wall #1 (2026-07-17, FIXED): phase-180.A left the cyclone
    `zephyr_ipv4_compat.h` force-include GLOBAL on Zephyr 3.7
    (`zephyr_compile_options`); the `$<OR:...,...>` genex's top-level comma
    breaks Zephyr 3.7 llext-edk's `$<JOIN:list,glue>` over the global
    interface options → CMake GENERATE fails for any consumer app
    ("$<JOIN> expression requires 2 comma separated parameters, but got 1"
    at zephyr/CMakeLists.txt:2145 ×3, evaluated even with CONFIG_LLEXT
    unset). Fix: scope `target_compile_options(nros PRIVATE ...)` on every
    Zephyr version — only the cyclonedds TUs need the header and they all
    live in `nros`. (`zephyr/cmake/nros_rmw_cyclonedds.cmake`.)
- [ ] W2.b Known suspects to pre-check: Cyclone-on-Zephyr with the 287 ament
  verbs (ASI is the first external cyclone+zephyr+workspace consumer);
  `NROS_CPP_STD=1` + std::string/vector param facade against the RFC-0044
  hardening; `system.toml [deploy.fvp]` resolution through the current
  planner.

### W3 — S32Z270 board crate (unblocks ASI W4; FVP-first, hardware-gated tail)
- [ ] W3.a `packages/boards/nros-board-s32z270dc2-rtu0-r52` (board.cmake +
  confs + the DTC overlay ASI currently hand-glues), phase-215 shape, so
  `nano_ros_use_board(s32z270dc2-rtu0-r52)` works. Build-proof against the
  Zephyr 3.7 board id `s32z270dc2_rtu0_r52@D`; runtime proof stays with ASI
  hardware.

### W4 — FreeRTOS-POSIX platform scoping (for ASI W5; scoping only, no impl)
- [x] W4.a nano-ros's freertos platform targets QEMU MPS2 (lwIP); ASI's
  `freertos-posix` is the FreeRTOS POSIX **simulator on a Linux host**
  (different port layer, host networking). Gap analysis below (2026-07-17).

#### W4.a gap analysis — `freertos-posix` flavor (2026-07-17)

**What each side actually is.**

- nano-ros `freertos` = a REAL cross-compiled embedded target: Cortex-M3
  QEMU MPS2-AN385, FreeRTOS `GCC/ARM_CM3` port, lwIP + LAN9118 netif,
  Cyclone self-provisioned `WITH_FREERTOS=ON WITH_LWIP=ON` (ddsrt
  freertos+lwip port), zenoh-pico `system/freertos/lwip/network.c`,
  `thumbv7m-none-eabi`. (`cmake/platform/nano-ros-freertos.cmake`,
  `packages/platforms/freertos-lwip/nros-platform.toml`,
  `packages/boards/nros-board-mps2-an385-freertos/`.)
- ASI `freertos-posix` = the FreeRTOS kernel's
  `portable/ThirdParty/GCC/Posix` port running as host pthreads inside a
  Linux process, `heap_3` (host malloc), and — the load-bearing fact —
  **Cyclone built as a plain native POSIX static lib** (no `WITH_FREERTOS`,
  no `WITH_LWIP`): DDS traffic rides HOST sockets, not a simulated NIC.
  (ASI `build.sh build_cyclonedds_target_posix`,
  `actuation_module/freertos/freertos_main.cpp`.) Upstream cyclonedds even
  ships `ports/freertos-posix/` describing exactly this simulator shape,
  with lwIP explicitly NOT integrated.

**Consequences.**

1. The RMW/network half of a `freertos-posix` flavor is nano-ros's
   EXISTING posix path verbatim: native Cyclone self-provision
   (`nano-ros-posix.cmake` Phase-186 block), host-socket ddsrt, pthread/rt
   link, host codegen. Zero new RMW work; `platform = posix` semantics.
2. The kernel half is small and additive: the FreeRTOS kernel compiled
   with the `GCC/Posix` port + `utils/wait_for_event.c` + `heap_3`, a
   host-arch `FreeRTOSConfig.h`, and a `main()` that starts the scheduler
   and runs the entry as a task. nano-ros's `nros-platform-freertos` C
   shim (`platform.c`/`timer.c`) mostly carries over (FreeRTOS API is
   port-independent); `net.c`'s lwIP hooks are simply not compiled.
3. What it is NOT: a new zenoh/zpico story (ASI is cyclone-only; if zenoh
   is ever wanted here, `posix`'s `system/unix/network.c` already runs in
   a pthread-hosted process — FreeRTOS tasks ARE pthreads under this
   port), a new board crate family (one `freertos-posix` board), or a new
   platform toml beyond a thin variant.

**Decision (go, small):** implement as a BOARD-level variant of the
existing freertos platform — `nros-board-freertos-posix` — that swaps the
kernel port (`GCC/Posix`) and selects the posix Cyclone provisioning
branch, rather than a whole new `platform-freertos-posix` layer. The
platform-vs-board split follows RFC-0049 duty rules: "runs on a Linux
host with host sockets" is a hardware(-analog) fact of the board, while
the FreeRTOS software stack (tasks, timers, run_tiers glue) is the
platform and is shared with mps2-an385.

**Phase plan (one wave, ~W-sized, after ASI W2/W3 land):**
1. `packages/boards/nros-board-freertos-posix/`: FreeRTOSConfig.h (host),
   kernel-port cmake block (`GCC/Posix` + wait_for_event + heap_3,
   link pthread), `main()` scheduler-start glue reusing
   `nros-board-freertos/c/freertos_run_tiers.c`.
2. `cmake/board/nano-ros-board-freertos-posix.cmake` sidecar: no cross
   toolchain, Cyclone provisioning = the posix (host ddsrt) branch.
3. Build-proof lane: the ws-realtime-cpp-mps2 workspace re-pointed at the
   new board (or a `ws-realtime-cpp-freertos-posix` variant), runnable in
   plain CI (no QEMU, no license gate) — which also gives the freertos
   family its first non-QEMU runtime e2e.
4. ASI W5.a consumes it: `--platform freertos-posix` drops the vendored
   cyclonedds + raw `dds.hpp` for the nano-ros RMW.

## Non-goals
- Implementing the FreeRTOS-POSIX platform (W4 scopes it only).
- S32Z2 (FreeRTOS side) — that is ASI W5.b's hardware territory.
- Changing ASI's manifest-authority model (`import: false` stays).

## Acceptance
- An in-tree FVP workspace-entry build lane proves ASI W2.b's exact shape.
- Every ASI-surfaced wall from the pin bump is closed or tracked as an issue.
- `nano_ros_use_board(s32z270dc2-rtu0-r52)` build-proven.
- FreeRTOS-POSIX gap analysis recorded with a go/no-go recommendation.
