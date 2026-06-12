# Phase 240 — Entry real-callback binding (RFC-0043 implementation)

**Goal.** Implement [RFC-0043](../design/0043-entry-real-callback-binding.md):
make the codegen Entry path run **real user logic** by routing to the real Rust
executor, with the component a **stateful object** binding callbacks **by
identity** (no naming). Retire the synthesizing `EntryNodeRuntime` interpreter +
the `DeclaredNode`/`record_callback_effect` string layer. Brings the C++/C Entry
path to runtime parity with the Rust embedded path (which already runs real
bodies). Unblocks RFC-0032 §8a / phase-236 236.C (ASI deletes its imperative
`main.cpp`).

**Status.** In progress (2026-06). Design = RFC-0043 (Draft). **240.1 DONE**
(2026-06-12) — `nros/component.hpp` member-callback binding + a native
component-object POC running real pub/sub on the executor
(`examples/native/cpp/component-poc`). The NuttX executor-callback path is also
spike-validated for pub/sub + timer (RFC-0043 §Spike); service/action on the
executor under the embedded lifecycle is unspiked (240.5). This phase carries
236.D's detailed breakdown (phase-236 236.D now points here).

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

### 240.1 — Component-object API (C++, native) — **DONE 2026-06-12**
- [x] Component shape: a class with member entity handles + state + a
      `Result configure(nros::Node&)` that binds real callbacks **by identity**.
      The typed callback-style API is *stateless* (`void(const M&)`, no ctx), so
      the binding uses the ctx-carrying paths (timer `cb,ctx`; the **raw**
      register `cb(data,len,ctx),ctx`) with the component pointer as `ctx`.
- [x] `packages/core/nros-cpp/include/nros/component.hpp`: `create_subscription_raw`
      (over `nros_cpp_subscription_register`) + `bind_timer<C,&C::m>` /
      `bind_subscription_raw<C,&C::m>` — member-fn-pointer-as-template-param →
      a **no-alloc** non-capturing-lambda trampoline (no `std::function`, no
      string name). `NROS_BIND_TIMER` / `NROS_BIND_SUB_RAW` convenience macros.
- [x] Proof: `examples/native/cpp/component-poc/` — a `Talker` (timer member
      `on_tick` publishes a real counter) + `Listener` (raw zero-copy member
      `on_raw` receives) constructed + `configure`d + spun on the **real
      executor** (no interpreter). Native, two-process vs zenohd:
      `Published 0..19` + `Received 0..16` (correct values).
- **Finding (raw vs typed type-name form):** the typed `Publisher<Int32>`
      registers the **DDS-mangled** keyexpr `std_msgs::msg::dds_::Int32_`, but the
      raw register uses the passed string verbatim — a raw sub must pass
      `M::TYPE_NAME` (the mangled form) to match a typed publisher. Raw-vs-typed
      type-name-form unification is a separate concern; noted for 240.2.
- [ ] `NROS_NODE(Talker)` factory/marker macro (factory + `sizeof` + present/
      class-name symbols, drop the register trampoline) → **moved to 240.2**,
      where the codegen entry's construction needs determine its exact shape.

### 240.2 — Typed codegen Entry (native first) — **core DONE 2026-06-12**
- [x] Board `run_components` (`main.hpp`) — the real-executor entry on every board
      (`NativeBoard`/`ZephyrBoard`/`NuttxBoard`): init → `setup()` (constructs +
      `configure`s the components) → `detail::component_spin_loop()` (pumps
      `spin_once`, dispatches the real callbacks; honors `$NROS_ENTRY_SPIN_MS`) →
      shutdown. **No** `EntryNodeRuntime`. Validated on native via
      `component-poc` (`Published 22` / `Received 22`).
- [x] `PlanNode` extended with `{class_name, class_header}`
      (`codegen/entry/mod.rs`); legacy emitters ignore them.
- [x] `emit_cpp::emit_typed` (`codegen/entry/emit_cpp.rs`) — per node
      `#include "<class_header>"` + static component/node storage + a
      `__nros_entry_setup` (construct node + `configure`) + `main` →
      `Board::run_components(&__nros_entry_setup)`. No register symbol, no
      `NodeContext`. 4 unit tests (headers/construct/run_components, dup-pkg →
      two instances one include, nuttx board, missing-class error).
