# Phase 269 — C/C++ entry feature parity (params · lifecycle · safety · tiers)

Implements **[#116](../issues/0116-cpp-c-component-launch-parameter-readback.md)** (params),
**[#117](../issues/0117-cpp-c-entry-lifecycle-autostart-codegen.md)** (lifecycle),
**[#118](../issues/0118-cpp-c-component-subscription-integrity-readback.md)** (safety integrity),
**[#119](../issues/0119-cpp-c-entry-scheduling-tiers-codegen.md)** (tiers). Realizes the C/C++ side of
RFC-0032 (entry codegen) / RFC-0043 (typed entry) / RFC-0015 (tiers), under the phase-263 "no
faking" guardrail. Sibling of phase-268 (RFC-0046) — the same "a Rust `nros::main!` surface the
C/C++ entry path lacks" pattern, four more instances.

## Why

Four Track-A/B features are **done for Rust** (`ws-{params,lifecycle,safety,realtime}-rust`) but
**blocked for C / C++ / mixed**. They share one root cause: the Rust `nros::main!` proc-macro wires
each feature per-component into the entry, while the **C/C++ codegen path does not** — the shared
entry `Plan` IR drops the fields, the component-install seam carries no context, and
`emit_c.rs`/`emit_cpp.rs` emit only the bare single-tier `run_components` path. (Full file:line map:
the four issues + this phase's research notes.)

Three structural gaps, common to the cluster:

1. **The entry `Plan` IR carries no feature data.** `Plan` (`codegen/entry/mod.rs:86`) =
   `board/nodes/depfile/bringup/launch_file`; `PlanNode` (:207) adds only
   `class_name/header/lang/shape/host/qos_overrides`. No lifecycle, params, safety, or tiers. And
   `plan_from_launch` (:339) reads only the launch XML — it never reads `system.toml`, so the
   `[lifecycle]`/`[param_services]`/`[safety]`/`[tiers]` blocks the planner already parses
   (`orchestration/planner.rs:753-798`) never reach the emitters.

2. **No bridge from the entry's executor handle to the executor-level feature ops.** The native
   board (`nros_board_native_run_components_named`, `nros-cpp/src/lib.rs:565`) passes the
   `CppContext*` storage as `void* executor` to `__nros_entry_setup` and to each component's
   `configure(node, executor, self)`. The lifecycle/param **C FFI** (`nros-c/src/lifecycle.rs:308`,
   `parameter.h:366`) takes `nros_executor_t*` — a different `#[repr(C)]` layout — so today nothing
   in the C/C++ entry can call `register_lifecycle_services` / `register_parameter_services`.
   **Keystone insight:** `CppExecutor` (`nros-cpp/src/lib.rs:382`) **and** `CExecutor`
   (`nros-c/src/executor.rs:41`) are both `type … = nros_node::Executor` — the *same* underlying
   executor. So the bridge is NOT a type-cast; it is a small set of **nros-cpp FFI shims over the
   shared `nros_node::Executor`** (calling the very methods `RuntimeCtx::apply_lifecycle` /
   `apply_param_services` / the tier path already use), emitted by the entry in a new post-configure
   section.

3. **Two component-callback surfaces have no feature variant.** Safety (#118): the executor-driven
   component subscription is `nros_c_subscription_callback_t = (data, len, ctx)` (`component.h:77`)
   with no integrity arg — only the imperative poll `try_recv_validated` carries
   `nros_integrity_status_t`. Tiers (#119): `nano_ros_node_register`
   (`cmake/NanoRosNodeRegister.cmake:140`) has no `CALLBACK_GROUPS`, so a C/C++ component cannot map
   itself to a tier.

## Design decisions

- **One shared foundation (W0), then one wave per feature.** All four need the same three things —
  Plan-IR fields, the `system.toml` read in `plan_from_launch`, and the executor-shim bridge — so
  build those once in W0; W1–W4 each project one feature onto `emit_c`/`emit_cpp` + the component
  surface and close with a real `ws-<feature>-{c,cpp}` fixture + e2e (no faking).
- **Bridge = nros-cpp FFI shims over the shared `Executor`, not a `CppContext*`→`nros_executor_t*`
  cast.** Add `nros_cpp_*` entry points that take the entry's executor storage handle and invoke
  `nros_node::Executor::{register_lifecycle_services, change lifecycle state,
  register_parameter_services, declare/seed params, create_sched_context, bind_handle_to_sched_context}`
  — the same methods the Rust macro/`generate.rs` path calls (`generate.rs:2414-2427, 2806-2843`).
  This avoids the ABI type-mismatch and keeps nros-c's `nros_executor_t` untouched.
- **Emit features in a post-configure section, mirroring the Rust macro order:** params seeded →
  lifecycle registered+autostarted → (sched-context bound during/around configure). The single
  linear `init → create_node → configure` sequence in both emitters gets one new "after all
  configure calls" block.
- **Component reads params via its existing `executor` handle — no configure-seam ABI change.** The
  component already receives `void* executor` in `configure(node, executor, self)`; add a
  param-get-by-name FFI it calls (the C/C++ analog of Rust `ctx.parameter::<T>`), seeded by the
  entry's `register_parameter_services`. Avoids growing the seam signature.
- **Guardrail:** every wave ships a real workspace fixture built by `build-test-fixtures` +
  consumed by an e2e that asserts the behavior (phase-263). No hand-written non-generated entries.

## Waves

### W0 — shared foundation (Plan IR + `system.toml` read + executor-shim bridge)
**Files:** `codegen/entry/mod.rs` (`Plan` += `lifecycle: Option<LifecycleSpec>`,
`param_services: bool`, `safety: Option<SafetySpec>`, `tiers: Option<TierTable>`; `PlanNode` +=
`params: Vec<(String,String)>`, `callback_groups: Vec<String>`, `sched_context: Option<u8>`);
`plan_from_launch` reads `system.toml` alongside the launch XML, mirroring
`orchestration/planner.rs:753-798` (factor a shared reader so the macro, the JSON planner, and the
entry plan agree); `nros-cpp/src/lib.rs` (new `nros_cpp_*` executor-shim FFI over the shared
`nros_node::Executor`); the launch `<param>` bake path (`plan_from_launch` already decomposes params
for QoS via `qos_overrides_from_params` — add a sibling that keeps the non-QoS params as
`PlanNode.params`).

- **Acceptance:** unit tests on the IR + `plan_from_launch` (a `system.toml` with each block →
  populated `Plan` fields; launch `<param>` → `PlanNode.params`). The shim FFI compiles + a Rust
  unit test drives `register_lifecycle_services` through it on a real `Executor`. No emitter change
  yet (W0 is plumbing).

### W1 — params (#116)
**Files:** `emit_c.rs`/`emit_cpp.rs` (bake each node's `PlanNode.params` initials; when
`Plan.param_services`, emit `nros_cpp_register_parameter_services(executor)` + per-param seed in the
post-configure block); `nros-c/include/nros/parameter.h` + `component.h`/`node.hpp` (a
`ctx.parameter`-equivalent get-by-name the component calls with its `executor` handle); enable the
`param-services` feature on the generated entry link.
- **Acceptance:** `ws-params-c` + `ws-params-cpp` fixtures (mirror `ws-params-rust`); e2e mirrors
  `param_reconfig_e2e` / `param_live_read_e2e` — boot the entry, `ros2 param set publish_period_ms
  N`, assert the C/C++ node's published value follows (live read).

### W2 — lifecycle (#117)
**Files:** thread `Plan.lifecycle` (W0) into `emit_c.rs`/`emit_cpp.rs`: emit
`nros_cpp_register_lifecycle_services(executor)` + autostart Configure→Activate (via the shim) in
the post-configure block; enable the `lifecycle-services` feature on the entry link. (No native-board
change if the autostart is emitted in `__nros_entry_setup`; alternatively add an `autostart` arg to
`run_components_named` — decide in W2, prefer the emit-in-setup path to keep the board API stable.)
- **Acceptance:** `ws-lifecycle-c` + `ws-lifecycle-cpp`; e2e mirrors the Rust `ros2 lifecycle get →
  active` interop (boot → assert state `active`).

### W3 — safety integrity (#118)
**Files:** an integrity-carrying executor-component subscription — add
`nros_cpp_subscription_register_validated(...)` + a C analog whose callback carries
`nros_integrity_status_t` (the component projection of Rust `create_subscription_…_with_safety` +
`CallbackCtx::integrity()`), in `nros-cpp`/`nros-c` + `component.h`/`node.hpp`; `Plan.safety` (W0)
drives the entry to request the backend `safety-e2e` feature (CRC-attach is automatic for the
publisher once built with it — `nros-c/Cargo.toml:34`). Reuse the existing
`[system].features=["safety"]` → `NANO_ROS_SAFETY_E2E=ON` lowering (`NanoRosCapabilities.cmake:60`).
- **Acceptance:** `ws-safety-c` + `ws-safety-cpp`; cross-process e2e asserts the listener validates
  the CRC and **catches a corrupted frame** (mirror `ws-safety-rust`). One surface unblocks both
  langs.

### W4 — tiers (#119)
**Files:** `cmake/NanoRosNodeRegister.cmake` (+ `CALLBACK_GROUPS` → `nros-metadata.json` →
`PlanNode.callback_groups`); tier resolution in the codegen path (port/`share` the macro's
`resolve_tiers`, `main_macro.rs:646-693`, into a crate both consume — the macro + the C/C++
emitters); `emit_c.rs`/`emit_cpp.rs` emit sched-context create + `bind_handle_to_sched_context` via
the shim (mirroring `generate.rs:2414-2427`) — prefer per-component sched binding over inventing a
C++ `run_tiers`, since binding achieves the same multi-tier scheduling without a new board entry.
- **Acceptance:** `ws-realtime-c` + `ws-realtime-cpp`; multi-tier runtime test (mirror
  `ws-realtime-rust` / `realtime_tiers_e2e`) asserting per-tier scheduling.

## Sequencing

W0 (foundation) first — it unblocks all. Then **W1 → W2** (both ride the param/lifecycle shim + the
post-configure block; do params first since lifecycle reuses the same emit slot). **W3** is largely
independent (subscription surface + feature lowering) — can run parallel to W1/W2 but serialize the
shared `emit_c`/`emit_cpp` edits. **W4** is the largest (cmake surface + shared tier-resolver
extraction + sched binding) — last. Each wave is independently shippable (its `ws-*` fixture + e2e).

## Acceptance (phase)

- `ws-{params,lifecycle,safety,realtime}-{c,cpp}` fixtures build via `build-test-fixtures` and pass
  e2e parity with their `-rust` siblings; no faked/hand-written entries.
- The four Track-A/B features project faithfully to C / C++ / mixed; #116–#119 resolved.
- One shared foundation (Plan IR + `system.toml` read + executor-shim bridge); no
  `CppContext*`→`nros_executor_t*` type-punning; no configure-seam ABI break.

## Risks / decisions

- **Shared tier-resolver extraction (W4):** `resolve_tiers` lives in the proc-macro crate; the
  C/C++ emitters are in `nros-cli-core`. Extract to a shared crate both depend on, or duplicate
  carefully — decide in W4 (prefer extraction; the macro should consume the same resolver to prevent
  Rust-vs-C/C++ tier drift).
- **`system.toml` discovery in `plan_from_launch`:** the entry plan currently reads only the launch
  file; locating the workspace `system.toml` from the launch path needs the same resolution the
  planner uses — reuse it, don't reinvent.
- **Mixed entries:** a mixed launch (Rust + C + C++ components in one entry) must apply each
  feature uniformly; the post-configure block runs once per entry over the shared executor, so it
  covers all langs — verify in each wave's mixed fixture where one exists.
- **Param read timing (#116):** prefer executor-backed get-by-name over a configure-seam 4th arg
  (no ABI break); confirm the param store is seeded BEFORE a component's first read (seed in
  post-configure, components read live each tick — matches the Rust `ctx.parameter` live-read model,
  not configure-time).
- **Lifecycle autostart placement (#117):** emit in `__nros_entry_setup` (board API stable) vs a
  `run_components_named` autostart arg — default to the former.
