---
rfc: 0027
title: "ROS 2 User Workflow for nano-ros"
status: Stable
since: 2026-05
last-reviewed: 2026-05
implements-tracked-by: []
supersedes: []
superseded-by: null
---

# ROS 2 User Workflow for nano-ros

**Status:** Stable (origin/rationale doc — current layout authority is RFC-0024/0025)
**Related:** [Phase 123](../roadmap/archived/phase-123-build-and-api-revision.md), [RTOS orchestration](0015-rtos-orchestration.md), [Colcon build type](archived/colcon-nano-ros-build-type.md)
**Related repo:** `~/repos/play_launch`

> **Terminology note (Phase 212.L.9 rename).** This doc predates the rename and
> the unified config model. Read its older vocabulary as the current names:
> *component package* → **Node pkg**; `nros::component!` → **`nros::node!`**;
> `ExecutableComponent` → **`ExecutableNode`**; the registration/`ComponentContext`
> surface → **`NodeContext`** (+ `CallbackCtx` in callbacks). The authoritative
> **current** multi-node layout is RFC-0024 / RFC-0025; configuration is RFC-0004;
> RMW selection is RFC-0031. This RFC remains the capstone rationale for *why* the
> workflow has its shape.
>
> **CLI-verb note (Phase 222).** This doc's `nros build` / `nros run` references predate Phase 222, which **removed** those verbs — `nros` is now provisioner + codegen + metadata only. Read `nros build` as the native build (`cargo build` / `cmake --build` / `west build` / `idf.py build`) and `nros run` as the native run (`cargo run -p <entry>` / `west run` / `probe-rs run`).

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
# build: west build -b mps2_an385 -- -DNANO_ROS_RMW=zenoh   (after: nros codegen-system)
# run:   west build -t run
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
proposed; the important contract is that node packages are libraries, all
schedulable entities are created through metadata-aware nano-ros APIs, and the
generated system owns process init, executor construction, and spin.

Every node/entity creation API should require a stable ID:

- node ID: `"control_node"`;
- callback/entity ID: `"control_tick"`, `"odom_sub"`, `"cmd_pub"`;
- ROS name: `"~/odom"`, `"~/cmd"`;
- type: from generic type parameter, C type support handle, or generated
  message binding.

This makes source metadata a natural byproduct of writing the component. Users
should not maintain a second callback list by hand.

Callback-effect annotations are additive metadata, not a breaking API change.
The rclc/rclcpp/rclrs-shaped creation calls remain valid; advanced users can
add `.reads(...)`, `.publishes(...)`, or `.writes(...)` builder calls when they
want `nros check` to connect source callbacks to ROS manifest paths.

#### C, rclc-shaped

rclc users are used to explicit handles, caller-owned allocation, and executor
handle counts known before spin. nano-ros should keep that property but move
the support/executor ownership into generated code.

```c
#include <nano_ros/nros.h>
#include <std_msgs/msg/int32.h>

typedef struct {
    nros_node_t *node;
    nros_publisher_t *cmd_pub;
    nros_timer_t *tick;
    std_msgs__msg__Int32 cmd;
} control_node_t;

static void control_tick(void *user_data)
{
    control_node_t *self = (control_node_t *)user_data;
    self->cmd.data += 1;
    nros_publish(self->cmd_pub, &self->cmd);
}

nros_ret_t control_node_register(nros_component_context_t *ctx)
{
    control_node_t *self =
        nros_component_alloc(ctx, sizeof(control_node_t), NROS_ALIGNOF(control_node_t));

    NROS_CHECK(nros_component_node(ctx, "control_node", &self->node));
    NROS_CHECK(nros_node_publisher(
        self->node,
        "cmd_pub",
        "~/cmd",
        ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32),
        nros_qos_default(),
        &self->cmd_pub));
    NROS_CHECK(nros_node_timer(
        self->node,
        "control_tick",
        nros_duration_ms(10),
        control_tick,
        self,
        &self->tick));

    return NROS_RET_OK;
}

NROS_COMPONENT(control_node_register)
```

This mirrors rclc's deterministic model:

- generated code sizes the executor from `nros-plan.json`;
- callback registration order is stable and visible in the plan;
- `nros_node_publisher(...)` and `nros_node_timer(...)` both create the
  runtime entity and emit metadata when called with a metadata context;
- the component never calls `rclc_support_init`, `rclc_executor_init`, spin, or
  `main()`.

#### C++, rclcpp-composable-shaped

rclcpp components normally expose a node class with a `NodeOptions`
constructor and register it with `RCLCPP_COMPONENTS_REGISTER_NODE`. nano-ros
should offer the same shape, but entity creation methods require stable IDs and
record metadata. Runtime class loading is replaced by static factories.

```cpp
#include <chrono>
#include <nano_ros/component_node.hpp>
#include <nano_ros/component.hpp>
#include <nav_msgs/msg/odometry.hpp>
#include <std_msgs/msg/int32.hpp>

using namespace std::chrono_literals;

class ControlNode final : public nros::ComponentNode {
public:
    explicit ControlNode(const nros::NodeOptions & options)
        : nros::ComponentNode("control_node", options)
    {
        cmd_pub_ = create_publisher<std_msgs::msg::Int32>(
            "cmd_pub",
            "~/cmd",
            nros::QoS::Default());

        odom_sub_ = create_subscription<nav_msgs::msg::Odometry>(
            "odom_sub",
            "~/odom",
            nros::QoS::SensorData(),
            [this](const nav_msgs::msg::Odometry & msg) {
                last_speed_ = msg.twist.twist.linear.x;
            });

        tick_ = create_wall_timer("control_tick", 10ms, [this] {
            std_msgs::msg::Int32 msg;
            msg.data = count_++;
            cmd_pub_.publish(msg);
        });
    }

private:
    nros::Publisher<std_msgs::msg::Int32> cmd_pub_;
    nros::Subscription<nav_msgs::msg::Odometry> odom_sub_;
    nros::Timer tick_;
    double last_speed_{0.0};
    int32_t count_{0};
};

NROS_COMPONENTS_REGISTER_NODE(ControlNode);
```

The constructor API generates entity metadata. The macro provides export glue:

- package/library symbol used by `nros build`;
- node factory accepting `nros::NodeOptions`;
- C ABI thunk used by the generated Rust orchestration package.

If a package has no exported component, `nros check` should fail with a direct
diagnostic. If a node creates callbacks without stable IDs, the component
should not compile in nano-ros component mode.

#### Rust, rclrs-shaped

rclrs uses Rust structs and builder options instead of inheritance. nano-ros
should follow that style while avoiding `Arc` as the default embedded story.
The `ComponentContext` is also the metadata recorder.

