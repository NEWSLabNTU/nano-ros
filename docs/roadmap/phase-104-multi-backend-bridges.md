# Phase 104 — Multi-RMW support + nros umbrella decoupling

**Goal.** Make the `nros` + `nros-node` umbrella crates fully
backend-agnostic at the Cargo / Rust API level so a single
binary can register and use multiple RMW backends (bridge
nodes) without compile-time mutual-exclusion. The user-facing
API mirrors `rclcpp`: one `Executor` holds many `Node`s,
each `Node` binds to one RMW backend, callbacks are scheduled
with first-class `SchedContext` per Node (default) and
per-handle (override). C and C++ APIs are thin wrappers
around the Rust surface — Phase 122 discipline applies.

Four coupled threads under one phase:

1. **API decoupling** — `nros` + `nros-node` carry no Rust
   deps on concrete RMW or platform crates. Core consumes
   only the generic ABI (`nros-rmw-cffi` vtable +
   `nros-platform-cffi` C header).
2. **Backend registration** — drop the singleton-vtable model.
   Registered backends form a named registry keyed on
   `"zenoh"`, `"xrce"`, `"dds"`, `"cyclonedds"`, future
   `"uorb"`. `NROS_RMW_MAX_BACKENDS` (build-time const,
   default 8) sets capacity.
3. **rclcpp-aligned API** — one `Executor` holds many `Node`s.
   Each `Node` binds to one (rmw, locator) tuple at creation.
   Sessions are cached per `(rmw, locator, domain_id)` and
   shared by sibling Nodes. Per-Node default `SchedContext`,
   per-handle override.
4. **Real-time integration** — multi-RMW + PiCAS, shared
   executor wake, cross-priority handoff guidance, per-backend
   WCET documentation.

**Status.** Plan rewritten 2026-05-14. Thread A (API
decoupling) landed on branch `phase-104-A-api-decoupling`
(commits `8f7667d3` … `6aebcea6`). Threads B/C/D/E not yet
started.

**Priority.** P1. Unblocks (a) PX4-on-drone bridge
(uORB ⇆ Zenoh), (b) ROS 2 cross-RMW gateways (XRCE ⇆ DDS),
(c) the "swap backend without rebuilding nros" promise the
phase 123 release-prep makes to users, (d) rclcpp parity for
the multi-Node-per-Executor pattern users expect.

**Depends on.** Phase 102 (typed entity structs — reserved
`vtable` slot in `nros_rmw_session_t`). Phase 110 (`SchedContext`
+ PiCAS + ARINC TT). Phase 121 (canonical platform-cffi).
Phase 122 (handle ABI collapse — handles already carry session
refs; C/C++ wrappers thin). Phase 123.A.1.x (physical archive
split — prerequisite for "link backend at outer layer" to be
real). Phase 117 (RMW vtable surface frozen).

## Background

Today's nano-ros build picks one RMW backend at compile time
and creates **one Executor with one node identity** per
process. Three load-bearing singletons enforce this:

1. **Cargo feature mutual-exclusion** —
   `compile_error!` in `nros/build.rs` if two `rmw-*` features
   are enabled.
2. **`ConcreteSession` type alias** in `nros-node` — collapses
   the executor to one Session type at compile time.
3. **`static VTABLE: AtomicPtr<NrosRmwVtable>`** in
   `nros-rmw-cffi/src/lib.rs:571` — one registered C backend
   per process.

The Executor also holds a single `node_identity` field — one
Node name+namespace per Executor — which is the load-bearing
constraint stopping multi-Node patterns even within a single
RMW.

This model differs from `rclcpp`:

```cpp
// rclcpp pattern that nros users expect
auto node_a = std::make_shared<rclcpp::Node>("node_a");
auto node_b = std::make_shared<rclcpp::Node>("node_b");
rclcpp::executors::SingleThreadedExecutor exec;
exec.add_node(node_a);
exec.add_node(node_b);
exec.spin();
```

`rclcpp` Executor holds N Nodes natively. RMW is process-level
(one per Context, fixed at launch via `RMW_IMPLEMENTATION`).
Bridges across RMW backends require separate processes; the
core doesn't multiplex.

Phase 104 keeps the rclcpp Executor-N-Nodes pattern and
**extends** with named-RMW-per-Node — Nodes attached to one
Executor can each bind to a different backend.

### The drone-bridge topology (driver use case)

```
[drone PX4 process]              [companion / cloud]
     uORB topics                       ROS 2 nodes
        ↓                                 ↑
   nros bridge ────── Zenoh ─────── zenohd ─────── rclcpp/rclrs
   (uORB sub +
    Zenoh pub)
```

The bridge subscribes to a small uORB topic set
(`vehicle_attitude`, `sensor_combined`,
`vehicle_local_position`, …) and republishes onto Zenoh keys
for the off-vehicle ROS 2 stack. Three reasons this needs both
backends in one binary:

