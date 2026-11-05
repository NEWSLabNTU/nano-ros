# Nucleo-F767ZI (Zephyr, Renode twin)

<!-- type: how-to · level: 1 -->

> **Support level:** [`nucleo-f767zi-zephyr`](../reference/boards/nucleo-f767zi-zephyr.md). Each page states what nano-ros can show for the board (where it runs, how often, and its breadth), generated from the board support registry; [Board Support Levels](../reference/board-support-levels.md) explains the words.

One talker on an ST Nucleo-F767ZI (STM32F767ZI, Cortex-M7) running Zephyr
3.7, talking to ROS 2 over the board's USB serial port. You do not need the
board: [Renode](https://renode.io) emulates it on your machine, and its
USART3, the UART the Nucleo's ST-LINK carries to your USB port, is wired to
your host the way the cable would be. No network is configured anywhere in
this chapter; [ethernet](#ethernet) comes after it.

This chapter assumes a Linux x86_64 host (or the
[container](../guide/build/install-options.md#ros-2-and-the-zenoh-router) the install page
describes) with a C/C++ compiler, ROS 2 Humble with
`ros-humble-rmw-zenoh-cpp`, and the [install](../start/install.md) done, so
`nros` is on your PATH. It uses Zephyr's own tools the way
[the Zephyr chapter](../guide/platforms/zephyr.md) does; read that one for what each
piece is.
WiFi is deferred to the Zephyr 4.4 line, where the Pico W's WiFi driver
exists, so no chapter here opens over WiFi yet.

## Provision the board

Zephyr's host tools first: `west` and the build tools
[Zephyr's Getting Started](https://docs.zephyrproject.org/3.7.0/develop/getting_started/index.html)
lists (`nros setup` installs no system packages). On Debian/Ubuntu:

```bash probe=30 track=installed
sudo apt-get install -y --no-install-recommends git cmake ninja-build gperf \
    device-tree-compiler python3-pip python3-venv xz-utils file wget
pip3 install --user west
export PATH="$HOME/.local/bin:$PATH"
```

Then the board:

```bash probe=32 track=installed
nros setup nucleo-f767zi-zephyr --rmw zenoh
export ZEPHYR_SDK_INSTALL_DIR="$(nros locate tool zephyr-sdk)"
```

That fetches what the board needs into a shared store under `~/.nros`: the
Zephyr SDK, the zenoh-pico sources, the Rust target, and Renode, the
emulator the board runs in here. It installs no router: the router is ROS's
`rmw_zenohd`, from the ROS 2 install above.

A west workspace that pins Zephyr 3.7 and fetches only the modules this
board builds with, `hal_stm32` among them (`import: true` would fetch every
vendor HAL):

```bash probe=34 track=installed
mkdir -p nucleo_ws/manifest && cd nucleo_ws
cat > manifest/west.yml <<'WEST_YML'
manifest:
  remotes:
    - name: zephyrproject-rtos
      url-base: https://github.com/zephyrproject-rtos
  projects:
    - name: zephyr
      remote: zephyrproject-rtos
      revision: v3.7.0
      import:
        name-allowlist: [cmsis, picolibc, mbedtls, tinycrypt, mcuboot, net-tools, hal_stm32]
    # Zephyr's Rust support. The board's setup patches it, so the workspace
    # needs it even for a C talker. Pinned to the revision nano-ros builds
    # with on the 3.7 line (it has no release tags).
    - name: zephyr-lang-rust
      remote: zephyrproject-rtos
      revision: 404fcefdbab0f2711cd342a1fab9d2eda52a1ba9
      path: modules/lang/rust
  self:
    path: manifest
WEST_YML
west init -l manifest
west update --narrow -o=--depth=1
pip3 install --user -r zephyr/scripts/requirements-base.txt
export ZEPHYR_BASE="$PWD/zephyr"
```

Last, give the workspace the patches nano-ros carries for Zephyr 3.7:

```bash probe=36 track=installed
nros setup board nucleo-f767zi-zephyr --zephyr-workspace "$PWD"
```

## Scaffold the talker

```bash probe=40 track=installed
nros new nucleo_ws --board nucleo-f767zi-zephyr --template talker --lang c
cd nucleo_ws
```

That wrote a workspace with two packages: `src/nucleo_node/`, the C talker,
and `src/nucleo_node_bringup/`, whose `system.toml` ends in the
`[image.zephyr]` block naming the board. The Zephyr application itself is
generated under `build/` when you build. The board reaches the zenoh router
on your host through USART3, which Zephyr calls `serial@40004800`, at 115200
baud. Say so with one line at the end of the bringup's `system.toml`:

```bash probe=45 track=installed
echo 'locator = "serial/serial@40004800#baudrate=115200"' >> src/nucleo_node_bringup/system.toml
```

The line lands in the `[image.zephyr]` block, under the
`board = "nucleo-f767zi-zephyr"` line the scaffold wrote. The board's own configuration already turns Zephyr's IP stack off and
gives USART3 to zenoh; its console is USART6.

## Run in the twin

Source ROS 2, then build and run. ROS 2 provides the router, and its message
definitions are what the build generates the talker's `std_msgs` code from.
This first run records the ROS 2 prefix in the workspace, so later runs of it
need no sourced shell:

```bash probe=50 probe-run=1500 track=installed
source /opt/ros/humble/setup.bash
nros run nucleo_node --expect 'Publishing:'
```

```text probe-expect=50
Publishing: 'Hello World: 1'
nros run: matched after
```

`nros run` generates what the build reads (the message code and the board's
build settings, under `build/`), builds the firmware through west (the first
build takes a few minutes; later ones take seconds), boots it in Renode's `nucleo_f767zi`
machine with USART3 on a terminal device of your host, starts `rmw_zenohd`
on that device, and then lets the board run. What it prints is the board's
console, USART6: `*** Booting Zephyr OS build v3.7.0 ***`, then the talker's
`Publishing: 'Hello World: 1'` and on. With `--expect` it stops everything
and exits 0 at the first console line containing that text, and exits 1 if
the board stops first.

Without `--expect` it keeps running until you press `Ctrl-C`. Start it that
way in one terminal:

```bash probe=60 probe-run=300 probe-keep track=installed
nros run nucleo_node
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

The talker publishes best-effort, so the echo asks for best-effort too.
`Ctrl-C` in the first terminal stops Renode and the router together.

## Ethernet

The Nucleo's on-chip ethernet MAC works in Renode: the talker reached a
router over `tcp/` with Zephyr's `eth_stm32_hal` driver in an early
spike. That run needed a host tap device bridged to Renode's virtual switch,
which takes `CAP_NET_ADMIN` (the spike made one inside a rootless network
namespace), and `nros run` does not set one up yet. So this chapter stops at
serial; the ethernet chapter comes with that support.

## Flash a real board

On the bench, `nros flash` programs the same image through Zephyr's own
runner for the board, OpenOCD over the Nucleo's ST-LINK:

```bash
nros flash
```

> **Not replayed.** No probe runs this flash line (a CI runner has no board), so it is written from the board's runner, not measured ([tracking issue](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/archived/2176-unprobed-fvp-sitl-flash-lines.md)).

On the real board USART3 is the ST-LINK's virtual COM port, so the zenoh
link arrives on your host as a USB serial device (`/dev/ttyACM0`, say), and
the router listens there instead of on Renode's terminal device. The
console, USART6, is on the board's pins rather than on the USB cable.

## If it didn't work

- `nros run` names what failed and the log it wrote. `no runner package
  drives` or `the plan's tools name no renode`: run the `nros setup
  nucleo-f767zi-zephyr` step again, which installs Renode.
- A router that would not start usually means `rmw_zenohd` was not found
  (source your ROS 2 setup in that terminal) or another router already holds
  port 7447 on this machine.
- A build that cannot find `hal_stm32`: the west workspace predates the
  `hal_stm32` line in its allowlist; add it and run `west update` again.
- Everything else: [Troubleshooting - First 10 Minutes](../reference/troubleshooting-index.md).

## Where to go next

- **What each Zephyr piece is** - [Zephyr (west module)](../guide/platforms/zephyr.md).
- **The same talker on QEMU, with no Zephyr** - [First Project](../tutorial/run-a-talker.md).
- **Talk to a real ROS 2 system** - [ROS 2 Interoperability](../tutorial/talk-to-ros2.md).
