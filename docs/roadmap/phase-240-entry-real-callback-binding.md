# Phase 240 — Entry real-callback binding (RFC-0043 implementation)

**Goal.** Implement [RFC-0043](../design/0043-entry-real-callback-binding.md):
make the codegen Entry path run **real user logic** by routing to the real Rust
executor, with the component a **stateful object** binding callbacks **by
identity** (no naming). Retire the synthesizing `EntryNodeRuntime` interpreter +
the `DeclaredNode`/`record_callback_effect` string layer. Brings the C++/C Entry
path to runtime parity with the Rust embedded path (which already runs real
bodies). Unblocks RFC-0032 §8a / phase-236 236.C (ASI deletes its imperative
`main.cpp`).

**Status.** Planned (2026-06). Design = RFC-0043 (Draft). The NuttX
executor-callback path is **spike-validated for pub/sub + timer** (RFC-0043
§Spike, 2026-06-12); service/action on the executor under the embedded lifecycle
is unspiked (240.5). This phase carries 236.D's detailed breakdown
(phase-236 236.D now points here).

**Depends on.** RFC-0043; the Rust executor + `ExecutorNodeRuntime`
(`packages/core/nros/src/node_runtime.rs`, `nros-node/src/executor/`); the
Phase 238 NuttX carrier (`cmake/NanoRosNodeRegister.cmake` +
`cmake/templates/nuttx_entry_main.cpp.in`); the entry codegen
(`packages/cli/nros-cli-core/src/codegen/entry/`). RFC-0041 (phase-239) for
*client* callbacks (orthogonal — this phase polls clients until 0041's C/C++
wave lands).

## Current state (verified 2026-06-12)

- **No-naming primitive exists**: the executor closure API — Rust
  `executor.node_mut(n).create_subscription::<M,_>(topic, |m|…)`
  (`nros-node/src/lib.rs:22`), C++ `node.create_subscription(sub, topic, lambda)`
  + the raw `nros_cpp_subscription_register(…, cb, ctx, …)` (`subscription.hpp:29`,
  spiked). The *declarative* macros on both sides (C++ `DeclaredNode`, Rust
  `nros::node!()` `on_callback`+`"on_tick"`) name — those are what we move off.
- **Component is static-only today**: `NROS_NODE_REGISTER(UserClass, "pkg::Class")`
  (`node_pkg.hpp:519`) emits a register trampoline + a class-name symbol; the
  class has only `static register_node(NodeContext&)` — no instance, no state.
- **Metadata has the class, not the header**: `nano_ros_node_register` records
  `{name, class:"pkg::Class", sources, pkg_dir, lang}` into `nros-metadata.json`
  (`NanoRosNodeRegister.cmake`), but the codegen `PlanNode{pkg,exec,name,ns}`
  (`codegen/entry/mod.rs:114`) reads only the launch XML — never the metadata.
  The launch→{class,header} map is the missing seam (Q5).
- **Rust ownership blueprint**: `ExecutorNodeRuntime` owns a per-node
  `ComponentCell{slot: leaked State, publishers, …}` for the app lifetime
  (`nros/src/node_runtime.rs`); subscription closures capture an `Arc<cell>`.
  The C++ entry-owned arena (Q2) mirrors this.

## Decisions to lock (RFC-0043 open Qs — recommendations)

- **Q1 ctor vs `configure`** → **`Result configure(nros::Node&)`** (two-phase,
  fallible). A ctor can't return `Result`, and entity creation can fail (arena
  full, RMW error); two-phase also lets the entry construct-then-configure in
  arena order. (Decide in 240.1.)
- **Q2 instance ownership** → **entry-owned arena slot per launch node**,
  `sizeof` known via the typed `#include`; mirrors Rust's `ComponentCell`. No
  heap.
- **Q5 launch→{class,header}** → add a **`class_header`** field to the component
  metadata JSON (`nano_ros_node_register` derives/accepts it) + have the entry
  codegen read `nros-metadata.json` to map `(pkg,exec)`→`{class, class_header}`.
- **Q10** → C++ is **`spin_once`-only** for v1 (the spike model); Rust Entry
  no-naming parity + RTIC/Embassy framework-dispatch is a separate decision.

## Work breakdown

### 240.1 — Component-object API + `NROS_NODE` macro (C++, native)
- [ ] New component shape: a class with member entity handles + state + a
      `Result configure(nros::Node&)` that binds real callbacks by identity
      (typed `create_subscription(sub_, topic, [this]{…})` + raw
      `create_subscription_raw(sub_, topic, [this](const uint8_t*, size_t){…})`).
- [ ] Add the C++ `create_subscription_raw` wrapper over
      `nros_cpp_subscription_register` (`subscription.hpp`) — the no-deserialize
      zero-copy form.
- [ ] `NROS_NODE(Talker)` macro: emit a factory (`construct(void* slot, Node&)`)
      + `sizeof`/`alignof` export + the present/class-name symbols. Drop the
      register trampoline.
