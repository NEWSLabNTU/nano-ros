---
id: 60
title: Platform/RMW-agnosticism audit — core + user libs leak platform-*/rmw-* features + concrete-backend deps
status: open
type: tech-debt
area: architecture
related: [rfc-0004, rfc-0031, rfc-0005, rfc-0006, rfc-0035, issue-0049, phase-248]
---

> **Convergence plan:** [phase-248](../roadmap/phase-248-platform-rmw-agnosticism-convergence.md)
> groups the fix into 6 crate-owned clusters across 3 dependency waves (Wave 1:
> C1 boards / C2 nros-node / C3 nros-c+nros-cpp / C4 docs — parallel; C5 nros
> umbrella keystone; C6 example node pkgs).

## The bar (target architecture)

1. **Core packages + user C/C++/Rust libraries are platform- AND RMW-agnostic.**
   They carry ONLY functional/capability features (`std`/`alloc`/`no_std`,
   `param-services`, `lending`, ROS edition). NO `platform-*` / `rmw-*`
   (concrete-backend) features; NO deps on concrete RMW/board crates; NO
   `#[cfg(feature="platform-*")]` branches in their `src/`.
2. **RMW functionality lives in RMW packages**, reached from core via the
   **vtable ABI** (`nros-rmw` traits + `nros-rmw-cffi` vtable). Same for
   platform (`nros-platform-api` trait + `nros-platform-cffi` vtable).
3. **Hardware/firmware specifics** (serial/network setup, boot path, memory
   layout, board init) live in **board packages**.
4. **In workspaces, global RMW + platform selection is config-file-driven**
   (bringup `system.toml` `[system].rmw` / `[deploy.<id>]` rmw+board, per
   RFC-0004/0031) — NOT cargo `platform-*`/`rmw-*` features on user node pkgs.

Depending on the INTERFACE crates (`nros-rmw`, `nros-rmw-cffi`,
`nros-platform-api`, `nros-platform-cffi`) is the sanctioned seam — fine.
Depending on CONCRETE backends (`nros-rmw-zenoh`, `nros-rmw-xrce-cffi`,
`nros-rmw-cyclonedds[-sys]`) or concrete board/platform crates is the violation.

## Findings (audit 2026-06-14, 3-agent sweep of packages/* + examples/*)

CLEAN (already agnostic): `nros-core`, `nros-params`, `nros-log`, `nros-serdes`,
`nros-orchestration` (only `rmw-cffi` = the vtable), `nros-rmw`, `nros-rmw-cffi`,
`nros-platform-api`, `nros-platform-cffi`, `nros-macros`,
`nros-orchestration-ir`; all of `packages/drivers/`, `packages/platforms/`,
`packages/interfaces/` (pure message crates), and the RMW packages
(`zpico`/`xrce`/`dds`) + most boards correctly OWN their concern.

### Tier 1 — core executor couples to a concrete RMW
- **`nros-node`** unconditionally deps **`nros-rmw-cyclonedds`** (Cargo.toml
  ~L171). It's the no_std descriptor-glue crate (not the C++ backend; the `-sys`
  backend is optional + `cfg(rmw_cyclonedds_present)`-gated), so footprint is
  small — but a concrete-RMW-named crate is baked into the core executor because
  the `MessageForRmw` helper + `cyclonedds_register` reference it unconditionally.
  Cyclone's type-descriptor-registration need leaked into core; zenoh/xrce don't.
  Target: the descriptor-registration seam should be a generic vtable hook, not a
  named-backend dep on `nros-node`.