1. **No agent in the middle.** `microxrcedds_agent` exists for
   the XRCE side; nothing equivalent for uORB. A bridge that
   lives inside or alongside PX4 is the cleanest path.
2. **Topic translation is the bridge's job.** PX4 doesn't
   speak Zenoh keys; the bridge maps uORB topic IDs ↔ ROS-2-
   style topic names.
3. **Single-binary deployment.** PX4 modules ship as one
   binary. Running two cooperating processes on flight
   hardware is a step backward.

## Design

### Conceptual model (rclcpp-aligned + multi-RMW)

```
Executor (1 per scheduler)
  ├── nodes:    Vec<Node>                ← rclcpp add_node pattern
  ├── sessions: cache<(rmw, locator, domain_id), Session>
  └── arena[handles]    each: session_ref + node_id + sched_ctx

Node (1+ per Executor)
  ├── name, namespace
  ├── session_ref       ← borrows one of Executor's cached sessions
  ├── default_sched     ← inherited by handles created via this Node
  └── factory methods for Publisher / Subscription / Service / Action / Timer

Session (1 per unique (rmw, locator, domain_id))
  ├── vtable            ← from named registry
  └── transport state

Handle (Pub / Sub / Service / Client / Action / Timer)
  ├── session_ref       ← copy of owning Node's session_ref
  ├── sched_ctx         ← Node default, overridable per-handle
  └── arena_entry_idx
```

Single-backend binary: registry has 1 entry, `create_node`
uses it implicitly. Multi-backend binary: registry has N
entries, `create_node_with_rmw(name, "xrce", locator)`
selects. Sessions deduped on `(rmw, locator, domain_id)` so
two Nodes with same triple share one session — rclcpp parity.

### What stays the same

- **Trait surface unchanged.** `Rmw + Session + RmwConfig` already
  support multiple Session instances at the type level. No trait
  additions.
- **Default single-backend builds: no code-size regression.** With
  one registered backend, registry has 1 entry, no name lookup
  on the hot path.
- **One `open()` call per session.** No adoption of upstream's
  `init_options_init` → `init` two-step.
- **No `implementation_identifier` per entity.** Rust
  monomorphisation catches cross-backend wiring at compile time.
  C / C++ trust the Node-Session binding established at
  Node creation. The runtime identifier would cost a pointer
  per entity for a use case the type system already covers.

### Threads

#### Thread A — API decoupling (LANDED)

Files: `packages/core/{nros,nros-node}/Cargo.toml`,
`packages/core/nros-platform/src/resolve.rs`,
`packages/core/nros-rmw-cffi/src/lib.rs`,
`packages/{zpico/nros-rmw-zenoh,dds/nros-rmw-dds,
xrce/nros-rmw-xrce-cffi}/src/lib.rs`, ~117 consumer
Cargo.tomls, `scripts/check-decoupling.sh`,
`justfile`.

- [x] **104.A.1** — Drop concrete RMW deps from `nros`.
- [x] **104.A.2** — Drop concrete RMW deps from `nros-node`.
      `register_active_backend` cfg cascade deleted; `Executor::open`
      probes `nros_rmw_cffi::backend_registered()` instead.
- [x] **104.A.3** — Inline POSIX net-size consts in `resolve.rs`.
- [x] **104.A.4** — `just check-decoupling` CI guard.
- [x] **104.A consumer sweep** — 117 Cargo.tomls collapsed.

#### Thread B — Backend registration model (LANDED)

Files: `packages/core/nros-rmw-cffi/{build.rs,src/lib.rs,
include/nros/rmw_vtable.h,src/rust_adapter.rs,tests/registry.rs}`,
`packages/{zpico/nros-rmw-zenoh,dds/nros-rmw-dds,
xrce/nros-rmw-xrce/src/vtable.c,xrce/nros-rmw-xrce-cffi}/src/lib.rs`,
`packages/core/nros-c/cmake/NanoRosCTargets.cmake`,
`packages/core/nros-c/c-stubs/weak_register_backends.c`,
`book/src/internals/rmw-backends.md`.

- [x] **104.B.1 — `NROS_RMW_MAX_BACKENDS` build-time const.**
      `nros-rmw-cffi/build.rs` reads
      `NROS_RMW_MAX_BACKENDS` env var (default 8) + emits
      `cargo:rustc-env=NROS_RMW_MAX_BACKENDS=<n>`. The crate
      consumes via `const MAX_BACKENDS: usize = parse(env!(…))`
      pattern, matching how `NROS_EXECUTOR_MAX_CBS` and
      `NROS_LET_BUFFER_SIZE` flow today. Cortex-M0+ users
      can drop to 2; companion-class users with bridge
      ambitions can bump to 16.
      **Files:**
      `packages/core/nros-rmw-cffi/build.rs`,
      `packages/core/nros-rmw-cffi/src/lib.rs`.

