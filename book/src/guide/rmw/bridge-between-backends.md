# Bridge between backends

<!-- type: how-to · level: 3 -->

**Needs first:** [Configure zenoh](configure-zenoh.md),
[Use Cyclone DDS](use-cyclone-dds.md)

Use a bridge when one image must forward topics from one RMW backend to
another: a field gateway that takes zenoh traffic from sensor boards and
republishes it on a DDS network, or a flight controller on XRCE-DDS whose
telemetry a DDS ground station reads.

A bridge is declared, not written. In the bringup's `system.toml`, each
backend session is a `[[domain]]` row and each forwarding is a `[[bridge]]`
row between two of them. The build resolves every topic name to its ROS type
and generates the forwarder; you write no bridge code. Most images use one
backend; a bridge image links two and drains both in each executor spin.

## Before you start

- A workspace with a Rust node package that publishes what you want to
  forward. The shipped
  [`examples/workspaces/bridge-cyclonedds`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces/bridge-cyclonedds)
  is this page's example.
- Both backends provisioned for the board, for example
  `nros setup linux --rmw zenoh` and `nros setup linux --rmw cyclonedds`.
- ROS 2 Humble with `rmw_zenoh_cpp` and `rmw_cyclonedds_cpp`, sourced.

## Steps

1. Declare what the node publishes, in its `Cargo.toml`, so the build can
   resolve a topic name to its type before anything is compiled:

   ```toml
   # src/talker_pkg/Cargo.toml
   [[package.metadata.nros.node.publishes]]
   topic = "/chatter"
   type = "std_msgs/msg/Int32"
   ```

2. Declare the two sessions. `name` is yours, `rmw` is the backend and `id`
   the domain. The node runs on the image's own backend, `[system] rmw`:

   ```toml
   # src/demo_bringup/system.toml
   [[domain]]
   name = "zen"
   rmw = "zenoh"
   id = 0

   [[domain]]
   name = "dds"
   rmw = "cyclonedds"
   id = 5
   ```

3. Declare the bridge. `from` and `to` are `<rmw>:<domain name>`. Leave out
   `topics` to forward every declared topic, or list some; set
   `bidirectional = true` to forward both ways without echoing a sample back:

   ```toml
   [[bridge]]
   name = "gw"
   from = "zenoh:zen"
   to = "cyclonedds:dds"
   topics = ["/chatter"]
   ```

4. Give a session its middleware file when it needs more than its defaults.
   A `[[domain]]` names it with `config`, in the backend's own language:

   ```toml
   [[domain]]
   name = "zen"
   rmw = "zenoh"
   id = 0
   config = "rmw/zenoh-zen.json5"
   ```

   ```json5
   // src/demo_bringup/rmw/zenoh-zen.json5
   {
     mode: "client",
     connect: { endpoints: ["tcp/127.0.0.1:7447"] },
   }
   ```

   A domain with a `config` states no `locator`: the file is its wiring.

5. Build the image. The build writes the bridge's configuration,
   `nros-bridge.toml`, beside the bringup and links both backends:

   ```bash
   nros build linux
   ls src/demo_bringup/nros-bridge.toml
   ```

   ```text
   src/demo_bringup/nros-bridge.toml
   ```

6. Run it. The image's own backend is zenoh, so `nros run` starts the router
   for it first:

   ```bash
   nros run linux
   ```

   ```text
   nros run: starting the zenoh router on tcp/127.0.0.1:7447
   ```

7. Read the forwarded topic on the Cyclone DDS side, on domain 5, in a
   second terminal:

   ```bash
   source /opt/ros/humble/setup.bash
   export RMW_IMPLEMENTATION=rmw_cyclonedds_cpp ROS_DOMAIN_ID=5
   ros2 topic echo /chatter std_msgs/msg/Int32
   ```

   ```text
   data: 7
   ```

