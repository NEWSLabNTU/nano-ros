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
  - [x] Wall #2 (2026-07-17, FIXED): `nano_ros_use_board()`'s
    `DTC_OVERLAY_FILE` append DISABLES Zephyr's auto-discovery of the APP's
    own overlays (Zephyr only auto-discovers when the var is unset) — and
    the FVP board crate deliberately leaves ethernet enablement to the app
    overlay, so the consumer's NIC silently vanished (`net_if: There is no
    network interface`, first ASI FVP boot). Fix: replicate Zephyr's
    app-overlay convention (`boards/<board>.overlay`, `app.overlay`) before
    appending the board-crate overlay, when the consumer hasn't curated the
    list. (`zephyr/cmake/nano_ros_use_board.cmake`.)
  - [x] Wall #3 (2026-07-17, FIXED): every conf we ship that enables
    `CONFIG_NET_TCP=y` (the three RMW snippets + the fvp/s32z board-crate
    prj.confs) left `CONFIG_NET_TCP_WORKQ_STACK_SIZE` at Zephyr's 1024 B
    default — on arm64 the `tcp_work` thread overflows that during early
    net-stack init, corrupting its context into a wild ERET to `__start`
    at EL0 (`ZEPHYR FATAL ERROR: CPU exception`, ESR EC 0x18 trapped
    `msr DAIFSet, #15`, ELR = the reset vector — looks like a boot bug,
    is a stack overflow). First ASI FVP boot hit it 53 ms in. Fix: pair
    every `NET_TCP=y` with `CONFIG_NET_TCP_WORKQ_STACK_SIZE=4096`.
  - [x] Wall #4 (2026-07-17, FIXED): board-crate consumers on Zephyr 3.7
    never receive the RMW-common Kconfig — the `-S nros-<rmw>` snippet is
    4.x-only and `nano_ros_use_board()` merged only the board confs, so
    Cyclone's pthread workers ran on the 2 KiB
    `CONFIG_DYNAMIC_THREAD_STACK_SIZE` default (snippet value: 32 KiB) and
    overflowed into a wild jump past `z_mapped_end` inside
    `dds_create_participant` (`ddsi_config_init`/`set_defaults` in the
    register dump, garbage ESR, "idle" as current thread). Fix:
    `nano_ros_use_board()` step 7b globs
    `zephyr/snippets/nros-${NANO_ROS_RMW}/*.conf` onto `EXTRA_CONF_FILE`
    on every Zephyr version — the snippet confs are plain fragments, and
    double-merging with a 4.x `-S` pass is harmless (identical values).
  - [x] Wall #5 (2026-07-17, FIXED — first cyclone participant EVER on the
    real FVP model): cyclone-on-Zephyr-native-IP-stack had never run (the
    `build-fvp-aemv8r-cyclonedds` recipe is a BUILD smoke; runtime was
    native_sim/NSOS-only). Two NSOS-shaped assumptions broke on hardware:
    1. `ddsrt_getifaddrs` (zephyr/cyclonedds-zephyr/link_stubs.c) fell
       back to a synthetic loopback 127.0.0.1 when the NSOS host
       trampoline is absent → Cyclone advertised AND bound loopback →
       `bind: ENOENT` (native stack ships no loopback address) → every
       `dds_create_participant` returned -1. Fix: enumerate the kernel's
       own `net_if` table (first UP iface with a routable IPv4 unicast
       addr) between the NSOS path and the loopback fallback.
    2. The recv-thread waitset self-pipe (`q_sockwaitset.c` make_pipe,
       Zephyr branch) is a TCP-loopback socket pair — same missing-
       loopback failure ("can't allocate sock waitset"). Fix (cyclonedds
       fork commit 4aa337b0, PENDING fork push): AF_UNIX `socketpair()`
       when sockets are NOT offloaded; NSOS keeps the TCP pair. Snippet
       now carries `CONFIG_NET_SOCKETPAIR=y`.
    Verified single-core: `dds_create_participant` returns a positive
    handle on FVP_BaseR_AEMv8R 11.31.28 ("tid in use" warnings = the
    known-benign class). Follow-ups now visible behind it, tracked
    below: multicast join error -1 (IGMP; unicast-only fallback engages),
    ComponentNode `declare_parameter` code -5 (issue-0116 projection
    class), and wall #6.
  - [ ] Wall #6 (2026-07-17, OPEN): SMP-4 images crash in newlib
    `_free_r((void*)0xffffffff)` (alignment abort, FAR 0xfffffff7) within
    the first few `printf` calls from `main` — single-core images are
    immune. Newlib malloc/stdio under Zephyr arm64 SMP suspect
    (retargeted `__malloc_lock` vs SMP?). Also in the same class: the
    RTPS-failure error path crashes through a garbage pointer
    (PC-alignment fault to 0x13 from picolibc `cbputc`) instead of
    halting cleanly — only reproducible while a boot-failure
    diagnostic is being printed.
  - [x] Wall #7 (2026-07-17, FIXED): `NrosRmwCycloneddsTypeSupport.cmake`'s
    `find_program(msg_to_cyclone_idl.py)` knew only the install layout —
    Zephyr-module/source-tree consumers silently lost descriptor codegen
    ("msg_to_cyclone_idl.py not found" STATUS at configure) and every
    runtime `find_descriptor()` failed. Added the source-tree hint
    (`<repo>/scripts/cyclonedds/`).
  - [x] Wall #8 (2026-07-17, FIXED): the Zephyr-module
    `nros_generate_interfaces()` (zephyr/cmake/) never emitted Cyclone
    topic descriptors at all — only the canonical workspace generator
    (phase-171.C.runtime) did. Ported the branch: per-package idlc
    descriptor+register static lib, whole-archived into `app` via ONE
    comma-joined `-Wl` link ITEM by literal archive path (link OPTIONS on
    the static `app` lib never reach the final zephyr link; a
    `$<TARGET_FILE:>` genex inside a link-libraries string trips
    `$<LINK_ONLY>`'s comma parsing; separate flag tokens hit the #192
    de-dup). Plus two idlc-0.10.5 crash classes fixed in the converter
    (`msg_to_cyclone_idl.py`): CPP include guards per emitted IDL (diamond
    include re-declares `Time_` → `delete_const_expr` abort) and
    per-package guards around rosidl array typedefs (`double__36` emitted
    by two files of one module → "collides with earlier declaration").
  - [x] 2026-07-17 — **ASI controller BOOTS AND SPINS on the FVP** (single
    core): participant + launch-seeded params + 5 subscriptions +
    publishers + timers all up; steady "Control is skipped since input
    data is not ready" idle ticks; SPDP streaming on tap0 (500+ frames).
    Consumer-side knobs that made it fit: `NROS_MAX_PARAMETERS=256`,
    `NROS_EXECUTOR_MAX_CBS=16`, `NROS_SUBSCRIPTION_BUFFER_SIZE=16384`
    (ASI build.sh), stub `package.xml` per vendored msg package
    (rosidl_adapter requirement). Remaining runtime gaps: multicast join
    error -1 (IGMP; unicast-only fallback engages — peers must SPDP to
    us), wall #6 (SMP-4), and the compose-bridge delivery check.