- **240.2b — plumbing DONE 2026-06-12** (the metadata → codegen → cmake seam):
  - [x] `nano_ros_node_register` accepts an optional `HEADER` and otherwise
        derives the component header from `CLASS` by convention
        (`pkg::Sub::Class` → `pkg/Sub/Class.hpp`), recording `class_header` in the
        `components[]` metadata JSON (`NanoRosNodeRegister.cmake`).
  - [x] `codegen/entry/metadata.rs` — `ComponentIndex` reads `nros-metadata.json`,
        keys components by `(pkg, exec)` (pkg = `class` prefix before `::`, L.4),
        and `enrich_plan` stamps `PlanNode.{class_name, class_header}` (errors on a
        launch node with no matching component / no header). Unit-tested.
  - [x] CLI `nros codegen entry --typed --metadata <json>` (C++ only) enriches the
        plan then calls `emit_cpp::emit_typed` (`cmd/codegen.rs`). Full Rust seam
        (plan → enrich → emit_typed) integration-tested against the
        `multi-node-workspace-cpp` template (`tests/entry_typed_plan.rs`).
  - [x] `nano_ros_entry(... TYPED)` opt-in threads `--typed --metadata
        ${CMAKE_BINARY_DIR}/nros-metadata.json` through `_nros_entry_invoke_codegen`
        (`NanoRosEntry.cmake`). Node pkgs' `add_subdirectory` must precede the entry
        (metadata must list every component; the entry links them anyway).
  - **240.2b-E2E — typed example + TU generation DONE 2026-06-12**:
    - [x] `examples/templates/multi-node-workspace-cpp-typed/` — talker/listener
          components expose `Result configure(nros::Node&)` (binding member
          callbacks by identity via `component.hpp`) + headers at
          `include/<pkg>/<Class>.hpp`; the Entry uses `nano_ros_entry(... TYPED)`.
    - [x] Verified `nros codegen entry --lang cpp --typed --metadata …` against
          that workspace emits a TU constructing both real components +
          `NativeBoard::run_components` (no `__nros_component_*`, no `NodeContext`).
    - [x] **Native build + run E2E validated 2026-06-12.** Built
          `multi-node-workspace-cpp-typed` with the real toolchain (nano-ros C++ +
          zenoh-pico host + `emit_typed` entry → `robot_entry`); ran two processes
          vs `build/zenohd`: `Published 0..7` + `Received 0..7` — the typed codegen
          entry constructs both components, `configure` binds real timer-publish +
          raw-sub callbacks, `run_components` spins the real executor, callbacks
          fire. (cmake-fixture registration for CI still TODO.)
  - [ ] raw↔typed type-name-form unification (240.1 finding) — still open.

### 240.3 — Carrier + embedded board adapter (NuttX) — **mechanism DONE 2026-06-12**
- [x] Typed NuttX carrier: `nano_ros_node_register(TYPED …)` (C++) emits
      `cmake/templates/nuttx_entry_main_typed.cpp.in` — construct the component +
      `configure(node)` + `NuttxBoard::run_components(locator, &setup)` — instead of
      the register-symbol → interpreter template. Substitution vars `NROS_ENTRY_CLASS`
      / `NROS_ENTRY_CLASS_HEADER` (= derived/`HEADER`) / `NROS_ENTRY_NODE_NAME`.
      Render-verified against the listener (matches the proven native typed TU shape).
- [x] Board lifecycle already lands in 240.2: `NuttxBoard::run_components` does
      `network_wait → init(locator,domain) → setup() → component_spin_loop →
      shutdown` (no `EntryNodeRuntime`). Slirp-locator bake + `app_main` shim kept.
- [x] Migrated `examples/qemu-arm-nuttx/cpp/listener` to a typed component
      (`Result configure(nros::Node&)` binding a raw member sub by identity;
      `TYPED HEADER Listener.hpp`; keeps the `Waiting for messages` rtos_e2e marker).
