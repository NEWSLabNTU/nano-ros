---
id: 275
title: "NROS_COMPONENT identity rule forces C++ namespace rename — class prefix must equal package name"
status: open
type: friction
severity: low
area: nros-cpp
related: [rfc-0057, phase-305]
---

## Finding (autoware-safety-island-example ports, 2026-07-24 — porting-notes 02)

Upstream Autoware components live in nested namespaces
(`namespace autoware::mrm_emergency_stop_operator`). The nano-ros component
identity rule requires the registered class prefix to equal the package
name, so the port had to flatten the namespace
(`namespace autoware_mrm_emergency_stop_operator`). One-line diff per node,
but it is a needless textual delta against upstream — the whole porting
story is "near-verbatim". The ASI fork solved the same constraint with a
wrapper subclass, which is worse.

## Direction

Let `NROS_COMPONENT` (or `nros_generate_interfaces` registration) accept an
explicit plugin-name argument decoupled from the enclosing namespace, the
way rclcpp_components' `RCLCPP_COMPONENTS_REGISTER_NODE` takes the
fully-qualified class regardless of package naming.

## Design (2026-07-25)

RFC-0057 retires L.4: cmake writes `pkg: ${PROJECT_NAME}` into
nros-metadata.json (the only mechanical consumer of the prefix rule),
lints accept nested class names, and the new
`nros_components_register_node(target PLUGIN … EXECUTABLE …)` surface is
keyword-parity with rclcpp_components. No explicit PKG argument — the
register site already knows `${PROJECT_NAME}`.
