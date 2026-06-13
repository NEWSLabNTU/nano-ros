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

**Status.** Accepted (2026-06-13 — [RFC-0044](../design/0044-rclcpp-faithful-component-model.md)
adoption decision; all 5 open Qs resolved). 242.3 (parameter sequences) DONE;
242.1/242.2/242.4 pending; 242.5 (ASI) gated on a Zephyr-SDK + FVP host. Driven
by ASI phase-2.C — the reference consumer whose real rclcpp-shaped node surfaced
the gap. Amends RFC-0043 Q1.

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

No-exceptions reconciliation (RFC-0044 Q2 resolved): ctor entity/param creation
failure sets a **`bool ok()`** flag the entry checks **post-construct**, then the
entry halts naming the failed node (boot is all-or-nothing on firmware, the same
outcome a thrown rclcpp ctor exception has, but with multi-node boot
diagnostics — *which* node failed). No `Result` threading in the ctor.

## Work Items

### 242.1 — `nros::ComponentNode` base (the IS-A-node shape)

- [x] **242.1.1** Add the node base `nros::ComponentNode` (RFC-0044 Q1: **wraps/
      owns** the `nros::Node` — keeps value-semantics + the clean FFI handle),
      constructed from the executor-bound handle + identity:
      `ComponentNode(NodeHandle, const char* name)`. Member `create_*` forward to
      the owned node. **Q2:** a creation failure in the ctor sets an internal flag
      surfaced via `bool ok() const`; the ctor does NOT abort — the entry checks
      `ok()` post-construct + halts naming the node (no `Result` in the ctor).
- [x] **242.1.2** `NROS_COMPONENT(Class)` — factory + the **`shape:"rclcpp"`**
      component metadata marker (RFC-0044 §impl) + the existing class/header
      (recorded by `nano_ros_node_register`). **No `sizeof` metadata** — the entry
      `#include`s the header, so `sizeof(Class)` / `Storage<Class>` is a
      compile-time fact (242.4), not a codegen input. Replaces `NROS_NODE_REGISTER`.

**Files.** `packages/core/nros-cpp/include/nros/component_node.hpp` (new),
`component.hpp`, `node.hpp`.

> **Status (2026-06-13) — aligned to spec by 242.4.** The `ComponentNode`
> base + `NROS_COMPONENT` + typed member-callback subs landed, and the two
> spec points aligned in 242.4: the ctor sets a **`bool ok()`-flag** (no
> abort; `error_what()`/`error_code()`), and `NROS_COMPONENT` emits the
> **`shape:"rclcpp"`** marker with **no `sizeof`/align** symbols. Verified:
> `cargo test -p nros-cpp` 8/8; emit-codegen unit tests (56) +
> `entry_typed_plan` (1) pass; `examples/native/cpp/component-node-poc`
> builds + links against the reworked base; the cmake seam
> (`nano_ros_node_register SHAPE rclcpp` → JSON `shape` + carrier
> `NROS_ENTRY_SHAPE_RCLCPP`) parses clean (project mode).

### 242.2 — Typed member-callback subscriptions

- [x] **242.2.1** `create_subscription<M>(topic, &C::on_msg [, qos])` member form
      — a member-fn-pointer-as-template-param trampoline (the RFC-0043 no-alloc
      pattern) that `M::ffi_deserialize`s the wire bytes then dispatches to the
      typed member `void C::on_msg(const M&)`. Register the DDS-mangled
      `M::TYPE_NAME` — **RFC-0044 Q4 CONFIRMED by 240.6**: the typed
      `Publisher<M>` already registers the mangled form (`std_msgs::msg::dds_::Int32_`),
      runtime-proven by the NuttX talker↔listener pairing, so the member sub on
      `M::TYPE_NAME` matches with no new divergence.
- [x] **242.2.2** Member timer + (later) service-server/action-server member
      callbacks, same trampoline shape.

**Files.** `node.hpp`, `component.hpp`, `subscription.hpp`.

### 242.3 — Parameter sequences

- [x] **242.3.1** `ParameterServer` fixed-capacity sequence parameters —
      `nros::Seq<T, N>` value type (`T` = double/int64/bool) + `declare_parameter`
      overload, bounded `no_std` storage (RFC-0044 Q3 — compile-time `N`).
      Per-parameter capacity is the `Seq<T,N>` compile-time `N`; the server owns
      the element bytes in an inline `SeqPoolBytes` pool + `SeqSlots` record
      table (no heap, no shared dynamic arena). Unblocks the MPC
      `std::vector<double>` weight matrices.
