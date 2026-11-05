# Use a serial link

<!-- type: how-to · level: 2 -->

**Needs first:** [Run a talker](../../tutorial/run-a-talker.md)

Use a serial link when a board has a UART but no network, or when a
point-to-point cable to one host is all the network you want. It is the link
the board starters use first, because it needs no IP stack, no address and no
switch.

The image's `locator` makes the link. A locator that starts with `serial/`
tells the build to leave the IP stack out, and tells zenoh-pico to send its
frames over that UART. On the host, `rmw_zenohd` listens on the serial
device and connects the board to every other node. For XRCE-DDS over a UART,
see [Use XRCE-DDS](use-xrce-dds.md); WiFi is not covered yet.

## Before you start

- The board provisioned for zenoh, for example
  `nros setup mps2-an385-baremetal --rmw zenoh`.
- ROS 2 Humble with `ros-humble-rmw-zenoh-cpp`, sourced in each terminal.
  It provides the router.

## Steps

1. Scaffold a project. A board that brings a UART out to the host gets a
   serial image from `nros new`:

   ```bash
   nros new serial_ws --board mps2-an385-baremetal --template talker
   cd serial_ws
   ```

   The image block in `src/serial_node_bringup/system.toml` names the
   board's UART and its rate:

   ```toml
   [image.baremetal]
   board = "mps2-an385-baremetal"
   locator = "serial/UART_0#baudrate=115200"
   ```

2. Run it in the emulator. `nros run` boots QEMU with the UART on a
   terminal device of your host, then starts the router on that device:

   ```bash
   nros run serial_node --expect 'Publishing:'
   ```

   ```text
   nros run: starting the zenoh router on serial//dev/pts/4#baudrate=115200
   Publishing: 'Hello World: 1'
   nros run: matched after
   ```

   The device number differs from run to run. The router also listens on
   `tcp/127.0.0.1:7447`, so ROS 2 nodes on this host can reach the board.

3. Hear it from ROS 2. Leave `nros run serial_node` running in one terminal,
   and in another:

   ```bash
   export RMW_IMPLEMENTATION=rmw_zenoh_cpp
   ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
   ```

   ```text
   data: 'Hello World: 2'
   ```

4. Change the rate, for a board on a real cable. Edit the `baudrate` in the
   locator; `nros run` gives the router the same options. QEMU ignores the
   rate, so it matters only on hardware:

   ```toml
   locator = "serial/UART_0#baudrate=921600"
   ```

   115200 works on any adapter. Many USB serial adapters take 460800, and
   921600 is the top rate of many MCU UARTs. A long or noisy cable gives
   framing errors at high rates.

5. Run the router yourself for a real board. Flash the board as its
   [starter](../../starters/nucleo-f767zi.md#flash-a-real-board) shows. Its
   UART then appears as a USB serial device, `/dev/ttyACM0` for example.
   `rmw_zenohd` reads no command-line arguments, so give it the endpoints in
   `ZENOH_CONFIG_OVERRIDE`: the serial device for the board, and a TCP port
   for ROS 2 nodes on this host:

   ```bash
   source /opt/ros/humble/setup.bash
   ZENOH_CONFIG_OVERRIDE='listen/endpoints=["serial//dev/ttyACM0#baudrate=115200","tcp/127.0.0.1:7447"]' \
       ros2 run rmw_zenoh_cpp rmw_zenohd
   ```

   The router listens and the board dials it. Reset the board, then run the
   `ros2 topic echo` of step 3.

6. On a Zephyr board, set the main thread below the zenoh read task. With
   Zephyr's default main priority, 0, the main thread outranks the read task
   while it creates entities, the UART receive ring overflows, and a board
   joining a busy domain misses the router's replies. Put this in the
   image's Zephyr fragment, the file its `os_config` names:

   ```text
   CONFIG_MAIN_THREAD_PRIORITY=5
   ```

   `nros build` refuses a serial image whose main thread is not below the
   read task, and its message names the value to use. If joins into a busy
   domain still fail, raise `CONFIG_NROS_ZENOH_SERIAL_RX_RING_BYTES` from its
   default of 1024; the ring is static RAM.

## Example

The scaffold's whole bringup, which runs as is in QEMU:

```toml
# src/serial_node_bringup/system.toml
[system]
name = "serial_node"
rmw = "zenoh"
ros_edition = "humble"
domain_id = 0

[[component]]
pkg = "serial_node"
class = "serial_node::Talker"
name = "serial_node"

[image.baremetal]
board = "mps2-an385-baremetal"
locator = "serial/UART_0#baudrate=115200"
```

```bash
source /opt/ros/humble/setup.bash
nros run serial_node
```

## If it doesn't work

- **The session never opens, or times out.** The router listens on a
  different device than the board's UART, or the two use different rates.
  Check the device in the router's endpoint and the `baudrate` on both
  sides.
- **The board runs but ROS 2 hears nothing.** The router has no TCP endpoint
  for host nodes (add `tcp/127.0.0.1:7447` as in step 5), the two sides use
  different `domain_id` values, or `ros2 topic echo` is missing
  `--qos-reliability best_effort`.
- **Nothing arrives on hardware, but QEMU works.** The UART may also carry
  the board's debug console. Give zenoh a UART of its own, or turn the
  console off.
- **`nros run` stops and names the router.** It could not find
  `rmw_zenohd`. Source ROS 2, or set `NROS_RMW_ZENOHD` to the binary.

## See also

- [Configure zenoh](configure-zenoh.md): network locators, a router on
  another host, and the session file.
- [Tune throughput and latency](tune-throughput-and-latency.md#set-the-wire-buffers):
  the buffers a serial image gets and how to change them.
- [Environment variables](../../reference/environment-variables.md#runtime-configuration):
  the locator forms `NROS_LOCATOR` takes.
- [Troubleshoot](../deploy/troubleshoot.md): failures that are not
  specific to serial.

## Design background

- [RFC-0109: network config layers](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0109-network-config-layers.md)
- [RFC-0121: workspace-first project layout](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0121-workspace-first-project-layout.md)
