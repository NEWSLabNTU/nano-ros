# What `nros build` Produces

<!-- type: reference · level: 3 -->

`nros build` ends at an **artifact** and stops. It discovers your packages,
resolves the image, checks the toolchain, generates the root build file and the
entry, and then hands off to the native tool — `cargo`, `cmake` or `west`. The last stage is an `exec`, so the artifact is exactly the one that
tool would have produced had you run it yourself.

This page answers the question the build does not: **what are you holding, and
where is it?**

## Running it: `nros run` and `nros flash`

`nros run` starts an image whose board is a host process or an emulator: it
builds the image if it is stale, starts the router the image's link needs,
streams the console and stops everything when it ends (`--expect <text>`
grades the console, `--dry-run --why` prints the plan it would execute). How a
board is launched is stated once, in its descriptor's `[board.run]`, so the
command `nros build` prints as its next step is the one `nros run` executes.

Real silicon is flashed, not run: `nros run` refuses a hardware image and
names `nros flash --monitor`. `nros flash` builds the image if stale, runs the
board's human steps (`[board.flash] prepare`), flashes through the method its
descriptor's `[board.flash]` names (`west`, or a `command` template), and
with `--monitor` watches the console on the same port; `--expect <text>`
grades it as `nros run` does. `nros monitor` opens the console of a board
that is already running. The port comes from `--port`, `NROS_PORT`, the bench
file (`nros bench scan`) or the one attached device that matches the board.

The commands quoted below are **the board tooling's**, reproduced here as a
reference for a board no flash method covers yet.

There is also no install step: `nros build` never runs `cmake --install`, writes
no `install/` prefix and stages nothing. Whatever the native tool wrote in its
own build tree is the deliverable.

## Where it lands

Which tool builds an image is decided by its platform and by whether the
workspace crosses languages:

| platform / shape | driver | the artifact |
| --- | --- | --- |
| Rust-only workspace, any non-Zephyr board | `cargo` | `<ws>/build/<coordinate>/<image>_entry/target/[<triple>/]<profile>/<image>_entry` |
| any workspace containing C or C++ | `cmake` | `<ws>/build/<coordinate>/cmake/<image>_entry` |
| a `zephyr` board | `west` | `<ws>/build/<coordinate>/<bringup>__<image>_west/zephyr/zephyr.{elf,bin,exe}` |

The **coordinate** is the platform and the RMW — `posix-zenoh`,
`freertos-cyclonedds` — plus, for CMake, the board, because CMake pins one
compiler per configure and two boards on one platform would otherwise share a
cache: `posix-zenoh-linux`, `freertos-zenoh-mps2-an385-freertos`.

Everything above is under `build/`, and that is the rule rather than a
coincidence: a workspace has **no root `Cargo.toml` and no root
`CMakeLists.txt`**. It is a directory of packages, like a
colcon workspace, and every generated thing — the entry, the root build
file, the cargo settings, the compiled output — lives in `build/`.

After a successful build `nros build` prints the artifact itself, and the
board's own command that runs or flashes it — never running either:

```text
nros build: next for `c_talker_node_bringup:linux`:
  artifact: /home/you/c_talker_ws/build/posix-zenoh-linux/cmake/linux_entry
  run:
    /home/you/c_talker_ws/build/posix-zenoh-linux/cmake/linux_entry
```

What the `run:` (or `flash`) line is depends on the board: the binary for a
host board, the board's QEMU runner with the provisioned `qemu-system-*` for an
emulated one, `west build -d <dir> -t run` / `west flash -d <dir>` for Zephyr.
`--dry-run` and `--check` build nothing, so they print no such block.

One path the build *also* prints is easy to mistake for the binary:

```text
nros build:   entry → …/build/posix-zenoh/linux_entry
```

That is the generated **entry package** — source that nano-ros wrote for you,
under `build/` because it is build output. The compiled program is elsewhere;
see below.

## cargo

Every image gets its own cargo settings file,
`build/<coordinate>/<entry>/nros-cargo.toml`, and `nros build` points cargo at
it:

```text
cargo build --manifest-path build/posix-zenoh/linux_entry/Cargo.toml \
            --config build/posix-zenoh/linux_entry/nros-cargo.toml
```

That file states the image's `target-dir`, so each image compiles into its
own tree rather than sharing one `target/`. It also carries the board's
triple, linker flags and cross compiler, the resolved pool budgets as
`[env]`, the in-repo `[patch.crates-io]` rows, and the `nros-*` build
profiles. It is generated on every build — read it, never edit it.

