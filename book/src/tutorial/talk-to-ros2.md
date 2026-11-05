# ROS 2 Interop

<!-- type: tutorial · level: 2 -->

nano-ros communicates with standard ROS 2 nodes using the
[rmw_zenoh_cpp](https://github.com/ros2/rmw_zenoh) middleware. Both sides
connect to the same zenohd router and exchange CDR-encoded messages with
compatible key expressions.

## Prerequisites

- ROS 2 installed. For the zenoh path use
  [ROS 2 Jazzy](https://docs.ros.org/en/jazzy/Installation.html):
  `sudo apt install ros-jazzy-rmw-zenoh-cpp`. **Humble and Iron ship no
  `rmw_zenoh_cpp` apt package** — on those, either build `rmw_zenoh` from
  source as a colcon overlay, or use the Cyclone DDS backend instead
  (direct `rmw_cyclonedds_cpp` interop, no router at all — see
  [Choosing an RMW](pick-the-rmw.md))
- The zenoh router running: `ros2 run rmw_zenoh_cpp rmw_zenohd`

## Quick Start

Open three terminals:

```bash
# Terminal 1: Start the zenoh router
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false' ros2 run rmw_zenoh_cpp rmw_zenohd
```

```bash
# Terminal 2: Scaffold, build and run a nano-ros talker
nros new talker_ws --board linux --template talker --lang rust --rmw zenoh
cd talker_ws
RUST_LOG=info nros run talker_node   # builds first, then runs it
```

```bash
# Terminal 3: Run a ROS 2 listener
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
export ZENOH_CONFIG_OVERRIDE='mode="client";connect/endpoints=["tcp/127.0.0.1:7447"]'
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

You should see the ROS 2 listener printing messages published by nano-ros:

```
data: 'Hello World: 1'
---
data: 'Hello World: 2'
---
```

## The Other Direction

ROS 2 publishers also work with nano-ros subscribers:

Scaffold a listener node instead, the same node as
[`listener_pkg`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces/rust/src/listener_pkg)
in the Rust language workspace (`nros run pubsub_bringup:linux_listener`
there runs it); it subscribes to `std_msgs/String` on `/chatter`, the demo talker's topic:

```bash
# Terminal 2: a nano-ros listener
nros new listener_ws --board linux --template listener --lang rust --rmw zenoh
cd listener_ws
RUST_LOG=info nros run listener_node

# Terminal 3: ROS 2 talker
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
export ZENOH_CONFIG_OVERRIDE='mode="client";connect/endpoints=["tcp/127.0.0.1:7447"]'
ros2 topic pub /chatter std_msgs/msg/String "{data: 'Hello from ROS 2'}" --qos-reliability best_effort
```

The listener logs each message ROS 2 publishes.

## Discovery

nano-ros publishes ROS 2-compatible liveliness tokens, so standard ROS 2
tools work:

```bash
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
export ZENOH_CONFIG_OVERRIDE='mode="client";connect/endpoints=["tcp/127.0.0.1:7447"]'

ros2 topic list        # Shows /chatter
ros2 topic info /chatter   # Shows publisher/subscriber count
ros2 node list         # Shows nano-ros nodes
```

## Configuration

### Domain ID

Both sides must use the same ROS domain ID. On the **native** (host) build
nano-ros reads `ROS_DOMAIN_ID` from the environment (default: `0`). On
**embedded** targets the domain ID is fixed at compile time — `system.toml`
`[system] domain_id` (or an image's own — on Zephyr the module renders it into
`CONFIG_NROS_DOMAIN_ID`), or a hand-written `config.toml` on a
direct-mode app (see
[Add an image](grow-your-workspace.md#4-add-an-image)):

```bash
ROS_DOMAIN_ID=42 ./build/linux/target/debug/talker   # nano-ros side
ROS_DOMAIN_ID=42 ros2 topic echo /chatter ...  # ROS 2 side
```

### QoS

nano-ros defaults to BEST_EFFORT reliability and VOLATILE durability. When
using ROS 2 subscribers, match the QoS:

```bash
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

Without `--qos-reliability best_effort`, ROS 2 defaults to RELIABLE, which
won't match the nano-ros publisher's BEST_EFFORT QoS.

### rmw_zenoh Client Mode

By default, `rmw_zenoh_cpp` uses peer mode and won't connect to a zenohd
router. Force client mode with:

```bash
export ZENOH_CONFIG_OVERRIDE='mode="client";connect/endpoints=["tcp/127.0.0.1:7447"]'
```

Set this before any `ros2` command.

## Protocol Details

nano-ros speaks the same wire format as `rmw_zenoh_cpp`: the same key
expressions, liveliness tokens, CDR encoding and per-message attachment.
You do not configure any of it. How each piece is encoded is described in
[Internals → Zenoh protocol](../internals/rmw/zenoh-protocol.html).

## ROS 2 Editions

Three editions are supported: `humble` (default;
`TypeHashNotSupported` key expressions, XCDR1), `iron`, and `jazzy` (both
RIHS01 type hashes). Pick the edition matching the ROS 2 install you
interop with — a Humble-edition image against a Jazzy bus (or vice versa)
discovers nothing, because the key expressions differ.

The edition is declared like the RMW, per image, in `system.toml`:
`[image.<id>] ros_edition = "jazzy"` (over `[image_defaults]`, then
`[system] ros_edition`). `nros build <id>` generates the messages for that
edition and builds the image; the
`ros-<edition>` cargo features are what the build lowers it to, not
something you pass. A workspace is scaffolded for one edition —
`nros new talker_ws --board linux --template talker --ros-edition jazzy` —
and a bringup that builds for several declares one image per edition,
as the bringup of the copy-out talker,
[`examples/linux/rust/talker/system.toml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/linux/rust/talker/system.toml),
does (`linux_jazzy`, `linux_xrce_jazzy`, …), then picks one:

```bash
nros build linux_jazzy
```

## Troubleshooting

These are the interop-specific failures. For problems that are not about
the ROS 2 side (build failures, sessions that never open, transport
errors), see [Troubleshooting](../guide/deploy/troubleshoot.md); for a
first run, see
[Troubleshooting — First 10 Minutes](../get-started/first-10-minutes.md).

**Topic not visible in `ros2 topic list`:**
Ensure both nano-ros and ROS 2 use the same domain ID and that
`rmw_zenoh_cpp` is in client mode pointing to the same router. Also check
that both sides use the same ROS 2 edition (see
[ROS 2 Editions](#ros-2-editions)).

**Discovery works but no messages received:**
Check that the QoS matches. Use `--qos-reliability best_effort` on the
ROS 2 side.

**rmw_zenoh not connecting:**
Verify `ZENOH_CONFIG_OVERRIDE` is set. Without it, rmw_zenoh uses peer mode
and won't find the router.

## Next Steps

- [Architecture](../concepts/architecture.md) — understand the layer model
- [Internals → Zenoh protocol](../internals/rmw/zenoh-protocol.html) —
  how the wire format is encoded

## Design background

- [RFC-0122](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0122-behaviour-bringups-road-witnesses-cadence-backed-ci.md): behaviour bringups, single-node witnesses, and cadence-backed CI.
  The copy-out talker and the language workspaces that hold the paired
  examples.
