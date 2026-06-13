# Phase 244 — Example source cleanliness (agnostic logic only)

**Implements.** [issue 0049](../issues/0049-example-source-platform-rmw-leakage.md).
Aligns example source with [RFC-0024](../design/0024-multi-node-workspace-layout.md)
(Node-pkg = agnostic logic) + [RFC-0032](../design/0032-entry-codegen-pipeline.md)
(`nros::main!()` owns the boot scaffold).

**Goal.** Every example's **source** carries only platform/RMW-agnostic application
logic. Platform + RMW selection and low-level boilerplate move to build/config files
or the board / macro layers. Target end state: 0 `major`, residual `minor` =
node-lib `#![no_std]` only (cleared once E4 lands).

**Status.** Planned (2026-06-13). Baseline audit: 200 pkgs — 86 clean / 33 minor /
81 major. Reference-clean shapes: Rust `nros::main!()`+`nros::Node`
(`phase216-rtic-e2e`, `stm32f4/rust/*-rtic`); C/C++ declarative `NROS_NODE_REGISTER`
(`qemu-arm-freertos/{c,cpp}`, `threadx-linux/cpp`, `zephyr/cpp/talker-typed`).

---

## How to process this (clusters + waves)

- A **cluster** is a self-contained work item. Clusters within the same wave are
  **file-disjoint** — they can be dispatched to parallel agents without rebase
  conflict (each `D*` cluster touches one example group; each `E*` enabler touches
  one framework crate).
- **Waves are dependency order**, not size. **Wave 0 (enablers)** lands the
  macro/board/library capabilities the cleanups need; **Wave 1** is the group
  cleanups that need only the *existing* macro layer; **Wave 2** is the cleanups
  that consume a Wave-0 enabler. Verify each enabler first — the clean reference
  examples prove much of the layer already exists, so several enablers may reduce
  to "confirm + document" rather than "build".
- Each cluster's acceptance: re-run the issue-0049 rubric over its group → all its
  examples `clean` (or `minor` = node-lib `#![no_std]` pending E4). `just <plat>
  build-fixtures` + the platform's E2E gate still green.

---

## Wave 0 — Framework enablers (parallel; land before their dependents)

Each enabler is one framework crate; verify-then-build. **Verified 2026-06-13
(5-agent fan-out); outcomes below.**

- [x] **E1 — RTIC entry macro. EXISTS → confirm-document.** `nros::main!()` already
  owns the RTIC scaffold: `#[rtic::app(device, dispatchers)]` emission
  (`nros-macros/src/main_macro.rs:1066`), board→device/dispatcher lookup
  `rtic_board_spec_for()` (`:1701`), `#[init]` → `RticBoardEntry::init_hardware`
  (`:1091`), `__nros_run` spin+dispatch task (`:1162`), custom-tasks support
  (`:1043`); `nros-board-rtic-stm32f4` impls `RticBoardEntry` (`:385`). Monotonic
  + WFI-idle are intentionally board/user-delegated. **No build needed — D1 just
  migrates the baremetal RTIC variants to bare `nros::main!()`.** **Blocks:** D1.
- [x] **E2 — custom-transport callback library. PARTIAL → build.** The vtable
  framework exists (`nros-rmw/src/custom_transport.rs` `NrosTransportOps` +
  `set_custom_transport`; C ABI `nros-rmw-cffi/include/nros/rmw_transport.h`), but
  there is **no reusable factory** — the 3 examples open-code ring-buffer loopback /
  TCP-bridge / callbacks. Build `packages/drivers/nros-transport-callbacks`:
  `loopback_transport_ops(capacity)` + `tcp_transport_ops(target)` factories (+ C
  mirrors) over the existing vtable, so examples replace 50+ lines with one call.
  **Blocks:** D4.
- [ ] **E3 — action helper-type auto-registration. MISSING → build (codegen, NOT
  nros-node consts).** Correction to the initial plan: `nros-node` is a core crate
  and **cannot name** `action_msgs::srv::CancelGoal{Request,Response}` /
  `action_msgs::msg::GoalStatusArray` (those are generated msg crates). The 8
  auto-registered types are `RosAction` **associated** types (`A::Goal`, …,
  `nros-node/src/executor/action.rs:159`); the 3 missing ones are fixed ROS-2
  protocol types the example currently registers by hand
  (`native/rust/action-server/src/main.rs:34`). Correct fix: add a default-no-op
  `fn register_protocol_types() -> Result<(),_>` to the `RosAction` trait
  (`nros-core/src/action.rs:53`), have `rosidl-codegen`
  (`packages/cli/rosidl-codegen/src/generator/mod.rs`) emit its body (registering
  the 3 action_msgs types from the generated action crate, which DOES dep
  action_msgs), call `A::register_protocol_types()` in the server+client register
  sites, then regenerate the bundled action interfaces. (A codegen change — its own
  careful pass.) **Blocks:** D3 (action leg), C1 (riscv64-threadx C action leg).