### Tier 2 — user libs carry concrete-backend deps + platform-*/rmw-* features
- **`nros`** (umbrella user lib): optional deps `nros-rmw-zenoh`,
  `nros-rmw-xrce-cffi`, `nros-rmw-cyclonedds-sys` + features `rmw-{zenoh,xrce,
  cyclonedds}` and `platform-*`. (This is the current "user picks RMW/platform
  via nros features" model — the one expectation #4 wants replaced by config.)
- **`nros-c`**, **`nros-cpp`** (user C/C++ libs): `platform-*` features +
  optional concrete-backend deps (`nros-rmw-zenoh`; nros-c also
  `nros-rmw-xrce-cffi`). `nros-cpp`'s `rmw-xrce-cffi` routes through the cffi
  interface (OK-ish) but `rmw-zenoh` is a concrete dep.

### Tier 3 — platform-specific code cfg'd into core/user libs
- **`nros-node/src/`**: `#[cfg(feature="platform-{zephyr,freertos,nuttx,threadx}")]`
  branches in `executor/{node_wake,wake_alloc,spin}.rs` (platform wake/alloc/spin
  logic in the core executor — belongs behind `nros-platform-api`/cffi).
- **`nros-c/src/lib.rs`**: platform-specific `#[global_allocator]` +
  critical-section for FreeRTOS/Zephyr/ThreadX (should call the platform vtable).
- **`nros/src/lib.rs`**: `platform-posix` / `platform-zephyr` cfg branches.

### Tier 4 — board crates hardcode a concrete RMW (should be optional/config)
- **`nros-board-native`**, **`nros-board-rtic-mps2-an385`**,
  **`nros-board-rtic-stm32f4`**, **`nros-board-embassy-stm32f4`**: unconditional
  `nros-rmw-zenoh` dep. Siblings (`nros-board-mps2-an385`, `-stm32f4`, `-nuttx`,
  `-esp32-qemu`, …) correctly gate it behind an optional feature — these 4 should
  follow that pattern so a board can build DDS-/XRCE-only.

### Tier 5 — user node/component pkgs carry a platform/rmw feature matrix
- 14 example node pkgs select platform/rmw at the manifest layer instead of
  staying agnostic: e.g. `examples/workspaces/rust/src/{talker,listener}_pkg`
  define a `native/freertos/threadx-linux/nuttx/zephyr/esp32` feature matrix
  enabling `nros/platform-*`+`nros/rmw-cffi`; mixed/embedded pkgs hardcode
  `platform-posix`/`platform-bare-metal` inline; C/C++ node pkgs pass
  `DEPLOY native` to `nano_ros_node_register()`. Entry pkgs are then FORCED to
  pick a node-pkg feature (`features=["native"]`). The bringup `system.toml`
  IS config-correct (`[system].rmw`), but the node pkgs duplicate selection.
  NOTE: single-binary APPLICATION examples (`examples/native/rust/{talker,
  listener}`, `[[bin]]`) hardcoding `platform-posix` are acceptable — they're
  apps, not reusable node libraries. Cross-ref issue #49 (example SOURCE leakage;
  this issue is the manifest DEP+FEATURE layer).

## Why it's this way (not all equally wrong)
- The node-pkg feature matrix + entry forwarding exist because cargo feature
  unification means *some* crate in the graph must turn on `nros/platform-X`.
  The target puts that selection in the BOARD crate / entry metadata / config —
  not the node lib.
- The `nros` umbrella carrying rmw/platform features is the pre-config selection
  model; RFC-0004/0031 already define the config-driven replacement, so this is
  convergence debt, not a fresh design question.

## Fix path (phased — none applied here)
1. **Tier 1:** make the cyclonedds descriptor-registration a generic vtable hook;
   drop the unconditional `nros-rmw-cyclonedds` dep from `nros-node`.
2. **Tier 3:** move platform wake/alloc/spin + the C allocators behind
   `nros-platform-api`/`nros-platform-cffi`; delete the `platform-*` cfg from
   `nros-node`/`nros-c`/`nros`.
3. **Tier 4:** gate the 4 boards' `nros-rmw-zenoh` behind an optional feature.
4. **Tier 2 + 5:** retire `platform-*`/`rmw-*` features from `nros`/`nros-c`/
   `nros-cpp` + user node pkgs; drive RMW/platform purely from board crate +
   `system.toml`/`[deploy.<id>]` config. Largest — touches the feature
   architecture (RFC-0005/0006) + every example; sequence last, likely its own
   phase/RFC-amendment.

Also: fix the STALE comments in `nros`/`nros-node` Cargo.toml claiming concrete
RMW deps were "removed" (Phase 104.A) — they are still present.
