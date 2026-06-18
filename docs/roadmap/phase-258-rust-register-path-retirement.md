# Phase 258 — retire the Rust register path; unify on the install seam

**Implements.** [RFC-0043](../design/0043-entry-real-callback-binding.md)
§Retirement → "Rust retirement" (resolves Q10). The continuation of phase-257,
which retired the C++ `EntryNodeRuntime` interpreter + the C/C++ declarative seam
and unified C/C++ entries on `__nros_component_<pkg>_install`. This phase brings
the **Rust** side onto the same seam.

**Status.** Planned (design done 2026-06-18). Track 1 ready; Track 2 is an
`Executor`-core change — its own validation pass.

**Priority.** P2 cleanup — phase-257 already removed the interpreter; the Rust
register machinery is dead (Track 1) or redundant-but-working (Track 2). No
behavioural gap blocks users; this is debt + the single-seam end state.

---

## Background — what's actually here (code-grounded)

There is **no Rust interpreter** (Rust always bound real callbacks). Two distinct
register mechanisms remain — `nros::node!`
(`packages/core/nros-macros/src/lib.rs:307–545`) emits both:

- **The C-ABI `_register` foreign-entry seam** — `__nros_component_<pkg>_register`
  → `nros::__register_node_cxx_abi::<C>` (`nros/src/node.rs:771`) →
  `CxxNodeContextRuntime` (`node.rs:640`), which drove the C++ `NodeContextOps`
  table. That table was deleted in phase-257; C++ typed entries now call `_install`.
  **Dead.** Only residual reference: a `#[used]` anchor in the generated mixed
  `nros_ws_runtime` (`_KEEP_NODE_<pkg>` → `_register`).

- **The owned-spin opaque-fn-ptr bridge** — `node!`'s `register(runtime: &mut
  RuntimeCtx)` wrapper (`lib.rs:479`) transmutes four typed fns (`r/i/d/t`) to
  `nros-platform`'s opaque `NodeRegisterFn`/`NodeInitFn`/`NodeDispatchFn`/
  `NodeTickFn` and calls `runtime.runtime.register_dispatch_slot_dyn(...)` →
  `ExecutorNodeRuntime::register_dispatch_slot` (`node_runtime.rs`), which builds a
  type-erased `BspDispatchSlot`, pushes it to `self.components`, binds real
  callbacks via `ExecutorSink`, and **ticks it in `run_ticks`**. `emit_rust::emit`
  (`packages/cli/nros-cli-core/src/codegen/entry/emit_rust.rs`) + `nros::main!`
  owned-spin (`main_macro.rs`) + the Zephyr entry use this.

The shared registration core `register_node::<C>(&mut dyn NodeRuntime)` + `ExecutorSink`
+ `ComponentCell` **stay** — both paths (and the typed `install`) use them.

### Two obstacles to a direct typed owned-spin

1. **Layering wall.** A direct `register_node::<C>()` is a method on the concrete
   `ExecutorNodeRuntime` (in `nros`). The owned-spin entry only holds the opaque
   `RuntimeCtx` (in `nros-platform`, the *lower* layer — it can't reference
   `nros::ExecutorNodeRuntime`). The opaque-fn-ptr bridge exists to cross this
   boundary without generics. The one thing that *does* cross cleanly is a
   **pointer** — exactly the W0-B seam `install_node_typed(executor: *mut c_void)`.

2. **Tick gap (W0-B D2).** `install_node_typed` → `register_node_borrowed`
   (`node_runtime.rs:1274`) **drops** the `ComponentCell` (kept alive only by the
   executor's callback `Arc` clones) → **no `tick`**. Fine for pub/sub/timer;
   **breaks service-client/action** poll nodes. The current owned-spin
   `register_dispatch_slot` path ticks (via `components`). Naively switching
   owned-spin `register`→`install` would regress action/service-client Rust nodes —
   and C++ typed entries already silently share this D2 gap.

---

## Track 1 — delete the dead C-ABI `_register` seam — **DONE (2026-06-18)**

- [x] **w1** (`f91066b16`) — retarget the `nros_ws_runtime` `#[used]` anchor
  `_register` → `_install` (`NanoRosRuntimeCrate.cmake`; 3-arg signature). Landed
  first so the anchor never dangles.
