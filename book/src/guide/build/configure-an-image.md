# Configure an image

<!-- type: how-to · level: 3 -->

**Needs first:** [Organize a larger workspace](organize-a-larger-workspace.md)

Configure an image when it needs more than a board: an address on a network,
a router to dial, a per-image build knob, a flash or console override, or a
smaller binary for a constrained board.

Everything here goes in the image's row of the bringup's `system.toml`, or in
a file that row names. The toolchain bakes it in at build time: an embedded
image has no configuration file on the device and parses nothing at run time.
`Cargo.toml` and `CMakeLists.txt` carry language facts only, and nothing a
board implies is written anywhere: the build derives it from the board's
descriptor.

## Building and configuring

`nros build` prepares everything the build needs (generated messages, the
resolved system, the image's build settings) and then runs the host build
tool. A project that stays in west, CMake, NuttX `make` or an IDE runs the
preparation step, `nros configure`, from that tool's own configure step
instead.

## Before you start

- A workspace with a bringup and at least one image, as in
  [Organize a larger workspace](organize-a-larger-workspace.md).

## Steps

### See what the image resolves to

`nros config show` prints the resolved configuration of the bringup, with the
file and table each value came from:

```bash
nros config show
```

```text
# Resolved config for system 'host_node' (bringup pkg: host_node_bringup)
# source: …/src/host_node_bringup/system.toml

[system]
rmw                = zenoh                        # system.toml [system]
domain_id          = 0                            # system.toml [system]
```

Resolution is a fixed ladder, not a merge: a command-line or build flag, then
`[image.<id>]`, then `[system]`, then the board descriptor's `[knobs]`, then
the built-in default. `nros config explain` prints the build-time knob ladder
for a platform.

### Retarget the image

`board` is the one key that has to change. The image's id is the name you
pass to `nros build`:

```toml
[image.mps2-an385-baremetal]
board   = "mps2-an385-baremetal"
locator = "tcp/10.0.2.2:7447"
```

`locator` is the router or agent the image dials. Without it, the image dials
`[system] locator`, then the board's default.

### Give the image an address

On a board where nano-ros owns the network (the reference FreeRTOS, ThreadX
and bare-metal boards), the address is in a board network file the image
names with `os_config`. On Linux, Zephyr and NuttX the operating system owns
the network, and you set the address in its own configuration.

1. Name the file on the image:

   ```toml
   [image.talker]
   board = "mps2-an385-baremetal"
   os_config = ["board-net.toml"]
   ```

2. Write one table per interface, beside `system.toml`. On QEMU's built-in
   network, `host` is enough:

   ```toml
   # board-net.toml
   [eth0]
   network = "qemu"
   host = 10                      # 10.0.2.10
   ```

3. For a network of your own, define it in the same file and state the
   router on the image:

   ```toml
   # board-net-sensor.toml
   [eth0]
   network = "lab"
   address = "192.168.50.20"

   [network.lab]
   subnet  = "192.168.50.0/24"
   gateway = "192.168.50.1"
   ```

   ```toml
   [image.sensor]
   board = "mps2-an385-baremetal"
   os_config = ["board-net-sensor.toml"]
   locator = "tcp/192.168.50.1:7447"
   ```

   Two images on one network each define it, and the definitions must agree.
   A mistake fails the build and names the image: an address outside the
   subnet, two images with one address, or a network no file defines.

4. To run that image under QEMU on a host bridge, say how it joins:

   ```toml
   [image.sensor.run]
   nic = { bridge = "qemu-br" }
   ```

   `nros run` passes the `-nic` value to QEMU. An image on a network other
   than `qemu` with no `nic` is physical only.

### The middleware's own file

How a session finds its peers is the middleware's own configuration, in its
own language: `rmw/zenoh.json5` or `rmw/cyclonedds.xml` beside `system.toml`,
the same documents ROS 2 reads. To ship a different file for one image, name
it:

```toml
[image.qemu]
board = "mps2-an385-freertos"
rmw_config = "rmw/zenoh-qemu.json5"
```

[Configure zenoh](../rmw/configure-zenoh.md) and
[Use Cyclone DDS](../rmw/use-cyclone-dds.md) show the files themselves.

### Per-image build knobs

A build knob one image needs, and its board does not state, goes in
`[image.<id>.knobs.<group>]`:

```toml
[image.linux.knobs.log]
max_level = "warn"           # NROS_LOG_MAX_LEVEL

[image.linux.knobs.executor]
max_cbs = 16                 # NROS_EXECUTOR_MAX_CBS
```

An unknown key, a wrong type or an out-of-range value stops `nros build`,
naming the key. An application's own build variable, one the knob registry
does not hold, goes in `env = { NAME = "value" }` instead.

### Override how a board is flashed and watched

A board's descriptor states its flasher and console. An image overrides any
key of them:

```toml
[image.lab.flash]
command = "pyocd flash -t stm32f767zi {hex}"
prepare = [{ say = "Set BOOT0 to 1, then press RESET", wait_for = "usb:0483:df11" }]

[image.lab.console]
baud = 921600
```

Both tables are checked when `system.toml` is read, and `nros flash` and
`nros monitor` use them.

### Shrink the image for a small board

1. Build with the size profile. Name it on the image, and `nros build`
   passes it to whichever tool it drives:

   ```toml
   [image.mps2-an385-baremetal]
   board   = "mps2-an385-baremetal"
   profile = "nros-minsizerel"
   ```

2. Drop what a brokered client does not need. These are ordinary build
   environment variables, and a value you export wins over the generated one:

   ```bash
   NROS_LINK_IP=0 ZPICO_NO_SMOLTCP=1 \
   NROS_SMOLTCP_MAX_SOCKETS=1 NROS_SMOLTCP_MAX_UDP_SOCKETS=1 \
       nros build mps2-an385-baremetal
   ```

   `NROS_LINK_IP=0` drops zenoh-pico's TCP and UDP links on a serial-only
   image, and `ZPICO_NO_SMOLTCP=1` skips the smoltcp glue on bare metal. A
   knob you always want for a board belongs in that board's descriptor
   `[knobs]`, not on every command line.

3. Keep the heap at the board default for a `nros::main!` image. Its executor
   backing is one large allocation; shrink `NROS_HEAP_SIZE` only on a
   hand-written direct-mode image whose peak you measured.

Moving from ethernet to a serial link is the largest saving. The measured
sizes are in [Build outputs](../../reference/build-outputs.md#measured-footprint).

## Example

A bare-metal image on a lab network, with its own router and a log ceiling:

```toml
# src/sensor_bringup/system.toml
[system]
name = "sensor"
rmw = "zenoh"
domain_id = 0

[[component]]
pkg = "sensor_node"
class = "sensor_node::Sensor"
name = "sensor"

[image.sensor]
board = "mps2-an385-baremetal"
os_config = ["board-net-sensor.toml"]
locator = "tcp/192.168.50.1:7447"

[image.sensor.run]
nic = { bridge = "qemu-br" }

[image.sensor.knobs.log]
max_level = "info"
```

```bash
nros build sensor
```

## A hand-written `no_std` app

An app that writes its own `main()` instead of using `nros::main!()` keeps its
network settings in a `config.toml` beside it, compiled in with
`include_str!`. It has the board network file's tables, plus a `[node]` table
for the session:

```toml
# config.toml
[node]
domain_id = 0
locator = "tcp/10.0.2.2:7447"

[eth0]
network = "qemu"
host = 10
```

```rust
use nros_board_mps2_an385::{Config, run};

fn main() -> ! {
    run(Config::from_toml(include_str!("../config.toml")), |config| {
        let exec = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id);
        // ...
    })
}
```

## If it doesn't work

- **The build names a retired key.** `ip`, `gateway`, `netmask`,
  `transport` and `[image.<id>.net.<if>]` moved to the board network file and
  the locator. The message names the new spelling.
- **A board network file is refused.** The board's network belongs to the
  host OS (`linux`, `freertos-posix`, `threadx-linux`), or the board declares
  no interfaces. Zephyr boards take their address from Kconfig.
- **An image with a zenoh file is refused.** It also states a `locator`. With
  a file, the file is the wiring; remove one of the two.

## See also

- [system.toml keys](../../reference/system-toml.md): every image key, the
  network file, the run, flash and console tables.
- [Environment variables](../../reference/environment-variables.md): the
  build-time and runtime variables, and the buffer knobs.
- [Configuration surface](../../reference/configuration-surface.md): the
  Kconfig symbols and where each decision is made.
- [Retired configuration files](../../migrating/from-older-releases.md#replace-a-retired-configuration-file).

## Design background

- [RFC-0004: configuration and transports](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0004-configuration-and-transports.md)
- [RFC-0098: generated leaf build config](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0098-generated-leaf-build-config.md)
- [RFC-0031: RMW selection and lowering](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0031-rmw-selection-and-lowering.md)
- [RFC-0109: network config layers](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0109-network-config-layers.md)
- [RFC-0103: one owner per fact](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0103-one-owner-per-fact.md)
- [RFC-0120: one resolver for every build road](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0120-nros-configure-one-resolver.md)
