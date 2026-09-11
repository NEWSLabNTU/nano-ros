# Scheduling-tiers (real-time) showcase workspace

A nano-ros **differentiator** demo (phase-263 B2): deployment-time real-time
scheduling (RFC-0015) — a control loop and a telemetry node on two priority tiers,
declared in config, no node-code change to retune.

```
src/ctrl_pkg/      — Node pkg, 10 ms control loop, callback group `ctrl` → tier `high`.
src/telem_pkg/     — Node pkg, 100 ms telemetry,  callback group `telem` → tier `low`.
src/demo_bringup/  — Bringup: system.toml declares [tiers.high] / [tiers.low].
src/zephyr_entry/  — the one hand-written entry (a west application).
```

Every other image's entry is generated from its `[image.*]`, so `src/` holds no
`native_entry` to read.

## How tiers are declared

1. The bringup binds each node's callback group to a tier, in the
   `[[component]]` row that declares the node:

   ```toml
   # demo_bringup/system.toml
   [[component]]
   pkg = "ctrl_pkg"
   class = "ctrl_pkg::Control"
   name = "control_node"
   group_tiers = { ctrl = "high" }
   ```

   …and the node labels its entities at runtime: `node.callback_group("ctrl")?`.
   (`ctrl_pkg/Cargo.toml` still carries the same binding as
   `[package.metadata.nros.node] callback_groups`, which the `nros::main!`
   proc-macro reads today. The `[[component]]` row is the source of truth; the
   manifest copy is deprecated — RFC-0047 / phase-273 W2.)

2. The bringup gives each tier its per-RTOS knobs:

   ```toml
   # demo_bringup/system.toml
   [tiers.high]
   spin_period_us = 1000
   [tiers.high.posix]
   priority = 80
   ```

`nros::main!()` reads both, resolves the 2-tier table, and emits the multi-tier
`run_tiers` entry — one (POSIX-priority) task per tier — instead of the single-tier
`run`. Retune priorities/periods by editing `system.toml`; the node code is
untouched. On native, priorities are advisory; on an RTOS deploy (FreeRTOS /
ThreadX) they are real task priorities.

## Build & run

```bash
source ./activate.sh
cd examples/workspaces/realtime-rust
nros setup native
nros sync
nros build native
./build/posix/native_entry/target/debug/native_entry
```
