# Phase 257 — retire the C++ `EntryNodeRuntime` interpreter (RFC-0043 §Retirement / 240.6)

> **Renumbered 247 → 257 (2026-06-18).** This doc collided with
> `phase-247-weak-symbol-determinism` (both id 247). The weak-symbol phase keeps 247
> (it was filed first + is cross-referenced); this one moves to the next free id 257.
> Self-references below updated; earlier commit messages may say "phase-247" meaning
> this interpreter-retirement work (now 257).

**Implements.** [RFC-0043](../design/0043-entry-real-callback-binding.md)
§Retirement — delete the synthesizing C++ `EntryNodeRuntime` interpreter
(`nros-cpp/main.hpp`, ~577 lines) + the `DeclaredNode` / `DeclaredCallback` /
`record_callback_effect` string-descriptor `NodeContextOps` seam
(`nros-c/include/nros/node_pkg.h`, `nros-cpp/.../node_pkg.hpp`) + the legacy
non-typed `emit_cpp::emit` / `emit_c::emit` entry codegen. Gated on **every**
declarative example migrating to the real-callback typed path first.

**Status.** In progress (2026-06-14). Stage-1 (app-nodes) **done + pushed**.
Stage-2: `examples/workspaces/cpp` migrated (typed entry, `18809bad2`).

**Collision (2026-06-14) — Stage-2 template migration PAUSED.** Two parallel
workers are restructuring this exact area, and `examples/workspaces/*` is THEIR
scope, not phase-257's:
- `f6bffd68d` **244.C4** added a native typed-entry **carrier** branch
  (`NanoRosNodeRegister.cmake:541`): a posix node pkg with `nano_ros_node_register(
  TYPED … DEPLOY native)` builds its own bootable native ELF. It FATALs (`:546`)
  if a posix node pkg lacks `TYPED`.
- `e88488b28` **phase-248 C6b** migrated `examples/workspaces/{c,cpp,mixed}` to
  "board-driven" (drop `DEPLOY` from node pkgs; entry/system.toml is the
  selection point). The worker built these via the `build-workspace-fixtures`
  recipe and verified them.

This broke the phase-257 **template** migrations I'd pushed: the `pure_c_workspace`
+ `c_mixed_workspace` **compile-check** fixtures build via `cmake -S <template>`,
under which the workspace-root `nano_ros_workspace(PLATFORM posix)` propagates to
node subdirs → 244.C4's `:541` fires → demands `TYPED` on the node register. Adding
`TYPED` then hits a 244.C4 framework bug: the C carrier does
`configure_file("${_NROS_NODE_REGISTER_DIR}/templates/native_entry_main_c_typed.cpp.in")`
but `_NROS_NODE_REGISTER_DIR` is **empty** in the workspace-subdir context →
`/templates/…does not exist`. So the typed template can't be made to configure
without fixing the worker's (actively-churning) cmake.