```rust
use core::time::Duration;
use nros::{Component, ComponentContext, NodeOptions, Publisher, Subscription};
use nav_msgs::msg::Odometry;
use std_msgs::msg::Int32;

pub struct ControlNode {
    cmd_pub: Publisher<Int32>,
    odom_sub: Subscription<Odometry>,
    last_speed: f64,
    count: i32,
}

impl Component for ControlNode {
    const NAME: &'static str = "control_node";

    fn create(ctx: &mut ComponentContext, options: NodeOptions) -> nros::Result<Self> {
        let node = ctx.node(options.name(Self::NAME))?;
        let cmd_pub = node.publisher::<Int32>("cmd_pub", "~/cmd")?;
        let odom_sub = node.subscription::<Odometry>(
            "odom_sub",
            "~/odom",
            |msg: &Odometry, state: &mut Self| {
                state.last_speed = msg.twist.twist.linear.x;
                Ok(())
            },
        )?;

        Ok(Self {
            cmd_pub,
            odom_sub,
            last_speed: 0.0,
            count: 0,
        })
    }

    fn register(&mut self, ctx: &mut ComponentContext) -> nros::Result<()> {
        ctx.timer::<Self>(
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

nros::node!(ControlNode);   // Phase 212.L.9: was nros::component!
```

The same Rust API can expose effect metadata without changing the basic
rclrs-shaped calls:

```rust
node.subscription::<Odometry>("odom_sub", "~/odom")
    .callback(|msg: &Odometry, state: &mut Self| {
        state.last_speed = msg.twist.twist.linear.x;
        Ok(())
    })?;

ctx.timer::<Self>("control_tick", Duration::from_millis(10))
    .reads("odom_sub")
    .publishes("cmd_pub")
    .callback(|state: &mut Self| {
        let msg = Int32 { data: state.count };
        state.count += 1;
        state.cmd_pub.publish(&msg)
    })?;
```

For C++, the equivalent can be a builder beside the familiar direct API:

```cpp
create_subscription<nav_msgs::msg::Odometry>("odom_sub", "~/odom", qos, cb);

subscription<nav_msgs::msg::Odometry>("odom_sub", "~/odom")
    .qos(qos)
    .publishes("cmd_pub")
    .callback(cb);
```

For C, effects can be a separate optional metadata call:

```c
nros_node_subscription(node, "odom_sub", "~/odom", type, cb, state, &sub);
nros_callback_publishes(node, "odom_sub", "cmd_pub");
```

The component macro should only expose the component to package discovery. The
entity metadata comes from `ctx.node(...)`, `node.publisher(...)`,
`node.subscription(...)`, and `ctx.timer(...)`. A forgotten component macro
should produce "package has no exported nros component"; forgotten entity
metadata should be impossible because there is no anonymous entity creation API
in component mode. Callback-effect metadata remains optional and improves
manifest-path validation.

For C++, nano-ros should reuse the familiar rclcpp component shape but not the
rclcpp runtime plugin mechanism as the embedded source of truth. The nano-ros
macro must emit the static factory metadata, generated C ABI thunk, and package
metadata needed by `nros`; importing rclcpp component metadata can be a
compatibility helper, not the primary embedded contract.

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

For the standard ROS 2 workflow, nano-ros should offer library/component mode
only. Supporting both component packages and package-local `main()` entrypoints
inside the same orchestration flow makes ownership ambiguous: launch files
would describe composition, while a hand-written `main()` could silently create
different executors, spin periods, or callback bindings. Users who want a tiny
manual application can use a separate simple-app path such as `nros new app`,
but `nros plan/check/build` should accept only components.

### Metadata Discovery

Manual callback lists are error-prone. nano-ros should discover component
metadata by compiling each component in a host-side metadata mode and invoking
the same library entry path with a fake `ComponentContext`.

In metadata mode:

- `ctx.node(...)` records node ID and resolved default namespace inputs;
- `node.publisher(...)` records publisher ID, ROS name, message type, and QoS;
- `node.subscription(...)` records subscriber callback ID, ROS name, message
  type, and QoS;
- `ctx.timer(...)` records timer callback ID and period;
- service/client/action builders record IDs, names, types, and callback roles;
- no RMW session is opened, no RTOS task is spawned, and no user callback body
  is executed.

Example discovered metadata:

```json
{
  "package": "control_node",
  "component": "control_node::ControlNode",
  "nodes": [
    {
      "id": "control_node",
      "publishers": [
        {
          "id": "cmd_pub",
          "source_name": "~/cmd",
          "name_kind": "private",
          "type": "std_msgs/msg/Int32"
        }
      ],
      "subscriptions": [
        {
          "id": "odom_sub",
          "source_name": "~/odom",
          "name_kind": "private",
          "type": "nav_msgs/msg/Odometry"
        }
      ],
      "timers": [
        {
          "id": "control_tick",
          "period_us": 10000
        }
      ],
      "callback_effects": [
        {
          "id": "odom_sub",
          "triggered_by": ["odom_sub"],
          "reads": ["odom_sub"]
        },
        {
          "id": "control_tick",
          "triggered_by": ["control_tick"],
          "reads": ["odom_sub"],
          "publishes": ["cmd_pub"]
        }
      ]
    }
  ]
}
```

This metadata should be generated into
`build/<system_pkg>/nros/metadata/<pkg>.json`.
`nros.toml` may still provide package-level facts that source cannot know
cleanly, such as language, CMake target, static archive name, and optional
default callback-group hints. It should not duplicate every callback.

Callback effects are local facts, not global chains. A callback can declare
what it is triggered by, which entities or state slots it reads, and which
publishers/services/actions or state slots it writes. `nros` derives possible
manifest paths from these local facts:

```text
manifest path: /odom -> /cmd
metadata:      control_tick reads odom_sub and publishes cmd_pub
derived path:  /odom -> odom_sub -> control_tick -> cmd_pub -> /cmd
```

Default effects keep the basic API usable:

- subscription callbacks are triggered by and read their own subscription;
- timer callbacks are triggered by their own timer;
- service callbacks read requests and write responses;
- action callbacks read goals/cancel events and write feedback/results;
- publisher effects are not inferred unless declared by `.publishes(...)` or an
  equivalent C/C++ metadata call.

If a ROS manifest path cannot be connected to callback effects, MVP `nros check`
should warn or fail based on validation mode. The user can add effect metadata
in source or a temporary `nros.toml` override; source metadata remains the
preferred location.

Services and actions follow the same metadata model as pub/sub: source
metadata records endpoint IDs, unresolved names, interface types, callback IDs,
and local effects; launch resolves names; ROS manifests validate graph
contracts.

Service metadata should distinguish request and response roles:

```json
{
  "id": "reset_srv",
  "kind": "service_server",
  "source_name": "~/reset",
  "type": "std_srvs/srv/Trigger",
  "callbacks": [
    {
      "id": "reset_srv_request",
      "triggered_by": ["reset_srv/request"],
      "reads": ["reset_srv/request"],
      "writes": ["reset_srv/response"]
    }
  ]
}
```

Action metadata should expose goal, cancel, execute, feedback, and result
roles without making them special in the scheduler:

```json
{
  "id": "follow_path",
  "kind": "action_server",
  "source_name": "~/follow_path",
  "type": "nav2_msgs/action/FollowPath",
  "callbacks": [
    {
      "id": "follow_path_goal",
      "triggered_by": ["follow_path/goal"],
      "reads": ["follow_path/goal"]
    },
    {
      "id": "follow_path_execute",
      "triggered_by": ["follow_path/execute"],
      "reads": ["follow_path/goal"],
      "publishes": ["follow_path/feedback"],
      "writes": ["follow_path/result"]
    }
  ]
}
```

`nros check` treats these as named entities and callbacks like pub/sub/timer:
they need interface bindings, optional manifest links, callback-group
membership, and `SchedContext` bindings when schedulable.

### Launch Cooperation

Launch files and the ROS launch manifest describe the intended system graph.
Source metadata describes what each component can actually create. The build
must compare both.

This follows standard ROS 2 ownership:

- launch files reference implementations by `package` + `executable`;
- one implementation can be launched multiple times with different names,
  namespaces, remaps, and parameters;
- source code commonly uses private names such as `"~/cmd"` as placeholders;
- remaps are static for the node lifetime, so nano-ros can resolve them at
  plan time.

Therefore, component `nros.toml` describes the reusable implementation, not a
node instance. Launch files own node instances. System `nros.toml` may bind
defaults for all instances of a component or override a specific launched
instance.

Inputs:

| Input                      | Owner               | Role                                                               |
|----------------------------|---------------------|--------------------------------------------------------------------|
| Launch files               | System integrator   | Node instances, namespaces, remaps, parameters, composition        |
| play_launch `record.json`  | Generated           | Frozen launch evaluation and scope table                           |
| ROS launch manifests YAML  | System/package team | Topic/service/action contracts, QoS, rates, paths, external edges  |
| Source metadata JSON       | Generated           | Actual nodes/entities/callback IDs produced by component source    |
| `nros.toml` + config       | Package/deployer    | Component linkage, callback groups, `SchedContext`, RTOS policy    |
| `nros-plan.json`           | Generated           | Checked, normalized build IR consumed by generated package/build.rs |

Check rules:

- every launched node/composable node maps to exactly one exported component;
- source node IDs and launch node names match after namespace/remap rules;
- source metadata records unresolved source names (`"~/cmd"`, `"odom"`,
  `"/tf"`); only plan normalization applies launch namespace and remap rules;
- manifest endpoints map to source publishers/subscriptions after name
  resolution by instance, direction, resolved topic/service/action name, and
  type;
- if multiple source entities can satisfy one manifest endpoint, `nros.toml`
  must provide an explicit endpoint mapping;
- source entities not present in launch/manifest are errors unless explicitly
  marked internal;
- manifest endpoints missing from source metadata are errors;
- every schedulable source callback maps to a callback group and
  `SchedContext`;
- RT policy comes from config overlays, not source metadata.

The ROS launch manifest supplies requirements: rates, QoS, freshness, latency,
drop tolerance, and causal paths. It should not contain RTOS scheduling knobs.
`nros` config supplies the scheduling policy: `SchedContext` class, period,
budget, deadline, task priority, stack, and core. `nros check` verifies the
policy is compatible with the requirements. For example, a `deadline_us = 20000`
binding is invalid for a manifest path requiring `max_latency_ms = 10` unless a
more specific analysis proves the path can still meet the requirement.

This keeps launch files authoritative for orchestration while making source
truth mechanically discoverable.

Parameter resolution follows ROS 2 precedence. `nros plan` resolves source
defaults, parameter YAML files, launch parameters, and command-line launch
arguments into final per-instance `ParamSpec` tables. Generated runtime code
does not decide precedence; it receives already-normalized parameters from
`nros-plan.json`/`nros_generated.rs`.

Example merge:

```text
source metadata:  cmd_pub publishes "~/cmd" as std_msgs/msg/Int32
launch instance:  pkg=control_pkg exec=control_node name=front_control ns=/front
launch remap:     ~/cmd -> /front/control/cmd
ROS manifest:     topic /front/control/cmd has pub [front_control/cmd]
nros plan link:   front_control/cmd_pub -> /front/control/cmd
```

The ROS manifest endpoint name (`cmd`) does not have to equal the source entity
ID (`cmd_pub`). The automatic match is by resolved graph name, type, direction,
and instance. Explicit mappings are only needed for ambiguous cases such as two
publishers with the same topic/type in one node or heavily remapped reusable
components.

Timers and local watchdog callbacks are discovered from source metadata but are
not usually present in ROS launch manifests. They must be assigned to callback
groups and `SchedContext`s through `nros.toml`/system config. Purely local
entities that are intentionally absent from the ROS graph need an explicit
internal-entity allowlist.

Name-resolution rule:

1. Source metadata records the literal source name: private (`~/cmd`),
   relative (`cmd`), or absolute (`/cmd`).
2. Launch applies node name and namespace for each instance.
3. Private names expand relative to the resolved node name/namespace.
4. Launch remap rules apply to the expanded name, following ROS 2 static
   remapping semantics.
5. ROS manifest matching uses the final resolved graph name.

`nros` should not require source entity IDs to match ROS manifest endpoint
names. Entity IDs are stable scheduling/debug handles; manifest endpoint names
are graph-contract handles. They are linked during normalization.

### Generated Orchestration Package

`nros build` should generate a Rust package even when the user workspace
contains C and C++ nodes. Rust remains the system-entry language because it can
own the `no_std` runtime, target features, linker script, generated constants,
and RTOS entry shims in one place.

Generated package shape:

```text
build/
  robot_bringup/
    nros/
      record.json
      nros-plan.json
      metadata/
        control_node.json
      interfaces/
        rust/
        c/
        cpp/
      generated/
        Cargo.toml
        build.rs
        src/main.rs
        config/
          system.toml
          freertos.toml
      target/
        thumbv7m-none-eabi/
          debug/
            robot_bringup.elf
            robot_bringup.bin
            robot_bringup.map
          release/
            robot_bringup.elf
            robot_bringup.bin
            robot_bringup.map
```

`Cargo.toml` depends on nano-ros runtime crates and Rust component crates by
path. C and C++ components are linked as static archives produced by
`build.rs`; the Rust main calls them through generated `extern "C"`
registration thunks.