- [x] **104.B.2 — Named registry replaces singleton VTABLE.**
      Replace `static VTABLE: AtomicPtr<NrosRmwVtable>` with
      `static REGISTRY: Mutex<heapless::Vec<Backend,
      MAX_BACKENDS>>`. New entry points:
      - `nros_rmw_cffi_register_named(name: *const c_char,
        vtable: *const NrosRmwVtable) -> NrosRmwRet`
      - `nros_rmw_cffi_lookup(name: *const c_char) ->
        *const NrosRmwVtable`
      - `nros_rmw_cffi_registered_names(buf: *mut *const
        c_char, cap: usize) -> usize` (for diagnostics).
      Existing `nros_rmw_cffi_register(vtable)` becomes a
      shim that calls `_register_named("default", vtable)`
      so single-backend ctors keep working unmodified
      through one release.
      **Files:**
      `packages/core/nros-rmw-cffi/src/lib.rs`,
      `packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`.

- [x] **104.B.3 — Duplicate-register semantics.**
      `_register_named("zenoh", v1)` then
      `_register_named("zenoh", v2)`: overwrite + log
      warning. Idempotent for ctor firing twice (e.g., the
      same backend `.a` reached via two link paths). Bug-
      catching via tests.
      **Files:** test addition in
      `packages/core/nros-rmw-cffi/tests/`.

- [x] **104.B.4 — Per-backend `register_named` calls.** Each
      backend's existing `_register` C entry point switches
      from `nros_rmw_cffi_register(&VTABLE)` to
      `nros_rmw_cffi_register_named("<name>", &VTABLE)`. POSIX
      ctors (Phase 104.A) continue to fire automatically.
      Names: `"zenoh"`, `"dds"`, `"xrce"`. Future:
      `"cyclonedds"` (C++ side, registered via
      `nros::init` hook today), `"uorb"`.
      **Files:**
      `packages/zpico/nros-rmw-zenoh/src/lib.rs`,
      `packages/dds/nros-rmw-dds/src/lib.rs`,
      `packages/xrce/nros-rmw-xrce-cffi/src/lib.rs`,
      `packages/xrce/nros-rmw-xrce/src/vtable.c`.

- [x] **104.B.5 — `nano_ros_link_rmw` whole-archive
      audit.** Done. `NanoRosCTargets.cmake`'s RMW link block
      now wraps the imported RMW target with
      `-Wl,--whole-archive` / `--no-whole-archive` (GNU ld /
      lld on Linux+BSD), `-Wl,-force_load,$<TARGET_FILE:…>`
      on macOS, `/WHOLEARCHIVE:<path>` on MSVC. The wrap
      tokens live in `INTERFACE_LINK_LIBRARIES` (not
      LINK_OPTIONS) so cmake preserves their position around
      the archive.

      Verified: legacy
      `target_link_libraries(t NanoRos::NanoRos)` users whose
      code path bypasses `nano_ros_link_rmw`'s explicit stub
      (104.B.6) now keep the auto-register `.init_array`
      ctor + all zenoh-pico C objects in the final binary.
      Example: `examples/native/c/zenoh/talker` binary jumps
      from a stub-only ~few-MB shape to 11 MB; `nm` shows
      `nros_rmw_zenoh_register` (T) + 100+ `_z_*` zenoh-pico
      symbols + a non-empty `.init_array` section. The
      explicit-stub path (104.B.6) remains the canonical
      mechanism for bare-metal targets without `.init_array`
      walking; the two paths coexist idempotently on POSIX.
      **Files:**
      `packages/core/nros-c/cmake/NanoRosCTargets.cmake`.

- [x] **104.B.6 — Bare-metal explicit-call stub.** Done
      (co-implemented with 123.A.11). nros-c calls
      `nros_app_register_backends()` from `nros_support_init`
      via `unsafe extern "C"`. The weak no-op default lives
      in `packages/core/nros-c/c-stubs/weak_register_backends.c`
      (cc-built by nros-c's `build.rs`, emits a `W`
      `nros_app_register_backends` symbol).
      `nano_ros_link_rmw(<target> [RMW <r>])` writes a
      strong-def stub to
      `<build>/_nano_ros_link/<target>/nros_app_register_backends.c`
      that `extern int nros_rmw_<r>_register(void);` +
      calls each. Multiple `nano_ros_link_rmw` calls on the
      same target accumulate the backend list (deduped via
      `_NANO_ROS_LINKED_RMWS` target property).
      Verified: `pkg_c_talker` in
      `examples/multi-package-workspace/` shows the
      generated stub + `nm` on the final binary reports
      `nros_app_register_backends` as `T` (strong), not `W`
      (weak) — linker picked the per-target strong def.
      **Files:**
      `packages/core/nros-c/cmake/NanoRosCTargets.cmake`,
      `packages/core/nros-c/c-stubs/weak_register_backends.c`,
      `packages/core/nros-c/build.rs`,
      `packages/core/nros-c/src/support.rs`.

