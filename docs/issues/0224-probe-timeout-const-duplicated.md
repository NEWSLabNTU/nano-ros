---
id: 224
title: "PROBE_TIMEOUT_MS = 1000 independently defined 4x across nros-node and nros-c — probe cadence can drift between languages"
status: open
type: tech-debt
area: core
related: []
---

## Finding (deep audit 2026-07-17, I4)

`handles.rs:2098`, `handles.rs:2711`, `nros-c/src/service.rs:1593`,
`nros-c/src/action/client.rs:353` each define their own
`const PROBE_TIMEOUT_MS: u32 = 1000`.

## Fix sketch

One shared const in nros-node (the C layer already links it); delete the
copies.
