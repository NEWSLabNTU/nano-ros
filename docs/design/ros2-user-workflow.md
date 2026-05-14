# ROS 2 User Workflow for nano-ros

**Status:** Living design note
**Related:** [Phase 123](../roadmap/phase-123-build-and-api-revision.md), [RTOS orchestration](rtos-orchestration.md), [Colcon build type](colcon-nano-ros-build-type.md)
**Related repo:** `~/repos/play_launch`

## Goal

Give standard ROS 2 users a fluent path from a colcon-style workspace and
normal launch files to a single nano-ros firmware binary.

The user should keep the familiar ROS 2 boundaries:

- Workspace contains packages under `src/`.
- Each reusable node package builds as a library/component.
- A bringup package owns launch files and deployment manifests.
- Launch files describe composition and remaps.
- nano-ros codegen turns that graph into a static RTOS executable.

The nano-ros-specific part should feel like a build target, not a new
application model.

## Target User Flow

```bash
mkdir -p robot_ws/src
cd robot_ws/src
git clone --depth=1 --branch=vX.Y.Z https://github.com/NEWSLabNTU/nano-ros.git

nros new component control_node --lang rust
nros new component perception_node --lang cpp
nros new system robot_bringup --target freertos --board mps2-an385

cd ..
nros setup --target freertos-zenoh
nros plan robot_bringup launch/system.launch.py -- robot_ns:=/car01
nros check
nros build --target freertos-zenoh --board mps2-an385
nros run --qemu
nros monitor
```

Workspace shape:

```text
robot_ws/src/
  nano-ros/
  control_node/
    package.xml
    Cargo.toml
    src/lib.rs
    nros.toml
  perception_node/
    package.xml
    CMakeLists.txt
    src/component.cpp
    nros.toml
  robot_bringup/
    package.xml
    launch/system.launch.py
    nros.toml
```

## Package Model

ROS 2 users already know the composable-node split:

- Linux executable model: node package provides `main()`.
- Composable model: node package provides a class/library; launch file loads it into a container.

nano-ros should mirror the composable model statically:

- Node package provides a registration entry point.
- System package provides launch files and RTOS deployment metadata.
- Generated orchestration crate provides the only `main()`.
- The RTOS "container" is a priority tier: one executor/task per tier.

Rust component entry point:

```rust
pub fn register_control_node(ctx: &mut nros::ComponentContext) -> Result<(), nros::NodeError> {
    let publisher = ctx.node().create_publisher::<ControlCmd>("~/cmd")?;
    ctx.register_timer("control_tick", 10.ms(), move || {
        // control loop
    })?;
    Ok(())
}
```

C++ component entry point:

```cpp
extern "C" nros_ret_t register_perception_node(nros_component_context_t *ctx) {
    nros::ComponentContext c(ctx);
    auto node = c.node();
    // create pubs/subs/timers
    return NROS_RET_OK;
}
```

The entry point must be library-shaped. It must not call global
`nros::init()`, own the executor spin loop, or define `main()`.

## Build Pipeline

### 1. Setup

`nros setup --target <platform>-<rmw>` wraps the Phase 123 source-ship path:

- fetch target-specific submodules;
- install/check Rust target and C cross toolchain;
- prepare workspace-level generated-interface cache;
- make nano-ros discoverable to colcon/CMake/Cargo.

### 2. Plan

`nros plan <bringup_pkg> <launch_file> -- <launch_args...>` runs
`play_launch`/`play_launch_parser` and emits raw `record.json`.

`record.json` remains the launch freeze artifact. It captures:

- regular nodes;
- composable node containers;
- loaded composable nodes;
- params, remaps, env, ROS args;
- resolved launch variables.

### 3. Normalize

`nros plan` also emits `nros-plan.json`, a nano-ros build IR derived from:

- `record.json`;
- per-node `nros.toml`;
- system `nros.toml`;
- package discovery from the workspace.

`nros-plan.json` adds what launch files cannot know:

- component entry symbol;
- Rust crate or C/C++ library target;
- callback groups;
- tier mapping;
- RTOS priority/stack/scheduler policy;
- shared-state layout;
- runtime-overridable parameter args;
- generated-main sizing inputs.

### 4. Check

`nros check` validates:

- every launch node maps to a component package or explicit external process;
- every component has exactly one entry point;
- every callback group maps to a tier;
- active RTOS priority/stack fields exist and are in bounds;
- remaps and namespaces resolve before codegen;
- parameter files can be represented by nano-ros parameter APIs;
- tier spin period is compatible with timer periods;
- shared state access either stays single-tier or has a lock strategy.

### 5. Build

`nros build` generates an orchestration crate/package:

- `main.rs` or platform-specific entry shim;
- one registration call per component;
- one executor per tier;
- shared session setup;
- parameter/default override setup;
- generated shared-context C/Rust/C++ headers;
- Cargo/CMake glue for all component packages.

Single-tier output should collapse to the hand-written nano-ros shape:

```rust
let mut executor = Executor::open(&config)?;
register_control_node(&mut ctx)?;
register_perception_node(&mut ctx)?;
loop {
    executor.spin_once(spin_period);
}
```

Multi-tier output:

```text
main
  open shared session
  spawn tier high   -> Executor::open_with_session(shared)
  spawn tier normal -> Executor::open_with_session(shared)
  spawn tier low    -> Executor::open_with_session(shared)
  idle/join
```

### 6. Run and Monitor

`nros run` delegates to the board runner:

- QEMU for supported test boards;
- `west flash` for Zephyr;
- OpenOCD/vendor flash path for bare-metal boards;
- POSIX process for native.

`nros monitor` should expose a play_launch-like view backed by nano-ros
in-binary telemetry, not Linux-only LD_PRELOAD interception.

## CLI Shape

The user-facing binary should be `nros`.

Initial subcommands:

| Command | Purpose |
| --- | --- |
| `nros setup` | workspace/toolchain/submodule setup |
| `nros new component` | scaffold library-shaped node package |
| `nros new system` | scaffold bringup/deployment package |
| `nros plan` | run launch freeze and emit `record.json` + `nros-plan.json` |
| `nros check` | validate manifests, plan, and target constraints |
| `nros build` | generate orchestration package and build firmware |
| `nros run` | run native/QEMU or flash board |
| `nros monitor` | observe process/device state and logs |
| `nros doctor` | diagnose workspace/toolchain/ROS env |

`cargo nano-ros` can remain the developer/internal entry for codegen, but
standard users should see `nros`.

## Manifest Split

Per-node manifest, owned by package author:

```toml
schema = "nano-ros/orchestration/v1"
kind = "node"

[node]
name = "control_node"
crate = "control_node"
entry = "register_control_node"

[[node.callback_groups]]
id = "control_loop"
type = "MutuallyExclusive"
tier = "high"

[[node.bindings.timers]]
name = "control_tick"
group = "control_loop"
```

System manifest, owned by deployer:

```toml
schema = "nano-ros/orchestration/v1"
kind = "system"
target_rtos = "freertos"

[tiers.high]
spin_period_us = 1000

[tiers.high.freertos]
priority = 5
stack_bytes = 8192

startup_order = ["control_node"]
```

## Gap Matrix

### `nros` CLI

| Gap | Needed |
| --- | --- |
| No single user-facing `nros` binary for the full flow | Add `nros-cli` commands that orchestrate setup, plan, check, build, run, monitor |
| `cargo nano-ros` is codegen-oriented | Keep for low-level/codegen; make `nros` the standard UX |
| No `plan` command | Add command that calls play_launch parser and writes `record.json` + `nros-plan.json` |
| No `doctor` for workspace state | Check sourced ROS env, nano-ros checkout, submodules, toolchains, board vars |

### nano-ros API/runtime

