---
rfc: 0044
title: "rclcpp-faithful component model — IS-A-Node, ctor-wired, typed member callbacks"
status: Draft
since: 2026-06
last-reviewed: 2026-06-13
implements-tracked-by: [phase-242]
supersedes: []
superseded-by: null
---

# RFC-0044 — rclcpp-faithful component model

## Summary

Amends [RFC-0043](0043-entry-real-callback-binding.md)'s **Q1** (component shape).
RFC-0043 chose **default-construct + two-phase `Result configure(nros::Node&)`**
for the C++ component. That shape **cannot host a real `rclcpp`-style node** — a
node that **IS-A `Node`**, takes its node identity in the **constructor**, and
**declares parameters + creates entities (with typed member callbacks) in the
ctor**. That shape is exactly the **rclcpp composable-node convention** nano-ros
committed to following, and exactly what real consumers already ship: the
Autoware Safety Island (ASI) `Controller` is a vendored `rclcpp`-shaped node and
**could not be migrated onto `configure(Node&)`** (phase-2.C blocker, 2026-06-13).

RFC-0044 makes the component model rclcpp-faithful: **the user node derives an
`nros` node base, its constructor receives the executor-bound node handle, and it
creates publishers/subscriptions/timers + declares parameters as member calls in
the ctor** — using **typed member callbacks** (`&Self::on_msg`), not raw bytes or
string names. Three capability gaps close: (1) typed *member*-callback
subscriptions, (2) parameter *sequences* (`std::vector<double>`), (3) the
ctor-wired IS-A-Node lifetime on the entry executor.

`configure(Node&)` + the `bind_*` helpers (RFC-0043) **remain** as the
lower-level, no-base-class option; rclcpp-faithful is the **recommended
convention** and the one the codegen entry targets.

## Motivation

The convention goal (CLAUDE.md + the 2026-06 design discussion) is **rclcpp
composable nodes**: author a node in normal `rclcpp`/`rclcpp_components` style,
statically composed into the Entry. RFC-0043 delivered the *runtime* half
(real callbacks by identity, executor-routed, no naming) but chose a component
*shape* — default-construct then `configure(Node&)` — that diverges from rclcpp
in three ways that block a real node:

1. **rclcpp nodes don't default-construct.** `MyNode(const NodeOptions&) :
   Node("my_node", options) {…}`. The node identity + options arrive in the ctor;
   there is no valid "constructed but not configured" state.
2. **Entities + params are created in the ctor.** `create_subscription<M>(topic,
   qos, &MyNode::on_msg)` and `declare_parameter("w", default)` run in the ctor
   body, bound to `this`.
3. **Callbacks are typed member functions** (`void on_msg(const M&)` /
   `void on_msg(M::ConstSharedPtr)`), not raw byte callbacks or named strings.

ASI's vendored `Controller` is precisely this (it IS-A node; its ctor creates 5
subs + 3 pubs + declares `std::vector<double>` MPC weights; compute is private).
RFC-0043's model can host none of it without rewriting the vendored control math.
The talker/listener demo never exposed this — a real Autoware node did.

**Why RFC-0043 chose two-phase, and how RFC-0044 reconciles it.** RFC-0043 Q1
picked `configure` because *"a ctor can't return `Result`, and entity creation can
fail"*, and nano-ros is **no-exceptions** (`no_std`). rclcpp uses ctor + throws.
RFC-0044's reconciliation: on embedded firmware **a failed entity/param creation
at boot is unrecoverable** — there is no graceful degradation, the image cannot
run. So ctor-wiring **aborts on fatal** (panic/`nros_abort`), the same outcome a
thrown rclcpp exception has at node construction. The fallible `configure(Node&)`
path stays available for callers that want an error return; the rclcpp-faithful
base trades it for ctor ergonomics + faithfulness, which is the right default for
a firmware whose boot either fully succeeds or halts.

## Design

### 1. The node base

A user node derives an `nros` node base (working name `nros::ComponentNode`,
final name in 242.1) that **wraps/owns** the executor-bound `nros::Node`:

```cpp
class Controller : public nros::ComponentNode {
    nros::Publisher<ControlMsg> pub_cmd_;
    LatestCache<TrajectoryMsg> traj_;            // member state
    MpcLateralController mpc_;                    // takes *this as the param/node source
  public:
    explicit Controller(nros::NodeHandle h)      // executor-bound handle from the entry
        : nros::ComponentNode(h, "controller") {
        pub_cmd_ = create_publisher<ControlMsg>(topics::control_cmd);
        create_subscription<TrajectoryMsg>(topics::trajectory, &Controller::on_trajectory);
        create_timer(ctrl_period_ms_, &Controller::on_control_tick);
        auto w = declare_parameter<std::vector<double>>("mpc_weights", {…});   // gap (3)
    }
    void on_trajectory(const TrajectoryMsg& m) { traj_.set(m); }               // typed member cb
    void on_control_tick() { auto cmd = mpc_.compute(traj_.get(), …); pub_cmd_.publish(cmd); }
};
NROS_COMPONENT(Controller);   // factory + sizeof + metadata (class/header)
```

- The ctor receives the **executor-bound node handle** (not a default-constructed
  shell) — the entry constructs it *after* `nros::init`, in arena storage.
- `create_*` are **members** (the node IS its own context); they bind **typed
  member callbacks** by member-fn-pointer (the no-alloc trampoline RFC-0043's
  `bind_*` already proves, lifted to the typed path) — no string names, no raw
  bytes at the authoring surface.
- Creation failure **aborts** (boot-fatal). No `Result` threading in the ctor.

### 2. The three capability gaps

- **(1) Typed member-callback subscriptions.** Today `create_subscription<M>(out,
  topic, F)` is stateless (`void(const M&)`, no `this`); RFC-0043 added raw
  member binding (`bind_subscription_raw<C,&C::m>` over bytes). 242.2 adds the
  **typed** member form: `create_subscription<M>(topic, &C::on_msg)` →
  deserialize-then-dispatch-to-member trampoline (reuses `M::ffi_deserialize` +
  the RFC-0043 member-fn-pointer-as-template-param no-alloc trampoline).
- **(2) Parameter sequences.** `nros::ParameterServer` is scalar-only; ASI's MPC
  needs `std::vector<double>` weight matrices. 242.3 adds fixed-capacity sequence
  parameters (`declare_parameter<Seq<double, N>>` / a `vector`-shaped accessor),
  bounded `no_std` storage.
- **(3) IS-A-Node lifetime on the entry executor.** The component instance is
  owned by the entry (arena/static), constructed with the entry's executor node
  handle — **one** node + executor, no separate node, **no per-node pthread spin**
  (the vendored shim's `spin()` thread is deleted; the entry's
  `Board::run_components` `spin_once` loop drives every component's callbacks).

### 3. Codegen entry + carrier

The typed entry (`emit_cpp::emit_typed` + the NuttX/Zephyr carriers) shifts from
`default-construct + configure(node)` to **construct-with-handle**: `static
Storage<Controller> __c; __c.emplace(entry_node_handle);` (placement-new into the
arena slot). `NROS_COMPONENT` supplies the factory + `sizeof` the entry needs.
The carrier templates (`{nuttx,zephyr}_entry_main_typed.cpp.in`) change the
construct line; `run_components` + the board lifecycle are unchanged.

## Migration

- RFC-0043's `configure(Node&)` + `bind_*` **stay** (lower-level / no-base-class
  path). rclcpp-faithful `ComponentNode` is the recommended convention.
- ASI's vendored `Controller`: **drop the legacy `common/node` shim base, derive
  `nros::ComponentNode`** — its ctor (create subs/pubs, declare vector params)
  works ~unchanged; make the private compute reachable from the timer member
  (it already is, within the class). No control-math rewrite.
- The Rust side already runs real bodies via `ExecutableNode`; whether Rust adopts
  an IS-A-node ctor shape for parity is a follow-up (242.6), not gating ASI.

## Open questions

1. **Base name + shape** — `nros::ComponentNode` (wraps a `Node`) vs the user
   deriving `nros::Node` directly. Wrapping keeps `Node`'s value-semantics + the
   FFI handle clean; deriving is closer to rclcpp's `: public rclcpp::Node`.
2. **Abort vs error-flag on ctor failure** — `nros_abort` (simplest, boot-fatal)
   vs a `bool ok()` the entry checks post-construct (lets the entry log which node
   failed before halting). Lean abort for v1; revisit if multi-node boot
   diagnostics need it.
3. **Param sequence capacity** — fixed `N` per parameter (compile-time) vs a
   shared arena. ASI MPC weight vectors are small + fixed; lean compile-time `N`.
4. **Typed-vs-raw type-name form** — the 240.1 finding (typed `Publisher<M>`
   registers the DDS-mangled `M::TYPE_NAME`); the typed member sub must register
   the same mangled form. Confirm the typed path already does (it should — it
   mirrors `create_subscription<M>`).
5. **Rust parity** — does Rust's `ExecutableNode` move to an IS-A-node ctor shape
   too, or stay name-dispatched? Separate decision (242.6).

## Adoption decision (2026-06-13)

**Adopt.** The design is sound, driven by a real consumer (the vendored ASI
`Controller`, which cannot sit on `configure(Node&)` without rewriting control
math), aligns with the committed rclcpp-composable-node convention, and is
**additive** — `configure(Node&)` + the `bind_*` helpers (RFC-0043) **stay** as
the lower-level path. Critically it **reuses the RFC-0043 runtime substrate**
unchanged: the no-alloc member-fn-pointer trampoline, executor routing,
`Board::run_components` single-`spin_once` loop, and the typed-entry codegen +
NuttX/Zephyr carriers. The only new substantive work is the `ComponentNode`
base + typed member subs + sequence params (phase-242); everything else is a
small delta on the proven 240.x path.

This was reinforced by the 240.x runtime validation: the full NuttX matrix
(pub/sub, service, action; C++ + C) and the native transform node run real
callbacks through `run_components` in QEMU/host — so the substrate this RFC
builds on is proven, not theoretical.

### Open-question resolutions

1. **Base name/shape →** `nros::ComponentNode` that **wraps/owns** a `Node`
   (keeps `Node`'s value-semantics + clean FFI handle; the user writes
   `create_publisher<M>(…)` as members regardless). Revisit deriving `Node`
   directly only if a consumer needs `Node`'s full surface on `this`.
2. **Abort vs error-flag →** ctor sets a **`bool ok()`** the entry checks
   post-construct, *then* the entry aborts/halts with the offending node named.
   Cheap, and multi-node entries need "which node failed" boot diagnostics; bare
   `nros_abort` inside the ctor loses that. (Single-node carriers can still just
   halt.)
3. **Param sequence capacity →** **compile-time fixed `N`** per parameter
   (`declare_parameter<Seq<double, N>>`), bounded `no_std` storage. ASI MPC weight
   vectors are small + fixed; a shared arena is unneeded complexity for v1.
4. **Typed-vs-raw type-name form → CONFIRMED (resolved by 240.6).** The typed
   `Publisher<M>` registers the **DDS-mangled** `M::TYPE_NAME`
   (`std_msgs::msg::dds_::Int32_`), proven by the runtime-green talker↔listener
   pairing on NuttX (typed pub ↔ raw sub on the mangled string). The typed
   *member* subscription must register the same mangled `M::TYPE_NAME` — it does,
   mirroring `create_subscription<M>`. No new divergence.
5. **Rust parity →** defer (242.6). Rust already runs real bodies via
   `ExecutableNode`; the IS-A-node ctor shape is a parity nicety, not an ASI gate.

### Implementation notes (delta on the 240.x path)

- **Construct ordering changes.** `configure(Node&)` = static-construct the
  component *before* `nros::init`, then `configure`. `ComponentNode` = the ctor
  creates entities, so it must be **placement-new'd in `__nros_entry_setup`
  *after* `nros::init`** (the live handle). `emit_cpp::emit_typed` + the
  `{nuttx,zephyr}_entry_main_typed.cpp.in` carriers gain a construct branch
  (`static Storage<C> __c; __c.emplace(handle);` vs `static C __c; __c.configure(node);`).
- **Metadata: one new field.** Add a `shape: "rclcpp" | "configure"` marker to the
  `components[]` JSON (`nano_ros_node_register` / `NROS_COMPONENT`); the entry
  branches construct on it. `class` + `class_header` are unchanged, and **`sizeof`
  needs no metadata** — the entry `#include`s the header, so `sizeof(C)` /
  `Storage<C>` is a compile-time fact, not a codegen input.
- `configure(Node&)` examples (the migrated NuttX set + the native templates)
  stay on the existing branch; **template/workspace migration to the recommended
  shape should target `ComponentNode` and therefore wait for 242.1–242.3** (else
  migrate-to-`configure` now + re-migrate later = churn).

## Changelog

- 2026-06 — created (Draft); amends RFC-0043 Q1; driver = ASI phase-2.C; tracked
  by phase-242.
- 2026-06-13 — **Adoption decision: adopt.** Resolved all 5 open questions
  (Q1 wrap-a-Node, Q2 `ok()`-flag-then-halt, Q3 compile-time `N`, Q4 confirmed
  mangled `M::TYPE_NAME` via 240.6, Q5 Rust parity deferred). Recorded the
  construct-ordering delta + the one-field `shape` metadata marker + the
  template-migration-defer.

## References

- Runtime binding: RFC-0043 (this RFC amends its Q1 component shape).
- Entry codegen: RFC-0032 §8a, RFC-0024 (workspace), RFC-0018 (C++ API).
- Thin-wrapper discipline: RFC-0019.
- Consumer driver: `autoware-safety-island` phase-2.C; tracked by phase-242.