- [x] **E4 — macro-injected `#![no_std]`. IMPOSSIBLE → confirm-document.**
  Proc-macros expand at the invocation point and **cannot inject crate-level inner
  attributes** (`#![no_std]` must precede all items) — confirmed by the explicit
  note in `nros-macros/src/main_macro.rs:1039`. So a node/component **lib that
  targets no_std must keep its own `#![no_std]`**; this is architecturally correct,
  not a leak to fix. **P2 is re-scoped:** node-lib `#![no_std]` is the **accepted
  residual `minor`** (NOT downgraded to clean). Clusters only hoist `#![no_std]`
  out of crates that don't need it (host-buildable libs / the std-agnostic pattern
  of `workspaces/.../mixed_rust_heartbeat_pkg`). **No build.**
- [x] **E5 — deploy-config net/locator threading. PARTIAL → build (2 board
  overrides).** The mechanism is **generic** and exists: the macro reads
  `[deploy.<board>]` (`main_macro.rs:1415`) → `DeployOverlay`
  (`nros-platform/src/board/entry.rs:28`) → `BoardEntry::run_with_deploy`
  (default body `:81`); FreeRTOS overrides it (`nros-board-mps2-an385-freertos`
  `config_with_overlay`). **Gap:** `nros-board-esp32-qemu` + `nros-board-threadx-linux`
  have `BoardEntry` but no `run_with_deploy` override → ignore the overlay. Build:
  add ~15-line `run_with_deploy` overrides (copy the FreeRTOS `config_with_overlay`
  shape) to those two board crates. **Blocks:** D2 (esp32), D6 (threadx-linux net).
  (Bare-metal mps2-an385 net threading folds into D1.)

---

## Wave 1 — Independent group cleanups (parallel; existing macro layer only)

- [ ] **C1 — qemu-riscv64-threadx (20 ex, all major).** The dirtiest group.
  Migrate every Rust/C/C++ example to `nros::main!()` / generated
  `nros_system_main()` / `NROS_NODE_REGISTER`; delete `Executor::open` /
  `nros_support_init`+`nros_executor_init` + spin loops (e.g.
  `rust/talker/src/lib.rs:49,60`, `c/talker/src/main.c:46,57`,
  `cpp/talker/src/main.cpp:19-30`). Leaks P1/P2/P3 (+P10 action leg → E3).
- [ ] **C2 — zephyr C/C++ 168.4 (~13 major).** Collapse the per-RMW `#if
  defined(CONFIG_NROS_RMW_*)` forks (`cpp/talker/src/main.cpp:37`,
  `c/talker/src/main.c:44`); remove `<zephyr/kernel.h>`/`<nros/platform_zephyr.h>`
  (`main.cpp:6,11`), `nros_platform_zephyr_wait_network(...)` (`main.cpp:32`),
  `k_sleep(...)` (`main.cpp:73`), per-app executor init
  (`c/listener/src/main.c:78`). Target shape = `zephyr/cpp/talker-typed` (clean).
  Leaks P4/P7/P1.
- [ ] **C3 — qemu-arm-freertos Rust host_shim (6 major).** Move the
  `#[cfg(any(target_os="linux",target_os="macos"))] mod host_shim { #[panic_handler]
  … GlobalAlloc … }` block (`talker/src/lib.rs:23`) into a board/compat crate; drop
  `#![no_std]` (`lib.rs:11`, pending E4). Leaks P5/P2.
- [ ] **C4 — workspaces entries + templates Pattern-A + zephyr-byo.** Lift
  `ExecutorConfig`/`Executor::open` from `templates/.../local-msg-package/.../main.rs:36,40`
  + `multi-package-workspace/.../main.rs:15`; remove `zephyr-byo/app/src/main.c:10,14,39`
  platform headers/bring-up. Migrate the bare-metal-scaffolded `workspaces`
  entries to the macro shape. Leaks P1/P7.