The generated package should use a dedicated runtime glue crate,
`nros-orchestration`. This crate owns system-level orchestration types and
keeps generated `main.rs` independent from CLI parsing and JSON handling.

Layer split:

```text
nros-node           node/executor/entities
nros-c / nros-cpp   language bindings
nros-orchestration  System, InstanceSpec, tiers, callback binding, C ABI glue
nros-cli            metadata/plan/check/build commands
generated package   typed tables and small registration functions
```

`nros-orchestration` consumes typed specs. It should not parse
`nros-plan.json` on RTOS. `build.rs` reads `nros-plan.json` and emits
`nros_generated.rs`; the JSON can be embedded only for optional diagnostics.

`build.rs` owns mechanical build integration:

- read `nros-plan.json` and deployment config;
- verify component metadata files are fresh enough for the selected source
  packages;
- generate `OUT_DIR/nros_generated.rs` with node table, callback IDs, static
  limits, and registration calls;
- generate shared C/C++ headers with plan IDs and config constants;
- build each C/C++ component package through CMake using the target toolchain
  file and generated include directory;
- print `cargo:rustc-link-search` and `cargo:rustc-link-lib=static=...` for
  C/C++ component archives in plan order;
- print `cargo:rerun-if-changed` for `record.json`, `nros-plan.json`, config
  files, package manifests, and component source manifests.

`build.rs` should not infer policy. It materializes the already-checked plan.
`nros plan` and `nros check` decide what must be built and whether config is
valid; `build.rs` converts that decision into compiler inputs.

For MVP, avoid clever incremental behavior. `nros build` can regenerate source
metadata, `record.json`, `nros-plan.json`, the generated package, and the
collective interface cache on every run. Staleness hashes and partial rebuilds
can come later after the workflow is stable.

`src/main.rs` should stay tiny:

```rust
#![no_std]
#![no_main]

include!(concat!(env!("OUT_DIR"), "/nros_generated.rs"));

#[nros::entry]
fn main() -> ! {
    let mut system = nros_generated::open_system();
    nros_generated::create_tiers(&mut system).unwrap();
    nros_generated::create_sched_contexts(&mut system).unwrap();
    nros_generated::register_instances(&mut system).unwrap();
    nros_generated::bind_callbacks(&mut system).unwrap();
    nros_generated::start(system)
}
```

Generated code should be debuggable: use named tables and small functions
instead of one giant generated function.

```rust
pub const INSTANCES: &[InstanceSpec] = &[...];
pub const SCHED_CONTEXTS: &[SchedContextSpec] = &[...];
pub const CALLBACK_BINDINGS: &[CallbackBindingSpec] = &[...];

pub fn register_front_control(system: &mut System) -> Result<()> { ... }
pub fn register_rear_control(system: &mut System) -> Result<()> { ... }
```

Runtime API sketch:

```rust
pub struct System<'a> { /* shared session, executors, registries */ }

pub struct InstanceSpec<'a> {
    pub plan_id: u32,
    pub component_id: &'a str,
    pub instance_id: &'a str,
    pub node_name: &'a str,
    pub namespace: &'a str,
    pub remaps: &'a [RemapSpec<'a>],
    pub params: &'a [ParamSpec<'a>],
}

pub struct SchedContextSpec<'a> {
    pub id: &'a str,
    pub tier: &'a str,
    pub class: SchedClass,
    pub priority: Priority,
    pub period_us: Option<u32>,
    pub budget_us: Option<u32>,
    pub deadline_us: Option<u32>,
}

impl<'a> System<'a> {
    pub fn add_rust_component<C: nros::Component>(
        &mut self,
        spec: InstanceSpec<'a>,
    ) -> nros::Result<InstanceHandle>;

    pub unsafe fn add_c_component(
        &mut self,
        spec: InstanceSpec<'a>,
        register: CComponentRegister,
    ) -> nros::Result<InstanceHandle>;

    pub fn create_sched_context(
        &mut self,
        spec: SchedContextSpec<'a>,
    ) -> nros::Result<SchedContextHandle>;

    pub fn bind_callback(
        &mut self,
        callback: CallbackRef<'a>,
        sched_context: SchedContextRef<'a>,
    ) -> nros::Result<()>;

    pub fn start(self) -> !;
}
```

Each launch instance gets a fresh `InstanceSpec`, component state allocation,
entity handles, callback handles, and telemetry IDs. Rust components are called
directly. C components and C++ components use the same C ABI registration
thunk shape, with C++ hiding construction behind the thunk.

Target-specific entry differences stay behind `#[nros::entry]` or generated
platform modules:

- native/POSIX: normal `fn main()` for tests and smoke runs;
- FreeRTOS/ThreadX/Zephyr: RTOS startup hook creates tier tasks;
- RTIC: codegen emits compile-time task macros and per-task priorities;
- bare metal: one loop or interrupt-driven task table.

Mixed-language boundary rules:

| Language package | Build input                       | Runtime boundary                                      |
|------------------|-----------------------------------|-------------------------------------------------------|
| Rust             | Cargo path dependency             | `Component` impl called directly                      |
| C                | CMake static library              | `extern "C" nros_ret_t register(...ctx)`              |
| C++              | CMake static library              | generated `extern "C"` thunk around static node factory |

C++ symbols should not cross the Rust boundary directly. The C++ component
macro emits a C ABI registration thunk; inside that thunk it can construct the
C++ node class/factory and register callbacks with the nano-ros executor.

### Interface Generation

Interface bindings should be generated collectively for the selected system,
not independently inside each component package. `nros plan/build` scans the
launch graph, ROS launch manifests, and generated source metadata to discover
all required message/service/action types, then generates one shared interface
cache:

```text
build/<system_pkg>/nros/interfaces/
  rust/
  c/
  cpp/
```

All Rust, C, and C++ components consume this shared cache. This follows the
direction used by `colcon-cargo-ros2`: generated Rust packages and build-time
interface products live under `build/`, not in source packages. Benefits:

- every language binds to the same type-support set;
- component packages stay reusable and do not vendor generated code;
- duplicate per-package bindings are avoided;
- `nros-plan.json` can record the exact interface set used by the firmware.

If a launched node or source metadata references a type that is not available
from the workspace/interface cache, `nros check` fails before firmware build.

## Build Pipeline

### 1. Setup

`nros setup --target <platform>-<rmw>` wraps the Phase 123 source-ship path:

- fetch target-specific submodules;
- install/check Rust target and C cross toolchain;
- prepare workspace-level generated-interface cache;
- make nano-ros discoverable to colcon/CMake/Cargo.

### 2. Plan