| Gap | Needed |
| --- | --- |
| User examples are `main()` shaped | Add library-shaped `ComponentContext` registration API |
| C++ global `nros::init()`/`spin_once()` model conflicts with generated main | Provide explicit component API taking executor/context; document globals as simple-app path only |
| `Executor::open_with_session(shared)` not available as safe API | Add safe shared-session constructor for per-tier executors |
| Timer registration has no sched-context/tier binding variant | Add `register_timer_on(sc_id, ...)` and C/C++ wrappers |
| Namespaces/remaps are not first-class component inputs | Add `ComponentContext` name resolver and remap-aware create helpers |
| Runtime params exist but plan-time parameter injection is not unified | Add boot-time parameter override loader from generated plan/runtime args |
| Shared state is ad hoc in hand-written apps | Generate shared-context structs/accessors with tier-aware locking |

### Build and colcon

| Gap | Needed |
| --- | --- |
| Phase 78 builds package binaries | Add component/library package mode and system/orchestration package mode |
| Generated orchestration package does not exist | Add `cargo nano-ros generate-main` or equivalent library called by `nros build` |
| Workspace interface cache is incomplete for C/C++ | Finish shared C/C++ generated-interface cache or make system package own it |
| Whole-firmware sizing is manual | Derive executor/node/callback/param limits from `nros-plan.json` |
| Mixed Rust/C/C++ component linking path unclear | Define generated Cargo+CMake bridge contract and static archive order |

### play_launch integration

| Gap | Needed |
| --- | --- |
| `record.json` is process-oriented | Add nano-ros normalization layer to produce `nros-plan.json` |
| Launch composable containers are Linux runtime concepts | Map containers/load nodes to static components/tier groups |
| Python launch freeze can include unsupported runtime behavior | Classify graph-shaping args as build-time only; reject unsupported dynamic cases clearly |
| No stable embedded codegen subcommand | Either add `play_launch generate-rtos` or have `nros plan` call parser crates directly |

### Manifest/schema

| Gap | Needed |
| --- | --- |
| Existing launch manifest describes graph contracts, not RTOS deployment | Add `nros.toml` schema or extend manifest types with tiers/callback groups/shared state |
| Callback groups are source-code concepts in rclcpp | Require sidecar declaration until static analysis can infer them |
| Entity binding can drift from source | Validate manifest-declared timers/topics/services against generated registration metadata where possible |
| Type and QoS reconciliation spans launch params, manifest, source | Add checker rules for remap-resolved names and QoS compatibility |

### Monitoring and execution

| Gap | Needed |
| --- | --- |
| play_launch monitoring assumes Linux processes | Add nano-ros telemetry events from generated binary |
| RTOS logs are board-specific | Normalize QEMU/serial/RTT/native logs behind `nros monitor` |
| No device lifecycle control | Add minimal start/stop/restart only where platform supports it; otherwise expose reset/flash/run |
| No plan/runtime correlation | Emit node/tier/callback IDs into firmware and monitor output |

## v0 Scope

Start with the smallest workflow that proves the model:

- Rust components only.
- Single-tier executor only.
- `record.json` from play_launch.
- Simple node/system `nros.toml`.
- Generated `main.rs` calls each component registration function.
- Existing board runner builds/runs the output.

Explicitly defer:

- C/C++ component ABI;
- multi-tier shared session;
- runtime parameter override persistence;
- generated shared state;
- monitor UI parity with play_launch.

This v0 still gives ROS 2 users the important mental model: write node
packages as libraries, compose with launch files, build one RTOS binary.

## Open Questions

- Should `nros-plan.json` be a public stable artifact or internal debug output?
- Should component packages use new build types (`nros.rust.component`) or keep platform in the type (`nros.rust.freertos`) and mark library mode in `nros.toml`?
- Should `play_launch` own RTOS generation as a subcommand, or should `nros` own it and use play_launch only as parser/recorder?
- How much ROS 2 composable-node metadata should C++ packages reuse versus declaring a separate nano-ros entry symbol?
- Should v0 require `nros.toml` for every node or allow implicit defaults for launch nodes that map cleanly to package names?