8. Forward to XRCE-DDS instead. An XRCE-DDS session has no configuration
   file, so its domain names the agent's address as `locator`:

   ```toml
   [[domain]]
   name = "agent"
   rmw = "xrce"
   id = 0
   locator = "udp/127.0.0.1:8888"

   [[bridge]]
   name = "gw"
   from = "zenoh:zen"
   to = "xrce:agent"
   ```

   Start the agent on that port, as in
   [Use XRCE-DDS](use-xrce-dds.md); the shipped
   [`examples/workspaces/bridge-xrce`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces/bridge-xrce)
   is this form.

9. Change a session's endpoint at run time on a host image, without a
   rebuild. For each `[[node]]` in the generated `nros-bridge.toml`,
   `NROS_BRIDGE_<NODE>_LOCATOR` replaces its locator and
   `NROS_BRIDGE_<NODE>_DOMAIN` its domain, with `<NODE>` the node's name in
   upper case.

## Example

The bringup of `examples/workspaces/bridge-cyclonedds`, which forwards
`/chatter` from zenoh to Cyclone DDS domain 5:

```toml
# src/demo_bringup/system.toml
[system]
name = "bridge_demo"
rmw = "zenoh"
domain_id = 0
default_launch = "system.launch.xml"

[[component]]
pkg = "talker_pkg"
class = "talker_pkg::Talker"
name = "talker"

[[domain]]
name = "zen"
rmw = "zenoh"
id = 0
config = "rmw/zenoh-zen.json5"

[[domain]]
name = "dds"
rmw = "cyclonedds"
id = 5
config = "rmw/cyclonedds-dds.xml"

[[bridge]]
name = "gw"
from = "zenoh:zen"
to = "cyclonedds:dds"

[image.linux]
board = "linux"
```

The two `rmw/` files are in the example. Build and run it with steps 5 to 7.

## A bridge you write yourself

When the forwarding needs logic of its own, open the second session in code
instead. Register each backend before the executor opens, open the primary
session with `Executor::open_with_rmw`, and give each node its backend with
`node_builder(name).rmw(backend)`. A node whose backend is the primary's
shares the primary session; any other opens a session of its own. Set
`NROS_RMW` to the primary's name when the image links several backends, so
the primary does not depend on link order:

```bash
NROS_RMW=zenoh ./my_bridge
```

Each extra session adds its backend's poll and dispatch time to every spin,
because one executor drains the sessions in turn. For a bridge with a
deadline, put the fast side's node in a higher-priority sched task; see
[Pick a scheduling model](../realtime/pick-a-scheduling-model.md).

## If it doesn't work

- **The build cannot resolve a topic.** The node package does not declare
  it under `[[package.metadata.nros.node.publishes]]`, or the `topics` list
  names a topic no node publishes.
- **The build refuses a domain's `config`.** A domain with a file also
  states a `locator`, or no `[[bridge]]` uses the domain. Drop the locator,
  or the unused domain.
- **Nothing arrives on the far side.** The reader is on another domain than
  the egress `[[domain]]`'s `id`, or, for XRCE-DDS, the agent is not running
  at the domain's `locator`.
- **A hand-written bridge fails to open its second session.** Both nodes
  asked for the primary's backend while the primary opened on another one.
  Set `NROS_RMW` to the primary's backend.

## See also

- [`nros-bridge.toml` reference](../../reference/nros-bridge-toml.md): the
  generated file, its keys and its errors.
- [Configure an image](../build/configure-an-image.md#the-middlewares-own-file):
  one middleware file per domain, and per-image overrides.
- [Use XRCE-DDS](use-xrce-dds.md): the agent an XRCE-DDS egress needs.
- [The execution model](../../background/execution-model.md): how one
  executor drains several sessions.

## Design background

- [RFC-0108: executor preemption levels](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0108-executor-preemption-levels.md)
- [RFC-0109: network config layers](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0109-network-config-layers.md)