`nros plan <bringup_pkg> <launch_file> -- <launch_args...>` uses
`play_launch_parser` and shared play_launch manifest crates to emit raw
`record.json`. The `play_launch` CLI remains the Linux ROS 2 launch
replacement; `nros` should depend on parser/manifest libraries rather than
shelling out to the CLI. Because play_launch is our codebase, shared crates can
be refactored when embedded orchestration needs cleaner APIs.

`record.json` remains the launch freeze artifact. It captures:

- regular nodes;
- composable node containers;
- loaded composable nodes;
- params, remaps, env, ROS args;
- resolved launch variables.

`record.json` and `nros-plan.json` are visible generated artifacts under
`build/<system_pkg>/nros/`, following colcon convention. They are not normally
source-controlled, but users should be able to inspect them, diff them, and
attach them to bug reports.

### 3. Normalize

`nros plan` also emits `nros-plan.json`, a nano-ros build IR derived from:

- `record.json`;
- resolved ROS launch manifest index;
- generated source metadata JSON;
- collective interface set;
- per-node `nros.toml`;
- system `nros.toml`;
- effective compile-time transport options from nano-ros environment/config;
- package discovery from the workspace.

`nros-plan.json` adds what launch files cannot know:

- component entry symbol;
- Rust crate or C/C++ library target;
- component definitions keyed by ROS `package` + `executable`;
- launched instances keyed by fully qualified node name;
- discovered nodes, entities, callback IDs, and callback groups;
- callback effects used to connect manifest paths to schedulable callbacks;
- manifest endpoint-to-source-entity bindings;
- unresolved source names plus launch-resolved graph names;
- message/service/action type-support set;
- tier and `SchedContext` mapping;
- selected deployment config/overlay;
- selected transport backend and effective compile-time transport options;
- resolved RTOS priority/stack/scheduler policy;
- shared-state layout;
- runtime-overridable parameter args;
- generated-main sizing inputs.

The plan should preserve traceability between all layers:

```json
{
  "components": [
    {
      "id": "control_pkg/control_node",
      "package": "control_pkg",
      "executable": "control_node",
      "language": "rust",
      "type": "control_pkg::ControlNode",
      "metadata": "build/robot_bringup/nros/metadata/control_pkg.json"
    }
  ],
  "instances": [
    {
      "id": "/front/front_control",
      "component": "control_pkg/control_node",
      "node_name": "front_control",
      "namespace": "/front",
      "scope_id": 3,
      "remaps": [
        {
          "from": "~/cmd",
          "to": "/front/control/cmd"
        }
      ]
    }
  ],
  "entity_links": [
    {
      "instance": "/front/front_control",
      "source_entity": "cmd_pub",
      "source_name": "~/cmd",
      "resolved_name": "/front/control/cmd",
      "kind": "publisher",
      "type": "std_msgs/msg/Int32",
      "manifest_endpoint": "front_control/cmd",
      "manifest_topic": "/front/control/cmd",
      "match": "auto"
    }
  ],
  "callback_effects": [
    {
      "instance": "/front/front_control",
      "callback": "control_tick",
      "triggered_by": ["control_tick"],
      "reads": ["odom_sub"],
      "publishes": ["cmd_pub"],
      "derived_paths": [
        {
          "manifest_path": "control",
          "input": "/front/odom",
          "output": "/front/control/cmd"
        }
      ]
    }
  ],
  "sched_bindings": [
    {
      "instance": "/front/front_control",
      "callback": "control_tick",
      "callback_group": "control_loop",
      "sched_context": "front_control_loop",
      "tier": "high"
    }
  ]
}
```

This resolved-link section is the debugging contract. It explains how ROS 2
launch instance data, ROS manifest endpoints, source metadata placeholders, and
nano-ros scheduling policy became one RTOS execution plan.

### 4. Check

`nros check` validates:

- every launch node maps to a component package or explicit external process;
- multiple launch instances of the same package/executable map to one
  component definition and distinct plan instances;
- every component has exactly one entry point;
- every component package emits source metadata in host metadata mode;
- every ROS launch manifest endpoint maps to source metadata by resolved name
  and type;
- explicit endpoint mappings exist for ambiguous source-to-manifest matches;
- every source publisher/subscription/service/action is declared by the launch
  manifest or marked internal;
- ROS manifest paths either resolve to callback effects or are explicitly
  accepted as unchecked for MVP;
- every referenced message/service/action type is present in the collective
  interface cache;
- lifecycle nodes are rejected for MVP with a direct unsupported-feature
  diagnostic;
- every callback group maps to a tier;
- every callback/entity binding maps to a valid `SchedContext`;
- active RTOS priority/stack fields exist and are in bounds;
- platform-specific deadline/budget/period values are internally consistent;
- chosen `SchedContext` deadlines/budgets/periods are compatible with ROS
  launch manifest rate, freshness, and path-latency requirements;
- remaps and namespaces resolve before codegen;
- parameter files can be represented by nano-ros parameter APIs;
- tier spin period is compatible with timer periods;
- shared state access either stays single-tier or has a lock strategy.

### 5. Build

`nros build` generates an orchestration crate/package:

- Rust `main.rs` plus platform-specific entry shim;
- `build.rs` for mixed Cargo/CMake/static-archive integration;
- generated source metadata refresh/check step;
- collective interface binding cache for Rust/C/C++;
- one registration call per component;
- one executor per tier;
- one generated `SchedContext` creation/binding table per executor;
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
| `nros metadata`      | generate/refresh component source metadata                   |
| `nros infer-manifest` | scaffold `nros.toml` from generated source metadata          |
| `nros plan`          | run launch freeze and emit `record.json` + `nros-plan.json` |
| `nros check`         | validate manifests, plan, and target constraints            |
| `nros build`         | generate orchestration package and build firmware           |
| `nros run`           | run native/QEMU or flash board                              |
| `nros monitor`       | observe process/device state and logs                       |
| `nros doctor`        | diagnose workspace/toolchain/ROS env                        |

`cargo nano-ros` can remain the developer/internal entry for codegen, but
standard users should see `nros`.

## Manifest Split

Per-node manifest, owned by package author. It declares package linkage and
logical grouping hints; source metadata supplies the actual entity list.
It describes a reusable implementation equivalent to a ROS 2
`package`/`executable`, not a launched node instance.