- [x] **242.3.2** `get/set_parameter` sequence accessors — `Seq<T,N>` (bounds-
      checked: too-small `out` or over-`N` set rejected, never UB), a zero-copy
      borrow accessor, and `std::vector<T>` overloads under `NROS_CPP_STD`.
      Sequences are **C++-storage-local — they do not cross the FFI** (the C
      array FFI stores a *borrowed* caller pointer + len, which dangles under
      server-owns-the-value semantics), so no C/Rust FFI mirror was added.

**Files.** `parameter.hpp` (sequence storage + API; no `parameter.cpp` /
FFI change needed). Verified by `examples/native/cpp/parameters/src/main.cpp`
+ `cpp_parameters_roundtrip`.

### 242.4 — Codegen entry + carrier: construct-with-handle (shape-branched)

The two shapes coexist (RFC-0044 keeps `configure(Node&)` lower-level), so the
entry **branches on the `shape` metadata field** — it does NOT replace the 240.x
construct path.

- [x] **242.4.1** Metadata seam: `nano_ros_node_register` / `NROS_COMPONENT`
      record `shape:"rclcpp"|"configure"` into the `components[]` JSON;
      `codegen/entry/metadata.rs` `ComponentIndex` reads it onto `PlanNode`
      (`class`/`class_header` unchanged).
- [x] **242.4.2** `emit_cpp::emit_typed` — per node, branch on `shape`:
      - `configure` (240.x): `static C __c; … __c.configure(node);` — static
        construct *before* init, then configure (unchanged).
      - `rclcpp`: **placement-new in `__nros_entry_setup` *after* `nros::init`**
        (the ctor needs the live handle) — `static Storage<C> __c;
        __c.emplace(node_handle);` then `if (!__c->ok()) return …;` (Q2).
- [x] **242.4.3** Carrier templates `{nuttx,zephyr}_entry_main_typed.cpp.in` —
      add the rclcpp construct line (placement-new + `ok()` check) gated the same
      way; `run_components` + the board lifecycle unchanged.

**Files.** `packages/cli/nros-cli-core/src/codegen/entry/{emit_cpp.rs,metadata.rs,mod.rs}`,
`cmake/NanoRosNodeRegister.cmake` (shape field),
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

### 242.7 — Value-returning rclcpp parameter facade on the node (gates 242.5)

Filed 2026-06-13 — the **third** ASI-consumer-surfaced gap (after 240.8 carrier
+ the RFC-0044 component model). 242.1 gave the entity/callback half
(`ComponentNode`) and 242.3 the storage half (`ParameterServer` + `Seq<T,N>`),
but **nothing joins them on the node**: `ComponentNode`/`Node` expose **zero**
parameter methods, and `ParameterServer`'s API is `Result`-returning +
`Seq<T,N>` compile-time-capacity. ASI's vendored MPC/PID couple to the node
**only** through the rclcpp-shaped, **value-returning** surface
(`node.declare_parameter<T>(name, default) -> T`, `get_parameter<T> -> T`,
`has_parameter`) — **151 call sites**, 4 of them
`declare_parameter<std::vector<double>>(name, {…})` with no compile-time `N`.
RFC-0044's "ctor works ~unchanged, no control-math rewrite" promise depends on
this facade; without it the controller cannot construct the MPC/PID.

- [ ] **242.7.1** Add a value-returning, rclcpp-faithful parameter API **on
      `ComponentNode`** (backed internally by an owned `ParameterServer`):
      `template<typename T> T declare_parameter(const char*/std::string name,
      const T& default_value = T{})`, `template<typename T> T
      get_parameter(name) const`, `bool has_parameter(name) const`. No-exceptions
      reconciliation: a failed declare/get sets the component `ok()`-flag and
      returns the default (consistent with 242.4's Q2). Scalars route to the
      existing `ParameterServer` scalar store.
- [ ] **242.7.2** `std::vector<double>` (and the other `Seq` element types)
      under `NROS_CPP_STD` **without** a caller-supplied compile-time capacity —
      `declare_parameter<std::vector<double>>(name, {…})` compiles unchanged.
      Back it by a default-capacity `Seq<T, NROS_PARAM_SEQ_DEFAULT_CAP>` (the MPC
      weight matrices are small + fixed); over-capacity sets `ok()`-flag.
      **Open:** default capacity value + whether `ComponentNode` carries its own
      `ParameterServer<Cap>` sizing knobs.

**Files.** `packages/core/nros-cpp/include/nros/component_node.hpp`,
`parameter.hpp`. Consumer proof: ASI 242.5.

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
