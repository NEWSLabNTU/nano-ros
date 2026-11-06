# How a build becomes an image

<!-- type: explanation · level: 3 -->

Ask `nros build` what it would do for a host image, and it prints the handoff
without running it:

```text
nros build: host_node_bringup:linux -> board linux (platform posix), driver cargo
cargo update --workspace --manifest-path build/posix-zenoh/linux_entry/Cargo.toml --config build/posix-zenoh/linux_entry/nros-cargo.toml
cargo build --manifest-path build/posix-zenoh/linux_entry/Cargo.toml --config build/posix-zenoh/linux_entry/nros-cargo.toml
```

Three things in those lines are the subject of this page. The image is named
by its bringup and its id, `host_node_bringup:linux`, and it resolves to one
board. The build is done by an ordinary tool, here cargo, on a project you
did not write: `linux_entry` lives under `build/`, generated. And every
setting the compiler sees comes from a generated file beside it,
`nros-cargo.toml`, never from your shell or a `.cargo/config.toml` of your
own.

The rest of this page explains why a nano-ros image is built this way: what
ends up in the binary, what `nros build` generates, which tool it hands off
to, and how nano-ros fits into a project whose build you already own.

## The layers in an image

An image is your nodes and the layers under them, linked into one binary:

```text
Application       your Rust, C or C++ node packages
Board package     hardware and network bring-up, the entry point
Core runtime      executor, nodes, publishers, services, actions,
                  parameters, CDR serialization
RMW backend       zenoh, XRCE-DDS, Cyclone DDS, or your own
Platform          clock, memory, threads, sleep, random, sockets
```

Each layer owns one job. The core runtime is middleware-agnostic: it owns the
executor, the entities and serialization. Your message types are generated
Rust, C or C++ types with CDR serialization, and the RMW backend receives
serialized bytes only. Publishing runs one way through the stack and receiving
runs back:

```text
your message -> CDR serializer -> RMW publish (bytes) -> transport
transport -> RMW receive (bytes) -> CDR deserializer -> your callback
```

That boundary keeps a backend small, and it lets one message type work the
same from all three languages.

The platform layer supplies what a desktop ROS 2 gets from its operating
system: a monotonic clock, memory, threads, sockets. On bare metal it can
also expose a network poll hook, so the runtime advances the network stack
while it waits. The board package joins a platform to real hardware: it
brings up the clock, the network interface and its drivers, and it provides
the entry point that starts the scheduler or the main loop.

Every layer is chosen when you build. The RMW backend is the image's `rmw`,
the platform follows from its board, and nothing is loaded at run time. A
binary may link more than one backend and bridge between them
([Bridge between backends](../guide/rmw/bridge-between-backends.md)).

## When `nros build` drives the build

In a nano-ros workspace you write node packages and a bringup, and no build
file for the image. `nros build` makes one. It is one pipeline for every
board:

1. **Discover.** It walks the `package.xml` files of the workspace and puts
   the packages in dependency order.
2. **Resolve.** It picks the image: the one you named, else the bringup's
   `[system] default_images`. With several images and no default it lists
   them and stops, rather than guess.
3. **Preflight.** It checks that the toolchains, SDKs and sources the board
   needs are present.
4. **Generate.** It writes the message bindings, the resolved system, the
   image's entry and a root build file under `build/`.
5. **Hand off.** It runs the native tool on what it generated, and passes the
   tool's output through untouched.

The tool is chosen by the board, not by the languages in the image:

| The image | The driver |
| --- | --- |
| a Zephyr board | west, on a generated Zephyr application |
| any C or C++ package, any other board | CMake; Rust packages join through Corrosion |
| only Rust packages, any other board | cargo, on the generated entry package |

CMake wins when the languages mix because Corrosion lets CMake consume a cargo
crate, and nothing lets cargo consume a CMake project. A Zephyr image gets no
generated root at all: a Zephyr application already is a complete CMake
project, and its Kconfig files are yours.

Because the last stage is the native tool, a compiler error is the compiler's,
unchanged. `--dry-run` prints the exact commands, and running them yourself
gives the same artifact. No command line carries a `--target`: the board's
triple, its link flags and its emulator runner are in the settings generated
for the image. What the tool leaves behind, and where, is listed in
[What `nros build` produces](../reference/build-outputs.md).

## The entry is the system

The generated entry is the `main()` of the image. It constructs each
component the bringup names, applies the launch file's parameters, and runs
them on the executor. Running the entry is launching the system.

So `system.toml` and the launch file are inputs on your host, and they never
reach the device. The build resolves them and bakes the result into code the
compiler sees: which nodes exist, their names and remappings, the parameter
values, the pool sizes the contract asked for. A launch condition is decided
at build time, and changing the system means building and flashing again.
A value meant to change at run time is a parameter.

The same rule decides where a configuration fact belongs. A fact true of
every copy of a board (its name, platform, target triple, how it boots) is in
the board's package, beside its `nros-board.toml`. A fact true of your
machine or your application (where your SDK lives, which serial port, which
config headers) is in the bringup's `system.toml`, in the image's
`[image.<id>]` block or the `[board_config.<board>]` block beside it. If a
fact would survive publishing the board, it is a board fact. If it names a
path on your disk, it is yours. The keys are in
[system.toml keys](../reference/system-toml.md), and the task is
[Configure an image](../guide/build/configure-an-image.md).