```toml
schema = "nano-ros/orchestration/v1"
kind = "node"

[component]
name = "control_node"
package = "control_pkg"
executable = "control_node"
language = "rust"
crate = "control_pkg"
type = "control_pkg::ControlNode"
metadata = "generated"

[launch_match]
package = "control_pkg"
executable = "control_node"

[[callback_groups]]
id = "control_loop"
type = "MutuallyExclusive"
default_sched_context = "control_loop"

[[callback_group_members]]
group = "control_loop"
callbacks = ["control_tick", "odom_sub"]

[[endpoint_mappings]]
manifest_endpoint = "control_node/cmd"
source_entity = "cmd_pub"
when = "ambiguous"

[[internal_entities]]
id = "diag_pub"
reason = "board-local diagnostics"

[[callback_effects]]
callback = "control_tick"
reads = ["odom_sub"]
publishes = ["cmd_pub"]
reason = "temporary override until source effect metadata is available"
```

For C and C++ packages, `[component]` points to the CMake target and exported C
ABI thunk instead of a Rust crate/type. The callback names above are checked
against generated source metadata; they are not the source of truth for entity
existence. `endpoint_mappings` should be rare; automatic matching by resolved
name, direction, type, and instance should cover normal ROS 2 remap usage.
`callback_effects` in `nros.toml` are an escape hatch; source API annotations
are preferred because they keep behavior and metadata together.

v0 should require `nros.toml` for every component package. Implicit defaults are
pleasant only when they are correct; during the first orchestration workflow,
missing metadata should fail loudly. A later `nros infer-manifest` or
`nros new component --from-source` can scaffold `nros.toml` from generated
source metadata.

System manifest, owned by deployer:

```toml
schema = "nano-ros/orchestration/v1"
kind = "system"
target_rtos = "freertos"
launch = "launch/system.launch.py"
manifest_dir = "manifests"

[tiers.high]
spin_period_us = 1000

[tiers.high.freertos]
priority = 5
stack_bytes = 8192

startup_order = ["control_node"]

[[component_bindings]]
component = "control_pkg/control_node"
callback_group = "control_loop"
sched_context = "control_loop"
tier = "high"

[[instance_bindings]]
instance = "/front/front_control"
callback_group = "control_loop"
sched_context = "front_control_loop"
tier = "high"
```

`component_bindings` apply to every launch instance of the component.
`instance_bindings` override them for a specific fully qualified node instance.

## Schema Drafts

All nano-ros-authored/generated files should carry explicit schema strings so
tools can reject incompatible versions early.

Source metadata, generated:

```json
{
  "schema": "nano-ros/source-metadata/v1",
  "package": "control_pkg",
  "components": [
    {
      "id": "control_pkg/control_node",
      "language": "rust",
      "rust_type": "control_pkg::ControlNode",
      "nodes": [
        {
          "id": "control_node",
          "entities": [],
          "callbacks": [],
          "callback_effects": []
        }
      ]
    }
  ]
}
```

Component `nros.toml`, authored:

```toml
schema = "nano-ros/component/v1"
kind = "component"

[component]
id = "control_pkg/control_node"
package = "control_pkg"
executable = "control_node"
language = "rust"
crate = "control_pkg"
type = "control_pkg::ControlNode"

[[callback_groups]]
id = "control_loop"
type = "MutuallyExclusive"

[[callback_group_members]]
group = "control_loop"
callbacks = ["control_tick", "odom_sub"]
```

System `nros.toml`, authored:

```toml
schema = "nano-ros/system/v1"
kind = "system"

[system]
name = "robot_bringup"
launch = "launch/system.launch.py"
manifest_dir = "manifests"
target_rtos = "freertos"
target_board = "mps2-an385"
profile = "debug"
overlay = "config/freertos.toml"

[[sched_contexts]]
id = "control_loop"
class = "Edf"
priority = "Critical"
period_us = 10000
budget_us = 800
deadline_us = 10000

[[component_bindings]]
component = "control_pkg/control_node"
callback_group = "control_loop"
sched_context = "control_loop"
tier = "control"
```

`nros-plan.json`, generated:

```json
{
  "schema": "nano-ros/plan/v1",
  "system": {},
  "inputs": {},
  "components": [],
  "instances": [],
  "entity_links": [],
  "callback_effects": [],
  "sched_contexts": [],
  "sched_bindings": [],
  "interfaces": {},
  "artifacts": {}
}
```

The generated plan is intentionally verbose. It is a debug/build IR, not a
hand-authored manifest.

## Real-Time Configuration

Real-time data has three layers. The ROS launch manifest defines requirements:
topic rates, endpoint freshness, jitter, QoS, path latency, drop tolerance, and
external edges. Component source metadata defines attachment points:
callbacks, timers, publishers, subscriptions, services, and actions. nano-ros
config defines scheduling policy: `SchedContext` class, period, budget,
deadline, task priority, stack, core, and transport task settings.

The source package should name real-time attachment points, but deployment
config should own real-time numbers. Deadline, budget, stack, OS priority, and
threading choices vary by board, RTOS, clock source, transport, and safety
case. Keeping them in config lets the same component package deploy to native,
FreeRTOS, Zephyr, ThreadX, NuttX, RTIC, or bare metal without source edits.

Ownership split:

| Owner              | Stable across platforms                                      | Platform-dependent                                      |
|--------------------|--------------------------------------------------------------|---------------------------------------------------------|
| ROS launch manifest | rates, QoS, freshness, path latency, drop tolerance          | none                                                    |
| Node source/API    | node IDs, entity IDs, callback IDs, topic names, message types | none                                                    |
| Source metadata    | generated proof of what source creates                       | none                                                    |
| Per-node manifest  | component linkage, logical callback groups, optional hints    | avoid hard RTOS values                                  |
| System config      | tier names, callback-to-`SchedContext` binding intent         | selected overlay                                        |
| Platform overlay   | deadlines, budgets, periods, OS priorities, stack sizes       | concrete RTOS numbers and policy names                 |
| Generated package  | checked constants and tables                                 | compiled result of selected config, not hand-authored   |

Suggested system config:

```toml
schema = "nano-ros/orchestration/v1"
kind = "system"
target_rtos = "freertos"
target_board = "mps2-an385"
platform_overlay = "config/freertos.toml"

[tiers.control]
executor = "control_exec"
spin_period_us = 1000

[[sched_contexts]]
id = "control_loop"
tier = "control"
class = "Edf"
priority = "Critical"
period_us = 10000
budget_us = 800
deadline_us = 10000
deadline_policy = "SkipLate"

[[bindings]]
node = "control_node"
callback_group = "control_loop"
sched_context = "control_loop"
```

Suggested platform overlay:

```toml
[tiers.control.freertos]
task_priority = 5
stack_bytes = 8192
core = 0

[sched_contexts.control_loop.freertos]
os_priority = 0

[transport.zenoh.freertos]
read_task_priority = 6
lease_task_priority = 6
read_stack_bytes = 5120
lease_stack_bytes = 5120
```

