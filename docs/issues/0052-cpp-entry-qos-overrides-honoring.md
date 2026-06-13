---
id: 52
title: C++ typed-entry does not honor per-topic qos_overrides (211.H wave3b)
status: open
type: tech-debt
area: cli
related: [phase-211, phase-242, phase-244, rfc-0032]
---

## Gap

Per-topic `qos_overrides` (lowered from `qos_overrides.<topic>.<role>.<policy>`
launch params) are honored on the Rust path — planner lowering
(`plan_system_lowers_qos_overrides`), runtime `NodeHandle::set_qos_overrides` +
`apply_overrides`, codegen-bake (`render_sub_qos_expr`), and live runtime
delivery (`qos_overrides_runtime_delivery`). The **C++ typed entry does not**:
the generated C++ entry (`emit_cpp`) emits no `QosOverride[]` table, no
`nros_cpp_node_set_qos_overrides` FFI, and no `set_qos_overrides` call before
`configure(node)`. So a C++-baked component silently ignores launch
qos_overrides.

## Why deferred (from 211.H)

Touches `emit_cpp.rs` + `component_node.hpp` — the phase-242 (rclcpp-faithful
component model) / phase-244 (example source cleanliness) emit hot path —
collision risk. The only thing exercising it (runtime delivery counters) rides
on the already-landed deploy second-stage, so this is now buildable; sequence it
after the 242/244 emit work settles.

## Fix sketch

Mirror the Rust path in `emit_cpp`: emit a static `QosOverride[]` for the
component's topics, an FFI `nros_cpp_node_set_qos_overrides`, and call it before
`configure(node)`. Add a C++ analogue of `qos_overrides_runtime_delivery`.
Split out of Phase 211 (substantially complete + archived); owned by the
242/244 emit work.

## Progress — runtime FFI slice LANDED (2026-06-14)

The collision-free runtime plumbing is in (the part that does NOT touch the
242/244 `emit_cpp` hot zone). A C++ (or C) entry that calls
`set_qos_overrides` before creating entities now honors launch overrides:

- **nros-cpp** (the C++ wrapper backing): `nros_cpp_qos_override_t` struct +
  `nros_cpp_node_set_qos_overrides` FFI + `apply_qos_overrides` folded into
  `nros_cpp_publisher_create` / `nros_cpp_subscription_create`; C++
  `Node::set_qos_overrides(const nros_cpp_qos_override_t*, size_t)` wrapper
  (`ComponentNode` reaches it via `node()`). cbindgen header regenerated.
- **nros-c** (rclc-style C API, parity bonus — same gap): `nros_qos_override_t`
  + `nros_node_set_qos_overrides` + apply in `nros_publisher_init` /
  `nros_subscription_init`.
- Both apply paths unit-tested (`apply_qos_overrides_*` in each crate); struct
  fields appended at the END of the node structs (additive — existing C/C++ ABI
  offsets unchanged).

**Remaining (still deferred behind 242/244 emit):**
1. **`emit_cpp` auto-bake** — emit the static `nros_cpp_qos_override_t[]` per
   node + the `set_qos_overrides` call before `configure(node)`, threading
   qos_overrides through the entry codegen model (entry `PlanNode` carries no
   qos_overrides yet). This is the hot-zone part.
2. **C++ runtime-delivery e2e** — a cmake C++ entry fixture that calls
   `set_qos_overrides` + delivers cross-process (the C++ analogue of
   `qos_overrides_runtime_delivery`). Rides on (1)'s emit or a hand-written
   fixture.
