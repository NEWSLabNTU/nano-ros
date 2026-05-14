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

### Node API Examples

These examples describe the target user-facing shape. The API names are
proposed; the important contract is that node packages are libraries and the
generated system owns process init, executor construction, and spin.

#### C, rclc-shaped

rclc users are used to explicit handles, caller-owned allocation, and executor
handle counts known before spin. nano-ros should keep that property but move
the support/executor ownership into generated code.

```c
#include <nano_ros/nros.h>
#include <std_msgs/msg/int32.h>

typedef struct {
    nros_node_t node;
    nros_publisher_t cmd_pub;
    nros_timer_t tick;
    std_msgs__msg__Int32 cmd;
} control_node_t;

static void control_tick(void *user_data)
{
    control_node_t *self = (control_node_t *)user_data;
    self->cmd.data += 1;
    nros_publish(&self->cmd_pub, &self->cmd);
}

nros_ret_t control_node_register(nros_component_context_t *ctx)
{
    control_node_t *self =
        nros_component_alloc(ctx, sizeof(control_node_t), NROS_ALIGNOF(control_node_t));

    NROS_CHECK(nros_component_init_node(ctx, &self->node, "control_node"));
    NROS_CHECK(nros_node_create_publisher(
        &self->node,
        &self->cmd_pub,
        ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32),
        "~/cmd",
        nros_qos_default()));
    NROS_CHECK(nros_component_create_timer(
        ctx,
        &self->tick,
        "control_tick",
        nros_duration_ms(10),
        control_tick,
        self));

    return NROS_RET_OK;
}
```

This mirrors rclc's deterministic model:

- generated code sizes the executor from `nros-plan.json`;
- callback registration order is stable and visible in the plan;
- the component never calls `rclc_support_init`, `rclc_executor_init`, spin, or
  `main()`.

#### C++, rclcpp-composable-shaped

rclcpp components normally expose a node class with a `NodeOptions`
constructor and register it with `RCLCPP_COMPONENTS_REGISTER_NODE`. nano-ros
should offer the same shape, but resolve it to static factories instead of
runtime class loading.

```cpp
#include <chrono>
#include <nano_ros/node.hpp>
#include <nano_ros/register_node.hpp>
#include <std_msgs/msg/int32.hpp>

using namespace std::chrono_literals;

class ControlNode final : public nros::Node {
public:
    explicit ControlNode(const nros::NodeOptions & options)
        : nros::Node("control_node", options)
    {
        cmd_pub_ = create_publisher<std_msgs::msg::Int32>("~/cmd", nros::QoS::Default());
        tick_ = create_wall_timer("control_tick", 10ms, [this] {
            std_msgs::msg::Int32 msg;
            msg.data = count_++;
            cmd_pub_.publish(msg);
        });
    }

private:
    nros::Publisher<std_msgs::msg::Int32> cmd_pub_;
    nros::Timer tick_;
    int32_t count_{0};
};

NROS_COMPONENTS_REGISTER_NODE(ControlNode)
```

The macro should generate static metadata:

- package/library symbol used by `nros build`;
- node factory accepting `nros::NodeOptions`;
- declared entity metadata for checker/codegen, where available.

Unlike rclcpp, the generated RTOS binary should not load a plugin at runtime.
It should link the component archive and instantiate the factory directly.

#### Rust, rclrs-shaped

rclrs uses Rust structs and builder options instead of inheritance. nano-ros
should follow that style while avoiding `Arc` as the default embedded story.

```rust
use core::time::Duration;
use nros::{Component, ComponentContext, NodeOptions, Publisher};
use std_msgs::msg::Int32;

pub struct ControlNode {
    cmd_pub: Publisher<Int32>,
    count: i32,
}

impl Component for ControlNode {
    fn create(ctx: &mut ComponentContext, options: NodeOptions) -> nros::Result<Self> {
        let node = ctx.create_node(options.name("control_node"))?;
        let cmd_pub = node.create_publisher::<Int32>("~/cmd")?;

        Ok(Self { cmd_pub, count: 0 })
    }

    fn register(&mut self, ctx: &mut ComponentContext) -> nros::Result<()> {
        ctx.create_wall_timer::<Self>(
            "control_tick",
            Duration::from_millis(10),
            |state: &mut Self| {
                let msg = Int32 { data: state.count };
                state.count += 1;
                state.cmd_pub.publish(&msg)
            },
        )?;

        Ok(())
    }
}

nros::component!(ControlNode);
```

The Rust API can also support a lower-level registration function for
`no_std` packages that do not want a stateful component trait, but the scaffold
should prefer the trait because it is closest to rclrs' node-as-struct model.

#### Generated main

For all three languages, the system package produces the only executable
entrypoint:

```rust
fn main() -> ! {
    let config = nros_generated::config();
    let mut system = nros::System::open(config).unwrap();

    system.add_component::<control_node::ControlNode>(
        nros::NodeOptions::new("control_node")
            .namespace("/car01")
            .remap("~/cmd", "/car01/control/cmd"),
    ).unwrap();

    unsafe {
        system.add_c_component(
            "perception_node",
            perception_node_register,
            nros::NodeOptions::new("perception_node"),
        ).unwrap();
    }

    system.spin()
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

| Command              | Purpose                                                     |
|----------------------|-------------------------------------------------------------|
| `nros setup`         | workspace/toolchain/submodule setup                         |
| `nros new component` | scaffold library-shaped node package                        |
| `nros new system`    | scaffold bringup/deployment package                         |
| `nros plan`          | run launch freeze and emit `record.json` + `nros-plan.json` |
| `nros check`         | validate manifests, plan, and target constraints            |
| `nros build`         | generate orchestration package and build firmware           |
| `nros run`           | run native/QEMU or flash board                              |
| `nros monitor`       | observe process/device state and logs                       |
| `nros doctor`        | diagnose workspace/toolchain/ROS env                        |

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

| Gap                                                   | Needed                                                                                |
|-------------------------------------------------------|---------------------------------------------------------------------------------------|
| No single user-facing `nros` binary for the full flow | Add `nros-cli` commands that orchestrate setup, plan, check, build, run, monitor      |
| `cargo nano-ros` is codegen-oriented                  | Keep for low-level/codegen; make `nros` the standard UX                               |
| No `plan` command                                     | Add command that calls play_launch parser and writes `record.json` + `nros-plan.json` |
| No `doctor` for workspace state                       | Check sourced ROS env, nano-ros checkout, submodules, toolchains, board vars          |

### nano-ros API/runtime

| Gap                                                                         | Needed                                                                                           |
|-----------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------|
| User examples are `main()` shaped                                           | Add library-shaped `ComponentContext` registration API                                           |
| C API lacks a rclc-like component context                                   | Add `nros_component_context_t`, plan-sized allocation, and C registration metadata               |
| C++ API lacks a rclcpp-like component class shape                           | Add `nros::NodeOptions`, `nros::Node` constructor, and `NROS_COMPONENTS_REGISTER_NODE`           |
| Rust API lacks a rclrs-like component trait                                 | Add `nros::Component`, `NodeOptions`, and `nros::component!` metadata macro                      |
| C++ global `nros::init()`/`spin_once()` model conflicts with generated main | Provide explicit component API taking executor/context; document globals as simple-app path only |
| Component entity metadata is not emitted                                    | Generate topic/timer/sub/service metadata from macros/build scripts for `nros check`             |
| `Executor::open_with_session(shared)` not available as safe API             | Add safe shared-session constructor for per-tier executors                                       |
| Timer registration has no sched-context/tier binding variant                | Add `register_timer_on(sc_id, ...)` and C/C++ wrappers                                           |
| Namespaces/remaps are not first-class component inputs                      | Add `ComponentContext` name resolver and remap-aware create helpers                              |
| Runtime params exist but plan-time parameter injection is not unified       | Add boot-time parameter override loader from generated plan/runtime args                         |
| Shared state is ad hoc in hand-written apps                                 | Generate shared-context structs/accessors with tier-aware locking                                |

### Build and colcon

| Gap                                               | Needed                                                                          |
|---------------------------------------------------|---------------------------------------------------------------------------------|
| Phase 78 builds package binaries                  | Add component/library package mode and system/orchestration package mode        |
| Generated orchestration package does not exist    | Add `cargo nano-ros generate-main` or equivalent library called by `nros build` |
| Workspace interface cache is incomplete for C/C++ | Finish shared C/C++ generated-interface cache or make system package own it     |
| Whole-firmware sizing is manual                   | Derive executor/node/callback/param limits from `nros-plan.json`                |
| Mixed Rust/C/C++ component linking path unclear   | Define generated Cargo+CMake bridge contract and static archive order           |

### play_launch integration

| Gap                                                           | Needed                                                                                   |
|---------------------------------------------------------------|------------------------------------------------------------------------------------------|
| `record.json` is process-oriented                             | Add nano-ros normalization layer to produce `nros-plan.json`                             |
| Launch composable containers are Linux runtime concepts       | Map containers/load nodes to static components/tier groups                               |
| Python launch freeze can include unsupported runtime behavior | Classify graph-shaping args as build-time only; reject unsupported dynamic cases clearly |
| No stable embedded codegen subcommand                         | Either add `play_launch generate-rtos` or have `nros plan` call parser crates directly   |

### Manifest/schema

| Gap                                                                     | Needed                                                                                                   |
|-------------------------------------------------------------------------|----------------------------------------------------------------------------------------------------------|
| Existing launch manifest describes graph contracts, not RTOS deployment | Add `nros.toml` schema or extend manifest types with tiers/callback groups/shared state                  |
| Callback groups are source-code concepts in rclcpp                      | Require sidecar declaration until static analysis can infer them                                         |
| Entity binding can drift from source                                    | Validate manifest-declared timers/topics/services against generated registration metadata where possible |
| Type and QoS reconciliation spans launch params, manifest, source       | Add checker rules for remap-resolved names and QoS compatibility                                         |

### Monitoring and execution

| Gap                                            | Needed                                                                                           |
|------------------------------------------------|--------------------------------------------------------------------------------------------------|
| play_launch monitoring assumes Linux processes | Add nano-ros telemetry events from generated binary                                              |
| RTOS logs are board-specific                   | Normalize QEMU/serial/RTT/native logs behind `nros monitor`                                      |
| No device lifecycle control                    | Add minimal start/stop/restart only where platform supports it; otherwise expose reset/flash/run |
| No plan/runtime correlation                    | Emit node/tier/callback IDs into firmware and monitor output                                     |

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
