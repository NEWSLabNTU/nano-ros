---
id: 276
title: "Launch *.param.yaml values not projected into node parameters — upstream no-default declares can't port verbatim"
status: open
type: enhancement
area: codegen
related: [0254, 0255]
---

## Finding (autoware-safety-island-example ports, 2026-07-24 — porting-notes 06)

Upstream Autoware nodes declare parameters with NO default
(`declare_parameter<double>("target_acceleration")`) and receive values from
`config/*.param.yaml` via the launch pipeline. nano-ros parameters are
node-local: `declare_parameter(name, default)` only. The port had to copy
every yaml value into in-code defaults — a silent-drift hazard against the
upstream configs (values now live in two places).

Distinct from 0254 (the compat API surface): even with a
`declare_parameter` shim, there is no channel that projects launch-time
param-file values into the node.

## Direction

The model/codegen already consumes the launch description: lower resolved
param-file values per node into the generated entry (compile-time baked on
embedded — matching the domain-id precedent), so no-default declares
resolve at boot. Runtime file loading is NOT required for parity on
embedded targets.