For MVP, transport configuration should stay close to the existing nano-ros
model: compile-time options and environment variables select the backend and
its build-time settings. `nros plan` should snapshot the effective values into
`nros-plan.json` for inspection, but it does not need a complete transport
schema yet. A later revision can lift stable transport knobs into typed
`nros.toml`/overlay fields after the orchestration path is proven.

Rules:

- logical IDs (`tier`, `sched_context`, `callback_group`) are stable and can be
  referenced by launch-derived plans;
- numeric RTOS values live in the selected platform overlay;
- current `[scheduling]` config keys can be treated as the single-tier legacy
  form and normalized into `tiers.default` plus transport task settings;
- `nros check` resolves the overlay before codegen and rejects missing or
  impossible values;
- `nros check` compares selected scheduling policy against ROS manifest
  requirements instead of assuming user-chosen callback bindings are correct;
- generated code calls existing `Executor::create_sched_context(...)` and
  bind APIs; source nodes do not manually choose board-specific deadlines.

Config overlays should live beside the system package, because the deployer
owns board and RTOS policy:

```text
robot_bringup/
  nros.toml
  config/
    freertos.toml
    zephyr.toml
```

The generated package copies the selected overlay into
`build/<system_pkg>/nros/generated/config/` so the build is reproducible and
easy to inspect, but the source of truth remains in the system package.

## Remaining Design Principles

The remaining details should be settled during implementation under one rule:
follow ROS 2 authoring and graph semantics where possible, but freeze dynamic
behavior into build-time artifacts that support RTOS, allocation-free hot
paths, and `no_std` targets.

### Static Sizing

ROS 2 lets nodes and entities appear dynamically. nano-ros freezes the selected
system at build time. `nros-plan.json` should derive static capacities for
nodes, publishers, subscriptions, timers, services, clients, actions,
callbacks, parameters, executor handles, and `SchedContext`s. Normal users
should not hand-tune these limits; generated constants should size static
arrays and arenas. System overlays can later provide explicit headroom when a
platform needs it.

### Internal Entities

ROS 2 nodes often create hidden or local entities such as diagnostics,
parameter events, watchdogs, or logging endpoints. For nano-ros, unmatched
source entities are errors by default because the firmware graph should be
intentional. The escape hatch is an explicit internal-entity allowlist in
component `nros.toml`. Standard hidden ROS-compatible entities can get
predefined policies later.

### Validation Modes

MVP should fail early for structural mismatches and unsupported static
features: missing component, ambiguous endpoint mapping, missing interface
type, unsupported lifecycle node, invalid remap, or invalid RTOS priority.
Checks that need richer analysis, such as missing callback effects for a ROS
manifest path, can be warnings by default with a future `--strict` mode that
promotes them to errors.

### Parameters

Parameter precedence follows ROS 2 launch behavior. `nros plan` resolves
launch substitutions, parameter YAML, launch-provided parameter dictionaries,
and command-line launch arguments into final per-instance static parameter
tables. Generated runtime code injects those tables before callbacks can run.
Details such as undeclared-parameter policy and runtime overrides can follow
the existing nano-ros parameter APIs during implementation.

### Schema Precision

Schemas should stay versioned and conservative. Authored files should be
minimal; generated files can be verbose. MVP tools should reject unknown fields
in authored `nros.toml` files to catch mistakes early. Field-level optionality,
exact enum names, and compatibility rules can evolve while the schema strings
stay explicit.

### Collective Interface Generation

The required interface set is the union of source metadata types, ROS manifest
types, service/action types, and parameter type references. `nros build`
generates one build-local interface cache for Rust, C, and C++ consumers. This
preserves ROS workspace convention while avoiding generated code in reusable
component packages.

### Generated Main Ordering

Generated main should execute the frozen plan in a deterministic order:

```text
open platform/session
create tiers/executors
create sched contexts
instantiate node instances with final params/remaps
bind callbacks to sched contexts
start executors/tasks
```

If a component needs parameters during construction, they are already present
in `InstanceSpec`; callbacks still do not run until executors start.

### Allocation Policy

ROS 2 commonly allocates dynamically. nano-ros should allow allocation only
during build/init-time construction from static arenas where possible. The
spin/callback hot path should be allocation-free. Static capacities come from
the plan; component APIs should make hidden runtime allocation visible during
review and testing.

### Debuggability

ROS 2 users inspect the runtime graph. nano-ros users should be able to inspect
the frozen graph. `record.json`, source metadata, `nros-plan.json`, generated
tables, and plan IDs embedded in telemetry provide the debugging path. A
polished `nros explain` command can come later, but the plan must contain
enough trace data from the start.

## Gap Matrix

### `nros` CLI

| Gap                                                   | Needed                                                                                |
|-------------------------------------------------------|---------------------------------------------------------------------------------------|
| No single user-facing `nros` binary for the full flow | Add `nros-cli` commands that orchestrate setup, plan, check, build, run, monitor      |
| `cargo nano-ros` is codegen-oriented                  | Keep for low-level/codegen; make `nros` the standard UX                               |
| No `plan` command                                     | Add command that calls play_launch parser and writes `record.json` + `nros-plan.json` |
| No `doctor` for workspace state                       | Check sourced ROS env, nano-ros checkout, submodules, toolchains, board vars          |
| No config selection story                             | Add `--config`/`--overlay` flags and record selected files in `nros-plan.json`        |
| No source metadata command                            | Add `nros metadata` or make `nros plan/check` refresh generated metadata automatically |

### nano-ros API/runtime

| Gap                                                                         | Needed                                                                                           |
|-----------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------|
| User examples are `main()` shaped                                           | Add library-shaped `ComponentContext` registration API                                           |
| C API lacks a rclc-like component context                                   | Add `nros_component_context_t`, plan-sized allocation, and C registration metadata               |
| C++ API lacks a rclcpp-like component class shape                           | Add `nros::NodeOptions`, `nros::Node` constructor, and `NROS_COMPONENTS_REGISTER_NODE`           |
| Rust API lacks a rclrs-like component trait                                 | Add `nros::Component`, `NodeOptions`, and `nros::component!` export macro                        |
| C++ global `nros::init()`/`spin_once()` model conflicts with generated main | Provide explicit component API taking executor/context; document globals as simple-app path only |
| Component entity metadata is not emitted                                    | Generate topic/timer/sub/service metadata from macros/build scripts for `nros check`             |
| Anonymous callback creation would bypass orchestration                      | Require stable IDs on all component-mode entity creation APIs                                    |
| Component export can be forgotten                                           | Make package discovery fail clearly when no `nros` component export exists                       |
| `Executor::open_with_session(shared)` not available as safe API             | Add safe shared-session constructor for per-tier executors                                       |
| Timer registration has no sched-context/tier binding variant                | Add `register_timer_on(sc_id, ...)` and C/C++ wrappers                                           |
| Namespaces/remaps are not first-class component inputs                      | Add `ComponentContext` name resolver and remap-aware create helpers                              |
| Runtime params exist but plan-time parameter injection is not unified       | Add boot-time parameter override loader from generated plan/runtime args                         |
| Shared state is ad hoc in hand-written apps                                 | Generate shared-context structs/accessors with tier-aware locking                                |
| Generated code cannot create/bind SC tables from config                     | Add plan-to-`create_sched_context` and handle binding codegen                                    |

