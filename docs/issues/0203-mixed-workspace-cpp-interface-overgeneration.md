---
id: 203
title: "mixed-workspace cpp codegen over-generates the interface set — cross-language service pairs blocked"
status: open
type: bug
area: codegen
related: [phase-263, phase-269, rfc-0026]
---

## Summary

In the MIXED (umbrella) workspace, `nros_find_interfaces(LANGUAGE CPP)`
over-generates the **full** interface set — including `action_msgs` — whose
per-pkg cpp FFI crate references `builtin_interfaces` types that are not in
scope (`cannot find type builtin_interfaces_msg_time_t`). The same pkg
compiles fine in the single-language cpp workspace; it breaks only under the
mixed multi-pkg generation.

Found during phase-263 A1 (2026-06-28, mixed services wave) and deferred
there without an issue — this files it (surfaced by the 2026-07-16 phase-263
audit).

## Impact

Cross-LANGUAGE service/action pairs (e.g. **C server + cpp client**) cannot be
demonstrated in `examples/workspaces/mixed`: its service demo is C+C
(`c_add_server_pkg` + `c_add_client_pkg`, reused from the C workspace), with
cross-language preserved only at the workspace level (C talker + C++ listener
+ Rust heartbeat). The phase-263 "no faking" guardrail keeps the demo honest
but degraded.

Also the reason the mixed workspace carries no cpp feature pkgs for the
phase-269-delivered surfaces (params/lifecycle/safety/tiers are demo'd in
`ws-*-c` / `ws-*-cpp` but have no mixed variants).

## Repro sketch

Add a cpp service pkg (e.g. a `cpp_add_client_pkg` mirroring the C one) to
`examples/workspaces/mixed/src/` + the launch, configure the mixed workspace →
the generated per-pkg cpp FFI crate for the over-generated `action_msgs`
fails: `cannot find type builtin_interfaces_msg_time_t`.

## Direction

Either scope `nros_find_interfaces(LANGUAGE CPP)` generation to the pkg's
declared dependency closure (don't emit `action_msgs` unless depended on), or
make the generated cpp FFI crate carry its own `builtin_interfaces` type
imports so over-generation is at worst wasteful, not broken.
