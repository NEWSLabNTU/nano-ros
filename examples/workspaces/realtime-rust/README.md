# Scheduling-tiers (real-time) showcase workspace

A nano-ros **differentiator** demo (phase-263 B2): deployment-time real-time
scheduling (RFC-0015) — a control loop and a telemetry node on two priority tiers,
declared in config, no node-code change to retune.

```
src/ctrl_pkg/      — Node pkg, 10 ms control loop, callback group `ctrl` → tier `high`.
src/telem_pkg/     — Node pkg, 100 ms telemetry,  callback group `telem` → tier `low`.
src/demo_bringup/  — Bringup: system.toml declares [tiers.high] / [tiers.low],
                     and boards/native_sim_native_64/ holds the Zephyr image's
                     Kconfig (RFC-0065 D4).
src/derived_bringup/ — Bringup for the SAME two nodes with NO authored tiers:
                     launch/system.contract.yaml states their rates and
                     `nros::main!` derives the table. Its one Zephyr image
                     lowers the transport band (prj-lowered-band.conf), so the
                     derived priorities must come from the image's own .config
                     to land below it (issue 1537; sched_dims_applied_e2e).
```

**Every** image's entry is generated from its `[image.*]`, so `src/` holds no
entry at all — not `native_entry`, and since phase-470 W5.b2 (issue 1288) not
`zephyr_entry` either: `nros build` writes the west application around the
generated staticlib under `build/<coord>/`.

The Zephyr image is also where the `[tiers.*]` block below stops being only a
runtime schedule and becomes a BUILD input. `nros-board-zephyr` keeps
`ZephyrBoard::run_tiers` behind a `tiers` cargo feature and the kernel-EDF call
behind `zephyr-edf`; the deleted entry named both by hand, and the generated one
derives them from this file — `tiers` from the table not collapsing to the single
`default` tier, `zephyr-edf` from `[tiers.high]` being `class = "real_time"` with
a `[tiers.high.zephyr] deadline`. Same predicates their consumers key on, so
there is nothing to keep in step.

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
./build/posix-zenoh/native_entry/target/debug/native_entry
```