- [x] **104.B.7 — Backend-name catalogue.** Document the
      reserved names (`"zenoh"`, `"dds"`, `"xrce"`,
      `"cyclonedds"`, `"uorb"`) + the naming policy
      (lowercase, ASCII, no transport variants — XRCE-UDP
      and XRCE-serial both register as `"xrce"`, the
      transport is selected via locator) in
      `book/src/internals/rmw-backends.md`.
      **Files:** `book/src/internals/rmw-backends.md`.

#### Thread C — rclcpp-aligned Executor + Node API

The Rust API is the source of truth; C and C++ are thin
wrappers (Phase 122 discipline).

##### Rust surface

```rust
// Single-backend (no change for current users)
let mut exec = Executor::new(ExecutorConfig::default())?;
let node = exec.create_node("my_node")?;
let pub_ = node.create_publisher::<Int32>("/topic", qos())?;
exec.spin();

// Multi-backend bridge
let mut exec = Executor::new(ExecutorConfig::default())?;
let node_in  = exec.node_builder("ingress")
    .rmw("zenoh")        // optional in single-backend builds
    .locator("tcp/127.0.0.1:7447")
    .sched(SchedContext::periodic(Priority::new(90), 10_000)
        .with_deadline(5_000)
        .with_os_pri(80))
    .build()?;
let node_out = exec.node_builder("egress")
    .rmw("xrce")
    .locator("udp/agent:8888")
    .sched(SchedContext::best_effort().with_os_pri(20))
    .build()?;

let pub_out = node_out.create_publisher::<Int32>("/fwd", qos())?;
node_in.create_subscription::<Int32, _>("/src", qos(), move |msg| {
    // handoff queue when crossing priority boundary
    egress_q.push(msg.clone()).ok();
})?;

exec.spin();
```

##### C surface (thin wrapper)

```c
nros_executor_t exec;
nros_executor_init(&exec, ...);

// Single-backend: registry has 1 entry, picks implicitly
nros_node_t node;
nros_node_init(&node, &exec, "my_node", "");

// Multi-backend bridge: name the rmw + locator
nros_node_t node_in, node_out;
nros_node_options_t opts_in = {
    .rmw_name = "zenoh",
    .locator  = "tcp/127.0.0.1:7447",
    .sched    = NROS_SCHED_PERIODIC(/*pri*/ 90, /*period_us*/ 10000),
};
nros_node_init_ex(&node_in, &exec, "ingress", "", &opts_in);

nros_node_options_t opts_out = {
    .rmw_name = "xrce",
    .locator  = "udp/agent:8888",
    .sched    = NROS_SCHED_BEST_EFFORT(/*os_pri*/ 20),
};
nros_node_init_ex(&node_out, &exec, "egress", "", &opts_out);

nros_publisher_t pub_out;
nros_publisher_init(&pub_out, &node_out, &INT32_TYPE, "/fwd");

nros_subscription_t sub_in;
nros_subscription_init(&sub_in, &node_in, &INT32_TYPE, "/src", on_msg, &pub_out);
nros_executor_register_subscription(&exec, &sub_in, NROS_EXECUTOR_ON_NEW_DATA);

nros_executor_spin(&exec);
```

`nros_node_init_ex` is the new entry point with options
struct; existing `nros_node_init` keeps the simple no-RMW-name
overload. Per Phase 122 discipline, both are thin wrappers
around the Rust `Executor::create_node` / `node_builder` path
— the C struct is `state + _opaque`, all logic in Rust.

##### C++ surface (thin wrapper)

```cpp
auto exec = nros::Executor::create();
auto node_in  = exec->node_builder("ingress")
    .rmw("zenoh")
    .locator("tcp/127.0.0.1:7447")
    .sched(nros::SchedContext::periodic(90, 10'000ms)
        .with_deadline(5'000us)
        .with_os_pri(80))
    .build();
auto node_out = exec->node_builder("egress")
    .rmw("xrce")
    .locator("udp/agent:8888")
    .sched(nros::SchedContext::best_effort().with_os_pri(20))
    .build();

auto pub_out = node_out->create_publisher<Int32>("/fwd", qos());
node_in->create_subscription<Int32>("/src", qos(),
    [pub_out](const Int32& m) { egress_q.push(m); });

exec->spin();
```

C++ class methods delegate to the C ABI via the Phase 122
opaque wrapper pattern (`nros::Executor` holds an
`nros_executor_t` storage, methods call C entry points). No
C++-side logic; C surface stays canonical.

##### API items