The artifact is the generated entry's binary, inside that entry's own target
dir:

```text
<ws>/build/<coordinate>/<image>_entry/target/[<triple>/]<profile-dir>/<image>_entry
```

The triple level appears whenever the board pins one — which it does in the
generated `[build] target`, not on your command line.

A Linux image on Cyclone DDS, for instance, is
`build/posix-cyclonedds/linux_entry/target/debug/linux_entry`.

`<profile-dir>` is `debug` unless the image declares
`[image.<id>] profile = …`, in which case it is that profile's directory
(cargo's rule: a profile builds into the directory of its own name, and `dev`
into `debug`).

**Next step:** on a host board, run it. It is an ordinary executable.

```bash
./build/linux/target/debug/talker
```

For a QEMU board, the generated settings file also carries that board's
`runner`, so `cargo run` through the same `--config` boots the image —
see [Build Profiles](../guide/build/build-profiles.md) for driving cargo yourself.

A multi-process zenoh example also needs a router; that is ROS's `rmw_zenohd`,
not something nano-ros ships — see
[Deployment Workflow](../guide/deploy/deploy-to-production.md) and
[ROS 2 and the Zenoh router](../guide/build/install-options.md#ros-2-and-the-zenoh-router).

## C, C++ and mixed workspaces (cmake)

CMake is chosen whenever the package graph contains anything that is not Rust,
on the host and when cross-compiling alike. The generated root is `<ws>/build/<coordinate>/CMakeLists.txt`,
it is configured into `<ws>/build/<coordinate>/cmake/`, and each image's
executable lands at the top of that directory:

```text
<ws>/build/<coordinate>/cmake/<image>_entry
```

For example, [`examples/workspaces/c`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces/c) builds `[image.linux]` to
`build/posix-zenoh-linux/cmake/linux_entry` and its FreeRTOS image to
`build/freertos-zenoh-mps2-an385-freertos/cmake/freertos_entry`. (Node packages
also build their own executables, under `cmake/pkg/<pkg>/<pkg>`; the *image* is
the one at the top level.)

The file has no extension in either case. On the host it is a host executable;
for a cross board it is a statically linked ELF for that architecture — the
FreeRTOS one above is `ELF 32-bit LSB executable, ARM, EABI5, statically
linked`.

**Next step:**

* **host** — run it directly.
* **QEMU** — the ELF is what `-kernel` takes, with the machine and CPU your
  board's guide names.
* **hardware** — hand the ELF to your flasher (`probe-rs`, OpenOCD, a
  vendor tool). nano-ros has no opinion about which.

## Zephyr (west)

`nros build` resolves your application and its overlays and runs `west build`
from the workspace root with each image's OWN build directory,
`<ws>/build/<coordinate>/<bringup>__<image>_west` (west's default,
`build/` under the cwd, would put every image of a workspace in one tree and
refuse the second). Pass `-- -d <dir>` to choose another. Zephyr writes its image under
`zephyr/` inside it; for `demo_bringup:zephyr` on zenoh:

```text
<ws>/build/zephyr-zenoh/demo_bringup__zephyr_west/zephyr/zephyr.elf   # always
<ws>/build/zephyr-zenoh/demo_bringup__zephyr_west/zephyr/zephyr.exe   # native_sim — a host executable
<ws>/build/zephyr-zenoh/demo_bringup__zephyr_west/zephyr/zephyr.bin   # cross targets — the raw image
<ws>/build/zephyr-zenoh/demo_bringup__zephyr_west/zephyr/zephyr.map   # the link map
```

Which of `.exe` and `.bin` appears is Zephyr's choice, not nano-ros's: a
`native_sim` build produces `zephyr.exe`, a Cortex-M build produces
`zephyr.bin`.

**Next step — Zephyr's own verbs**, against the image's build directory
(`nros build` prints whichever the board supports):

```bash
west build -d build/zephyr-zenoh/demo_bringup__zephyr_west -t run   # native_sim and emulated boards
west flash -d build/zephyr-zenoh/demo_bringup__zephyr_west          # hardware: the board's flash runner
./build/zephyr-zenoh/demo_bringup__zephyr_west/zephyr/zephyr.exe    # native_sim: the same as -t run
```

The `-d` is not optional here: without it `west` uses `build/` under the
current directory, which is not where `nros build` put the image. `west flash`
picks its runner from the board's own `board.cmake`; nano-ros adds nothing to
it.

## Finding it yourself

When in doubt, ask the build what it is about to do. `--dry-run` prints the
stages and the exact command, and performs no I/O:

```bash
nros build <image> --dry-run
```

The tool named in that command owns the output location, and its own
documentation is the authority — which is the whole point of handing off rather
than wrapping.

For the generation side of this — which root file is written, why the entry is
derived rather than hand-written, and what the coordinate directory holds — see
[How a build becomes an image](../background/how-a-build-becomes-an-image.md).

## The generated entry

| Part of the entry | Derived from |
| --- | --- |
| the `no_std` / `no_main` shell | the board's `entry_kind` |
| board boilerplate (`use panic_semihosting as _;`, `esp_app_desc!()`) | the board descriptor's `entry.crate_root_extra` |
| `nros::main!(launch = …, args = …)` | the image |

| `nros::main!` form | Meaning |
| --- | --- |
| `nros::main!(launch = "demo_bringup")` | the bringup's default launch file |
| `nros::main!(launch = "demo_bringup:sim.launch.xml")` | a named launch file |
| `nros::main!(launch = "…:multihost.launch.xml", args = [("host", "robot1")])` | one host's slice, by launch argument |
| `nros::main!(model = "demo_bringup")` | deprecated: names the resolved model instead of its input |
| `nros::main!(board = X)` | retired: a compile error |

On the CMake driver, the generated root holds one
`nano_ros_add_executable(<name> BOARD … BRINGUP … LAUNCH … LANG … TYPED DEPLOY …)`
per image, with no `SOURCES`: the verb generates the translation unit that
carries `main`. `nano_ros_entry(NAME … LANG … BRINGUP … LAUNCH …)` is the
hand-written spelling, in an entry taken over with `nros materialize`.

## Measured footprint

Per `(platform, transport, backend, profile)`, built from the in-tree
examples. RAM is `data + bss`, after `--gc-sections`. The release profile is
cargo's default; the size profile is opt-level `s`, fat LTO and `strip`.

| Platform | Transport | Backend | Profile | text | data | bss | RAM |
| --- | --- | --- | --- | --- | --- | --- | --- |
| mps2-an385-baremetal (Cortex-M3) | ethernet (smoltcp) | zenoh-pico | release | 177.4 KB | 67.0 KB | 91.7 KB | 158.7 KB |
| mps2-an385-baremetal | ethernet | zenoh-pico | size | 158.3 KB | 67.0 KB | 91.7 KB | 158.7 KB |
| mps2-an385-baremetal | serial (no IP stack) | zenoh-pico | release | 128.6 KB | 25.2 KB | 75.8 KB | 101.0 KB |
| mps2-an385-baremetal | serial | zenoh-pico | size, with the size knobs | 116.1 KB | 25.2 KB | 75.8 KB | 101.0 KB |
| stm32f4 (Cortex-M4F) ¹ | ethernet | zenoh-pico | release | 186.9 KB | 13.7 KB | 123.0 KB | 136.7 KB |
| stm32f4 ¹ | ethernet | zenoh-pico | size | 138.1 KB | 13.7 KB | 123.0 KB | 136.7 KB |
| mps2-an385-freertos (lwIP) | ethernet | zenoh-pico | release | 240.6 KB | 10.7 KB | 3.3 MB ² | 3.3 MB |
| mps2-an385-baremetal | serial (custom transport) | XRCE-DDS | size, 24 KB heap, tight XRCE pools ³ | 60.3 KB | 25.2 KB | 8.8 KB | ~34 KB |

¹ Historical: the stm32f4 board crates have left the tree, so nothing in the
tree reproduces these rows.
² lwIP's heap and the FreeRTOS task stacks: configured headroom, not nano-ros.
³ `NROS_XRCE_STREAM_HISTORY=4`, `NROS_XRCE_CUSTOM_TRANSPORT_MTU=512`,
`NROS_XRCE_MAX_SUBSCRIBERS=1`, `NROS_XRCE_MAX_SERVICE_SERVERS=1`,
`NROS_XRCE_MAX_SERVICE_CLIENTS=1`, `NROS_XRCE_SUBSCRIBER_RING_DEPTH=1`,
`NROS_XRCE_BUFFER_SIZE=256`.

| Static heap (`NROS_HEAP_SIZE`, bare metal) | Peak working set | Recommended |
| --- | --- | --- |
| zenoh-pico over TCP | ~16 KB | 24–32 KB |
| zenoh-pico over serial | below TCP | 16–24 KB |
| XRCE-DDS | ~3 KB | ~8 KB |

These are direct-mode figures. A `nros::main!` image needs the board's
default heap, because its executor backing is one large allocation.
