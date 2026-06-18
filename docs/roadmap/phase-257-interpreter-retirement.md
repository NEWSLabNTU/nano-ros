# Phase 257 — retire the C++ `EntryNodeRuntime` interpreter (RFC-0043 §Retirement / 240.6)

> **Renumbered 247 → 257 (2026-06-18).** This doc collided with
> `phase-247-weak-symbol-determinism` (both id 247). The weak-symbol phase keeps 247
> (it was filed first + is cross-referenced); this one moves to the next free id 257.
> Self-references below updated; earlier commit messages may say "phase-247" meaning
> this interpreter-retirement work (now 257).

**Implements.** [RFC-0043](../design/0043-entry-real-callback-binding.md)
§Retirement — delete the synthesizing C++ `EntryNodeRuntime` interpreter
(`nros-cpp/main.hpp`, ~577 lines) + the legacy non-typed `emit_cpp::emit` /
`emit_c::emit` entry codegen **(both done — Stage-3a)**, then excise the
`DeclaredNode` / `DeclaredCallback` / `record_callback_effect` string-descriptor
`NodeContextOps` seam — delete `declared_node.hpp` and **refactor in place** (not
delete) the umbrella headers `nros-c/include/nros/node_pkg.h` +
`nros-cpp/.../node_pkg.hpp` **(Stage-3b — see below)**. Gated on **every**
declarative example migrating to the real-callback typed path first (done — only the
scaffolder + a few tests still emit/assert the seam).

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

### Stage 2 — workspace examples → multi-node typed entry — **DONE (2026-06-18)**
All workspace examples migrated to the typed entry (W0-A + W0-B landed); both
framework W0s are in. Stage-3 deletion is now unblocked for every language.
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

