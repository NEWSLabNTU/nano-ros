# Phase 242 — rclcpp-faithful component model (RFC-0044)

**Goal.** Implement [RFC-0044](../design/0044-rclcpp-faithful-component-model.md):
make the C++ component model rclcpp-faithful — the user node **IS-A** node, its
**constructor** receives the executor-bound node handle and creates
publishers/subscriptions/timers + declares parameters (with **typed member
callbacks**, no names, no raw bytes) in the ctor body. Closes the three gaps that
blocked ASI's vendored Autoware `Controller` from migrating onto RFC-0043's
`configure(Node&)` shape: (1) typed member-callback subscriptions, (2) parameter
sequences (`std::vector<double>`), (3) ctor-wired IS-A-node lifetime on the entry
executor.

**Status.** Proposed (2026-06-13). Driven by ASI phase-2.C — the reference
consumer whose real rclcpp-shaped node surfaced the gap. Amends RFC-0043 Q1.

**Priority.** P1 — the only path to ASI 2.C (a real Autoware node through the
generated Entry on FVP) and to the stated "follow rclcpp composable-node
convention" goal. RFC-0043's `configure(Node&)` cannot host a real node.

**Depends on.** RFC-0043 + Phase 240 (the executor-routed real-callback runtime,
the typed entry codegen, the NuttX + Zephyr carriers — all landed); the C++
`nros::Node` (`packages/core/nros-cpp/include/nros/node.hpp`) + `ParameterServer`
(`parameter.hpp`); the member-fn-pointer no-alloc trampoline pattern
(`component.hpp` `bind_*`).

---

## Overview

RFC-0043 routed callbacks to the real executor by identity (no naming) — correct,
and kept. But its component *shape* (default-construct + two-phase
`configure(Node&)`) is not rclcpp-faithful and cannot host a node that IS-A node
with ctor-created entities + declared params. This phase adds the rclcpp-faithful
shape alongside (RFC-0043's `bind_*`/`configure` stays as the lower-level option).

No-exceptions reconciliation: ctor entity/param creation **aborts on fatal** (boot
is all-or-nothing on firmware), the same outcome a thrown rclcpp ctor exception
has — see RFC-0044 §Motivation.

## Work Items

### 242.1 — `nros::ComponentNode` base (the IS-A-node shape)

- [ ] **242.1.1** Add the node base (working name `nros::ComponentNode`) that
      wraps/owns the executor-bound `nros::Node`, constructed from a node handle +
      identity: `ComponentNode(NodeHandle, const char* name)`. Member `create_*`
      forward to the owned node. Abort-on-fatal creation (RFC-0044 Q2).
- [ ] **242.1.2** `NROS_COMPONENT(Class)` — factory + `sizeof` + the class/header
      metadata the typed entry needs (replaces/augments `NROS_NODE_REGISTER`).
- [ ] **242.1.3** Decide base name + wrap-vs-derive (RFC-0044 Q1) — confirm value
      semantics + the FFI handle stay clean.

**Files.** `packages/core/nros-cpp/include/nros/component_node.hpp` (new),
`component.hpp`, `node.hpp`.

### 242.2 — Typed member-callback subscriptions

- [ ] **242.2.1** `create_subscription<M>(topic, &C::on_msg [, qos])` member form
      — a member-fn-pointer-as-template-param trampoline (the RFC-0043 no-alloc
      pattern) that `M::ffi_deserialize`s the wire bytes then dispatches to the
      typed member `void C::on_msg(const M&)`. Register the DDS-mangled
      `M::TYPE_NAME` (240.1 finding, RFC-0044 Q4).
- [ ] **242.2.2** Member timer + (later) service-server/action-server member
      callbacks, same trampoline shape.

**Files.** `node.hpp`, `component.hpp`, `subscription.hpp`.

### 242.3 — Parameter sequences

- [ ] **242.3.1** `ParameterServer` fixed-capacity sequence parameters —
      `declare_parameter<Seq<double, N>>` / a `vector`-shaped accessor, bounded
      `no_std` storage (RFC-0044 Q3 — compile-time `N`). Unblocks the MPC
      `std::vector<double>` weight matrices.
- [ ] **242.3.2** `get/set_parameter` sequence accessors + the C/Rust FFI mirror
      if the param surface crosses the boundary.

**Files.** `parameter.hpp`, `parameter.cpp` / the param FFI.

### 242.4 — Codegen entry + carrier: construct-with-handle

- [ ] **242.4.1** `emit_cpp::emit_typed` — shift the generated entry from
      `default-construct + configure(node)` to placement-new the component into
      the arena slot with the entry's node handle (`Storage<C> __c;
      __c.emplace(handle)`), via the `NROS_COMPONENT` factory.
- [ ] **242.4.2** Carrier templates `{nuttx,zephyr}_entry_main_typed.cpp.in` —
      update the construct line; `run_components` + the board lifecycle unchanged.

**Files.** `packages/cli/nros-cli-core/src/codegen/entry/emit_cpp.rs`,
`cmake/templates/{nuttx,zephyr}_entry_main_typed.cpp.in`.

### 242.5 — ASI migration (the consumer proof)

- [ ] **242.5.1** ASI `controller_pkg::Controller` derives `nros::ComponentNode`,
      drops the legacy `common/node` shim base; its ctor (create 5 subs/3 pubs,
      declare vector params, MPC/PID) works ~unchanged — no control-math rewrite.
      Delete the per-node pthread spin (the entry drives it).
- [ ] **242.5.2** FVP smoke: the `controller` node runs through the generated
      Zephyr Entry path, `/control/.../control_cmd` observed by stock ROS 2
      (= phase-236 236.C / phase-240 240.7). Gated on a Zephyr-SDK + FVP host.

**Files.** (external) `autoware-safety-island/actuation_module/`.

### 242.6 — Rust parity (deferred)

- [ ] **242.6.1** Decide whether Rust's `ExecutableNode` adopts an IS-A-node
      ctor shape for parity (RFC-0044 Q5) or stays name-dispatched. Not gating ASI.

## Acceptance

- [ ] A C++ node written in rclcpp-faithful style (IS-A node, ctor-creates
      entities + declares vector params, typed member callbacks) boots through the
      generated Entry with **live** pub/sub + real logic — native + Zephyr.
- [ ] ASI's vendored `Controller` migrates onto `nros::ComponentNode` with **no
      control-math rewrite** and runs on FVP, output observed by stock ROS 2.
- [ ] RFC-0043's `configure(Node&)` + `bind_*` still work (lower-level path
      retained).

## Notes / cross-refs

- Design + reconciliation + open Qs: RFC-0044 (amends RFC-0043 Q1).
- This is the resolution of the ASI phase-2.C entanglement blocker (the vendored
  Autoware `Controller` is a real rclcpp node RFC-0043's shape couldn't host).
- Consumer plan: `autoware-safety-island/docs/roadmap/phase-2-workspace-mode-migration.md`.