- [x] W2.b (2026-07-17) All three suspects pre-checked by loading them
  INTO the W1.a lane, which now carries the full ASI consumer profile:
  1. Cyclone+zephyr+workspace-verbs — proven by W1.a/W1.b directly.
  2. `NROS_CPP_STD=1` (set on ctrl_pkg exactly like ASI's controller):
     first attempt FAILED — `nros/timer.hpp` pulls `<functional>` under
     `NROS_CPP_STD` and zephyr's default minimal libcpp has neither it
     nor `<string>` (the issue-0112 class). The escape ASI already ships
     is the libc trio `CONFIG_NEWLIB_LIBC=y + CONFIG_STD_CPP17=y +
     CONFIG_GLIBCXX_LIBCPP=y`; the fvp_entry prj.conf now carries it and
     the lane is green. CONTRACT to document for consumers: NROS_CPP_STD
     on zephyr requires a real C++ stdlib — minimal libcpp cannot serve
     the std facade (candidate hardening: a `#error` hint in
     component_node.hpp/timer.hpp when NROS_CPP_STD is set without
     `__has_include(<functional>)`).
  3. `[deploy.fvp]` — the exact ASI block (`kind = "zephyr"`, `target`,
     `board`, `launch`) added to the lane's system.toml; `nros check`
     passes (2 components / 2 deploy targets) and the entry codegen +
     build are unaffected.

### W3 — S32Z270 board crate (unblocks ASI W4; FVP-first, hardware-gated tail)
- [x] W3.a (2026-07-17) The crate already existed as
  `nros-board-s32z270dc2-r52` (kept that name) but had NO board.cmake —
  `nano_ros_use_board()` couldn't consume it. Added the phase-215 sidecar
  + a `board_import_s32z` fixture and `just zephyr build-s32z-board-import`
  smoke: green, Cyclone-RMW ELF for `s32z2xxdc2@D/s32z270/rtu0`. Findings:
  (1) **the Zephyr 3.7 board id is `s32z2xxdc2@D/s32z270/rtu0`** — the
  `s32z270dc2_rtu0_r52@D` name this doc (and ASI's build.sh target list)
  carries is the 3.5-era name and does NOT resolve on 3.7; ASI must update
  at the pin bump. (2) `NROS_BOARD_DEFAULT_RMW` is ADVISORY-only — it sets
  the `NANO_ROS_RMW` cmake var, which the zephyr module's
  `CONFIG_NROS_RMW_*` Kconfig choice never reads; the crate now carries
  `CONFIG_NROS_RMW_CYCLONEDDS=y` in its board fragment instead (candidate
  follow-up: make use_board forward the default into Kconfig).
  (3) `NROS_RMW_CYCLONEDDS` depends on `NET_SOCKETS && POSIX_API && CPP` —
  a fragment setting the RMW without `CONFIG_CPP=y` is SILENTLY dropped
  and the choice falls back to zenoh (the crate fragment now sets both).
  (4) Folded in ASI's hardware-validated memory map (7 MiB `sram2` CRAM —
  without it Cyclone can't fit the RTU's ~1 MiB default sram) + the
  LinFlexD pinctrl the binding requires. (5) Applied the W1.a
  language-neutrality lesson: rust rows split out of the crate prj.conf
  into `prj-rust.conf`; the sourceless (#217-class) `build-s32z` rust
  smoke retired in favor of the import smoke. Runtime proof stays with
  ASI hardware (ASI phase-3 W4).

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