### Build and colcon

| Gap                                                     | Needed                                                                                  |
|---------------------------------------------------------|-----------------------------------------------------------------------------------------|
| Phase 78 builds package binaries                        | Add component/library package mode and system/orchestration package mode                |
| Generated orchestration package does not exist          | Add `cargo nano-ros generate-main` or equivalent library called by `nros build`         |
| Generated package lacks a mixed-language `build.rs` contract | Generate Rust package whose `build.rs` drives CMake archives and Cargo linking     |
| Source metadata generation path does not exist           | Add host metadata mode with fake `ComponentContext` for Rust/C/C++ components           |
| Interface cache is not collective across the system      | Generate one build-local Rust/C/C++ interface cache under `build/<system_pkg>/nros/`    |
| Whole-firmware sizing is manual                         | Derive executor/node/callback/param limits from `nros-plan.json`                        |
| Mixed Rust/C/C++ component linking path unclear          | Define generated Cargo+CMake bridge contract and static archive order                   |
| C++ component ABI cannot be linked from Rust safely      | Require generated C ABI registration thunks for C++ static factories                    |

### play_launch integration

| Gap                                                           | Needed                                                                                   |
|---------------------------------------------------------------|------------------------------------------------------------------------------------------|
| `record.json` is process-oriented                             | Add nano-ros normalization layer to produce `nros-plan.json`                             |
| Launch composable containers are Linux runtime concepts       | Map containers/load nodes to static components/tier groups                               |
| ROS launch manifest describes graph but not source reality    | Compare resolved manifest endpoints against generated source metadata                    |
| Python launch freeze can include unsupported runtime behavior | Classify graph-shaping args as build-time only; reject unsupported dynamic cases clearly |
| No stable embedded codegen subcommand                         | Keep codegen in `nros`; call play_launch parser/manifest crates directly                 |

### Manifest/schema

| Gap                                                                     | Needed                                                                                                   |
|-------------------------------------------------------------------------|----------------------------------------------------------------------------------------------------------|
| Existing launch manifest describes graph contracts, not RTOS deployment | Add `nros.toml` schema or extend manifest types with tiers/callback groups/shared state                  |
| Scheduling parameters have no stable owner                             | Put board/RTOS numbers in system config overlays; keep node manifests logical                          |
| Callback groups are source-code concepts in rclcpp                      | Declare grouping policy in `nros.toml`; validate members against generated source metadata               |
| Entity binding can drift from source                                    | Treat source metadata as truth; compare launch manifest and config bindings against it                  |
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
- Source metadata from Rust `ComponentContext` host metadata mode.
- ROS launch manifest endpoint check for publishers/subscriptions.
- Optional callback effects for connecting manifest paths to callbacks.
- Collective interface generation under `build/<system_pkg>/nros/interfaces/`.
- Simple node/system `nros.toml` plus one selected platform config.
- Generated Rust orchestration package with `main.rs` and `build.rs`.
- `nros-orchestration` runtime crate used by generated `main.rs`.
- Generated `main.rs` calls each Rust component registration function.
- User-visible `build/<system_pkg>/nros/nros-plan.json`.
- Cargo-like target/profile artifact layout under `build/<system_pkg>/nros/target/`.
- Effective compile-time transport options captured from current nano-ros env
  vars into `nros-plan.json`.
- One default generated `SchedContext` from config, bound to all callbacks.
- Always regenerate metadata/plan/generated package/interfaces during MVP
  builds; no incremental/staleness tracking yet.
- Existing board runner builds/runs the output.

Explicitly defer:

- C/C++ component ABI;
- CMake archive orchestration for mixed-language packages;
- automatic callback-group inference;
- formal callback-chain inference from source code;
- lifecycle node orchestration;
- typed transport config schema;
- incremental build optimization;
- hardened metadata-mode sandboxing;
- polished `nros explain` diagnostics;
- multi-tier shared session;
- runtime parameter override persistence;
- generated shared state;
- monitor UI parity with play_launch.

This v0 still gives ROS 2 users the important mental model: write node
packages as libraries, compose with launch files, build one RTOS binary.

## MVP Decisions

- Component package metadata should describe language and library shape, not
  deployment target. Keep RTOS/platform in the system package config.
- Standard orchestration accepts library-shaped components only. Hand-written
  `main()` belongs to a separate simple-app path, outside `nros plan/build`.
- Generated main uses a dedicated `nros-orchestration` crate for `System`,
  `InstanceSpec`, `SchedContextSpec`, callback binding, and C ABI component
  registration.
- `build.rs` reads `nros-plan.json` and generates typed Rust tables; RTOS code
  does not parse JSON at runtime.
- Parameter precedence follows ROS 2 convention and is resolved during
  planning into final per-instance parameter tables.
- C++ should reuse the rclcpp component user shape, but nano-ros export
  metadata and C ABI thunks are the embedded source of truth.
- v0 requires `nros.toml` for every component package. Later tooling can infer
  or scaffold it from source metadata.
- RT config overlays live beside the system package and are copied into
  `build/<system_pkg>/nros/generated/config/` for reproducibility.
- `deadline_us` is class-dependent: required for EDF, optional for FIFO,
  paired with period/budget for Sporadic, and replaced by window fields for
  time-triggered scheduling.
- `nros metadata` exists as a debug/CI command, while `nros plan/check/build`
  may refresh metadata automatically.
- Source entities absent from launch manifests are errors by default. An
  explicit internal-entity allowlist can support board telemetry, watchdogs, or
  local diagnostics.
- Callback effects are optional additive metadata. They do not replace the
  rclc/rclcpp/rclrs-shaped creation APIs; they improve `nros check` coverage
  for ROS manifest paths.
- Model local callback effects (`reads`, `publishes`, `writes`) instead of
  asking users to declare global paper-style callback chains.
- Interface bindings are generated collectively under `build/`, not inside
  each package.
- Lifecycle nodes are unsupported in MVP and should fail during `nros check`.
- Transport selection uses existing compile-time options/env vars in MVP;
  `nros-plan.json` records the effective values.
- Artifact layout follows colcon outside and Cargo target/profile conventions
  inside `build/<system_pkg>/nros/`.