- [x] **w2** (`d1ccf6fd4`) — `nros::node!` drops the `__nros_component_<pkg>_register`
  emission; nros deletes `__register_node_cxx_abi` + `CxxNodeContextRuntime` + the
  Cxx* repr(C) declarative mirrors + `CStrBuf` + `map_cxx_ret` + `NROS_CXX_RET_*`
  (node.rs) + the re-export (lib.rs). Shared `register_node::<C>` core stays.
- [x] **w3** — verified: mixed workspace (Rust node in a C++ typed entry) builds +
  links green; the Rust node exports `__nros_component_<pkg>_install` only,
  `_register` gone (`nm`). cargo check (nros + nros-macros) green.

Note (tidy follow-up, not blocking): `PlanNode::register_symbol()`
(`codegen/entry/mod.rs`) + the `codegen-system` `system_main.c` baker still build a
`*_register` *string* — dead post-257 legacy (they emit a string, don't link the
deleted Rust symbol); sweep when touching that path.

## Track 2 — unify Rust owned-spin onto `install` (design 2a: executor-owned ticks)

**Decision (RFC-0043 §Retirement): 2a.** Make `install_node_typed(void*)` the
single complete component seam for C / C++ / **and Rust owned-spin**, closing D2.

- [ ] **D2 — executor-owned tick-list.** Move the component tick-list out of
  `ExecutorNodeRuntime` into the `Executor` (or an executor-attached registry):
  `install_node_typed`/`register_node_borrowed` enrolls each `ComponentCell`;
  `Executor::spin_once` runs `tick` on the enrolled cells. Now `install`'d nodes
  (C, C++, Rust) tick — fixes the C++ D2 gap too.
- [ ] **node!**: emit an ergonomic Rust `install(exec: *mut c_void) -> i32`
  (or reuse `__nros_component_<pkg>_install`) — already present from W0-B.
- [ ] **Board boundary**: expose the executor handle (`*mut Executor` as `void*`)
  to the owned-spin entry — `RuntimeCtx::executor_handle()` (a pointer crosses the
  `nros-platform` boundary fine).
- [ ] **emit_rust::emit_typed** + **`nros::main!` owned-spin** + **Zephyr entry**:
  call `::<pkg>::install(rt.executor_handle())` per node instead of
  `::<pkg>::register(runtime)`.
- [ ] **Retire** `node!`'s `register(runtime)` wrapper + `register_dispatch_slot_dyn`
  + the `RuntimeCtx` dispatch machinery (no owned-spin caller left);
  `ExecutorNodeRuntime` collapses to a thin wrapper or is removed.
- [ ] **Verify**: native rust workspace + an embedded owned-spin board (zephyr or
  freertos owned-spin), incl. a service-client/action Rust node to prove ticks.

**Out of scope (stays):** RTIC/Embassy *framework* dispatch — `node!`'s
`register_dispatch(&mut Executor)` + `__nros_node_<pkg>_on_callback` trampoline +
`register_dispatch_slot` (the framework owns dispatch, interrupt-driven, not
`spin_once`; it needs name-keyed slots). Q10 resolved: framework stays
name-dispatched.

### Rejected alternative — 2b (Rust entry owns the runtime)

The typed Rust entry constructs `ExecutorNodeRuntime` directly (bypassing the
platform Board), calls `rt.register_node::<pkg::Node>()` (typed, ticks via
`components`), spins. Lower-risk (no `Executor` change) but **leaves D2 unsolved**
(C++ still can't tick action nodes) and **forks the board model** (entry owns the
runtime on native, board owns it on embedded). 2a's executor-owned tick-list is
the general fix; 2b is a local patch.

---

## Sequencing

Track 1 is independent + low-risk → land first. Track 2 touches the `Executor`
core (tick-list + spin) → its own change, gated on full `just ci` (the flaky
build host makes this the expensive part). Track 2 also retires the
`nros-platform` dispatch-slot ABI for owned-spin — coordinate with phase-216
(framework dispatch) which keeps the *framework* half of that ABI.

## Acceptance

- `__nros_component_<pkg>_register` / `__register_node_cxx_abi` /
  `CxxNodeContextRuntime` gone; grep-clean.
- Every entry (C, C++, Rust owned-spin) registers components through
  `__nros_component_<pkg>_install`; `install`'d nodes tick (D2 closed).
- `register()` wrapper + `register_dispatch_slot_dyn` gone; RTIC/Embassy
  `register_dispatch` intact + still building.
- native rust + cpp + mixed workspaces + an embedded owned-spin board build +
  run green; a Rust service-client/action node polls under owned-spin.