## When your build drives the build

An RTOS project you already have keeps its own build: a vendored Zephyr or
NuttX tree, an STM32Cube or MCUXpresso project, a PX4 firmware. Here nano-ros
is a library your project imports, and your build tool stays in charge. It
never compiles your kernel, never owns your linker script, and never replaces
your build system.

What nano-ros adds to such a build is small and fixed: the client runtime as a
static library behind a C ABI, the generated code for your messages and your
system, and a thin C shim that your build compiles. The shim is source, not a
prebuilt, so it compiles against your own `FreeRTOSConfig.h`, `lwipopts.h` or
`sdkconfig`. A mismatch between your kernel's configuration header and a
prebuilt binary would be an ABI break nothing reports.

Each host build system gets a small shell that makes nano-ros look native to
it:

| Your build | How nano-ros joins it | The shell in the release |
| --- | --- | --- |
| Zephyr (`west`) | a west module | `zephyr/` |
| NuttX (`make` and Kconfig) | an `apps/external/` application | `integrations/nuttx/` |
| FreeRTOS or ThreadX, CMake project | `add_subdirectory(nano-ros)` | `cmake/platform/` |
| PX4 (`make px4_…`) | a module directory, generated ahead of PX4 | `integrations/px4/` |
| PlatformIO | a library with a pre-build codegen script | `library.json`, `integrations/platformio/` |

In every row the split is the same: your build owns compiling and linking,
and nano-ros owns the shell and the code it generates on your host. PX4 and
PlatformIO have no configure hook that could run nano-ros's generator from
inside, so the generator runs before them and leaves a tree they build
natively. The steps are in
[Build inside an existing CMake or vendor tree](../guide/build/existing-cmake-or-vendor-tree.md),
[Zephyr](../guide/platforms/zephyr.md), [NuttX](../guide/platforms/nuttx.md)
and [PX4](../guide/platforms/px4.md).

FreeRTOS has one exception. For the QEMU reference board, nano-ros can build
the FreeRTOS kernel and lwIP for you from a kernel checkout, so the demo works
with no SDK. With a kernel your SDK vendors, do not use that path: your SDK
builds the kernel, picks its port and owns `FreeRTOSConfig.h`, and nano-ros
compiles against your include directories like every other row above.

## Why it has this shape

Three choices shape almost everything else in the stack.

**The RMW layer is nano-ros's own.** ROS 2's `rmw.h` assumes a heap, an OS
scheduler with preemptive threads, a dynamic loader and middleware that
dispatches in its own background threads. None of that holds on a Cortex-M3
with 64 KB of RAM. The nano-ros RMW leaves the I/O buffers to the caller,
replaces wait sets with an explicit call that drives I/O, and binds the
backend when the image links, through a C ABI table. A backend can be written
in C or C++ as well as Rust. [RMW API design](../extending/rmw-design.md) has
the full argument.

**One future type, and no hidden spin.** rclcpp owns the spin loop, and rclc
has no future type at all; neither suits a single-threaded MCU. nano-ros has
one `Promise<T>` that you poll, block on with a timeout, or `.await`, and the
library never runs a spin loop you did not ask for: every blocking helper
takes the executor and drives it. [The client library](client-library.md)
explains it.

**Platform capabilities are separate.** zenoh-pico needs a heap, threads,
sockets and a clock. XRCE-DDS needs a clock and four C functions. Bare metal
has no scheduler. So each capability is its own trait, which a platform may
stub when it lacks it, and each method states whether it blocks, whether it
can fail and what an unsupported call does.
[Platform API design](../extending/platform-api-design.md) lists them.

## Where nano-ros differs from ROS 2

- **`nros build` takes the place of `colcon build`.** It reads the same
  `package.xml` files, but it generates the image's build itself and has no
  install space: what the native tool writes in its build tree is the
  deliverable.
- **The RMW is chosen when you build, not when you run.** There is no
  `RMW_IMPLEMENTATION` plugin loader, because many targets have no dynamic
  loader.
- **No `ros2 launch` on the device.** The launch file is read at build time,
  and the entry is the launched system.
- **The board is part of the build.** An image names a board, and the board
  decides the toolchain, the build tool and how the image runs.

[How nano-ros differs from ROS 2 and micro-ROS](differences-from-ros2.md)
covers the API side.

## Design background

- [RFC-0003](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0003-rtos-integration-pattern.md):
  your build owns compiling and linking; nano-ros owns the shell and the
  host-time code generation; the bringup never reaches the device.
- [RFC-0065](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0065-colcon-like-workspace-builder.md):
  the five stages, and the driver chosen by the board.
- [RFC-0054](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0054-c-header-abi-ssot.md):
  C headers as the one source of the ABI.
- [RFC-0121](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0121-workspace-first-project-layout.md):
  the workspace layout and the generated entry.
- [RFC-0122](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0122-behaviour-bringups-road-witnesses-cadence-backed-ci.md):
  behaviour bringups and the copy-out examples.
