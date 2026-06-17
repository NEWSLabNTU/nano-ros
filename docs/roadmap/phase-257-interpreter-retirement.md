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

## Acceptance
- No example/template uses `NROS_NODE_REGISTER` / `record_callback_effect` /
  `nros_declared_node_*` / the string-descriptor `NodeContext` seam.
- `EntryNodeRuntime` + the declarative seam + the legacy emit are deleted; the only
  C++/C entry path is the typed `run_components` carrier (+ Rust `ExecutorNodeRuntime`).
- All platform example tiers still build.