**Action: reverted** the `pure-c-workspace` + `c-and-cpp-mixed-workspace`
**template** migrations to their pre-257 legacy state (which 248 confirmed still
builds with `DEPLOY` now optional). Template typed-migration is deferred to the
244.C4/248 follow-up sweep the workers already planned (they noted
`examples/templates/*` is out of 248's scope). phase-257 should resume on
templates only AFTER 244.C4's `_NROS_NODE_REGISTER_DIR` workspace-context bug is
fixed and the workspace-vs-carrier model for posix multi-node is settled.

> **UPDATE (2026-06-14, `dfbdbd1ff`): both 244.C4 collision causes fixed —
> template migration UNBLOCKED.** (1) The native branch no longer FATALs on a
> non-TYPED posix node — `_NRC_TYPED` now gates the branch, so a posix node that
> propagates `nano_ros_workspace(PLATFORM posix)` to its subdirs without TYPED
> falls through (no-op) instead of `:546` aborting. (2) The carrier
> `configure_file` uses `CMAKE_CURRENT_FUNCTION_LIST_DIR` (not the
> workspace-empty `_NROS_NODE_REGISTER_DIR`), so the template path resolves in the
> workspace-subdir context. Verified: `pkg_c_talker` TYPED reaches "Generating
> done"; `pure-c-workspace` legacy no longer FATALs. phase-257 can re-apply the
> `pure-c-workspace` + `c-and-cpp-mixed-workspace` typed migration.

Remaining phase-257 work (unblocked): the lockstep `multi-node-workspace-cpp`
legacy-fixture retirement with Stage-3 (cpp-only path, no posix-carrier collision).

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

### Stage 2 — workspace examples → multi-node typed entry — **IN PROGRESS**
Multi-node workspaces (Node libs + Entry pkg + `<launch>`). Migrate via the typed
multi-node entry (`nano_ros_entry(... TYPED)` → `nros codegen entry --typed`;
per-Node `configure(Node&)` for C++, `NROS_C_COMPONENT` for C). The proven
reference is `examples/templates/multi-node-workspace-cpp-typed`.

**Map (2026-06-14), refined.** The dispatch is in `cmd/codegen.rs:245`: `--typed`
→ `emit_cpp::emit_typed` (C++ only; `bail!("--typed is C++ only")`), else the
legacy `emit_cpp::emit` / `emit_c::emit`. Crucially **`emit_cpp::emit_typed`
already handles C *nodes*** (the `__nros_c_component_<pkg>_{create,configure}`
seam) — so a workspace whose **Entry is C++** migrates with NO framework change,
even with C Node pkgs. Only a workspace whose **Entry itself is C**
(`--lang c --typed`) is blocked — `emit_c::emit_typed` does not exist. So:

- [x] **`c-and-cpp-mixed-workspace`** — Entry is C++ → unblocked. **DONE**
  (`79d33412c`): `cpp_listener_pkg` → `include/cpp_listener_pkg/Listener.hpp` +
  `configure(Node&)` raw sub; `c_talker_pkg` → `NROS_C_COMPONENT` raw publisher;
  `robot_entry` → `nano_ros_entry(TYPED)`. Verified: typed TU (C++ listener
  `.configure()`, C talker via the C seam), full build links
  (`__nros_c_component_c_talker_pkg_*` defined). Test `c_mixed_workspace.rs`
  asserts only the linked binary → green.
- [x] **`examples/workspaces/cpp`** (pure C++) — **DONE** (`18809bad2`):
  talker/listener → `configure(Node&)` (bind_timer + Publisher<Int32> /
  bind_subscription_raw); `native_entry` → `nano_ros_entry(TYPED)`. Verified:
  typed TU, links, runtime binary enters the native spin loop
  (`cmake_cpp_workspace_entry_starts_prebuilt_runtime`).
- [x] **`multi-node-workspace-cpp`** (pure C++) — **DONE (2026-06-18).** Adopted the
  typed form as canonical: deleted the legacy non-typed template, `git mv`'d
  `multi-node-workspace-cpp-typed` → `multi-node-workspace-cpp` (project renamed
  `multi_node_workspace_cpp`), folded the `cpp_robot_entry_typed` compile-check
  fixture into `cpp_robot_entry` (now builds the typed template) + dropped the legacy
  `cpp_robot_entry` template entry, renamed `cpp_multi_node_entry_typed.rs` →
  `cpp_multi_node_entry.rs` (the typed-shape inspector is now THE cpp multi-node test;
  fixture ref repointed), and **deleted** the legacy `cpp_multi_node_entry.rs` (asserted
  `robot_entry_nros_main_generated.cpp` interpreter shape) + `cpp_entry_runtime.rs` (ran
  the legacy Entry binary). Verified: the canonical template cmake-configures + builds
  the typed `robot_entry` (component `configure()` + `NativeBoard::run_components`, no
  interpreter). Pure-C++ Stage-3 deletion is now unblocked.

**Two framework W0s gate the rest** (each purely additive — no deletion):
- [ ] **W0-A — typed C entry** for **`pure-c-workspace`** + **`examples/workspaces/c`**.
  Entry is C (`NROS_MAIN_C`, `nano_ros_entry LANG c`). Needs `emit_c::emit_typed`
  + a C `run_components` ABI (`nros_board_native_run_components`) + cmake
  `nano_ros_entry(TYPED LANG c)` (today gated to cpp, `NanoRosEntry.cmake:118`).
  The only gate on deleting `emit_c::emit`. Fixtures `pure_c_workspace` /
  `c_mixed_workspace.rs` assert only the linked binary → compatible once it lands.
- [ ] **W0-B — typed Rust node in a C++ entry** for **`examples/workspaces/mixed`**
  (`c_talker` + `cpp_listener` + `rust_heartbeat_pkg`). `emit_cpp::emit_typed` has
  no `lang == "rust"` branch (only `c` / `cpp`; else → C++ class), so a Rust node
  is mis-emitted as a C++ class. The legacy path routes it via the
  `__nros_component_rust_heartbeat_pkg_register` symbol the Rust node exports. Needs
  a `lang == "rust"` branch in `emit_cpp::emit_typed` routing to a Rust component
  seam (or its `register` symbol). Gates the workspaces/mixed runtime fixture.

So the legacy `emit_cpp::emit` deletion is clean for the **pure-C++** path (typed
cpp already parallel); but the **C** path (W0-A) and the **Rust-in-cpp-entry** path
(W0-B) each need additive framework before their legacy emit can go.

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