- [ ] **C5 — stm32f4 legacy `talker/`.** Migrate the one legacy `talker/` (major:
  no_std + panic + RMW register + net) to the `*-rtic`/`*-embassy` entry shape that
  the rest of the group already uses. `*_pkg` libs are `minor` (node-lib no_std →
  E4). Leaks P2/P3/P5/P6.

---

## Wave 2 — Enabler-dependent cleanups (parallel; after Wave 0)

- [ ] **D1 — qemu-arm-baremetal Rust (13/15 major). Needs E1, E4, E5.** Route every
  variant through `nros::main!()` (+ the E1 RTIC surface). Remove `#![no_std]`/
  `#![no_main]` (`talker/src/main.rs:13-14`), `use panic_semihosting as _;`
  (`:19`), `nros_rmw_zenoh::register()` (`:61`), hardcoded `Config{mac,ip}` +
  `const LOCATOR` (`:31,40` → deploy metadata via E5), and the RTIC plumbing
  (`talker-rtic/src/main.rs:47,70,72,78`). `phase216` pair is the in-group
  reference. Leaks P2/P3/P5/P6/P8.
- [ ] **D2 — esp32 (esp32/rust 2 + qemu-esp32-baremetal 2, densest). Needs E4, E5.**
  Strip `#![no_std]`/`#![no_main]`/`#[entry]` (`talker/src/main.rs:19-20`),
  `use esp_backtrace as _;`/`esp_app_desc!()` (`:22,27`), `nros_rmw_zenoh::register()`
  (`:71`), hardcoded MAC/IP + `esp_println` + smoltcp diagnostics (`:32,36,55`).
  Network → deploy metadata (E5); logging → agnostic `nros::log!`.
- [ ] **D3 — native Rust RMW guards + action types. Needs E3.** Remove the
  `compile_error!` RMW guards + `ACTIVE_RMW_NAME` + per-RMW `main()` forks
  (`talker/src/main.rs:32,51,104`; `listener/src/main.rs:29,44,55`) — the guard
  belongs in the framework crate; example calls `nros::init()` unconditionally.
  Delete the `#[cfg(feature="rmw-cyclonedds")] nros_rmw_cyclonedds::register::<…>()`
  action-type setup (`action-server/src/main.rs:41`, `action-client/src/main.rs:37`)
  → E3 auto-registers. Leaks P4/P10. (native C/C++ already clean.)
- [ ] **D4 — custom-transport examples (3). Needs E2.** Move the FFI callbacks +
  `set_custom_transport` from `native/custom-transport-talker/src/main.rs:81,162`,
  `native/custom-transport-loopback/src/main.c:60,189`,
  `qemu-arm-baremetal/.../talker-xrce/src/main.rs:51` into the E2 library; examples
  instantiate + plug a named transport. Leak P9.
- [ ] **D5 — bridges + px4. Independent (no enabler).** Remove `.rmw("zenoh")`/
  `.rmw("xrce")`/`open_with_rmw("zenoh")` + `register()` literals from
  `bridges/tt-zenoh-to-xrce/src/main.rs:84-85,94,99,104`. In px4
  `nros-register-check.cpp`: drop `#include "nros_rmw_uorb.h"`/`nros/rmw_vtable.h`
  (`:17-18`), the weak `nros_rmw_cffi_register` stub (`:26`), and replace
  `PX4_INFO`/`PX4_ERR` (`:40,43`) with agnostic logging; SITL stub → build/board.
  Leaks P3/P7. (px4 Rust is `minor` — manual executor.)
- [ ] **D6 — threadx-linux C (6 major). Needs E5 (net) — executor lift independent.**
  Lift `nros_support_init`/`nros_executor_init` + spin loops
  (`c/talker/src/main.c:51,61`) into the generated runtime; move `tcp/127.0.0.1:PORT`
  defaults (`c/talker/src/main.c:41`, `c/service-client/src/main.c:46`) to deploy
  metadata (E5). (threadx-linux C++ 12/12 + Rust entries already clean/minor.)

---

## Acceptance (phase close)

- Issue-0049 rubric re-run over all 200 pkgs → 0 `major`; every residual `minor` is
  node-lib `#![no_std]` only (and 0 of those once E4 lands).
- Each platform's `just <plat> build-fixtures` + E2E gate green after its cluster.
- Reference-clean examples unchanged; the audit's "not a leak" list (link-forcing
  `extern crate … as _`, `NROS_APP_MAIN_REGISTER_POSIX`, `build.rs` bridges,
  rclcpp-compat idioms) is preserved.
- Update issue 0049 → `resolved`; archive this phase doc.
