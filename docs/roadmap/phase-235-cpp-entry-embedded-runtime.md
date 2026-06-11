# Phase 235 — C++ Entry-pkg embedded board adapter + real NodeContext runtime

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

### 235.A — Real `NodeContext` runtime ops (host/native first)

- [ ] **235.A.1** Replace `NativeBoard`'s recording `NodeContextOps`
      with a real op set that constructs `nros::Node` + entities via
      `nros-cpp`. Land native-first so it is testable without an
      embedded board.
- [ ] **235.A.2** Entity storage — executor-owned handle storage so
      created pubs/subs outlive `register_fn` (ASI keeps
      `std::shared_ptr<Publisher<M>>`; pick the `no_std`-friendly
      equivalent).
- [ ] **235.A.3** Subscription callback → poll-loop wiring
      (`record_callback_effect`), mirroring ASI `SubscriptionHandler<T>`.
- [ ] **235.A.4** Native E2E: a 2-node C++ Entry pkg fixture publishes
      and receives over loopback (external-observer style per RFC-0032 §8).

**Files.** `packages/core/nros-cpp/include/nros/main.hpp`,
`packages/core/nros-cpp/src/`, a fixture under
`packages/testing/nros-tests/`.

### 235.B — Embedded (Zephyr) Board adapter

- [ ] **235.B.1** Add the embedded `Board::run()` adapter: Zephyr +
      Cyclone `init → network-wait → register_fn → spin → shutdown`.
      Blueprint: ASI `actuation_module/src/main.cpp` +
      `include/common/node/node_nros.hpp`.
- [ ] **235.B.2** Wire it to the Phase 215 board import so the Entry
      codegen / `NROS_MAIN(<Board>, …)` resolves the embedded board
      from `board.cmake` (`NROS_BOARD_RUNNER`, default RMW).
- [ ] **235.B.3** Domain-id + locator come from the board / Entry
      metadata (compile-time on embedded per the CLAUDE.md domain-id
      rule), not a runtime env.

**Files.** `packages/core/nros-cpp/include/nros/`,
`packages/boards/nros-board-fvp-aemv8r-smp/`, `zephyr/cmake/`.

### 235.C — ASI reference-consumer validation

- [ ] **235.C.1** ASI `actuation_module` builds as a C++ Entry pkg
      against the 235.B board adapter (replaces the hand-written
      `main.cpp` boot with `NROS_MAIN` + `nano_ros_use_board`).
- [ ] **235.C.2** FVP smoke: the `controller` node publishes
      `/control/trajectory_follower/control_cmd` via the generated
      Entry path, observed by stock `ros2 topic echo` — parity with
      ASI phase-1 acceptance gate 1.9.

**Files.** (external) `autoware-safety-island/actuation_module/`.

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
