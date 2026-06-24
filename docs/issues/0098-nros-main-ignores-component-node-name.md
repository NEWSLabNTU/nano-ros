---
id: 98
title: "`nros::main!` runtime ignores `system.toml` component `name` — node registers under executor default (`/node`)"
status: open
type: bug
area: core
related: [phase-264, rfc-0004]
---

## Summary

A node booted via `nros::main!` registers in the ROS graph under the **executor default
node name** (`node` → FQN `/node`), **not** the `name` declared for its `[[component]]`
in `system.toml` (or the `<node name=…>` in the launch file). The macro hardcodes the
node name to the **entry crate's** `CARGO_PKG_NAME`
(`packages/core/nros-macros/src/main_macro.rs:1081`/`:1084`,
`.node_name(::core::env!("CARGO_PKG_NAME"))`); the component `name` parsed from
`system.toml` is never threaded into the runtime node name.

The W4c design note (phase-264) flagged this as a known limitation —
"one param server, registered under the executor default node name; multi-node per-node
param scoping is out of scope" — but it has a user-visible + test-visible cost beyond
param scoping, captured here so it is not lost when phase-264 archives.

## Repro

`examples/workspaces/ws-params-rust` declares:

```toml
# src/demo_bringup/system.toml
[[component]]
pkg  = "param_talker_pkg"
name = "param_talker"
```

```xml
<!-- src/demo_bringup/launch/system.launch.xml -->
<node pkg="param_talker_pkg" exec="param_talker" name="param_talker">
  <param name="publish_period_ms" value="250"/>
</node>
```

Boot the native entry against a dedicated zenohd + wire-matched `rmw_zenoh_cpp` overlay
(`just rmw_zenoh setup`), then:

```
$ ros2 node list
/node                       # expected /param_talker
$ ros2 param list /node
  publish_period_ms         # services ARE up — just under the wrong name
$ ros2 param get /node publish_period_ms
Integer value is: 250
$ ros2 param set /node publish_period_ms 500
Set parameter successful   # live ctx.parameter read republishes 500 — runtime correct
```

The parameter machinery (W4a/W4b/W4c) works end-to-end; only the **node name** is wrong.

## Impact

- **User-facing:** `ros2 node list` / `ros2 node info` / `ros2 param …` all key off
  `/node` instead of the configured `param_talker`. Two `nros::main!` apps on one graph
  would both claim `/node` (or their entry crate names), colliding instead of using the
  distinct names the launch file assigns.
- **Test:** `packages/testing/nros-tests/tests/params.rs::test_ros2_param_set_reconfigures_live_read`
  originally grepped `ros2 node list` for `param_talker`, so it could **never** match and
  fell through to `skip!` → nextest **FAILED** in the `ros2-interop` group. The
  `ros2 param set` reconfig half of W4c was therefore never actually exercised in CI
  despite the "VERIFIED" claim. Worked around (2026-06-25) by discovering the node via the
  parameter it exposes (`publish_period_ms`) rather than its name — see the doc-comment
  referencing this issue. The workaround should revert to a name match once this is fixed.

## Fix direction

Thread each `[[component]].name` from `system.toml` (and the launch `<node name>`) into
the runtime node name the macro emits, instead of the entry crate `CARGO_PKG_NAME`. The
single-component case is small (replace the `CARGO_PKG_NAME` node-name with the parsed
component name in `main_macro.rs`). The multi-node / per-node param-store scoping case is
the larger piece phase-264 W4c deferred and should be designed alongside (one param
server today, keyed to the default node — per-node stores need the node→store mapping).

## Evidence

Found 2026-06-25 while running the phase-264 W4c interop test to close the verification
gap; root-caused to `main_macro.rs:1081`. Runtime reconfig behaviour confirmed correct by
manual `ros2 param set` → `Received: 500` over the wire.
