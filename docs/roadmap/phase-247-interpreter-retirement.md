# Phase 247 — retire the C++ `EntryNodeRuntime` interpreter (RFC-0043 §Retirement / 240.6)

**Implements.** [RFC-0043](../design/0043-entry-real-callback-binding.md)
§Retirement — delete the synthesizing C++ `EntryNodeRuntime` interpreter
(`nros-cpp/main.hpp`, ~577 lines) + the `DeclaredNode` / `DeclaredCallback` /
`record_callback_effect` string-descriptor `NodeContextOps` seam
(`nros-c/include/nros/node_pkg.h`, `nros-cpp/.../node_pkg.hpp`) + the legacy
non-typed `emit_cpp::emit` / `emit_c::emit` entry codegen. Gated on **every**
declarative example migrating to the real-callback typed path first.

**Status.** In progress (2026-06-14). Stage-1 (app-nodes) **done + pushed**.

---

## Stages

The deletion can't happen until all declarative consumers are off the seam. The
examples split into two shapes:

### Stage 1 — app-node examples → typed carrier — **DONE**
Single-package examples (`examples/<platform>/<lang>/<role>`) with `main()`/board
boot. Migrated to the typed carrier (`nano_ros_node_register(TYPED)` cpp/c,
`nros::node!()`/`nros::main!()` rust):
- [x] **FreeRTOS (12)** — needed a freertos-W0 first: `FreertosBoard` adapter in
  `main.hpp`, `freertos_entry_main_{,c_}typed.cpp.in` + `freertos_app_config.c.in`
  templates, a FreeRTOS TYPED carrier branch in `NanoRosNodeRegister.cmake` (the
  one freertos-specific extra: the cmake carrier generates `NROS_APP_CONFIG`, which
  on the Rust path the board `build.rs` emits). All 12 (cpp + c) cross-build
  (ARM Cortex-M3, mps2-an385). App entry is `app_main`, like nuttx/threadx.
- [x] **nuttx-riscv c/talker** — `NROS_C_COMPONENT`, mirror of nuttx-arm.
- [x] **stm32f4 rust** — already `nros::node!()`/`nros::main!()` (the declarative
  hit was a doc comment).
(ThreadX + riscv64-threadx + threadx-linux app-nodes were done earlier in
phase-245/246.)

### Stage 2 — workspace examples → multi-node typed entry — **TODO (test-coupled)**
Multi-node workspaces (Node libs + Entry pkg + `<launch>`). Migrate via the typed
multi-node entry (`nros codegen entry --typed` over a populated `<launch>`;
per-Node `configure(Node&)`). The proven reference is
`examples/templates/multi-node-workspace-cpp-typed`.

**Important (mapped 2026-06-14): Stage-2 is interwoven with the legacy-emit
retirement + the test suite — it is NOT a standalone example edit.**
`examples/templates/multi-node-workspace-cpp` (declarative) is the regression
fixture for the **legacy** `emit_cpp::emit` path:
- `packages/testing/nros-tests/tests/cpp_multi_node_entry.rs` builds it (via the
  `cpp_robot_entry` cmake fixture, `compile-check-fixtures.sh:146`) and asserts the
  **legacy** generated TU `robot_entry_nros_main_generated.cpp` (register-symbol
  calls into the interpreter) — this test must move to the typed shape or be
  retired with the legacy emit.
- `packages/cli/nros-cli-core/tests/entry_typed_plan.rs` reads its
  `src/demo_bringup/launch/system.launch.xml` for the **typed** plan test (lenient:
  skips if the template is absent).
- `packages/testing/nros-tests/tests/cpp_entry_runtime.rs` runs its Entry binary.

So Stage-2 + the legacy-emit deletion (Stage-3) must land in lockstep:
- [ ] `multi-node-workspace-cpp` → migrate its Node pkgs to `configure(Node&)` +
  populated `<launch>` (match `-typed`), OR delete it and re-point the 3 tests +
  the `cpp_robot_entry` fixture at `-typed` (then drop the legacy
  `robot_entry_nros_main_generated.cpp` assertions). Decide once Stage-3's legacy
  `emit_cpp::emit`/`emit_c::emit` removal is staged.
- [ ] `examples/templates/pure-c-workspace` — no typed sibling → migrate.
- [ ] `examples/templates/c-and-cpp-mixed-workspace` — migrate.
- [ ] `examples/workspaces/{c,cpp,mixed}` — fixture-generating workspaces → migrate
  (update `build-workspace-fixtures`/`build-workspace-codegen`).

Because these flip the legacy-emit regression tests, Stage-2 needs the test suite
green to verify — best done as one focused pass with Stage-3, not piecemeal.

### Stage 3 — the deletion — **TODO (after Stage 2)**
- [ ] Delete `EntryNodeRuntime` + `detail::entry_*` synthesis helpers from `main.hpp`.
- [ ] Retire `DeclaredNode`/`DeclaredCallback`/`record_callback_effect`/the
  `NodeEntityDescriptor` `NodeContextOps` seam from `node_pkg.{h,hpp}`.
- [ ] Drop the legacy `emit_cpp::emit`/`emit_c::emit` non-typed entry codegen
  (make `--typed` the only C++/C entry path); prune the dispatch in `codegen.rs`.
- [ ] Remove any carrier non-typed branches left after Stage 1/2.
- [ ] Build-sweep the migrated examples to confirm nothing references the deleted
  symbols.

## Acceptance
- No example/template uses `NROS_NODE_REGISTER` / `record_callback_effect` /
  `nros_declared_node_*` / the string-descriptor `NodeContext` seam.
- `EntryNodeRuntime` + the declarative seam + the legacy emit are deleted; the only
  C++/C entry path is the typed `run_components` carrier (+ Rust `ExecutorNodeRuntime`).
- All platform example tiers still build.
