# First Project

<!-- type: tutorial · level: 1 -->

> **Support level:** [`mps2-an385-baremetal`](../reference/boards/mps2-an385.md) and [the Linux host](../reference/boards/linux.md). Each page states what nano-ros can show for the board (where it runs, how often, and its breadth), generated from the board support registry; [Board Support Levels](../reference/board-support-levels.md) explains the words.

In this chapter you scaffold a talker for an Arm Cortex-M3 board, run it in
QEMU, and watch its messages arrive in ROS 2. You write no code and edit no
configuration to get there.

**What this assumes:** a Linux host with ROS 2 Humble and its Zenoh
middleware (`ros-humble-rmw-zenoh-cpp`) installed, or the container from
[Installation Details](installation-details.md#ros-2-and-the-zenoh-router),
and the [install](../start/install.md) done, so `nros` is on your PATH.

**What it leaves for later:** the board talks to your machine over its serial
port, the way a real board's first program does. The network comes in the
board's own chapter, [Bare-metal Cortex-M3](../targets/bare-metal.md). WiFi is not
covered yet: it arrives with the Zephyr 4.4 line.

## Provision the board

```bash probe=30 track=installed variant=qemu
nros setup mps2-an385-baremetal --rmw zenoh
```

This fetches the Arm cross-compiler, a QEMU that emulates the MPS2-AN385
board, and the Rust target. Nothing is installed system-wide.

## Scaffold

```bash probe=40 track=installed variant=qemu
nros new sensor_ws --board mps2-an385-baremetal --template talker
cd sensor_ws
```

That wrote two packages:

```text
sensor_ws/
└── src/
    ├── sensor_node/            # your node: package.xml, Cargo.toml, src/lib.rs
    └── sensor_node_bringup/    # which board it runs on; you don't need to open it yet
```

`sensor_node` is the code: a talker that publishes `std_msgs/String` on
`/chatter` once a second. `sensor_node_bringup` says what that code runs on.
The scaffold filled it in, and this chapter never needs you to open it.

This is the shape of
[`examples/mps2-an385-baremetal/rust/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/mps2-an385-baremetal/rust/talker)
in the nano-ros repository, the example kept for reading beside this chapter:
one node package and a bringup with no launch file of its own, because
`nros run` generates it.

## Run

Source ROS 2 first, then build and run. ROS 2 provides the router the board
talks to, and its message definitions are what `nros run` generates the
talker's `std_msgs` bindings from. This first build records the ROS 2
prefix in the workspace, so a later build of it needs no sourced shell
(the router still does):

```bash probe=50 probe-run=1500 track=installed variant=qemu
source /opt/ros/humble/setup.bash
nros run sensor_node --expect 'Publishing:'
```

```text probe-expect=50 variant=qemu
Publishing: 'Hello World: 1'
nros run: matched after
```

`nros run` does everything between your source and a running board: it
builds the firmware (the first build compiles nano-ros for the Cortex-M3 and
takes a few minutes; later builds take seconds), boots it in QEMU with the
board's serial port connected to a terminal device on your host, and starts
the router on that device. The talker's own output is the board's console.
With `--expect`, `nros run` stops everything and exits 0 the moment the
console prints a line containing that text, and exits 1 if the board stops
first, which makes it a one-line check that the whole path works.

Without `--expect` it keeps streaming until you press Ctrl-C, which stops
QEMU and the router together. Start it that way in one terminal:

```bash probe=55 probe-run=300 probe-keep track=installed variant=qemu
nros run sensor_node
```

```text probe-expect=55 variant=qemu
Publishing: 'Hello World: 2'
```

## See it in ROS 2

Leave `nros run` going. In a second terminal:

```bash probe=60 probe-run=120 track=installed variant=qemu
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

```text probe-expect=60 variant=qemu
data: 'Hello World:
```

The talker publishes best-effort, and `ros2 topic echo` defaults to reliable,
which receives nothing from it. That is why the command asks for best-effort.

## Change it

Open `src/sensor_node/src/lib.rs`, change the `"Hello World"` text, and run
`nros run sensor_node` again. It rebuilds what changed and boots the new
image.

## No QEMU, no router: run it on this machine

The same flow runs on your Linux host. Cyclone DDS needs no router, so
nothing else has to be running. The build still generates the messages from
ROS 2, so source it before the first run (later runs of `host_ws` need no
sourced shell):

```bash probe=30 track=installed variant=linux
source /opt/ros/humble/setup.bash
nros setup linux --rmw cyclonedds
nros new host_ws --board linux --template talker --lang cpp --rmw cyclonedds
cd host_ws
nros run host_node --expect 'Publishing:' --timeout 120
```

Only the board in the scaffold line changed. The node code is the same
shape, and so is every command after it.

## If nothing appears

- **`nros run` stops at once and names `AMENT_PREFIX_PATH`:** this
  shell has not sourced ROS 2, and no earlier build of the workspace
  recorded it. Run `source /opt/ros/humble/setup.bash` and run again.
- **`nros run` stops at once and names the router:** it could not find
  the Zenoh router. Source ROS 2 in that shell (and install
  `ros-humble-rmw-zenoh-cpp` if it is missing), or point `NROS_RMW_ZENOHD`
  at an `rmw_zenohd` binary.
- **No `Publishing:` line within a minute of boot:** the router started
  but the board did not reach it. `nros run` names the router's log when
  it stops.
- **`nros run` lists several names and refuses:** you are in a workspace
  with more than one thing to run. Name one of the spellings it prints.
- **`ros2 topic echo` prints nothing:** check the `--qos-reliability
  best_effort` flag and the `RMW_IMPLEMENTATION` line.

[First 10 Minutes](first-10-minutes.md) has the rest.

## Next

- [Anatomy of What You Just Built](anatomy.md): what each package is for,
  and what `nros run` generated.
- [Growing your workspace](../build/growing-your-workspace.md): add a second
  node, another board, or a launch file.
- [Bare-metal Cortex-M3](../targets/bare-metal.md): the same board over its network.

## Design background

The chapter follows [RFC-0116](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0116-adoption-purposes-and-front-doors.md)
D5 and D6 and [RFC-0121](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0121-workspace-first-project-layout.md)
D4 and D5. Its pointer to the copy-out talker follows [RFC-0122](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0122-behaviour-bringups-road-witnesses-cadence-backed-ci.md) (behaviour
bringups, single-node witnesses, and cadence-backed CI) D3.
