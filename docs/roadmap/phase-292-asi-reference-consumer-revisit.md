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
- [ ] W1.a The in-tree FVP lanes (`just zephyr build-fvp-all` =
  `build-fvp-aemv8r-cyclonedds{,-rust}`; the sourceless base lane was
  retired by #217) build the pre-workspace single-app talkers. Add an FVP variant of the canonical
  workspace C++ Entry (`ws-realtime-cpp`-shaped: `nano_ros_add_executable`
  + `nano_ros_use_board(fvp-aemv8r-smp)` coexisting in one configure) so the
  exact shape ASI W2.b adopts is proven in-tree BEFORE ASI hits it. Zenoh or
  cyclone RMW — whichever the board contract supports first; cyclone matches
  ASI.
- [ ] W1.b Confirm `nano_ros_use_board()` + `find_package(nano_ros)` (287-W6
  verbs) compose in the west-module consumption model — ASI's exact stack.
  Fix or document any ordering constraint (board include must precede
  `find_package(Zephyr)` — verify it still holds with the verbs in play).

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
- [ ] W4.a nano-ros's freertos platform targets QEMU MPS2 (lwIP); ASI's
  `freertos-posix` is the FreeRTOS POSIX **simulator on a Linux host**
  (different port layer, host networking). Write the gap analysis: what a
  `platform-freertos-posix` flavor needs (port glue, net stack = host
  sockets?, allocator funnel) vs reusing platform-posix semantics. Output =
  a decision + a phase plan, not code.

## Non-goals
- Implementing the FreeRTOS-POSIX platform (W4 scopes it only).
- S32Z2 (FreeRTOS side) — that is ASI W5.b's hardware territory.
- Changing ASI's manifest-authority model (`import: false` stays).

## Acceptance
- An in-tree FVP workspace-entry build lane proves ASI W2.b's exact shape.
- Every ASI-surfaced wall from the pin bump is closed or tracked as an issue.
- `nano_ros_use_board(s32z270dc2-rtu0-r52)` build-proven.
- FreeRTOS-POSIX gap analysis recorded with a go/no-go recommendation.