- [ ] **240.3-rest** — migrate `…/cpp/talker` to a typed `Publisher<Int32>`
      timer component (needs C++ `std_msgs` header provisioning via
      `nros_find_interfaces(LANGUAGE CPP)` on the example); cross-build the typed
      ELFs + run the NuttX two-process real-logic pub/sub E2E (build tier).

### 240.4 — C path parity — **mechanism DONE 2026-06-12**
- [x] C component shape: a `struct` (state) + `nros_ret_t configure(const
      nros_cpp_node_t*, StructT*)` binding C callbacks (`fn ptr + void* ctx`)
      by identity. `packages/core/nros-c/include/nros/component.h`:
      `NROS_C_COMPONENT(StructT, configure_fn)` emits the C-ABI factory +
      configure (`__nros_c_component_<pkg>_{create,configure}`, keyed on
      `NROS_PKG_NAME`); plain-C prototypes for the C-ABI `nros_cpp_subscription_register`
      + a `nros_cpp_qos_t` mirror + `nros_c_qos_default()`. **The bridge:** the
      `nros_cpp_*` FFI symbols are C-ABI (the `cpp` is a namespace prefix, not C++
      linkage), so C calls them directly against the node the Entry hands it — C
      and C++ components share the SAME executor + node. Header gcc-syntax-checked;
      macro symbol-names verified. (Q7 → factory, storage in the C TU — no sizeof
      leak to the Entry; timer-from-C deferred since the executor handle is
      private, so the C mechanism is sub-only like the cpp listener.)
- [x] Typed entry constructs + configures a C component: `emit_typed` branches on
      `lang` — a `"c"` node forward-declares + calls the factory/configure seam
      with `node.ffi_handle()` (no class, no header), a `"cpp"` node uses its class
      (`emit_cpp.rs`; `lang` threaded through `PlanNode` + the metadata reader).
      Unit-tested (C-only + mixed C/C++). NuttX C typed carrier:
      `nuttx_entry_main_c_typed.cpp.in` + the `nano_ros_node_register(TYPED LANGUAGE
      C)` carrier branch (render-verified).
- [x] Migrated `examples/qemu-arm-nuttx/c/listener` to a typed C component
      (`NROS_C_COMPONENT` + raw member sub; `TYPED LANGUAGE C`; keeps the
      `Waiting for messages` marker). gcc-syntax-checked.
- [ ] **240.4-rest** — NuttX C talker (publish needs a typed publisher / a raw
      publisher-create-from-C seam — the timer-from-C gap) + cross-build the C
      typed ELFs and run the NuttX two-process real-logic E2E (build tier).

### 240.5 — Service / action on the executor (the unspiked transports)
- **Service-server DONE 2026-06-12** (callback dispatch on the executor):
  - [x] C++ `bind_service_raw<C, &C::on_request>` over
        `nros_cpp_service_server_register` (`component.hpp`) — member handler
        `bool(req, req_len, resp, resp_cap, resp_len)` bound by identity, `this`
        as ctx, no-alloc trampoline; `create_service_raw` wrapper.
  - [x] C `nros_cpp_service_server_register` prototype + callback typedef
        (`component.h`); the C component's `configure` calls it directly.
  - [x] Migrated `examples/qemu-arm-nuttx/{cpp,c}/service-server` to typed
        components with **real** AddTwoInts handlers (decode CDR int64 a/b →
        write int64 sum), `TYPED` carrier; keep the `Waiting for requests`
        marker. Both gcc-syntax-checked; cpp mirrors the proven `bind_*` pattern.
        The typed carrier is component-agnostic (same `configure(node)` shape) —
        no template change needed.
- **Action-server (C++) DONE 2026-06-12**:
  - [x] `Node::executor_handle()` accessor (the raw action FFI is executor- not
        node-scoped). `component.hpp`: `ActionServerStorage` (arena-held buffer) +
        `create_action_server_raw` (create → register → set_callbacks) +
        `bind_action_server_raw<C, &C::on_goal, &C::on_cancel>` — ctx-carrying
        goal/cancel trampolines bound by identity.
  - [x] Migrated `examples/qemu-arm-nuttx/cpp/action-server` to a typed Fibonacci
        component: `on_goal` decodes the CDR `int32 order` + accepts, a bound
        timer executes (computes the sequence, hand-encodes the `int32[]` result
        CDR, `nros_cpp_action_server_complete_goal`); prints `Waiting for goals` /
        `Goal accepted`. **CDR hand-encoding + action protocol need build-tier
        validation** (no C++/zenoh+NuttX cross-build in this env).
