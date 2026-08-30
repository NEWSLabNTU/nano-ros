# ROS 2 Interop

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
  [Choosing an RMW](../user-guide/rmw-choosing.md))
- The zenoh router running: `ros2 run rmw_zenoh_cpp rmw_zenohd`

## Quick Start

Open three terminals:

```bash
# Terminal 1: Start the zenoh router
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false' ros2 run rmw_zenoh_cpp rmw_zenohd
```

```bash
# Terminal 2: Run the nano-ros talker
nros sync                            # materialize generated/ bindings (once)
cd examples/native/rust/talker
RUST_LOG=info cargo run
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

```bash
# Terminal 2: Run the nano-ros listener instead
cd examples/native/rust/listener
RUST_LOG=info cargo run

# Terminal 3: ROS 2 talker
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
export ZENOH_CONFIG_OVERRIDE='mode="client";connect/endpoints=["tcp/127.0.0.1:7447"]'
ros2 topic pub /chatter std_msgs/msg/String "{data: 'Hello from ROS 2'}" --qos-reliability best_effort
```

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
**embedded** targets the domain ID is fixed at compile time — per-example
`config.toml` / `package.xml` deploy metadata, or `CONFIG_NROS_DOMAIN_ID`
on Zephyr (see [Workflow](../user-guide/workflow.md)):

```bash
ROS_DOMAIN_ID=42 cargo run    # nano-ros side
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

nano-ros uses the same wire format as `rmw_zenoh_cpp`:

- **Data key expression**: `<domain_id>/<topic>/<type>/TypeHashNotSupported`
  (Humble). Example: `0/chatter/std_msgs::msg::dds_::String_/TypeHashNotSupported`
- **Liveliness tokens**: `@ros2_lv/<domain>/<zid>/0/...` for discovery
- **Message encoding**: CDR little-endian with 4-byte encapsulation header
  `[0x00, 0x01, 0x00, 0x00]`
- **RMW attachment**: 33-byte metadata (sequence number, timestamp, GID)
  appended via Zenoh attachment

### ROS 2 Editions

Three editions ship as Cargo features / `--ros-edition` values (RFC-0056):
`ros-humble` (default; `TypeHashNotSupported` key expressions, XCDR1),
`ros-iron`, and `ros-jazzy` (both RIHS01 type hashes). Pick the edition
matching the ROS 2 install you interop with — a Humble-edition image
against a Jazzy bus (or vice versa) discovers nothing, because the key
expressions differ.

## Troubleshooting

**Topic not visible in `ros2 topic list`:**
Ensure both nano-ros and ROS 2 use the same domain ID and that
`rmw_zenoh_cpp` is in client mode pointing to the same router.

**Discovery works but no messages received:**
Check that the QoS matches. Use `--qos-reliability best_effort` on the
ROS 2 side.

**rmw_zenoh not connecting:**
Verify `ZENOH_CONFIG_OVERRIDE` is set. Without it, rmw_zenoh uses peer mode
and won't find the router.

## Next Steps

- [Architecture](../concepts/architecture.md) — understand the layer model
- [RMW Zenoh Protocol](../internals/rmw-zenoh-protocol.md) — full wire
  format reference