- [x] **104.C.1 — Per-session vtable pointer.**
      Embed `vtable: *const NrosRmwVtable` in
      `nros_rmw_session_t` (C) / `NrosRmwSession`
      (Rust). All dispatch sites
      (`Session::create_publisher`, `Publisher::publish_raw`,
      `Subscriber::try_recv_raw`, …) thread through
      `session->vtable->fn(...)` instead of reading the
      static `VTABLE`. Phase 102's reserved slot makes this
      a one-pointer addition.
      **Files:**
      `packages/core/nros-rmw-cffi/include/nros/rmw_entity.h`,
      `packages/core/nros-rmw-cffi/src/lib.rs`,
      every backend's session-creation path.

- [x] **104.C.2 — `Executor` holds `Vec<Node>` +
      `session_cache<(rmw, locator, domain_id), Session>`.**
      Move the single `node_identity` field on Executor to
      a `Vec<Node>`. Add session cache. `create_node` /
      `create_node_with_rmw` look up or open Sessions
      lazily.
      **Files:**
      `packages/core/nros-node/src/executor/spin.rs`,
      `packages/core/nros-node/src/executor/mod.rs`,
      `packages/core/nros-node/src/node.rs`.

- [x] **104.C.3 — `Executor::node_builder(name)` API.**
      Builder pattern returns a `NodeBuilder` with
      `.rmw(name)`, `.locator(s)`, `.domain_id(d)`,
      `.namespace(s)`, `.sched(sc)`, `.build() -> Node`.
      `Executor::create_node(name)` becomes shorthand for
      `node_builder(name).build()`. Existing
      `Executor::open(cfg)` migrates to construct the
      Executor + auto-create one Node named `cfg.node_name`
      so single-backend single-node apps need no source
      change.
      **Files:**
      `packages/core/nros-node/src/executor/spin.rs`,
      `packages/core/nros-node/src/node.rs`.

- [ ] **104.C.4 — Per-Node default `SchedContext`.**
      `NodeBuilder::sched(sc)` stores a default
      `SchedContext` in the Node. Handle factories
      (`create_publisher` / `create_subscription` / etc.)
      inherit the Node's default unless an override is
      passed at handle creation.
      **Files:**
      `packages/core/nros-node/src/node.rs`,
      `packages/core/nros-node/src/executor/handles.rs`.

- [~] **104.C.5 — `multi-backend` Cargo feature on `nros`.**
      Lifts the `compile_error!` mutual-exclusion check on
      the four `rmw-*` features (post-104.A those features
      are inert aliases). Default off. Audit
      `nros/build.rs` for any other assumptions of single-
      backend.
      **Files:**
      `packages/core/nros/Cargo.toml`,
      `packages/core/nros/build.rs`.