**Two framework W0s gated the rest** (each purely additive — no deletion). **Both
landed 2026-06-18; Stage-3 deletion is now unblocked for every language.**
- [x] **W0-A — typed C entry** for **`examples/workspaces/c`**. **DONE (2026-06-18,
  `8699c0dd2`).** Added the C-ABI `nros_board_native_run_components` (nros-cpp Rust
  extern "C": init → setup(executor) → spin → fini — the real-executor lifecycle the
  legacy no-op `nros_board_native_run` never had) + `emit_c::emit_typed` (pure-C
  `main.c` driving each node's `__nros_c_component_<pkg>_{create,configure}` seam) +
  ungated cmake `nano_ros_entry(TYPED LANG c)`. A TYPED C entry/component links
  NanoRosCpp (the `nros_cpp_*` seam home, which bundles nros-c's C ABI). `examples/
  workspaces/c` adopts it (c_talker/c_listener → `NROS_C_COMPONENT`); builds green,
  runtime publishes on the real executor. Gate on deleting `emit_c::emit` is now lifted.
- [x] **W0-B — typed Rust node in a C++ entry** for **`examples/workspaces/mixed`**
  (`c_talker` + `cpp_listener` + `rust_heartbeat_pkg`). **DONE (2026-06-18, `d258833bf`).**
  `nros::node!` now emits `__nros_component_<pkg>_install(node, executor, self)`
  (D7 Option C: the Rust node self-creates its node + owns its state on the shared
  executor handle, ignoring entry-side node/qos); `emit_cpp::emit_typed` gained a
  `lang == "rust"` branch that forward-declares the seam, skips entry-side
  Node/class storage + the class-header `#include`, and hands it
  `::nros::global_handle()`. `nros::install_node_typed::<C>` registers an
  `ExecutableNode` against the borrowed `*mut Executor`. Mixed adopts the typed
  entry (c_talker → `NROS_C_COMPONENT`; cpp_listener → `configure(Node&)`); builds
  green, binary carries all three seams, runtime publishes on the shared executor.
  Also fixed a latent cmake ordering bug (component lib used stale generator target
  names `nros_{cpp,c}_cargo_build` → clean typed-C builds raced the
  `nros_config_generated.h` mirror; now `cargo-build_nros_{cpp,c}`).

So the legacy `emit_cpp::emit` deletion is clean for the **pure-C++** path (typed
cpp already parallel); but the **C** path (W0-A) and the **Rust-in-cpp-entry** path
(W0-B) each need additive framework before their legacy emit can go.

### Stage 3 — the deletion — **3a DONE (2026-06-18, `33d364f80`); 3b deferred**
- [x] Delete `EntryNodeRuntime` + `detail::entry_*` synthesis helpers from `main.hpp`
  (kept the typed `run_components`/`component_spin_loop`/`entry_parse_u32`/
  `entry_tick_yield`; deleted the legacy `Board::run(register_fn)` overloads).
- [x] Drop the legacy `emit_cpp::emit`/`emit_c::emit` non-typed entry codegen —
  `codegen.rs` bails for a non-typed C/C++ entry (Rust stays register-based).
- [x] Delete the no-op C board stub (`nros_board_native_run` / `c-stubs/main_board.c`
  + its build-helpers compile + `main.h` decl + the unused `host_os()`), and the
  non-typed NuttX carrier template (`nuttx_entry_main.cpp.in`); `nano_ros_node_register`
  errors on a non-typed NuttX carrier.
- [x] Migrated the last two non-typed-LAUNCH templates (`pure-c-workspace`,
  `c-and-cpp-mixed-workspace`) to the typed entry.
- [x] Build-swept: native workspaces c/cpp/mixed + cmake fixtures cpp_robot_entry /
  c_mixed_workspace / pure_c_workspace all green with the interpreter gone.
- [ ] **Stage-3b — retire the declarative seam.** Re-scoped below (design 2026-06-18).
- [ ] Owed: a full `just ci` pass (the host's flaky rustc blocked it; the targeted
  fixture sweep above covers the Stage-3a blast radius).
- [ ] Pre-existing/unrelated: the `shadowing` ament/rclcpp fixture fails to link
  (`nros_app_register_backends`) — a phase-249 P4a RMW-wiring gap for a pure-rclcpp
  consumer that transitively links a nano-ros msg binding; not touched by Stage-3.

### Stage 3b — retire the declarative seam — **design re-explored 2026-06-18**

Goal: delete the string-descriptor declarative node-registration API now that the
interpreter that consumed it is gone (3a). The seam:
- **C++** — `declared_node.hpp` (`NodeEntityKind`, `CallbackEffectKind`,
  `NodeOptions`, `NodeEntityDescriptor`, `DeclaredEntity`, `DeclaredCallback`,
  `DeclaredNode`) + `node_pkg.hpp`'s `NodeContext`/`NodeContextOps`/`NodeRegisterFn`
  + all `DeclaredNode::create_*` inline bodies + the `NROS_NODE_REGISTER`/
  `NROS_NODE` macros.
- **C** — `node_pkg.h`'s `nros_node_context_t`/`_ops_t`, `nros_declared_node_t`,
  `nros_declared_*` fns, `nros_node_entity_*`, `NROS_NODE_REGISTER`/`NROS_COMPONENT`.

**Re-scoping findings (corrects the earlier "delete node_pkg.{h,hpp}" note):**
- `node_pkg.hpp` / `node_pkg.h` are **umbrella public headers** (`nros.hpp`/`nros.h`
  pull them; `main.h` declares `nros_board_native_run_components` there). They must be
  **refactored in place** — excise the declarative machinery, keep the file — NOT
  file-deleted. `declared_node.hpp` *can* be deleted outright (only `node_pkg.hpp`
  includes it). `main.hpp`'s `node_pkg.hpp` include is now likely **vestigial**
  (only comments referenced EntryNodeRuntime) — confirm + drop during impl.
- **No shipped example uses the seam** — every example/template is on the typed path
  (`configure(Node&)` / `NROS_C_COMPONENT` / `nros::node!`+install). The only live
  consumers are the **scaffolder** and a few **tests**.
- **Rust is out of scope.** `__register_node_cxx_abi` / the `__nros_component_<pkg>_register`
  descriptor path is the **Rust entry's** register seam (`emit_rust::emit` is
  register-based — there is no `emit_rust::emit_typed`), orthogonal to the C/C++
  declarative seam. `ExecutableNode` + `install_node_typed` (W0-B) stay. Retiring the
  Rust register path is a *separate* future item, only meaningful once a typed Rust
  entry emitter exists.
- Build artifacts (`*/nros_components.cmake` under `build*/`) are gitignored — ignore.

**W4 settled (2026-06-18):** `declared_node_typed_helpers.cpp` exercised the **seam**
(`DeclaredNode`/`DeclaredEntity`/`DeclaredCallback` + `create_publisher<M>(DeclaredEntity&)`),
not the typed surface (which is `configure(Node&)` + `Publisher<M>` + `bind_timer`),
despite its comment → those types are part of the seam, deleted with it (fixture + its
drift test + compile-check entry removed).

**Work items (migrate-first, then delete):**
- [x] **W1 — scaffolder** (`9c7a8fdb7`). `scaffold_component_{cpp,c}` now emit typed
  components: C++ `configure(::nros::Node&)` + `Publisher<Int32>` + `bind_timer`
  (header → `include/<pkg>/`); C `NROS_C_COMPONENT` (raw publisher + timer,
  `LANGUAGE C TYPED`). Rust/direct-mode paths untouched.
- [x] **W2 — scaffold tests** (`9c7a8fdb7`). C test asserts the `NROS_C_COMPONENT`
  shape; added a C++ test asserting `configure`/`bind_timer`. 4 scaffold tests green.
- [x] **W3 — cmake-fn tests** (`cd7021b46`). Kept the CLASS-prefix + embedded-deploy
  validation cases (survive 3b); only the staged `dummy.c` dropped its seam refs.
- [x] **W4 — settled** (above): the types are seam → deleted.
- [x] **W5 — C++ excise** (`cd7021b46`). Deleted `node_pkg.hpp` + `declared_node.hpp`;
  dropped the vestigial includes from `nros.hpp` + `main.hpp`. (`component_node.hpp`'s
  own `NROS_COMPONENT` rclcpp factory is independent — stays.)
- [x] **W6 — C excise** (`cd7021b46`). Refactored `node_pkg.h` in place — kept the
  shared `nros_ret_t` + `NROS_RET_*` surface, excised the declarative block.
- [x] **W7 — verify.** Native workspaces c/cpp/mixed all build green with the seam gone
  (they compile `nros.hpp`/`main.hpp` sans `node_pkg.hpp` + the refactored `node_pkg.h`).
  Compile-check templates re-running past host glibc/rustc flakiness.
- [ ] **W8 — docs (book migration).** 8 book pages still teach the retired declarative
  `register_node`/`NodeContext`/`nros_declared_*` shape (heaviest:
  `getting-started/workspace-cpp.md`, 6 hits with code blocks; also
  `workspace-node-pkgs.md`, `workspace-mixed-language.md`, `porting-a-cpp-node.md`,
  `user-guide/component-and-entry-pkg.md`, `internals/dispatch-strategy.md`,
  `user-guide/{rtic,embassy}-integration.md`). Rewrite to the typed `configure(Node&)` /
  `NROS_C_COMPONENT` shape (mirror `examples/workspaces/{cpp,c}`). CLAUDE.md is clean.
  Sizeable user-facing pass — best done against a `just book` build.

## Design exploration (2026-06-18) — unified cross-language component-install seam

W0-A (typed C entry) and W0-B (Rust node in a C++ entry) are not two ad-hoc patches;
they are one missing abstraction: **a single language-agnostic C-ABI for "install a
node's entities + dispatch into the shared executor", consumed by one entry
`run_components` regardless of the entry's or the node's language.** Designing that
once subsumes both W0s and makes the 3×3 (entry-lang × node-lang) matrix collapse to
3 thin per-language adapters + one runtime.

### The substrate is already unified

Every typed entry, in every language, drives the **same** real executor — an opaque
`nros-rmw-cffi` executor handle. The current typed C++ entry already proves this: its
generated TU hands each component
`__nros_node_{i}.executor_handle()` (a `void*` cffi handle) + `__nros_node_{i}.ffi_handle()`
(a `nros_cpp_node_t*`). So C/C++/Rust nodes installed against the same handle land in
the same executor; `spin_once` dispatches all of them. **Nothing about the executor
needs unifying** — only the *install seam* differs per language today:

| node lang | install seam today | dispatch |
| --- | --- | --- |
| **C** | `__nros_c_component_<pkg>_configure(node, executor, self)` (+ `_create`) | trampolines bound on the node (identity) |
| **C++** | `self->configure(::nros::Node&)` (object method) | member-fn trampolines (identity) |
| **Rust** | `register(&mut NodeContext)` + `init`/`dispatch`/`tick` fn-ptrs → `ExecutorNodeRuntime::register_dispatch_slot` | `dispatch_fn(state, cb_id, ctx)` (name/id demux) |

The C seam is **already** the target shape: `(node, executor, self) -> int32_t`. C++ is
that minus the free-function wrapper; Rust is that plus an opaque `state` and an
id-demuxed dispatch instead of identity trampolines — but both ultimately register
per-entity callbacks into the one executor.

### Proposed canonical seam

One extern-C install function per Node pkg, identical signature across languages:

```c
// "construct + install this pkg's node into the executor"; returns nros_ret_t.
int32_t __nros_component_<pkg>_install(const nros_cpp_node_t* node,
                                       void* executor,
                                       void* self /* nullable */);
```

- **C** — rename/alias the existing `__nros_c_component_<pkg>_configure` (already this
  signature). `_create` supplies `self`.
- **C++** — the `NROS_C_COMPONENT`/typed-component macro emits a free-fn
  `__nros_component_<pkg>_install` that does `static <Class> obj; obj.configure(Node(node))`
  (the class object is the `self`; identity trampolines bind as today).
- **Rust** — `nros::node!` emits `__nros_component_<pkg>_install` that runs the
  `register_dispatch_slot` body against the **handed-in** executor handle (not a
  Rust-owned `ExecutorNodeRuntime`): `init()` → boxed `state` (stashed in the executor's
  component arena, keyed by node), `register(NodeContext over (node, executor))` declares
  entities whose executor callback is a thunk to `dispatch_fn(state, cb_id)`. The 4
  fn-ptrs stay as the macro's internals; `_install` is the uniform façade.

### One `run_components`, any entry language

The entry (C / C++ / Rust) becomes a thin loop with **no per-node-language knowledge**:

```
exec = open_executor(config)
for each launch node i (pkg, name, namespace, qos):
    node_i = create_node(exec, name, namespace)
    apply_qos_overrides(node_i, ...)        // already baked by emit
    __nros_component_<pkg_i>_install(node_i, exec, self_i)   // uniform call
spin_once-loop(exec)                          // dispatches identity + id-demux alike
shutdown(exec)
```

Codegen (`emit_cpp::emit_typed` / a new shared emitter) emits the **same** install-call
list for all three node languages — the `lang == c|cpp|rust` branches collapse to "emit
`__nros_component_<pkg>_install(...)`", differing only in the forward-declaration
(extern "C") and whether a `self`/`_create` is needed. **W0-B falls out for free** (a
Rust node is just another `_install` call); **W0-A** becomes "emit the same loop from a
C `main` (`NROS_MAIN_C`) calling a C `run_components` over the install list" — no new
`emit_c::emit_typed` synthesis logic, just the C entry shell.

### Why this is the right cut

- **Deletes, not adds, a dimension.** The interpreter (`EntryNodeRuntime`) existed to
  *synthesize* behaviour from descriptors precisely because there was no uniform install
  seam. With one, Stage-3's deletion is unconditional for every language — the legacy
  `emit_cpp::emit`/`emit_c::emit` + the descriptor `NodeContext` seam all go.
- **Entry-language ⟂ node-language.** A C entry can host a Rust node and vice versa,
  because the seam is C-ABI + the executor is shared. The 3×3 matrix becomes 3 adapters.
- **No new runtime.** `ExecutorNodeRuntime` (Rust) and the C++ executor wrapper both
  already operate on the cffi handle; `_install` just lets each register against a handle
  it was *given* rather than one it *owns*.

### Open questions / risks (to settle before W0 impl)

1. **Rust `state` ownership.** Today `ExecutorNodeRuntime` owns the component arena; under
   the handed-handle model the executor (cffi) must own/stash the boxed Rust `state` +
   the dispatch thunk for the node's lifetime. Either extend the cffi executor with a
   per-node opaque-component slot, or keep a thin Rust-side registry keyed by the cffi
   executor handle (simpler; what `register_dispatch_slot` already does — just stop
   requiring it to *own* the `Executor`).
2. **Drop order / lifetime.** identity (C/C++) self lifetime vs Rust boxed state — the
   entry/arena must outlive `spin`. The current typed C++ TU uses `static` storage; the
   uniform loop keeps that.
3. **`self` for C++/Rust.** C uses `_create`→`self`; C++ uses a `static` object; Rust a
   boxed state. The seam takes `void* self` (nullable) — each adapter decides.
4. **QoS-override bake** already runs before `configure`; keep it before `_install`.

### Incremental path (so it lands safely)

1. Land the **Rust `_install` façade** in `nros::node!` (additive; the 4 fn-ptrs stay) +
   make `ExecutorNodeRuntime` able to register against a borrowed handle. Unit-test the
   Rust `_install` against a live executor.
2. `emit_cpp::emit_typed`: add the `lang == "rust"` branch emitting `_install` — **W0-B
   done**, validated on `examples/workspaces/mixed`.
3. Add the C++ `_install` free-fn wrapper + switch the C/C++ branches to the uniform
   `_install` call (behaviour-identical to today's `configure`).
4. The C entry shell (`NROS_MAIN_C` → C `run_components` over the install list) + cmake
   `nano_ros_entry(TYPED LANG c)` — **W0-A done**, validated on `pure-c-workspace` +
   `examples/workspaces/c`.
5. Stage-3 deletion (now unconditional).

This keeps each step additive + independently validated, with the interpreter deletion
as the final, now-unblocked-for-all-languages step.

### Decisions (2026-06-18) — open questions resolved (code-grounded)

Investigated the runtime internals (`nros/src/node_runtime.rs`,
`nros-node/src/executor/spin.rs`); the open questions are now settled. **Implementation
is gated on this whole subsection being decided — it now is, except the single
PRE-IMPL CHECK in D5.**

- **D1 — Rust state ownership: register directly on the shared `Executor` behind the
  handle; no registry, no ownership transfer. DECIDED (refined by D5).** The
  `void* executor` handle IS a live `nros_node::Executor` (see D5), so Rust `_install`
  recovers `&mut Executor` from it and registers the node's entities against that **same**
  instance — there is no separate Rust executor to reconcile. Component state lives in an
  `Arc<ComponentCell>`; the executor's per-entity callbacks own clones of it
  (`ExecutorSink`, node_runtime.rs:651/658 `self.cell.clone()` moved into the callback),
  so the cell stays alive for the executor's lifetime **with no external owner**. The
  needed new code is a thin `register_borrowed(&mut Executor, register_fn, init_fn,
  dispatch_fn, tick_fn)` — the body of `register_dispatch_slot` (node_runtime.rs:371)
  operating on a borrowed `&mut Executor` instead of an owned `ExecutorNodeRuntime`.
  Reuses `ComponentCell` / `ExecutorSink` / `NodeContext` verbatim; **no cffi ABI
  extension, no global registry, no `ExecutorNodeRuntime` ownership.**

- **D2 — tick pumping: NOT needed for pub/sub/timer (W0-B); a scoped extension for
  service-client/action Rust nodes. DECIDED.** `run_ticks` (node_runtime.rs:478) iterates
  `components` per spin ONLY to poll service-client `call_raw` + complete/feedback action
  servers; pub/sub/timer nodes tick to a no-op. So **W0-B (the mixed workspace:
  `rust_heartbeat_pkg` = timer publisher) needs no tick pump** — dispatch via the
  Arc-owned callbacks suffices, and the registry just keeps the cells alive for the
  executor's lifetime. A Rust node with a service-client/action *inside a non-Rust entry*
  needs its ticks pumped: store its `Arc<ComponentCell>` in a tick-list **on
  `nros_executor_t`** (nros-c/executor.rs:165) and run those ticks inside that struct's
  existing `spin_once` (executor.rs:1629) — which the entry already pumps every loop. So
  the entry needs NO extra call; the shared executor drives Rust-node ticks alongside
  C/C++ dispatch. **Clearly-scoped follow-up after W0-B, not a W0-B blocker.** (Rust
  ENTRIES already pump via `ExecutorNodeRuntime::spin`; C/C++ node ticks ride the same
  `spin_once` — unchanged.)

- **D3 — `self` semantics: `void* self` is the identity-node object; null/ignored for
  register-style nodes. DECIDED.** C passes its `_create()` object; C++ passes its
  `static <Class>` object; Rust ignores `self` (it builds + owns `State` via `init()` in
  the registry `ComponentCell`). The seam stays uniform; each adapter interprets `self`.

- **D4 — lifetime/teardown. DECIDED (refined by D5).** Identity (C/C++) `self` lives in
  TU `static` storage (unchanged, outlives spin). Rust state (`Arc<ComponentCell>`) is
  kept alive by the shared executor's callback clones (D1) and freed when that executor
  drops — i.e. tied to the entry's `nros_executor_t` lifetime. **No separate
  shutdown C-ABI** (the executor already owns the keep-alive). The tick-list case (D2)
  frees with `nros_executor_t`.

- **D5 — handle-type match: RESOLVED (positively).** The `void* executor` the entry
  passes is a `*mut nros_executor_t` whose `_opaque` holds a live **`nros_node::Executor`**
  — `CExecutor` is literally `type CExecutor = nros_node::Executor` (nros-c/executor.rs:41),
  built by `CExecutor::from_session_ptr` (executor.rs:279) and recovered by
  `get_executor_from_ptr(ptr) -> &mut CExecutor` (executor.rs:69; cast also at
  node.rs:602 / timer.rs:175). So C/C++/Rust install seams operate on **one shared Rust
  `Executor`** — Rust `_install` calls `get_executor_from_ptr` and registers on it. No
  handle conversion, no borrowed-session juggling. (The earlier `from_session_ptr`
  borrowed-session worry is moot — the handle is already the Executor.)

- **D6 — naming. DECIDED.** Canonical seam: `__nros_component_<pkg>_install(node,
  executor, self) -> int32_t`. The existing `__nros_c_component_<pkg>_configure` is
  aliased/renamed to it (C); `nros::node!` + the C++ component macro emit it. The legacy
  `__nros_component_<pkg>_register` (descriptor seam) is retired with the interpreter in
  Stage 3.

- **D7 — Rust node ownership / naming / qos in a foreign (non-Rust) entry. OPEN —
  BLOCKER surfaced 2026-06-18 during W0-B impl-prep; needs a decision before coding.**
  The typed entry creates ONE `::nros::Node` per launch `<node>` (launch name + per-topic
  qos-overrides applied to it) and C/C++ components bind their entities **on that given
  node** (`configure(__nros_node_i)` / `__nros_c_component_*_configure(node, …)`,
  emit_cpp.rs:378-408). But a Rust node's `register(&mut NodeContext)` **self-creates** its
  node with a hardcoded name — `ctx.create_node(NodeOptions::new("heartbeat"))` (= the
  node's own `Node::NAME`; see `examples/workspaces/mixed/.../rust_heartbeat_pkg/src/lib.rs`)
  — and `ExecutorSink::create_node` (node_runtime.rs:599) builds a fresh node from it. So
  for a Rust node in a C++/C entry: (a) the entry's pre-created node is unused; (b) the
  entry's qos-overrides never reach the Rust entities; (c) the Rust node's name is its
  `NAME`, not the launch `<node>` name (they coincide in the mixed workspace but it is not
  enforced). Options:
  - **A — uniform (bind on the given node).** Make the Rust register run against the
    entry's existing node (NodeContext adopts a provided node handle + name + qos). Rust
    becomes identical to C/C++ (entry owns the node, qos works). Cost: `NodeContext` /
    `register` / `nros::node!` must support "register onto a provided node" (today
    `create_node` always builds one) — touches the rust runtime + macro.
  - **B — Rust self-owns; thread name+qos through the seam.** `_install` passes the launch
    name + qos into the register so it self-creates correctly; entry skips its own
    `create_node` for rust. Localized, keeps the rust model. Cost: extend the seam +
    register to accept name+qos.
  - **C — scope-cut + documented constraint.** W0-B supports Rust nodes that self-name
    (`Node::NAME` must equal the launch `<node>` name) and carry NO entry-side
    qos-override (a documented rust-in-foreign-entry limitation). Minimal — works for the
    mixed workspace today (heartbeat: name matches, no qos). Defer A/B until a rust node
    needs entry qos. Risk: a silent qos/name mismatch for future rust nodes (mitigate
    with an `nros check` warning).

  **DECIDED (2026-06-18): Option C now + A as a follow-up.** W0-B ships the scope-cut —
  Rust nodes in a foreign entry self-name (`Node::NAME` must equal the launch `<node>`
  name) + carry no entry-side qos-override; documented limitation + (follow-up) an
  `nros check` warning when a rust node has a launch qos-override or a name mismatch.
  Option A (uniform bind-on-given-node) is the principled follow-up once a rust node
  needs entry qos.

**Net: D1–D7 resolved (D7 = Option C).** Executor-sharing (D1/D5) + the rust node model
in a foreign entry (D7, scope-cut) settled. Implementation proceeds: W0-B = a rust
`_install` that self-creates its node on the shared executor (entry skips `create_node`
for rust) + the `emit_cpp::emit_typed` `lang=="rust"` branch, validated on
`examples/workspaces/mixed`; A-uniformity + the `nros check` guard are follow-ups. The cffi
executor handle is a shared `nros_node::Executor` (D5), so the seam is `_install(node,
executor, self)` where each language registers on that one executor; Rust state stays
alive via the executor's own callback `Arc` clones (D1); ticks (only for
service-client/action Rust nodes) ride the executor's existing `spin_once` via a tick-list
on `nros_executor_t` (D2). New code is small + additive: a `register_borrowed(&mut
Executor, …)` helper + the three `_install` adapters + the codegen emitting one uniform
call. Implementation proceeds per the 5-step path. **Step-1 scope (W0-B):** `register_borrowed`
+ Rust `_install` + the `emit_cpp::emit_typed` `lang=="rust"` branch, validated on
`examples/workspaces/mixed` (timer-pub node → no tick-list needed yet).

## Acceptance
- No example/template uses `NROS_NODE_REGISTER` / `record_callback_effect` /
  `nros_declared_node_*` / the string-descriptor `NodeContext` seam.
- `EntryNodeRuntime` + the declarative seam + the legacy emit are deleted; the only
  C++/C entry path is the typed `run_components` carrier (+ Rust `ExecutorNodeRuntime`).
- All platform example tiers still build.