- **C executor seam + C action-server DONE 2026-06-12**:
  - [x] Uniform C `configure(node, executor, self)` — the C component now gets the
        opaque executor handle (the C analog of `Node::executor_handle()`) for
        executor-scoped transports. Rippled through `NROS_C_COMPONENT`, the
        codegen `emit_typed` C branch + the C typed carrier template + the
        existing C listener/service-server configures (executor unused there).
  - [x] `component.h`: C action FFI (create/register/set_callbacks/complete_goal +
        goal/cancel typedefs + GoalResponse/CancelResponse consts + storage size)
        + a C timer FFI. Migrated `c/action-server` to a typed Fibonacci component
        (symmetric with the cpp one). gcc-syntax-checked.
- **Poll clients (service + action, C++ + C) DONE 2026-06-12**:
  - [x] `component.hpp`: `ServiceClientStorage`/`ActionClientStorage` + `create_*_raw`
        wrappers. `component.h`: the C poll-client FFI (service_client create/send/
        try_recv; action_client create/send_goal/try_recv_goal_response/get_result)
        + storage sizes.
  - [x] Migrated `{cpp,c}/service-client` (timer-driven send/poll → prints
        `Response: N`) and `{cpp,c}/action-client` (timer-driven poll state machine:
        send goal → poll acceptance → get_result → prints `Result received: N terms`)
        to typed poll components. All hand-rolled CDR; gcc-syntax-checked (C),
        pattern-aligned (C++). (Clients move to callbacks when RFC-0041's C/C++
        wave lands.)
- **Build-tier cross-build validated 2026-06-12.** With the real NuttX toolchain
  (arm-none-eabi-gcc + nightly-2026-04-11 `-Z build-std` + staged kernel), the
  **entire migrated NuttX matrix** cross-compiles + links into bootable ARM ELFs:
  `{cpp,c}/{listener, service-server, action-server, service-client,
  action-client}` (10 ELFs). Validates the typed C++/C carriers (240.3/240.4),
  the `NROS_C_COMPONENT` C-ABI bridge + executor-arg seam, `bind_*_raw` +
  `create_*_client_raw`, and the hand-rolled CDR / action FFI (240.5) all compile
  + link on-target. The generated NuttX entry is the typed
  `NuttxBoard::run_components` (no `EntryNodeRuntime`, no `NodeContext`).
- **240.5-runtime-E2E — partial 2026-06-13** (QEMU boot + exchange):
  - [x] **Service E2E PASSES** on NuttX (cpp) in QEMU (`rtos_e2e`
        `test_rtos_service_e2e` — an existing case, now green via the typed
        migration): 15 responses `3,5,7,…,31` (correct `a+b`). The typed
        `bind_service_raw` handler + the typed poll client + the hand-rolled
        AddTwoInts CDR all work at runtime.
  - [x] **Action SERVER runtime-validated**: in QEMU the server logs
        `Goal accepted: order=5` → `Goal succeeded: 5 terms (rc=0)` — the raw
        action server (create/register/set_callbacks + the timer-driven execute +
        the hand-rolled Fibonacci result CDR + `complete_goal`) executes a real
        goal end-to-end.
  - [ ] **Pub/sub E2E — blocked on the talker (not the typed listener).** In QEMU
        the typed listener boots + reaches `Waiting for messages`, but 0 received:
        the paired NuttX talker is still the **declarative** example and published
        nothing (empty output) — the publish-from-component talker gap (240.6
        blocker). Also undetermined until then: the raw-sub keyexpr the listener
        must match — services passed with the ROS slash form, native pub/sub used
        the DDS-mangled form (the raw↔typed type-name unification, 240.1 finding).
        Resolve by migrating the talker to a typed `Publisher` component.
  - [ ] **Action CLIENT poll — open gap.** The raw poll client sends one goal
        (fixed: blocking `send_goal` re-enters the executor from the spin_once
        timer → switched to `send_goal_async`; `setvbuf` unbuffers the
        transition-only output; `nros_cpp_action_client_poll` each tick) and the
        server receives + completes it, but the client's goal-response/result
        queryable replies are not surfaced by `try_recv_goal_response` /
        `try_recv_result`. Needs the client RX wiring investigated (likely a
        register/async-poll nuance). NuttX kept OUT of `test_rtos_action_e2e`
        until fixed.

