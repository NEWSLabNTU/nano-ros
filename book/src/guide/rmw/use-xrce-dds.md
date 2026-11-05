# Use XRCE-DDS

<!-- type: how-to · level: 2 -->

**Needs first:** [Pick and configure the RMW](../../tutorial/pick-the-rmw.md)

Use XRCE-DDS when the image must be as small as possible, or must join an
existing micro-ROS or DDS system through an agent.

The XRCE-DDS backend is the Micro-XRCE-DDS client, the one micro-ROS uses.
The client does no discovery of its own: it asks a **Micro-XRCE-DDS Agent**
on a host to create DDS entities for it, and the agent carries the data
between the client and the DDS network. ROS 2 nodes on any DDS-based RMW see
those entities. The agent is required: an image whose agent stops loses all
connectivity until it comes back.

## Before you start

- A workspace, such as the host one from
  [Run a talker](../../tutorial/run-a-talker.md#no-qemu-no-router-run-it-on-this-machine).
- ROS 2 Humble, to see the image from ROS 2.

## Steps

1. Provision the backend. `--rmw xrce` installs the client sources and the
   agent at the version nano-ros pins:

   ```bash
   nros setup linux --rmw xrce
   ```

2. Add an XRCE-DDS image and tell it where the agent listens. XRCE-DDS has
   no configuration file; its whole wiring is the image's `locator`. Port
   2018 is the client's default agent port:

   ```toml
   [image.posix_xrce]
   board = "linux"
   rmw = "xrce"
   locator = "udp/127.0.0.1:2018"
   ```

3. Run it. For a UDP locator on this machine, `nros run` starts the agent on
   that port first:

   ```bash
   nros run posix_xrce --expect 'Publishing:' --timeout 120
   ```

   ```text
   nros run: starting the XRCE agent (udp4 -p 2018)
   Publishing: 'Hello World: 1'
   nros run: matched after
   ```

4. See it from ROS 2. The agent creates ordinary DDS entities, so the
   default ROS 2 RMW receives them:

   ```bash
   source /opt/ros/humble/setup.bash
   ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
   ```

   ```text
   data: 'Hello World: 5'
   ```

   Run `nros run posix_xrce` without `--expect` in the first terminal so the
   talker keeps going.

5. Run the agent yourself instead, for example on another host or with your
   own options. Start the pinned agent, and tell `nros run` to start nothing:

   ```bash
   "$(nros locate tool xrce-agent)/bin/MicroXRCEAgent" udp4 -p 2018
   ```

   ```bash
   nros run posix_xrce --router none
   ```

   For an agent on another host, put that host's address in the `locator`.

6. Use a serial link. With a `serial/` locator, `nros run` waits for the
   runner to report the serial device and then starts the agent on it with
   the locator's baud rate (`serial --dev <device> -b 115200`). For a board
   on your desk, start the agent on the board's USB serial device yourself:

   ```bash
   "$(nros locate tool xrce-agent)/bin/MicroXRCEAgent" serial --dev /dev/ttyACM0 -b 115200
   ```

   How a bare-metal board carries XRCE-DDS over its UART is the board's: the
   mps2-an385 example
   [`examples/mps2-an385-baremetal/rust/talker-xrce`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/mps2-an385-baremetal/rust/talker-xrce)
   states `locator = "custom://uart"`, and its build then leaves out the IP
   transports.

7. Size the session for a small target. The client's buffers and entity
   counts are build-time knobs, stated per image:

   ```toml
   [image.posix_xrce.knobs.xrce]
   buffer_size = 256
   stream_history = 4
   ```

   `stream_history` must be at least 4. A received sample larger than
   `buffer_size` is refused on take.

## Example

The host scaffold with a zenoh image and an XRCE-DDS image side by side. The
bringup:

```toml
# src/host_node_bringup/system.toml
[system]
name = "host_node"
rmw = "zenoh"
ros_edition = "humble"
domain_id = 0

[[component]]
pkg = "host_node"
class = "host_node::Talker"
name = "host_node"

[image.posix]
board = "linux"

[image.posix_xrce]
board = "linux"
rmw = "xrce"
locator = "udp/127.0.0.1:2018"
```

```bash
nros run posix_xrce
```

```bash
source /opt/ros/humble/setup.bash
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

## If it doesn't work

- **`no XRCE agent at …`.** The agent is not provisioned. Run
  `nros setup linux --rmw xrce`, or start an agent of your own and pass
  `--router none`.
- **`the agent exited (…) … so the target cannot work`.** Another agent or
  program already holds the port. Stop it, or change the port in the
  locator.
- **Entity creation times out.** The agent is not running or not reachable
  at the locator's address, or `stream_history` is below 4.
- **`ros2 topic echo` shows nothing.** Both sides must use the same
  `domain_id`; the agent joins the domain the client asks for.

## See also

- [Environment variables](../../reference/environment-variables.md#xrce-dds-nros_xrce_):
  every XRCE-DDS knob, its default and its minimum.
- [Per-RMW feature matrix](../../reference/rmw-feature-matrix.md): what
  XRCE-DDS implements compared with zenoh and Cyclone DDS.
- [Support status](../../reference/support-status.md#rmw-backends--versions-and-interop-pairing):
  the pinned client and agent versions.
- [Use a serial link](use-a-serial-link.md) and
  [Bridge between backends](bridge-between-backends.md).

## Design background

- [RFC-0031: RMW selection and lowering](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0031-rmw-selection-and-lowering.md)
- [RFC-0109: network config layers](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0109-network-config-layers.md)