- [ ] **104.C.6 — Shared executor wake.**
      Replace per-session wakers with a shared
      `Executor::wake_flag: AtomicBool` (or platform
      equivalent — `eventfd` on POSIX, semaphore on
      FreeRTOS). Each Session's `notify_fn` sets the flag;
      `spin_once` waits on the flag (zero-cost when idle,
      wakes on any backend's event).
      **Files:**
      `packages/core/nros-node/src/executor/spin.rs`,
      backend-side waker hooks.

- [ ] **104.C.7 — Drop static `VTABLE`.**
      Once 104.C.1 lands and all dispatch threads through
      `session->vtable`, delete the singleton in
      `packages/core/nros-rmw-cffi/src/lib.rs`. The
      `nros_rmw_cffi_register` shim becomes a thin wrapper
      around `_register_named`. Extend the typed-struct
      roundtrip test (Phase 102.5) to drive two
      simultaneous sessions with two stub vtables.
      **Files:**
      `packages/core/nros-rmw-cffi/src/lib.rs`,
      `packages/core/nros-rmw-cffi/tests/typed_struct.rs`.

##### C.3.3 — Gaps surfaced by the bridge example (2026-05-14)

Building the `zenoh-to-dds` bridge (104.C.10) exposed concrete
follow-up items that finish the rclcpp-aligned story:

- [ ] **104.C.3.3.a — Typed `_on(node_id, ...)` register
      variants.** Today only
      `register_subscription_buffered_raw_on` is Node-aware
      (Phase 104.C.3.2). Mirror the rest using the same
      template:
      - `register_subscription_buffered_on<M, F>` (typed)
      - `register_subscription_on<M, F>` (typed convenience)
      - `register_subscription_sized_on<M, F, RX_BUF>`
      - `register_service_on<S, F>` / `register_service_raw_on`
      - `register_service_client_on` / `_raw_on`
      - `register_action_server_on` / `_raw_on`
      - `register_action_client_on` / `_raw_on`
      - `register_timer_on` (no session needed but Node-bound
        for telemetry + per-Node SchedContext inheritance).
      ~15 methods; each is a 6-line refactor over the existing
      method via `session_at_mut(node.session_idx)`.
      **Files:** `packages/core/nros-node/src/executor/spin.rs`.

- [ ] **104.C.3.3.b — `ExecutorConfig::default()`.** Bridge
      example currently uses `from_env()` because no `Default`
      impl exists. rclcpp users expect
      `ExecutorConfig::default()`. Trivial.
      **Files:** `packages/core/nros-node/src/executor/types.rs`.

- [ ] **104.C.3.3.c — `Executor::spin()` no-arg sugar.**
      Existing `spin(Duration)` is `-> !`. Add `spin()` that
      defaults to a sensible 10-100 ms tick. Match
      `rclcpp::spin(node)`.
      **Files:** `packages/core/nros-node/src/executor/spin.rs`.

- [ ] **104.C.3.3.d — Flatten `with_node` double-`?`.**
      Today's `with_node(node_id, |n| n.create_...()?)??` is
      awkward. Add `with_node_try(node_id, |n| Result<R, E>)`
      that flattens both error layers when the closure already
      returns `Result<_, NodeError>`. Or `with_node` could
      always require the closure to return `Result` and use
      `Result::flatten`.
      **Files:** `packages/core/nros-node/src/executor/spin.rs`.

- [ ] **104.C.3.3.e — Backend-ctor ordering doc.** Multiple
      `.init_array` ctors fire at lib load; first wins for
      `default_vtable`. Bridges should use `open_with_rmw` to
      avoid non-determinism. Document the trap + recommend
      `open_with_rmw` for any binary linking ≥ 2 backends.
      **Files:** `book/src/user-guide/cross-backend-bridges.md`
      (created in 104.D.6).

- [ ] **104.C.3.3.f — Bridge example `.gitignore` +
      workspace exclusion polish.** The
      `examples/native/rust/bridge/zenoh-to-dds/` directory
      needs its own `.gitignore` so `target/` + `Cargo.lock`
      don't get committed. The repo-root `.gitignore` doesn't
      catch nested example targets; per-example file is the
      established pattern.
      **Files:**
      `examples/native/rust/bridge/zenoh-to-dds/.gitignore`.

##### C / C++ wrapper items (Phase 122 discipline)

- [ ] **104.C.8 — C-side `nros_node_options_t` + thin-
      wrapper `nros_node_init_ex`.** Replaces today's
      separate `nros_node_t` storage with an opaque
      `state + _opaque` shape that calls into Rust's
      `node_builder`. Existing `nros_node_init` becomes a
      thin shim that constructs default options.
      **Files:**
      `packages/core/nros-c/src/node.rs`,
      `packages/core/nros-c/include/nros/node.h`.

- [ ] **104.C.9 — C++ `Executor::node_builder` mirror.**
      `nros::Executor::node_builder(name)` returns a C++
      `NodeBuilder` that delegates each chained method to a
      corresponding C entry point. `nros::Node` wraps
      `nros_node_t` storage opaquely (Phase 122 pattern).
      **Files:**
      `packages/core/nros-cpp/include/nros/executor.hpp`,
      `packages/core/nros-cpp/include/nros/node.hpp`,
      `packages/core/nros-cpp/src/node.rs` (if any FFI
      glue needed; otherwise pure header).

- [x] **104.C.10 — Rust example refactor: bridge.**
      `examples/native/rust/bridge/uorb-to-zenoh/`.
      Subscribes to PX4 SITL uORB topics via
      `nros-rmw-uorb` (out of scope for this phase — stub
      until 104.D); republishes onto Zenoh keys via
      `nros-rmw-zenoh`. Demonstrates the multi-Node + per-
      Node SchedContext pattern. Topic-name translation
      table as a `phf` perfect-hash.
      **Files:**
      `examples/native/rust/bridge/uorb-to-zenoh/`.

#### Thread D — Validation

- [ ] **104.D.1 — C bridge example.**
      `examples/native/c/bridge/xrce-to-dds/`. Same shape
      as 104.C.10, C audience. Demonstrates
      `nros_node_init_ex` + `nano_ros_link_rmw(target xrce
      dds)` two-backend CMake link.
      **Files:**
      `examples/native/c/bridge/xrce-to-dds/`.

- [ ] **104.D.2 — C++ bridge example.**
      `examples/native/cpp/bridge/zenoh-to-cyclonedds/`.
      Demonstrates the C++ builder + lambda subscription.
      **Files:**
      `examples/native/cpp/bridge/zenoh-to-cyclonedds/`.

- [ ] **104.D.3 — Bridge E2E test (uORB→Zenoh).**
      `packages/testing/nros-tests/tests/bridge_uorb_to_zenoh.rs`.
      Boots PX4 SITL via Phase 98's `Px4Sitl::boot_in()`
      fixture, runs the bridge example, runs a host-side
      rclcpp listener via the existing ROS 2 interop
      fixture, asserts ≥ 80 % message delivery on at
      least one topic in a 10 s window.
      **Files:**
      `packages/testing/nros-tests/tests/bridge_uorb_to_zenoh.rs`,
      `.config/nextest.toml`.

- [ ] **104.D.4 — Cross-RMW E2E test (XRCE↔DDS).**
      Verifies the C bridge example end-to-end against a
      Cyclone DDS listener.
      **Files:**
      `packages/testing/nros-tests/tests/bridge_xrce_to_dds.rs`.

- [ ] **104.D.5 — Decoupling CI guard.** Already shipped
      as `just check-decoupling` (Phase 104.A.4). Once
      Threads B + C land, add to the top-level `just
      check` aggregate so CI enforces.
      **Files:** `justfile`.

- [ ] **104.D.6 — Book chapter.**
      `book/src/user-guide/cross-backend-bridges.md`. Covers
      the rclcpp-aligned Executor + Node model, the
      `multi-backend` Cargo feature, the registration model
      per audience, the memory-budget table, the per-RT-
      class examples, and walkthroughs of each bridge
      example. Cross-link from
      `book/src/concepts/ros2-comparison.md` ("backend
      selection at compile time" section) and from
      `examples/README.md`.
      **Files:**
      `book/src/user-guide/cross-backend-bridges.md`,
      `book/src/SUMMARY.md`,
      `book/src/concepts/ros2-comparison.md`.

#### Thread E — Real-time integration

- [ ] **104.E.1 — Per-backend WCET + memory documentation.**
      Each RMW backend documents its `poll_wcet_us`
      (worst-case poll-loop budget) + buffer-pool size in
      `book/src/internals/rmw-backends.md`. Bridge users
      compose: `bridge_wcet = Σ poll_i + Σ dispatch_j`.
      **Files:** `book/src/internals/rmw-backends.md`,
      per-backend `README.md`.

- [ ] **104.E.2 — PiCAS + bridge interaction test.**
      `packages/testing/nros-tests/tests/bridge_picas_priority.rs`:
      high-priority sub on backend A + low-priority pub on
      backend B; measure end-to-end priority inheritance
      under the PiCAS dispatcher. Asserts no priority
      inversion.
      **Files:**
      `packages/testing/nros-tests/tests/bridge_picas_priority.rs`.

- [ ] **104.E.3 — Cross-priority handoff pattern.**
      Add `Executor::handoff_queue<M>` convenience API
      that wires a sub callback at priority A into a
      timer-driven pub at priority B with a bounded queue
      between. Optional sugar; existing pattern using
      `Arc<Mutex<Queue>>` + manual timer remains.
      **Files:**
      `packages/core/nros-node/src/executor/handoff.rs` (new),
      `packages/core/nros-node/src/lib.rs` (export).

- [ ] **104.E.4 — ARINC TT bridge example.**
      `examples/native/rust/bridge/tt-zenoh-to-xrce/`:
      time-triggered cyclic bridge with non-overlapping
      ingress/egress windows in a 10 ms major frame.
      Demonstrates `tt_window_offset_us` +
      `tt_window_duration_us` per Node default SchedContext.
      **Files:**
      `examples/native/rust/bridge/tt-zenoh-to-xrce/`.

## Memory + code-size budget

Multi-backend cost on a companion-class target (Jetson Orin /
Raspberry Pi):

| Component | Flash | Heap |
|-----------|-------|------|
| zenoh-pico C client | ~80 KB | ~64 KB |
| uORB rmw (intra-process) | ~5 KB | ~0 |
| XRCE C client | ~60 KB | ~24 KB |
| nros runtime + executor | ~30 KB | per-arena |
| Registry overhead (N=8) | ~256 B | 0 |
| Bridge logic | trivial | trivial |
| **Total (zenoh + uORB)** | **~115 KB Flash, ~64 KB heap** | comfortable |
| **Total (zenoh + xrce)** | **~170 KB Flash, ~88 KB heap** | comfortable |

On a Cortex-M4 with 256 KB Flash + 128 KB SRAM: tight but
feasible (zenoh-pico's TLS feature stays off). On a
Cortex-M0+: not viable — code size alone breaks the budget.

Validates the opt-in design: default single-backend builds
unchanged, only binaries that explicitly opt in pay the
cost.

## Acceptance Criteria

### API decoupling (Thread A — DONE)

- [x] `cargo tree -p nros --no-default-features --features
      rmw-cffi` shows no concrete RMW or platform crates.
- [x] `cargo tree -p nros-node --no-default-features
      --features rmw-cffi` same.
- [x] `nros-platform/src/resolve.rs` has no
      `cfg(feature = "platform-posix")` block referencing
      `nros_platform_posix::net`.
- [x] `just check-decoupling` green.

### Registration (Thread B)

- [ ] `NROS_RMW_MAX_BACKENDS=4 cargo build -p nros-c` works;
      `NROS_RMW_MAX_BACKENDS=2 cargo build -p nros-c` works
      with reduced footprint.
- [ ] Two backends registered concurrently:
      `nros_rmw_cffi_registered_names(buf, cap)` returns
      `["zenoh", "xrce"]`.
- [ ] `nros_rmw_cffi_lookup("nonexistent")` returns
      `NULL` cleanly.
- [ ] `nano_ros_link_rmw(target zenoh xrce)` builds; both
      ctors run; `Executor::open` + `create_node_with_rmw`
      for both succeeds.

### API + Executor (Thread C)

- [ ] `Executor::new(cfg)` + `exec.create_node("a")` +
      `exec.create_node("b")` produce two Nodes sharing
      the same Session (single-backend builds).
- [ ] `Executor::node_builder("a").rmw("zenoh").build()` +
      `Executor::node_builder("b").rmw("xrce").build()`
      produce two Nodes with different Sessions.
- [ ] `nros-rmw-cffi` no longer holds a global `VTABLE`.
      Two simultaneous `CffiSession::open` calls with
      different stub vtables both succeed (verified by an
      extension to `tests::typed_struct_roundtrip`).
- [ ] C `nros_node_init_ex` + C++ `node_builder` produce
      bit-identical handle storage as the Rust path
      (verified by Phase 122 ABI parity tests).
- [ ] Default builds (no `multi-backend`) still fail at
      compile time when two backends' loader-equivalents
      are linked in — the mutual-exclusion check stays on
      by default.

### Validation + RT (Threads D + E)

- [ ] PX4 SITL bridge E2E test green: ≥ 80 % delivery on
      `vehicle_attitude` over 10 s.
- [ ] XRCE↔DDS bridge E2E test green.
- [ ] PiCAS priority inversion test: end-to-end sub→pub
      latency under high-pri sub matches single-backend
      single-pri baseline within 10 %.
- [ ] Book chapter renders clean (`mdbook build`).
- [ ] No regression in any single-backend test suite (full
      `just test` green).

## Notes

- **Why opt-in `multi-backend` instead of always-on?**
  Code-size: each linked backend adds 5–80 KB Flash.
  Embedded users running a single backend don't want to pay
  for runtime backend-selection plumbing they'll never use.
  Default-off keeps the smallest targets cheap.

- **Why not adopt upstream's `rmw_init_options_t` +
  `rmw_context_t` split?** Our `RmwConfig` + `Session`
  already covers the same ground in fewer steps (one
  constructor instead of three). The three-call dance is
  upstream working around C's lack of constructors; we have
  Rust + a struct-out-param C calling convention, so we
  don't need it. Multi-instance doesn't require multi-step
  init.

- **Why fold `rclcpp::Context` into `Executor`?** rclcpp
  splits because the C-language `rcl_context_t` predates
  the C++ executor. nano-ros's Executor is the only
  process-scoped object the user encounters; folding
  Context into it removes one concept without losing
  expressiveness (the per-Executor session cache fills
  Context's role).

- **Why not adopt `implementation_identifier`?** Upstream's
  cross-backend identifier check defends against plugin-
  loader-induced confusion (every entity is opaque
  `rmw_publisher_t *`, implementation-agnostic). Our typed-
  with-monomorphisation model catches the same mistakes at
  compile time on the Rust side; on the C side the Node's
  session_ref pins backend identity at creation. The
  runtime identifier would add a pointer per entity for a
  use case the type system + Node binding already cover.

- **Hot-path latency.** Bridge runs the executor's spin loop
  driving both Sessions' I/O. For a 100 Hz uORB topic going
  to a 100 Hz Zenoh peer, the bridge adds one re-publish
  hop = one CDR encode + one Zenoh `z_put`. On a Jetson-
  class CPU this is sub-millisecond per sample; uORB→Zenoh
  end-to-end latency is dominated by Zenoh scout/routing
  on the egress side, not by the bridge.

- **Phase 123 interaction.** Phase 123.A.1.x splits the
  physical archives; Thread A removed the Rust-side Cargo
  deps that still pinned the backends into `nros`. Both
  landed before Thread B starts. Sequence: 123.A.1.x →
  104.A (done) → 104.B → 104.C → 104.D → 104.E.

- **`compile_error!` mutex removal.** Phase 104.C.5 lifts
  the mutex behind `multi-backend`. Bridge nodes are the
  driver; the same lift accidentally enables future "two
  zenoh sessions on different domains" use cases, which
  is fine.

- **Cross-language API parity.** C and C++ APIs are thin
  wrappers per Phase 122 discipline. Every new method on
  Rust `Executor` / `Node` / `NodeBuilder` gets a
  one-to-one C entry point + a C++ class method that
  forwards to it. The Rust path is the source of truth;
  C/C++ ABI tests (Phase 122) catch drift.

- **Multi-node-per-session.** DDS supports many nodes per
  Participant; Zenoh's session ↔ node is 1:1 today.
  Bridges typically use one Node per Session — the simple
  case. Multi-node-per-session works automatically when
  two `create_node` / `create_node_with_rmw` calls
  resolve to the same `(rmw, locator, domain_id)` cache
  key. Backend-specific limits (Zenoh's 1:1 today) are
  enforced inside the backend.
