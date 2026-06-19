# Scheduling-tiers (real-time) showcase workspace

A nano-ros **differentiator** demo (phase-263 B2): deployment-time real-time
scheduling (RFC-0015) — a control loop and a telemetry node on two priority tiers,
declared in config, no node-code change to retune.

```
src/ctrl_pkg/      — Node pkg, 10 ms control loop, callback group `ctrl` → tier `high`.
src/telem_pkg/     — Node pkg, 100 ms telemetry,  callback group `telem` → tier `low`.
src/demo_bringup/  — Bringup: system.toml declares [tiers.high] / [tiers.low].
src/native_entry/  — Entry, resolves the 2-tier table → run_tiers.
```

## How tiers are declared

1. Each Node pkg names its callback group + the tier it maps to, in Cargo metadata:

   ```toml
   # ctrl_pkg/Cargo.toml
   [package.metadata.nros.node]
   callback_groups = [{ id = "ctrl", tier = "high" }]
   ```

   …and labels its entities at runtime: `node.callback_group("ctrl")?`.

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
cd examples/workspaces/ws-realtime-rust
nros setup native
nros ws sync
cargo run -p native_entry
```
