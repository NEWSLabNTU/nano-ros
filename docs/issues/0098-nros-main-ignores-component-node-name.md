---
id: 98
title: "`nros::main!` runtime ignores `system.toml` component `name` — node registers under executor default (`/node`)"
status: open
type: bug
area: core
related: [phase-264, rfc-0004]
---

## Status (2026-06-25)

**Single-node launch: RESOLVED.** A single-node launch now threads the component `name`
into the primary session via `DeployOverlay.node_name`, so `ros2 node list` shows
`/param_talker` (verified) instead of `/node`. The `ws-params-rust` interop test asserts
the proper name. **Multi-node launch: still open** — N components share one primary session,
so per-node graph naming + per-node param-store scoping remain the deferred piece (see Fix
direction). This issue stays `open` until the multi-node case is addressed.

Changed: `DeployOverlay.node_name` (`nros-platform`); `nros::main!` bakes it when the launch
declares exactly one node (`nros-macros`); `PosixBoard`/`NativeBoard` apply it before
`Executor::open` (`nros-board-posix`, `nros-board-native`).

## Summary

A node booted via `nros::main!` registers in the ROS graph under the **executor default
node name** (`node` → FQN `/node`), **not** the `name` declared for its `[[component]]`
in `system.toml` (or the `<node name=…>` in the launch file).

**Root cause (corrected 2026-06-25).** The graph node name is the name of the **primary
zenoh session**, which the board opens at `Executor::open` *before* the macro's register
closure runs. On the hosted native path (`[deploy.native] kind="self"`), `NativeBoard::run`
→ `PosixBoard::run` builds the config with `ExecutorConfig::from_env()`
(`packages/boards/nros-board-posix/src/lib.rs:183`), and `from_env()` hardcodes
`node_name: "node"` (`packages/core/nros-node/src/executor/types.rs:321`). The component
`name` from `system.toml` *does* reach `ExecutorNodeRuntime::create_node` →
`node_builder(name).build()`, but `build()` reuses the primary session (returns `NodeId(0)`)
whenever the new node's rmw+locator match the primary
(`packages/core/nros-node/src/executor/node_record.rs:228`), so the name is recorded but a
session carrying it is never opened — the graph keeps the primary's `"node"`. (The
`main_macro.rs:1081` `.node_name(CARGO_PKG_NAME)` originally cited here is the *OwnedSpin /
native_sim* arm — a different, no_std path — not the hosted native board, so it is not the
active cause for the `ws-params-rust` repro.)

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

The fix must name the **primary session** at open time, since that is what the ROS graph
reports. Two scopes:

**Single-component (the common showcase shape) — tractable.** Thread the lone
`[[component]].name` into the board's `ExecutorConfig` before `Executor::open`. The
existing macro→board boot channel is `DeployOverlay` (already carries
locator/domain/transport). Plan:
1. `DeployOverlay` += `node_name: Option<&'static str>`
   (`packages/core/nros-platform/src/board/entry.rs`).
2. `nros::main!` populates it from the single component name **only when the launch
   declares exactly one node** (`main_macro.rs`, `deploy_overlay_tokens`).
3. `PosixBoard::run` / `run_with_deploy` applies `overlay.node_name` onto
   `ExecutorConfig::from_env()` via `.node_name(..)`
   (`packages/boards/nros-board-posix/src/lib.rs`). NB: hosted boards currently treat the
   overlay locator as a no-op (issue #48) — node_name would be the first overlay field they
   *do* honor, which is correct (locator stays env-driven for dev; node name is a launch
   identity).

**Multi-component — the larger deferred piece (phase-264 W4c).** N components on one
executor share ONE primary session = one graph node name; correct per-node naming needs a
session (or graph liveliness token) per node, plus the per-node param-store scoping W4c
deferred (one param server today, keyed to the default node). Design together.

## Evidence

Found 2026-06-25 while running the phase-264 W4c interop test to close the verification
gap; root-caused (corrected) to `PosixBoard::run`'s `from_env()` `node_name:"node"` +
`node_record.rs:228` primary-session reuse. Runtime reconfig behaviour confirmed correct by
manual `ros2 param set` → `Received: 500` over the wire.
