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
- [ ] **E3 — action protocol-type auto-registration. MISSING → build (codegen +
  regen; design-complete, needs a clean build env).** Implementation-ready plan
  (verified 2026-06-13). The example hand-registers 3 **fixed** ROS-2 action-
  protocol types before `create_action_server/client`
  (`native/rust/action-server/src/main.rs:42`): `#[cfg(feature="rmw-cyclonedds")]
  nros_rmw_cyclonedds::register::<action_msgs::srv::CancelGoal{Request,Response}>()`
  + `::<action_msgs::msg::GoalStatusArray>()`. The framework auto-registers the **8
  `RosAction` associated** types generically
  (`register_type::<A::Goal>()` …, `nros-node/src/executor/action.rs:159`) but not
  these 3. **Verified blockers:** (a) `nros-node` is core — it cannot name
  `action_msgs`; (b) `register_type` returns `nros-node::NodeError`, which the
  trait (in `nros-core`) cannot name; (c) the **generated action crate does NOT
  currently dep `action_msgs`** (its 8 envelopes are locally generated —
  `examples/.../generated/example_interfaces/src/action/fibonacci.rs:454`).
  **Method (non-breaking; preferred over adding required assoc types, which would
  break every existing `impl RosAction` until atomic regen):**
  - **E3a** `nros-core`: add `fn register_protocol_types() -> Result<(), ()> {
    Ok(()) }` (default no-op) to `RosAction` (`nros-core/src/action.rs:53`).
  - **E3b** `nros-node`: call `A::register_protocol_types().map_err(|()| <a
    NodeError variant>)?` in `register_action_server_sized` +
    `register_action_client_sized` (+ the `Node::create_action_*_sized` paths),
    right after the 8 `register_type` calls.
  - **E3c** `rosidl-codegen`: in the action template (`generator/action.rs:358`
    render), emit a `register_protocol_types` override whose body (under
    `cfg(feature="rmw-cyclonedds")`) registers the 3 `action_msgs` types; and have
    codegen ADD `action_msgs` + optional `nros-rmw-cyclonedds` to the generated
    action crate's `Cargo.toml` emission.
  - **E3d** regen the bundled action interfaces + example `generated/` dirs; build
    the native action server+client examples; then delete the manual
    `#[cfg(feature="rmw-cyclonedds")] { … }` blocks from the examples (folds in the
    D3 action leg) and confirm they still register via the trait.
  Why deferred from this session: E3c/E3d is a codegen change + interface regen that
  needs building all action crates across platforms to verify — the verification
  this session's env could not run reliably (0-byte nextest, cross-toolchain).
  **Blocks:** D3 (action leg), C1 (riscv64-threadx C action leg).
  - **Progress (2026-06-13): E3a + E3b DONE** (non-breaking seam, compiles —
    `nros-core` + `nros-node` build green). `RosAction::register_protocol_types()`
    default-no-op added (`nros-core/src/action.rs`); called after the 8 `register_type`
    in BOTH `register_action_server_sized` and the typed client
    `register_action_client_callback` (`nros-node/src/executor/action.rs`), mapping
    `Err(())` → `NodeError::ActionCreationFailed` (no new variant). **Inert until
    E3c emits the override** (default no-op), so examples still self-register for now.
  - **E3c/E3d cascade is BIGGER than the plan above — two gaps found, do NOT do a
    partial E3c (it breaks every action crate's Cargo.toml on the next regen):**
    1. **ws-sync lookup:** `nros_crate_path_lookup()` (`nros-cli-core/src/cmd/ws.rs:934`)
       has `nros-rmw-cyclonedds-sys` but NOT `nros-rmw-cyclonedds` → the generated
       crate's new `nros-rmw-cyclonedds = "*"` dep won't get a `[patch.crates-io]`
       path → unresolved. Add the entry; also confirm `nros ws sync` scans the
       *generated* crates' Cargo.tomls (not just the consumer's) so the dep is seen.
    2. **feature forwarding:** the generated crate needs an `rmw-cyclonedds` feature
       (`dep:nros-rmw-cyclonedds`) AND the consumer's `rmw-cyclonedds` feature must
       forward to `example_interfaces/rmw-cyclonedds` — else the override body is
       cfg'd out and the types never register. The consumer feature-wiring is itself
       codegen-touched.
    Recommend doing E3c/E3d as one focused vertical with native cyclonedds
    build + action server/client e2e as the gate, then cross-platform regen.
  - **E3c + E3d DONE (2026-06-13).** Codegen now emits the override + wiring:
    - `action_nros.rs.jinja` — `impl RosAction` gains `register_protocol_types()`
      that (under `#[cfg(feature="rmw-cyclonedds")]`) registers the 3 `action_msgs`
      types via `nros_rmw_cyclonedds::register::<…>()`.
    - `rosidl-bindgen::generator` (the combined-crate emitter ws-sync actually
      uses — NOT the standalone `rosidl-codegen` cargo path) — injects an
      `action_msgs` path dep + an `rmw-cyclonedds` feature
      (`dep:nros-rmw-cyclonedds`) + the optional `nros-rmw-cyclonedds = "*"` dep
      into generated action crates' `Cargo.toml`.
    - ws-sync (`cmd/ws.rs`) — added `nros-rmw-cyclonedds` to `nros_crate_path_lookup`
      AND made `render_patch_block` scan the *generated* crates' Cargo.tomls, so the
      `[patch.crates-io]` block resolves the generated crate's new dep.
    - examples — action-server + action-client: `rmw-cyclonedds` feature forwards
      to `example_interfaces/rmw-cyclonedds`; deleted the `dep:nros-rmw-cyclonedds`
      / `dep:action_msgs` + the hand-rolled `#[cfg] { … }` registration blocks
      (folds the D3 action leg). Standalone `rosidl-codegen` cargo path updated for
      parity (`has_actions` field) though ws-sync doesn't use it.
    Verified: rosidl-codegen/bindgen unit tests green; `nros ws sync` regenerates
    the correct generated Cargo.toml + override + patch block; action-server +
    action-client BUILD under **both** zenoh (tested path, override inert) and
    cyclonedds; at runtime the framework's `register_protocol_types` succeeds
    (instrumented + confirmed — replaces the manual block). NOTE: a native
    *cyclonedds* action server then fails at downstream entity creation
    (`ActionCreationFailed`) — a **pre-existing** issue in an untested path (no
    native-cyclonedds-action fixture exists; only zenoh + xrce rows), independent
    of registration (which succeeds). Not an E3 regression; tracked separately if
    native cyclonedds action is ever gated.
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