- [ ] Convert ONE example (native cpp talker→listener) to the new shape; prove
      real pub/sub through the **executor** (not the interpreter) on native.

### 240.2 — Typed codegen Entry (native first)
- [ ] `nano_ros_node_register` records `class_header` into the metadata JSON.
- [ ] Entry codegen (`codegen/entry/`): read `nros-metadata.json`, extend
      `PlanNode` with `{class, class_header}`; map launch `(pkg,exec)` → them.
- [ ] `emit_cpp`: new typed-entry mode — per node `#include "<class_header>"`,
      construct into an arena slot, `configure(node)`, run the real executor
      (`nros::init → spin_once loop → shutdown`). Retire the `NodeContextOps`
      recording dispatch.
- [ ] Native multi-node workspace entry runs real logic end-to-end (replaces the
      `phase235_a` synthesized-counter path).

### 240.3 — Carrier + embedded board adapter (NuttX)
- [ ] Rewrite the NuttX carrier branch (`NanoRosNodeRegister.cmake`) +
      `nuttx_entry_main.cpp.in`: emit `#include` + construct + executor spin
      instead of `NuttxBoard::run(register_symbol)`. Pass `CLASS` + `class_header`
      (the carrier already has `_NRC_CLASS`).
- [ ] Board lifecycle: `nros::init(locator, domain)` → construct components →
      `spin_once` loop → `shutdown`; delete the `EntryNodeRuntime` use in
      `NuttxBoard`. (Keep the slirp-locator bake + `app_main` shim from 238.)
- [ ] NuttX cpp talker/listener **real-logic** pub/sub E2E (real counter from the
      user `on_tick`, not synthesized). Migrate `examples/qemu-arm-nuttx/cpp/{talker,listener}`.

### 240.4 — C path parity
- [ ] C component shape: a `struct` (state) + `nros_ret_t configure(nros_node_t*)`
      registering C callbacks (`fn ptr + void* ctx`) via the C FFI
      (`nros_subscription_callback_t (data,len,ctx)` — exists in nros-c). C
      `NROS_NODE` equivalent (factory + sizeof) [Q7].
- [ ] Typed C++ entry constructs + configures a C component (define the C
      factory/configure seam the entry calls — mixed 238.C build).
- [ ] NuttX C talker/listener real-logic E2E.

### 240.5 — Service / action on the executor (the unspiked transports)
- [ ] Prove service-server / action-server **callback** dispatch
      (`nros_cpp_service_server_register`, `nros_cpp_action_server_register`) +
      the **poll** clients (`try_recv_*`) boot + exchange on NuttX. (Closes the
      RFC-0043 §Spike scope gap.)
- [ ] Migrate `examples/qemu-arm-nuttx/{c,cpp}/{service-*,action-*}` to real
      handler bodies. (Clients: poll now; move to callbacks when RFC-0041's C/C++
      wave lands — phase-239 follow-up.)

### 240.6 — Retire the interpreter
- [ ] Delete `EntryNodeRuntime` + `detail::entry_*` synthesis (`main.hpp`); delete
      `DeclaredNode` / `DeclaredCallback` / `record_callback_effect` + the
      `NodeEntityDescriptor` string-descriptor `NodeContextOps` seam.
- [ ] Remove the 238 synthesized bodies (counter / `a+b` / fixed result) once all
      examples run real logic. Update RFC-0032 §8a + RFC-0043 to `Stable`.

### 240.7 — Non-counter E2E + ASI (gates 236.C)
- [ ] A node with a real subscription→publish callback (transform), C++ and C,
      proving 240.1/.2 (= phase-236 236.D.5).
- [ ] ASI `actuation_module` `Controller` runs through the generated Entry path
      on FVP (Zephyr+Cyclone), output observed by stock ROS 2 (phase-236 236.C
      acceptance).

## Acceptance

- Generated C++/C Entry boots a multi-node app with **live** pub/sub through the
  executor (no synthesized counter), native + NuttX.
- Service/action examples run real handler bodies (servers callback, clients
  poll/RFC-0041) on NuttX.
- `EntryNodeRuntime` + the string-descriptor declarative layer deleted; no
  `__nros_component_<pkg>_register` symbol.
- ASI `Controller` runs through the declarative Entry path on FVP (236.C).
- Phase 238 NuttX E2E matrix stays green — now on real logic.

## Notes / cross-refs

- Design rationale + alternatives + open questions: RFC-0043. This doc is the
  work breakdown only.
- The no-naming primitive is the executor *closure* API, not `nros::node!()`
  (which names) — see RFC-0043 §Summary correction.
- Rust embedded already runs real bodies (`ExecutorNodeRuntime` + `nros::main!`);
  this phase is the C++/C runtime-parity half (RFC-0032 §8a).
- Migration is stageable per transport + per example — pub/sub first (spiked),
  service/action after 240.5, retire the interpreter last (240.6).
