# Switch the RMW per image

<!-- type: how-to · level: 2 -->

**Needs first:** [Pick and configure the RMW](../../tutorial/pick-the-rmw.md)

Use this page to build one bringup on more than one RMW backend, or to move
an image from one backend to another.

nano-ros picks the backend when it builds an image. There is no
`RMW_IMPLEMENTATION` to change at run time, as there is in ROS 2: an image is
built for one backend, and a second backend is a second image. The backend
names are `zenoh`, `cyclonedds` and `xrce`, everywhere. A bringup that names
none builds `zenoh`.

## Before you start

- A workspace whose bringup builds, such as the one from
  [Run a talker](../../tutorial/run-a-talker.md).
- Each backend you add provisioned for the board:
  `nros setup <board> --rmw <backend>`.

## Steps

1. Choose the backend every image gets by default, in the bringup's
   `system.toml`:

   ```toml
   [system]
   rmw = "zenoh"
   ```

2. Add an image on another backend. An image's own `rmw` wins over
   `[system] rmw`:

   ```toml
   [image.posix]
   board = "linux"

   [image.posix_cyclonedds]
   board = "linux"
   rmw = "cyclonedds"
   ```

3. Build the image by name. Each image builds into a directory of its own,
   so nothing from the other backend's build is reused:

   ```bash
   nros build posix_cyclonedds
   ```

4. Run it. A host image logs the backend it was built for in its first line:

   ```bash
   nros run posix_cyclonedds --expect 'Publishing:' --timeout 120
   ```

   ```text
   boot config: … rmw cyclonedds (baked)
   Publishing: 'Hello World: 1'
   nros run: matched after
   ```

5. To move every image instead, change `[system] rmw` and drop the
   per-image lines. Then provision the new backend and build again:

   ```bash
   nros setup linux --rmw cyclonedds
   nros build posix
   ```

Node packages need no edit for any of this. A Rust node package names no
backend feature and calls no backend `register()`; the entry `nros build`
generates for the image links the backend and registers it.

### On an RTOS image

The same `rmw` line selects the backend on every platform; the build turns
it into the platform's own setting. On Zephyr it becomes exactly one of
`CONFIG_NROS_RMW_ZENOH`, `CONFIG_NROS_RMW_XRCE` or
`CONFIG_NROS_RMW_CYCLONEDDS`, which you do not write in a `.conf` file
yourself. Give each backend's image the Zephyr fragment that backend needs:

```toml
[image.zephyr_zenoh]
board = "zephyr"
rmw = "zenoh"
os_config = ["prj-zenoh.conf"]

[image.zephyr_cyclonedds]
board = "zephyr"
rmw = "cyclonedds"
os_config = ["prj-cyclonedds.conf"]
```

A Cyclone DDS image also needs multicast networking on the target; see
[Use Cyclone DDS](use-cyclone-dds.md).

### In a CMake project that is not a workspace

A CMake project with no `system.toml` that uses nano-ros through
`find_package(nano_ros)` or `add_subdirectory` picks its backend on the
configure line with `NROS_RMW`. `nros setup presets` writes the project a
`CMakePresets.json` that already carries the SDK root and the board's
toolchain:

```bash
nros setup presets
cmake --preset linux -DNROS_RMW=cyclonedds    # zenoh | xrce | cyclonedds
cmake --build build/linux
```

Clear the build directory, or use one per backend, when you switch back
and forth.

## Example

One bringup, three images on the Linux host, one per backend:

```toml
# src/host_node_bringup/system.toml
[system]
name = "host_node"
rmw = "zenoh"
ros_edition = "humble"
domain_id = 0
default_images = ["posix"]

[[component]]
pkg = "host_node"
class = "host_node::Talker"
name = "host_node"

[image.posix]
board = "linux"

[image.posix_cyclonedds]
board = "linux"
rmw = "cyclonedds"

[image.posix_xrce]
board = "linux"
rmw = "xrce"
locator = "udp/127.0.0.1:2018"
```

```bash
nros setup linux --rmw zenoh
nros setup linux --rmw cyclonedds
nros setup linux --rmw xrce
nros build posix_cyclonedds
nros run posix_xrce --expect 'Publishing:' --timeout 120
```

A bare `nros build` builds the `default_images`; with several images and no
`default_images`, it lists them and stops rather than guess.

## If it doesn't work

- **`nros build` lists the images and stops.** The bringup has several
  images and no `default_images`. Name the image, or add `default_images`.
- **The build cannot find the backend's sources.** That backend is not
  provisioned for the board; run `nros setup <board> --rmw <backend>`.
- **The image still talks the old backend.** You ran a different image than
  you built. `nros run <image> --dry-run` prints the plan; its `image` line
names the image it runs.

## See also

- [Configure zenoh](configure-zenoh.md), [Use Cyclone DDS](use-cyclone-dds.md)
  and [Use XRCE-DDS](use-xrce-dds.md): what each backend needs after you
  select it.
- [Configuration surface](../../reference/configuration-surface.md): where
  `rmw` is set and what it becomes on each build road.
- [CLI reference](../../reference/cli.md#nros-build): `nros build` and its
  image argument.
- [How a build becomes an image](../../background/how-a-build-becomes-an-image.md).

## Design background

- [RFC-0031: RMW selection and lowering](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0031-rmw-selection-and-lowering.md)
- [RFC-0048: CMake / ament consumption](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0048-cmake-ament-consumption.md)
- [RFC-0098: generated leaf build config](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0098-generated-leaf-build-config.md)