- [→] **C1 — qemu-riscv64-threadx (20 ex, all major) — RE-SCOPED to
  [phase-245](phase-245-riscv64-threadx-example-port.md).** Investigation showed
  this is not a delete-the-wiring cleanup but a **re-architecture**: each example
  is a single dual-entry crate (`main()` pure-cargo + `app_main()` CMake/Cyclone)
  sharing a manual `run_app` (open `Executor` + create entities + spin) — porting
  it to the clean threadx-linux Node+Entry+baker shape, across both build paths, is
  ~10× the other Wave-1 clusters. Carved into its own phase (245); the
  per-(lang,role) work clusters + waves live there.
- [ ] **C2 — zephyr C/C++ 168.4 (~13 major).** Collapse the per-RMW `#if
  defined(CONFIG_NROS_RMW_*)` forks (`cpp/talker/src/main.cpp:37`,
  `c/talker/src/main.c:44`); remove `<zephyr/kernel.h>`/`<nros/platform_zephyr.h>`
  (`main.cpp:6,11`), `nros_platform_zephyr_wait_network(...)` (`main.cpp:32`),
  `k_sleep(...)` (`main.cpp:73`), per-app executor init
  (`c/listener/src/main.c:78`). Target shape = `zephyr/cpp/talker-typed` (clean).
  Leaks P4/P7/P1.
- [x] **C3 — qemu-arm-freertos Rust host_shim (6 major) — DONE 2026-06-13.** The
  `#[cfg(host)] mod host_shim { #[panic_handler] + GlobalAlloc }` block existed only
  because the Component was `crate-type = ["rlib","staticlib"]` (a no_std staticlib
  needs both on host). #45 already dropped it to `["rlib"]`, so the shim is **dead**
  — deleted from all 6 libs (no compat crate needed). `#![no_std]` stays (E4:
  accepted residual minor). Verified: `freertos_rs_talker_entry` release rebuilds
  clean. The 6 examples go major → minor. Leak P5 cleared.
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
- [~] **D2 — PARTIAL (2026-06-13). qemu-esp32-baremetal talker+listener migrated +
  compiled** (nros::main!() Node+Entry; net/domain → deploy metadata; compiles
  riscv32imc build-std). **esp32/rust left** (ESP-IDF staticlib stubs — no leaks,
  already minor; cleaning needs the deferred Wi-Fi+pubsub integration). **Known-inert
  follow-up:** the `Framework::Esp32` macro branch (`main_macro.rs:988`) calls
  `BoardEntry::run`, not `run_with_deploy`, so the deploy overlay is not threaded yet
  (both nodes use the board-default IP) — a 1-line macro switch (mirror the
  `None=>run_with_deploy` branch) lands it; deferred (esp32 e2e unverifiable in this
  env, CI-cell-gated). Pending CI esp32-cell verification (branch phase-244-wave2).
  Original D2 plan:
- [ ] **D2 (orig) — esp32 (esp32/rust 2 + qemu-esp32-baremetal 2, densest). Needs E4, E5.**
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
- [~] **D4 — PARTIAL (2026-06-13). custom-transport-talker migrated + compiled**
  (open-coded TcpBridge + FFI callbacks → `nros_transport_callbacks::tcp_transport_ops`
  + `set_custom_transport`). **C loopback BLOCKED:** E2 shipped Rust-only — NO C-mirror
  factories (the "+ C mirrors" half of E2 was not built), and the C example asserts
  per-callback counters the E2 loopback doesn't expose → E2 must grow C-callable
  factories w/ observable counters first. **talker-xrce already clean** (no_std XRCE
  UART factory; E2 N/A; D1-group). Also noted: `custom-transport-listener` (4th dir,
  omitted from the D4 bullet) has the same open-coded TCP-bridge leak — follow-up.
  Original D4 plan:
- [ ] **D4 (orig) — custom-transport examples (3). Needs E2.** Move the FFI callbacks +
  `set_custom_transport` from `native/custom-transport-talker/src/main.rs:81,162`,
  `native/custom-transport-loopback/src/main.c:60,189`,
  `qemu-arm-baremetal/.../talker-xrce/src/main.rs:51` into the E2 library; examples
  instantiate + plug a named transport. Leak P9.
- [~] **D5 — bridges + px4. PARTIAL (2026-06-13) — see findings.**
  - **px4 `nros_register_check.cpp` — DONE.** Hoisted the SITL-only weak
    `nros_rmw_cffi_register` link stub out of the agnostic module source into a
    `sitl_register_stub.c` build-scaffold TU (added to the module CMake `SRCS`);
    dropped the now-unused `#include "nros/rmw_vtable.h"` from the `.cpp`. **Kept**
    (intrinsic, not leaks): `nros_rmw_uorb.h` + the `nros_rmw_uorb_register()`
    call — uORB IS the subject of a *uORB register-check*; and `PX4_INFO`/`PX4_ERR`
    — this TU is a PX4 module main (`__EXPORT` + `px4_add_module MAIN`), i.e. PX4
    platform glue, not agnostic application logic, so PX4's shell-logging idiom is
    correct here. (px4 SITL is not buildable in-repo / not CI-gated — change is
    structurally a weak-symbol relocation: identical module link semantics.)
  - **bridge `tt-zenoh-to-xrce` — NOT cleaned; accepted residual (verified).**
    Replacing the explicit `nros_rmw_{zenoh,xrce_cffi}::register()` with the
    audit's link-force pattern (`extern crate … as _`) **breaks** the example:
    `Executor::open_with_rmw("zenoh", …)` then returns
    `Transport(ConnectionFailed)` (consistent across runs) vs the original
    "Primary session open". On the raw `Executor` multi-session path the explicit
    `register()` does more than link-force (vtable registers — hence
    `ConnectionFailed`, not `NoBackend` — but the connect setup is incomplete);
    the `register()`-free path only works through `nros::main!()`/`nros::init()`,
    which a 2-RMW-in-one-binary bridge cannot use. The per-session `.rmw("zenoh")`
    /`.rmw("xrce")` is likewise intrinsic (two RMWs in one binary can't be a
    single `[deploy]` rmw). So the bridge's `register()` + per-node `.rmw()` are
    **functional requirements**, recategorised from leak → accepted residual.
  Leaks P3/P7 (px4 stub cleared). (px4 Rust is `minor` — manual executor.)
- [~] **D6 — all 6 threadx-linux C migrated + compiled (2026-06-13); pending CI.**
  Hand-wired `main.c` → thin entry + `src/<Role>.c` declarative component
  (`NROS_NODE_REGISTER`) + `nano_ros_node_register(LANGUAGE C DEPLOY threadx-linux)`
  — matches the (already-clean) threadx-linux C++ shape; all 6 build
  (`build-zenoh/threadx_c_<role>`). **CAVEAT — verify on CI:** the threadx-linux
  generated runtime (`nros_threadx_codegen_system`) is a build-smoke PLACEHOLDER (no
  live executor), so the migrated C examples no longer do live pub/sub at runtime
  (matching the already-stub C++). If the threadx_linux cell e2e-gated real C pub/sub,
  this regresses it → then revert + reclassify D6 as needing a real threadx generated
  runtime (framework enabler, like D1). `just threadx-linux build-fixtures` (the
  effective gate per investigation) passes. Pending CI threadx_linux-cell check.
  Original D6 plan:
- [ ] **D6 (orig) — threadx-linux C (6 major). Needs E5 (net) — executor lift independent.**
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