### 240.6 — Retire the interpreter — **BLOCKED (retirement plan + RFC done 2026-06-12)**

The interpreter deletion is the **terminal** step (RFC-0043 is now `Stable` — the
binding mechanism is implemented + validated across every transport and both
languages). Deletion is **gated** on removing every remaining declarative
consumer first; doing it before then breaks unmigrated platforms that the build
tier (not this env) must validate.

- [x] RFC-0043 `Draft → Stable`; RFC-0032 §8a "callback bodies" → resolved by
      RFC-0043. Retirement plan + blockers recorded here.

**Blockers — declarative consumers still live (must migrate/delete first):**
- **~46 example sources** on platforms not yet migrated + unbuildable here:
  - `examples/qemu-arm-freertos/{c,cpp}/*` (talker/listener/service-*/action-*)
  - `examples/threadx-linux/cpp/*` (all roles)
  - `examples/qemu-arm-nuttx/{c,cpp}/talker`, `examples/qemu-riscv-nuttx/c/talker`
    (talker = the publish-from-component / timer gap; needs the typed `Publisher`
    + C++ msg-header provisioning, or a raw-publisher-create seam)
  - native templates/workspaces: `examples/templates/{multi-node-workspace-cpp,
    c-and-cpp-mixed-workspace,pure-c-workspace}`, `examples/workspaces/{c,cpp,mixed}`
    (a typed `multi-node-workspace-cpp-typed` already exists as the replacement
    shape — 240.2b).
- **Codegen:** the legacy `emit_cpp::emit` + `emit_c::emit` (register-symbol →
  interpreter) and their CLI dispatch; replace with `emit_typed` once all launch
  entries are typed.
- **C++ headers:** `EntryNodeRuntime` + `detail::entry_*` + `Board::run(lambda)`
  (`main.hpp`); `declared_node.hpp`; the declarative bits of `node_pkg.hpp` +
  `NodeContextOps` / `NodeEntityDescriptor` string-descriptor seam.
- **C headers:** the `nros_declared_node_*` / `NROS_NODE_REGISTER` declarative API
  (`node_pkg.h`) + the 238 carrier's non-typed `nuttx_entry_main.cpp.in`.
- **Synthesized 238 bodies** (counter / `a+b` / fixed result) — removable once no
  example relies on them.

**Ordered deletion (when unblocked, per-step build-validated):**
1. Migrate the publish-from-component talker gap (typed `Publisher` or raw
   publisher-create seam) → migrate all talker/pub examples.
2. Migrate freertos + threadx example sets (their carriers must learn the typed
   entry, mirroring the NuttX 240.3/240.4 carriers).
3. Migrate the native templates/workspaces to the typed shape; delete the legacy
   `emit_cpp::emit`/`emit_c::emit` + wire `emit_typed` as the only C/C++ emitter.
4. Delete `EntryNodeRuntime` + `Board::run(lambda)` + `declared_node.hpp` + the
   `NodeContextOps`/`NROS_NODE_REGISTER`/`nros_declared_node_*` declarative API +
   the non-typed carrier template + the synthesized bodies.

### 240.6-prep — deprecation markers (done 2026-06-12)
- [x] Doc-comment the interpreter + declarative entry API (`main.hpp`
      `EntryNodeRuntime`, `declared_node.hpp`) as **deprecated**, pointing at
      `component.hpp` / RFC-0043 as the replacement + naming phase-240.6 as the
      removal step. Non-breaking (comments only).

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
