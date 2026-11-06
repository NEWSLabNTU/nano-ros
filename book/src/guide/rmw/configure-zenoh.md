# Configure zenoh

<!-- type: how-to · level: 2 -->

**Needs first:** [Pick and configure the RMW](../../tutorial/pick-the-rmw.md)

Use this page when an image runs on the zenoh backend and you need to say
where its router is: on this machine, on another host, or at the other end of
a serial link.

A nano-ros zenoh image is a zenoh-pico **client**. It dials one router and
reaches every other node, nano-ros or ROS 2, through it. The router is the one
ROS 2 ships, `rmw_zenohd` from `rmw_zenoh_cpp`; nano-ros ships none of its
own. The image's **locator** is the address it dials, and it is the only thing
a board needs to find the router.

## Before you start

- A workspace with a zenoh image, such as the one from
  [Run a talker](../../tutorial/run-a-talker.md).
- ROS 2 Humble with `ros-humble-rmw-zenoh-cpp`, sourced in each terminal
  (`source /opt/ros/humble/setup.bash`). It provides `rmw_zenohd`.

## Steps

1. See which locator the image dials. `--dry-run` prints the run plan
   without running anything:

   ```bash
   nros run sensor_node --dry-run
   ```

   The `[link]` table is the image's locator. The scaffold's first image
   reaches the router over the board's serial port:

   ```text
   [link]
   kind = "serial"
   locator = "serial/UART_0#baudrate=115200"
   rmw = "zenoh"
   ```

   A host image (`board = "linux"`) that states no locator dials the zenoh
   default, `tcp/127.0.0.1:7447`.

2. Point the image at a router over the network. Set `locator` in the image's
   block of `src/sensor_node_bringup/system.toml`. Inside QEMU's user
   network, the host is `10.0.2.2`:

   ```toml
   [image.baremetal]
   board = "mps2-an385-baremetal"
   locator = "tcp/10.0.2.2:7447"
   ```

   Run it. `nros run` sees that `10.0.2.2` is this host as the emulator
   reaches it, and starts `rmw_zenohd` on the matching local port first:

   ```bash
   nros run sensor_node --expect 'Publishing:'
   ```

   ```text
   nros run: starting the zenoh router on tcp/127.0.0.1:7447
   Publishing: 'Hello World: 1'
   nros run: matched after
   ```

3. Use a router on another host instead. Name its address in the locator:

   ```toml
   [image.baremetal]
   board = "mps2-an385-baremetal"
   locator = "tcp/192.168.50.1:7447"
   ```

   `nros run` starts no router for an address that is not this machine's,
   and says so:

   ```text
   nros run: starting no router (tcp/192.168.50.1:7447 is not an address of this host, so its router runs elsewhere)
   ```

   On the other host, start the router. It reads no command-line arguments;
   its default configuration listens on port 7447:

   ```bash
   source /opt/ros/humble/setup.bash
   ros2 run rmw_zenoh_cpp rmw_zenohd
   ```

4. Reuse a router that is already running on this machine. Pass its locator
   to `--router`, and `nros run` starts nothing:

   ```bash
   nros run sensor_node --router tcp/127.0.0.1:7447
   ```

   ```text
   nros run: starting no router (the router at tcp/127.0.0.1:7447)
   ```

5. On a host image, change the locator at run time. A host image reads
   `NROS_LOCATOR` when it starts, and the variable wins over the locator
   baked from `system.toml`. Its first log line says which one it used:

   ```bash
   NROS_LOCATOR=tcp/192.168.50.1:7447 nros run host_node
   ```

   ```text
   boot config: domain 0 (baked), locator tcp/192.168.50.1:7447 (env), …
   ```

   An embedded image has no environment: its locator is fixed when it is
   built.

6. For more than one endpoint, TLS, or a user name and password, write the
   session's own configuration file instead of a locator. Create
   `rmw/zenoh.json5` beside `system.toml`. It is the same document
   `rmw_zenoh` reads from `ZENOH_SESSION_CONFIG_URI` on a ROS 2 host:

   ```json5
   // src/sensor_node_bringup/rmw/zenoh.json5
   {
     mode: "client",
     connect: { endpoints: ["tcp/192.168.50.1:7447", "tcp/192.168.50.2:7447"] },
   }
   ```

   Then delete the image's `locator` line: the file is now the session's
   wiring, and `nros build` refuses an image that states both. It also
   refuses a key zenoh-pico cannot take, and names the key, rather than
   dropping it. Because the file names routers elsewhere, run with
   `--router none`.

