# First Project

> **Support level:** [`mps2-an385-baremetal`](../reference/boards/mps2-an385.md). Each page states what nano-ros can show for the board (where it runs, how often, and its breadth), generated from the board support registry; [Board Support Levels](../reference/board-support-levels.md) explains the words.

One node on one board, talking to ROS 2 over a serial cable: you scaffold a
talker, run it, and watch its messages arrive in `ros2 topic echo`. The board
is an Arm Cortex-M3 (MPS2-AN385) that QEMU emulates on your machine, so no
hardware is needed, and its serial port is wired to your host the way a USB
serial cable would be. No network is configured anywhere in this chapter.

This chapter assumes a Linux x86_64 host (or the
[container](installation.md#do-i-need-ros-2-installed) the install page
describes) with a C/C++ compiler, ROS 2 Humble with
`ros-humble-rmw-zenoh-cpp`, and the [install](installation.md) done, so
`nros` is on your PATH.
WiFi is deferred to the Zephyr 4.4 line, where the Pico W's WiFi driver
exists, so no chapter here opens over WiFi yet.

## Provision the board

```bash probe=30 track=installed
nros setup mps2-an385-baremetal --rmw zenoh
```

This fetches what the board needs into a shared store under `~/.nros`: the
Arm cross-compiler, the Rust target, and the QEMU that emulates the board.
It installs no router: the router is ROS's `rmw_zenohd`, from the ROS 2
install above.

## Scaffold the talker

```bash probe=40 track=installed
nros new sensor_node --board mps2-an385-baremetal --template talker
cd sensor_node
```

That wrote one package:

```text
sensor_node/
├── system.toml      # what this package runs on: the board, and how it reaches ROS 2
├── Cargo.toml       # the package manifest
├── package.xml      # its ROS name and message dependencies
├── README.md        # these steps, for whoever opens the package next
└── src/
    ├── lib.rs       # the talker node: publishes on /chatter once a second
    └── main.rs      # boots the board and starts the node
```

The board talks to ROS 2 through a zenoh router on your host. Tell it to
reach the router over its first serial port (`UART_0`, at 115200 baud) by
adding one line at the end of `system.toml`:

```bash probe=45 track=installed
echo 'locator = "serial/UART_0#baudrate=115200"' >> system.toml
```

The line lands under the `board = "mps2-an385-baremetal"` line the scaffold
wrote. Leave it out and the board looks for the router over QEMU's network
instead, which is the [network chapter](bare-metal.md).

## Run it

First generate what the build reads: the message code the talker uses and
the board's build settings, all under `build/`. Run it again whenever you
edit `system.toml`:

```bash probe=48 track=installed
nros sync
```

Then build and run:

```bash probe=50 probe-run=1500 track=installed
nros run --expect 'Publishing:'
```

```text probe-expect=50
Publishing: 'Hello World: 1'
nros run: matched after
```

`nros run` does everything between your source and a running board: it
builds the firmware (the first build compiles nano-ros for the Cortex-M3 and
takes a few minutes; later builds take seconds), boots it in QEMU with the
board's serial port connected to a terminal device on your host, and starts
`rmw_zenohd` on that device. The talker's own output, `Publishing: 'Hello
World: 1'` and on, is the board's console. With `--expect`, `nros run` stops
everything and exits 0 the moment the console prints a line containing that
text, and exits 1 if the board stops first, which makes it a one-line check
that the whole path works.

Without `--expect` it keeps running until you press `Ctrl-C`. Start it that
way in one terminal:

```bash probe=60 probe-run=300 probe-keep track=installed
nros run
```

```text probe-expect=60
Publishing: 'Hello World: 2'
```

## See it from ROS 2

In a second terminal, ask ROS 2 for the topic. The router `nros run` started
also listens where a ROS 2 node on this machine looks for one, so nothing
needs to be configured:

```bash probe=70 probe-run=120 track=installed
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

```text probe-expect=70
data: 'Hello World:
```

The talker publishes best-effort, so the echo asks for best-effort too: a
plain `ros2 topic echo` asks for reliable delivery, matches no publisher, and
prints nothing. `Ctrl-C` in the first terminal stops QEMU and the router
together.

## If it didn't work

- `nros run` names what failed and the log it wrote. A router that would not
  start usually means `rmw_zenohd` was not found (source your ROS 2 setup in
  that terminal) or another router already holds port 7447 on this machine.
- No `data:` lines in the second terminal: check `RMW_IMPLEMENTATION` and
  the `--qos-reliability best_effort` flag.
- Everything else: [Troubleshooting — First 10 Minutes](troubleshooting-first-10-min.md).

## Where to go next

- **The same board over a network** — [Bare-metal Cortex-M3 (QEMU)](bare-metal.md)
  connects the talker through QEMU's network instead of the serial port.
- **More than one node, on your Linux host** —
  [Several Nodes on the Linux Host](multi-node-linux.md).
- **Talk to a real ROS 2 system** — [ROS 2 Interoperability](ros2-interop.md).
- **Put it on another board** — [How Integration Works](how-integration-works.md).
