# Phase 236 — C++ Entry-pkg embedded board adapter + real NodeContext runtime

**Goal.** Make a C++ Entry pkg (`nano_ros_entry(LAUNCH …)` + `NROS_MAIN`)
boot a real multi-node topology on an **embedded Zephyr board** with
**live** publishers/subscriptions — closing the two gaps that today
keep the Phase 219 C++ Entry-codegen path native-only and
record-only:

- **G1 — embedded Board adapter.** `nros::board::NativeBoard` is the
  *only* `Board::run()` in `nros-cpp`
  (`packages/core/nros-cpp/include/nros/main.hpp:56`). There is no
  Zephyr/FVP board that runs the Entry-pkg register sequence into a
  live Zephyr + Cyclone runtime.
- **G2 — real NodeContext runtime.** Even `NativeBoard::run()` builds a
  *recording* `NodeContext` whose ops are no-ops
  (`main.hpp` Phase 219.B comment: "Real per-Node publishers /
  subscriptions arrive when the Native NodeContext runtime lands as a
  follow-up"). The register sequence is dispatched but instantiates
  nothing.

**Status.** Proposed (2026-06-11). Driven by Autoware Safety Island
(ASI) as the reference embedded consumer — ASI already has a *working*
imperative Zephyr + Cyclone runtime (`actuation_module`'s
`common/node` shim over `nros::Node` + direct `create_publisher` /
`create_subscription`), so this phase is **not greenfield**: it lifts
that proven runtime under the declarative Entry/NodeContext seam, with
ASI's shim as the blueprint.

**Priority.** P1 — unblocks ASI's workspace-mode migration
(ASI `phase-2-workspace-mode-migration`) and every future embedded
C++ Entry pkg. Without it the C++ Entry-codegen path (Phase 219) is
a native-only orchestration spike with no live runtime.

**Depends on.** Phase 219 (C++ Entry-pkg orchestration — landed; the
`NROS_MAIN` macro, `nano_ros_entry` cmake fn, `nros codegen entry
--lang cpp`, register-sequence dispatch, C++ identity synthesis in
`node_pkg.hpp`). Phase 215 (board-crate import — `board.cmake` +
`nano_ros_use_board()`; the embedded board this phase runs on is
imported through 215). RFC-0032 (entry-codegen pipeline — design of
record). RFC-0024 (workspace layout). RFC-0015 (execution model).

---

## Overview

The C++ Entry-pkg story has **two halves that both already exist but
are not connected on embedded**:

| Half | State | Owner |
|---|---|---|
| Declarative orchestration — launch tree → register-sequence → `NodeContext` dispatch | done, **native + recording** | Phase 219 / RFC-0032 |
| Live runtime — a `NodeContext` op set that constructs real `nros::Node` + pub/sub on a board | done **imperatively in ASI**, absent from `nros-cpp` | **this phase** |

The seam between them is `nros::NodeContextOps` (the three function
pointers `create_node` / `create_entity` / `record_callback_effect`
in `main.hpp`). Phase 219 wired the *recording* op set. This phase
supplies:

1. a **real** op set that maps each recorded entity to an `nros-cpp`
   construction call, and
2. an **embedded `Board::run()`** that owns the Zephyr + Cyclone
   `init → network-wait → register_fn(context) → spin → shutdown`
   lifecycle (the same ritual ASI's `main.cpp` runs by hand today).

ASI's `common/node` shim is the executable specification for both:
its `create_publisher`/`create_subscription` wrappers ARE the real
`NodeContext` ops; its `main.cpp` IS the embedded `Board::run()`.

## Architecture

- **Board trait surface.** Add an embedded board adapter sibling to
  `NativeBoard` exposing the same `static int32_t run(Lambda&&
  register_fn)` shape, but driving the Zephyr lifecycle. Open: whether
  this is a single `ZephyrBoard` or per-board (`FvpAemv8rBoard`) — see
  Open questions.
- **NodeContext runtime ops.** Replace the no-op `NodeContextOps` with
  a real set that: `create_node` → `nros::create_node`; `create_entity`
  (pub/sub/service/client/timer) → the matching `nros-cpp` create call,
  storing handles in executor-owned storage; `record_callback_effect`
  → wire the subscription callback into the executor poll loop. Mirrors
  ASI `node_nros.hpp` `SubscriptionHandler<T>` polling.
- **Board import.** The embedded board is selected through Phase 215
  `nano_ros_use_board(<name>)` in the Entry pkg's `CMakeLists.txt`;
  `board.cmake`'s `NROS_BOARD_DEFAULT_RMW` / runner feed the Entry
  codegen + `west fvp run`.
- **Identity.** Node identity continues to come from the launch tree +
  `system.toml` resolved at codegen (Phase 219 / RFC-0024); this phase
  only changes what the *resolved* register call constructs, not how
  identity is computed.

## Work Items

### 236.A — Real `NodeContext` runtime ops (host/native first)

- [x] **236.A.1** Replace `NativeBoard`'s recording `NodeContextOps`
      with a real op set that constructs `nros::Node` + entities via
      `nros-cpp`. Land native-first so it is testable without an
      embedded board. *(Done — `detail::NativeNodeRuntime` in
      `main.hpp`: `create_node` → `nros::create_node`; `create_entity`
      → raw `nros_cpp_{publisher,subscription}_create` keyed on the
      descriptor's `type_name`/`type_hash` — the op boundary is
      type-erased, so the runtime can't use the typed
      `create_publisher<M>` templates and goes through the raw FFI via a
      new internal `Node::ffi_handle()` accessor.)*
- [x] **236.A.2** Entity storage — executor-owned handle storage so
      created pubs/subs outlive `register_fn` (ASI keeps
      `std::shared_ptr<Publisher<M>>`; pick the `no_std`-friendly
      equivalent). *(Done — fixed-capacity arena
      (`NROS_ENTRY_MAX_{NODES,ENTITIES}`) held in a process-scope
      template-static member (`NativeBoard::RuntimeHolder`), no heap/STL;
      mirrors `Node::GlobalStorageHolder`'s COMDAT-folded `.bss` trick.)*
- [x] **236.A.3** Subscription callback → poll-loop wiring
      (`record_callback_effect`), mirroring ASI `SubscriptionHandler<T>`.
      *(Done — `NativeNodeRuntime::spin()` drains every `Reads`
      subscription each tick; a timer-driven `Publishes` effect fires its
      publisher on the timer period. Since the declarative register fn
      carries no callback **body** (RFC-0032 §8a "Open: callback bodies"),
      the v1 runtime synthesizes a monotonic `std_msgs/Int32` counter for
      a timer-`Publishes` binding — matching the canonical Talker's
      "fires `on_tick`, which publishes a counter" intent.)*
- [x] **236.A.4** Native E2E: a 2-node C++ Entry pkg fixture publishes
      and receives over loopback (external-observer style per RFC-0032 §8).
      *(Done — `packages/testing/nros-tests/tests/phase235_a_cpp_entry_runtime.rs`
      builds the in-tree `multi-node-workspace-cpp` Entry pkg (talker +
      listener nodes), boots `robot_entry` for a bounded
      `NROS_ENTRY_SPIN_MS` window, and asserts a stock native Rust
      `listener` subscribing to `/chatter` over the same zenohd router
      observes the talker's live samples. Verified 2026-06-11:
      `Received: 0` → `test result: ok`.)*

**Files.** `packages/core/nros-cpp/include/nros/main.hpp`,
`packages/core/nros-cpp/include/nros/node.hpp`, a fixture under
`packages/testing/nros-tests/`.

**Status.** 236.A landed 2026-06-11. The Native NodeContext runtime is
live (no more recording no-op). Verifications run: standalone
`g++ -std=c++14 -fsyntax-only` of `<nros/main.hpp>`;
`cpp_multi_node_entry` (compile + link of the real template, 90 s);
`phase235_a_cpp_entry_runtime` (live pub/sub over zenohd, 63 s);
`cargo test -p nros-cpp` (8 passed). **Known v1 limitation:** a
timer-`Publishes` effect synthesizes a `std_msgs/Int32` counter because
the declarative register fn carries no callback *body* — non-Int32
publishers are created live but not auto-driven until the
callback-body-binding work (see RFC-0032 §8a open item). Services /
clients / actions are recorded (no hard error) but not yet constructed
by the native runtime.

### 236.B — Embedded (Zephyr) Board adapter

- [x] **236.B.1** Add the embedded `Board::run()` adapter: Zephyr +
      Cyclone `init → network-wait → register_fn → spin → shutdown`.
      Blueprint: ASI `actuation_module/src/main.cpp` +
      `include/common/node/node_nros.hpp`. *(Done —
      `nros::board::ZephyrBoard` in `main.hpp`, sibling to `NativeBoard`.
      **Board granularity (RFC-0032 §8a open item): ONE metadata-driven
      `ZephyrBoard`, not per-board `FvpAemv8rBoard` types.** Everything
      board-specific — Zephyr `BOARD` id, DTS overlay, default RMW, `west`
      runner — is already supplied by the Phase 215
      `nano_ros_use_board(<name>)` cmake import + Kconfig at build time, so
      the C++ adapter has nothing board-specific left to specialize; every
      Phase 215 Zephyr board compiles with `__ZEPHYR__` and shares the one
      adapter. **236.A runtime REUSED, not duplicated:** the 236.A
      `NativeNodeRuntime` was renamed `detail::EntryNodeRuntime`
      (lifecycle-agnostic; a `NativeNodeRuntime` alias is kept) and the
      ops + arena were factored into `detail::entry_node_context_ops()` +
      `detail::entry_register()` + `detail::EntryRuntimeHolder`. Both
      boards install the SAME ops + share the SAME
      `EntryNodeRuntime::spin()` body. The ONLY platform-divergent line is
      a per-tick `detail::entry_tick_yield()` — no-op on native, `k_yield()`
      on Zephyr. Network-wait is a weak `nros_board_network_wait()` hook
      (default no-op — Zephyr auto-brings-up networking; ASI's
      `configure_network()` can provide a strong override).)*
- [x] **236.B.2** Wire it to the Phase 215 board import so the Entry
      codegen / `NROS_MAIN(<Board>, …)` resolves the embedded board
      from `board.cmake` (`NROS_BOARD_RUNNER`, default RMW). *(Done —
      `emit_cpp::board_cpp_path` maps `"zephyr"` / `"fvp-aemv8r-smp"` /
      `"armfvp"` (and any `::nros::board::…` path) → `ZephyrBoard::run`
      (unit-tested). `cmake/NanoRosEntry.cmake` derives the `"zephyr"`
      codegen board key from the cached `NROS_BOARD_RUNNER` (set by
      `nano_ros_use_board`) when DEPLOY is non-`native` and no explicit
      `BOARD` was passed, and relaxes the pre-236 native-only DEPLOY gate
      to allow a non-`native` deploy iff a Board resolves. Default RMW
      continues to flow from `board.cmake` → `nano_ros_use_board` →
      `NANO_ROS_RMW` (unchanged).)*
- [x] **236.B.3** Domain-id + locator come from the board / Entry
      metadata (compile-time on embedded per the CLAUDE.md domain-id
      rule), not a runtime env. *(Done — `ZephyrBoard::run` resolves a
      compile-time `NROS_ENTRY_DOMAIN_ID`: Cyclone keys off
      `CONFIG_NROS_CYCLONE_DOMAIN_ID` (matches ASI), else the generic
      `CONFIG_NROS_DOMAIN_ID`, else 0; overridable by defining
      `NROS_ENTRY_DOMAIN_ID` before include. `nros::init("", domain)` with
      an empty locator (backend discovery default, as the in-tree FVP
      Cyclone example uses) — NO runtime `ROS_DOMAIN_ID`/`getenv` on the
      embedded path. `NativeBoard` keeps the host runtime-env exception.)*

**Files.** `packages/core/nros-cpp/include/nros/main.hpp`,
`packages/cli/nros-cli-core/src/codegen/entry/emit_cpp.rs`,
`cmake/NanoRosEntry.cmake`.

**Status.** 236.B landed in the worktree (2026-06-11). **Verification
reality:** the Zephyr SDK (`ZEPHYR_BASE` unset, `third-party/zephyr/`
absent) and ARM FVP (`ARM_FVP_DIR` unset) are NOT provisioned in this
worktree, so a full FVP build/boot of `ZephyrBoard` could not run here —
deferred to a Zephyr-SDK-equipped host (and to 236.C's ASI validation).
What WAS verified:
- `g++ -std=c++14 -fsyntax-only` of `<nros/main.hpp>` on BOTH paths —
  native (no `__ZEPHYR__`) and the embedded branch (`__ZEPHYR__` + a
  stubbed `<zephyr/kernel.h>` for `k_yield`, all three domain-id branches);
- the generated TU for `--board zephyr` emits
  `::nros::board::ZephyrBoard::run(...)` and syntax-checks under
  `__ZEPHYR__`; the no-`--board` TU still emits `NativeBoard::run(...)`;
- `cargo test -p nros-cpp` (8 passed — no NativeBoard/236.A regression);
- `cpp_multi_node_entry` (full cmake compile+link of the real native
  template, 83 s);
- `nros-cli-core` `emit_cpp` unit tests (9 passed incl. 3 new board-key
  cases).

### 236.C — ASI reference-consumer validation

- [ ] **236.C.1** ASI `actuation_module` builds as a C++ Entry pkg
      against the 236.B board adapter (replaces the hand-written
      `main.cpp` boot with `NROS_MAIN` + `nano_ros_use_board`).
- [ ] **236.C.2** FVP smoke: the `controller` node publishes
      `/control/trajectory_follower/control_cmd` via the generated
      Entry path, observed by stock `ros2 topic echo` — parity with
      ASI phase-1 acceptance gate 1.9.

**Files.** (external) `autoware-safety-island/actuation_module/`.

### 236.D — Real callback-body binding + monolithic-app composition (gates 236.C)

Discovered 2026-06-11 by the ASI reference consumer (ASI phase-2.C): the
236.A/B runtime constructs entities and, for a timer-`Publishes` binding,
*synthesizes* a `std_msgs/Int32` counter — it runs **no real user
callback bodies**. The talker/listener demo passed on the synthesized
counter; ASI's vendored MPC/PID `Controller` (real C++ sub/timer
callbacks publishing `AckermannControlCommand`) cannot be driven. So the
generated register sequence boots a node that creates entities but runs
no control logic. This is RFC-0032 §8a's "callback bodies" open item,
now a hard blocker.

**Design decided — [RFC-0043](../design/0043-entry-real-callback-binding.md).**
Under the no-callback-naming + thin-Rust-wrapper (RFC-0019) principles: route the
Entry path to the **Rust executor** (the same one the native examples use), not
the type-erased string-descriptor register. The component becomes a **stateful
object** binding real callbacks **by identity** (typed *or* raw zero-copy) — no
`declare_callback("name")`. The synthesizing `EntryNodeRuntime` + the
`DeclaredNode`/`record_callback_effect` string layer are **retired**; the Phase
238 NuttX C/C++ E2E migrates onto the executor and runs real logic for free.

**Spike (2026-06-12) — risk retired.** The one unproven edge was whether the
executor's callback dispatch runs under the embedded board lifecycle via the C++
FFI (native proven; embedded always ran the interpreter). A throwaway imperative
NuttX entry (`init → create_node → create_timer(cb) +
nros_cpp_subscription_register(raw zero-copy cb) → spin_once loop`, ~10 lines of
C++ glue, direct `nros-nuttx-ffi` cargo build) booted in QEMU vs the talker:
`tick 0..88` (executor timer callback) + `Received 0..38` (executor raw zero-copy
sub callback, correct `Int32`). Executor real-callback dispatch works on NuttX;
the C++ side is a thin wrapper.

- [ ] **236.D.1** Component-object shape + `NROS_NODE(Talker)` macro
      (factory + `sizeof` + per-pkg register symbol). Ctor-binds-`Node&` vs
      `configure(Node&)` — RFC-0043 open Q1. Binds real callbacks via the typed
      (`create_subscription(sub_, topic, cb)`) + raw zero-copy
      (`create_subscription_raw(sub_, topic, raw_cb)`) APIs. C parity via the C
      callback FFI (`fn ptr + void* ctx`).
- [ ] **236.D.2** Typed codegen Entry — per launch node, `#include` the component
      header, construct into an entry-owned arena slot (`sizeof` known), run the
      executor (`spin_once`). Replaces the `NodeContextOps` recording dispatch +
      the synthesizing spin loop. Instance-arena sizing — RFC-0043 open Q2.
- [ ] **236.D.3** Monolithic-app composition — `nano_ros_entry` today
      `add_executable`s + links per-Node `<pkg>_<exec>_component` static
      libs. A Zephyr consumer that links everything into the
      `find_package(Zephyr)`-owned `app` target (ASI) needs: (a) the
      `nano_ros_entry(NAME app …)` append-to-existing-target path proven
      in a real Zephyr build, and (b) the link-libs sidecar to tolerate a
      Node pkg compiled as `APP_SOURCES` rather than its own
      `nano_ros_node_register` `project()`.
- [ ] **236.D.4** Retire the interpreter — delete `EntryNodeRuntime` +
      `detail::entry_*` synthesis (`main.hpp`) and the `DeclaredNode` /
      `record_callback_effect` string seam; migrate the Phase 238 NuttX C/C++
      examples (pub/sub + service + action) onto the executor with real bodies.
- [ ] **236.D.5** A non-trivial (non-counter) C++ + C Entry E2E — a node with
      a real subscription→publish callback, proving 236.D.1/.2 before ASI
      consumes it.

**Files.** `packages/core/nros-cpp/include/nros/`, `cmake/NanoRosEntry.cmake`,
`packages/cli/nros-cli-core/src/codegen/entry/`, a fixture under
`packages/testing/nros-tests/`.

> **236.C is blocked on 236.D.** ASI cannot delete its imperative
> `main.cpp` until the declarative path runs the real controller.

## Acceptance

- [ ] A C++ Entry pkg with ≥2 nodes boots on native with **live**
      pub/sub through the generated `NROS_MAIN` path (no recording
      no-op).
- [ ] The same Entry-pkg shape boots on FVP AEMv8-R (Zephyr + Cyclone)
      via the embedded board adapter + Phase 215 `nano_ros_use_board`.
- [ ] ASI `actuation_module` runs its `controller` node through the
      generated Entry path on FVP, output observed by stock ROS 2.

## Notes / cross-refs

- This phase is the missing runtime half of RFC-0032; see RFC-0032
  §8 "Embedded board adapter + NodeContext runtime binding".
- The Rust side already has live embedded Entry boot (the
  `OwnedSpin`-RTOS boards + `nros::main!`); this phase brings the C++
  Entry path to runtime parity on embedded.
- Consumer plan: `autoware-safety-island/docs/roadmap/phase-2-workspace-mode-migration.md`.