## Client, not peer

zenoh defines a peer mode in which two devices talk with no router. nano-ros
does not build it: zenoh-pico is compiled without the multicast transport and
scouting that peer mode needs, to save flash. A session asked for peer mode,
by `mode: "peer"` in the file or `NROS_SESSION_MODE=peer`, refuses to open
and logs:

```text
peer mode unsupported: shim built without multicast transport/scouting
```

Two nano-ros boards therefore talk through a router, as two `rmw_zenoh_cpp`
nodes do. This differs from the ROS 2 DDS backends, where nodes discover each
other directly; for that, use [Cyclone DDS](use-cyclone-dds.md).

## Downsample at the router

When a subscriber's contract declares an arrival curve, `nros configure`
writes a router `downsampling` rule beside the resolved model.
`nros router config --model <model>` prints it as the
`ZENOH_CONFIG_OVERRIDE` entry `rmw_zenohd` reads (`--format json5` prints the
rule file itself):

```sh
ZENOH_CONFIG_OVERRIDE="$(nros router config --model build/nros/models/<bringup>/<stem>.yaml)" rmw_zenohd
```

It fails when there is no rule: no subscriber declares an arrival curve, no
image links zenoh, or the model was never configured. `nros router` is a
command package (`nros_zenoh_tools`), not a built-in verb, so
`nros help --all` lists it and `nros router config --help` prints its flags.

## Example

A talker in QEMU and a ROS 2 listener on the host, sharing one router that
`nros run` starts. The whole bringup:

```toml
# src/sensor_node_bringup/system.toml
[system]
name = "sensor_node"
rmw = "zenoh"
ros_edition = "humble"
domain_id = 0

[[component]]
pkg = "sensor_node"
class = "sensor_node::Talker"
name = "sensor_node"

[image.baremetal]
board = "mps2-an385-baremetal"
locator = "tcp/10.0.2.2:7447"
```

In one terminal:

```bash
source /opt/ros/humble/setup.bash
nros run sensor_node
```

In a second terminal:

```bash
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

```text
data: 'Hello World: 3'
```

## If it doesn't work

- **`nros run` stops with `no zenoh router: rmw_zenoh_cpp/rmw_zenohd was not
  found`.** Source ROS 2 in that terminal, install
  `ros-humble-rmw-zenoh-cpp`, or set `NROS_RMW_ZENOHD` to the `rmw_zenohd`
  binary.
- **`the zenoh router exited on tcp/127.0.0.1:7447 … is the port taken?`**
  Another router already holds the port. Stop it, or reuse it with
  `--router tcp/127.0.0.1:7447`.
- **The board boots but nothing arrives.** Check the locator with
  `--dry-run`. `10.0.2.2` means the host only from inside QEMU's user
  network; a board on a real network needs the router host's real address.
  Both sides must use the same `domain_id`, and `ros2 topic echo` needs
  `--qos-reliability best_effort` for a best-effort talker.
- **A host image stops at start-up naming `NROS_LOCATOR`.** `ZENOH_LOCATOR`
  and `ZENOH_MODE` are retired names. Use `NROS_LOCATOR` and
  `NROS_SESSION_MODE`.

## See also

- [Use a serial link](use-a-serial-link.md): the `serial/` locator and a
  router on a serial device.
- [Configure an image](../build/configure-an-image.md#the-middlewares-own-file):
  the session file, per-image files and the keys it takes.
- [Environment variables](../../reference/environment-variables.md#runtime-configuration):
  `NROS_LOCATOR`, `NROS_SESSION_MODE` and the TLS variables.
- [CLI reference](../../reference/cli.md#nros-run): every `nros run` flag.
- [Tune throughput and latency](tune-throughput-and-latency.md): zenoh-pico's
  transmit path and buffers.
- [How nano-ros differs from ROS 2](../../background/differences-from-ros2.md).

## Design background

- [RFC-0075: zenoh router provenance and the unstable seam](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0075-zenoh-router-provenance-and-the-unstable-seam.md)
- [RFC-0109: network config layers](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0109-network-config-layers.md)
