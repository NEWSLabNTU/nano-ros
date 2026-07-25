---
rfc: 0057
title: "Ament-parity component registration — split library/registration macros, retire the L.4 class-prefix rule, auto-wired interface deps"
status: Draft
since: 2026-07
last-reviewed: 2026-07
implements-tracked-by: [phase-305]
supersedes: []
superseded-by: null
---

# RFC-0057 — Ament-parity component registration

Amends the registration surface of RFC-0044 (rclcpp component shape) and
retires the Phase 212.L.4 class-prefix rule. Motivating issues: 0275
(namespace flattening), 0277 (per-call FFI superset / manual topo-last
linking), and the simple-autoware-safety-island port
(`docs/porting-notes.md` 02, 08, 12), whose goal — near-verbatim ports of
unmodified Autoware nodes — is the benchmark this RFC is judged against.

## Problem

Porting an upstream rclcpp component today forces three deltas that have
nothing to do with nano-ros's actual constraints:

1. **Namespace flattening (issue 0275).** `nano_ros_node_register`
   enforces L.4: `CLASS` must start with `${PROJECT_NAME}::`. Upstream
   `namespace autoware::mrm_emergency_stop_operator` must be renamed to
   `namespace autoware_mrm_emergency_stop_operator` in every header and
   source. The rule's only *mechanical* basis is that
   `nros-metadata.json` carries no `pkg` field, so the codegen derives
   `pkg` by splitting the class string — even though cmake knows
   `${PROJECT_NAME}` at the register site (L.4's own check uses it), and
   `system.toml` `[[component]]` rows already carry an explicit `pkg`.
   The convention duplicates information that is always adjacent.

2. **One overloaded macro.** `nano_ros_node_register(NAME CLASS SOURCES
   HEADER SHAPE …)` fuses `add_library(STATIC ${SOURCES})` with
   component registration. Ament splits these
   (`ament_auto_add_library` + `rclcpp_components_register_node(target
   PLUGIN … EXECUTABLE …)`), so upstream registration never mentions
   sources. The fused shape is why the port's CMakeLists looks nothing
   like upstream's.

3. **Manual interface-dep wiring (issue 0277 UX).** The per-call
   topo-last superset FFI crate forces consumers to hand-link exactly
   one generated interface lib (`if(TARGET tier4_system_msgs__nano_ros_cpp)
   target_link_libraries(… )`) — a footgun ament_auto users never see.

## Decision

### D1 — split the macros, adopt rclcpp_components keywords

```cmake
nano_ros_auto_add_library(mrm_emergency_stop_operator STATIC
    src/mrm_emergency_stop_operator/mrm_emergency_stop_operator_core.cpp)

nros_components_register_node(mrm_emergency_stop_operator
    PLUGIN "autoware::mrm_emergency_stop_operator::MrmEmergencyStopOperator"
    EXECUTABLE mrm_emergency_stop_operator)
```

- `PLUGIN` ≙ today's `CLASS`; `EXECUTABLE` ≙ today's `NAME` —
  keyword-for-keyword parity with `rclcpp_components_register_node`, so
  an upstream CMakeLists ports with a macro-name swap.
- Registration operates on an EXISTING target; `SOURCES` leaves the
  registration surface entirely.
- `SHAPE` defaults to `rclcpp`; the legacy `configure(Node&)` shape
  becomes the explicit opt-in (`SHAPE configure`).
- `HEADER` remains the one nano-ros-specific keyword, optional:
  the typed entry codegen `#include`s the class header to placement-new
  the component — the price of static linking (no pluginlib/dlopen on an
  RTOS). Derived from `PLUGIN` by convention
  (`a::b::Class` → `a/b/Class.hpp`) when omitted; explicit when the
  upstream layout deviates (Autoware's `…_core.hpp`).
- Entry/deploy knobs (`TYPED`, `DEPLOY`, `CALLBACK_GROUPS`, `LANGUAGE`)
  stay as optional keywords for now; moving them into
  `system.toml`/the model is a candidate follow-up that would make the
  register call identical to rclcpp's modulo macro name.

### D2 — retire L.4; `pkg` becomes explicit metadata

- `nros_components_register_node` (and the legacy macro) write
  `pkg: "${PROJECT_NAME}"` into `nros-metadata.json` `components[]`.
- `codegen/entry/metadata.rs` keys by the explicit `pkg`; the
  class-prefix split remains only as a fallback for pre-RFC metadata.
- The cmake FATAL (NanoRosNodeRegister L.4) and the
  `lint_class_pkg_prefix` workspace/bringup lints are retired. The
  system.toml lint keeps checking that `class` is a qualified
  (`::`-containing) name; `pkg` on the `[[component]]` row is the
  authority it always structurally was.
- `NROS_COMPONENT`'s `__nros_component_class_<pkg>` string keeps its
  `"<pkg>::<Class>"` spelling (symbol keyed by pkg; the string is a
  label, not an identity input).

Consequence: upstream namespaces port verbatim —
`namespace autoware::mrm_emergency_stop_operator` compiles and registers
unchanged.

### D3 — `nano_ros_auto_add_library` wires interface deps

The ament_auto analog: `add_library(STATIC …)` + automatic linking of
the workspace's generated interface libraries. Consumers stop
hand-picking interface libs; the `if(TARGET <last_pkg>__nano_ros_cpp)`
block in every ported CMakeLists disappears. (Written against the
0253-era topo-last superset routing; phase-306's per-package FFI crates
since retired that mechanism entirely — the auto-wiring UX here is
unchanged, and 0277's mixed-subset hazard is gone by design.)

## Compatibility / migration

- `nano_ros_node_register` / `nano_ros_add_node` remain as compat
  spellings (internally forwarding to the split implementation), with
  L.4 downgraded from FATAL to nothing (D2 applies to them too — the
  rule, not just the new macro, is what retires).
- Old `nros-metadata.json` without `pkg` keeps working via the
  class-prefix fallback.
- Examples + the ASI port migrate to the new spelling as the reference.

## Verification

- Unit: metadata.rs explicit-pkg + fallback paths; lint tests updated
  (nested class accepted; unqualified class still rejected).
- Integration: ASI workspace rebuilt with verbatim upstream namespaces
  (`autoware::…`) on the new macros — native + zephyr images boot, the
  full MRM demo receipt passes.
- Grep gate: no `${PROJECT_NAME}::` prefix requirement anywhere in
  cmake/lints.

## Rejected

- **Explicit `PKG` argument on the register macro** — redundant:
  `${PROJECT_NAME}` is in scope at the register site.
- **Keeping L.4 with an escape hatch** — preserves a convention whose
  information content is already carried by adjacent explicit fields;
  every escape-hatch user is exactly the verbatim-porting user the rule
  hurts most.
