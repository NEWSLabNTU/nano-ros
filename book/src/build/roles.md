# Role Reference: Node pkgs, Bringup pkgs, and Images

A nano-ros workspace has **two kinds of package you write**, and a third thing
that is not a package at all:

- **Node pkg** — a reusable, board-agnostic node library. Defines what a node *does* (publishers, subscribers, timers, services, actions) and registers itself with the `nros::node!(T)` macro. No `main()`, no board pick, no deploy config. (Previously called a *component package*; renamed to *Node pkg* to match ROS 2 composable-node naming.)
- **Bringup pkg** — pure declarative: owns the launch topology and the images built from it. Contains a `package.xml` and `system.toml`, and optionally `launch/*.launch.xml`, `config/` and `boards/`. **No** `Cargo.toml`, no compiled code. Named `<system>_bringup`.
- **Image** — a row, not a directory. `[image.<id>]` in that `system.toml` names a board, a launch file, and a backend; `nros build <id>` generates the entry package from it, under `build/`, and hands off to cargo / cmake / west / idf.py. The workspace itself has no root build file.

The split exists because a node's logic is portable across boards, but boot +
transport + deploy config is not. One Node pkg is reused across native POSIX,
FreeRTOS, and Zephyr by adding an `[image.*]` row per target — never a package
per target.

> **Every project is a workspace**, including one with a single node: one node
> package plus one bringup package, which `nros new <ws> --board <board>` writes
> together. `system.toml` lives only in a bringup; one beside a `Cargo.toml` or
> `CMakeLists.txt` is refused. See [One node, one bringup](#one-node-one-bringup)
> below.

## Node pkg

A Node pkg is a normal Rust library (or C++ static library) with a few nano-ros-specific knobs:

```
src/talker_pkg/
├── Cargo.toml          # [lib] crate-type = ["rlib", "staticlib"] — Rust-toolchain
│                       # facts only; the node is declared by a [[component]] row
│                       # in the bringup's system.toml
├── package.xml         # ROS 2 package manifest (<exec_depend> etc.)
├── src/
│   └── lib.rs          # impl Component for Talker { … }
│                       # nros::node!(Talker);
└── launch/             # OPTIONAL — per-node launch fragment
    └── talker.launch.xml
```

`src/lib.rs` declares the user class, implements `Node` +
`ExecutableNode` (`init` / `on_callback` / optional `tick`), and
ends with `nros::node!(Talker);` to emit the register trampoline.
Codegen owns the spin loop — your code only describes what the node
*has* and what its callbacks *do*.

Key rules:

- **No `fn main()`.** A Node pkg builds as `rlib + staticlib` and is *linked into* the entry the build generates. Codegen synthesises the spin driver; you never hand-write one.
- **`class` must be a namespace-qualified C++ name.** The old prefix rule (`class` starting with the pkg dir name) is retired — `pkg` on the `[[component]]` row is the identity authority, so upstream namespaces (`autoware::x::Node`) port verbatim. `nros check` still rejects an unqualified `class`.
- **C++ / C analogue:** `nano_ros_auto_add_library(<lib> STATIC <srcs>)` + `nros_components_register_node(<lib> PLUGIN … EXECUTABLE …)` (rclcpp_components keyword parity) + a typed component in the source — C++ a `configure(::rclcpp::Node&)` method, C a `RCLC_C_COMPONENT(StateT, configure_fn)` seam. Same conceptual shape, no Cargo.toml.
- **`package.xml` is mandatory.** Even pure-Rust Node pkgs ship one — `<exec_depend>` lines drive ROS 2 launch discovery when the system runs through `ros2 launch` outside the nano-ros toolchain.

## Bringup pkg

A Bringup pkg is **pure declarative** — it owns the launch topology and the
image table, and contains no compiled code:

```
src/demo_bringup/
├── package.xml          # <name>demo_bringup</name>, <exec_depend> per node
├── system.toml          # [system] + [[component]] + [image.<id>] + [host.<name>] (+ [[domain]]/[[bridge]])
├── launch/
│   └── system.launch.xml   # ROS 2 launch schema, verbatim
├── boards/                 # optional — per-board RTOS overlays (prj.conf, *.overlay, sdkconfig.defaults)
└── config/                 # optional — params.yaml, etc.
```

No `Cargo.toml`, no `CMakeLists.txt`, no `src/`. Naming convention
`<system>_bringup` (alias `<system>_launch`), matching nav2 / Autoware /
turtlebot3. `[image.*]` lives here, so it is the package that gives a workspace
images to build. (A workspace with no bringup at all still builds — every
package in dependency order, each into its own `build/<pkg>/`, which is
colcon's default; there are simply no images.)

`launch/` is optional. A bringup without it gets a launch file generated from
its `[[component]]` rows, written to
`build/nros/launch/<bringup>/launch/system.launch.xml` and resolved exactly as
an authored one. Adding `launch/system.launch.xml` replaces it. A bringup with
neither `launch/` nor a `[[component]]` row is refused.

`launch/*.launch.xml` is the ROS 2 launch schema verbatim — `<launch>`,
`<arg>`, `<node>`, `<param>`, `<remap>`, `<group>`, `<include>`, with
`$(find <pkg>)` / `$(var)` / `$(env)` substitutions. Stock nav2/Autoware
XML pastes in and Just Works; Python `.launch.py` files are resolved by
the build too (the launch resolver executes them).
See [Add a launch file](growing-your-workspace.md#3-add-a-launch-file-when-you-need-one).

## Image

An image is one buildable program: a topology, a board, and a backend. It is a
table row in the Bringup pkg's `system.toml`:

```toml
[image_defaults]           # base every block folds over
rmw = "zenoh"

[image.linux]
board = "linux"

[image.freertos]
board = "mps2-an385-freertos"
panic = "own"

[image.linux_robot1]
board  = "linux"
launch = "multihost.launch.xml"
args   = { host = "robot1" }
```

| Key | Meaning |
|---|---|
| `board` | nano-ros board id, resolved through `packages/boards/board-support.toml` (which supplies the rustc triple, the platform, and the framework's own board string) |
| `launch` | Launch file relative to the Bringup pkg. Absent ⇒ `[system] default_launch` |
| `args` | Launch arguments bound at resolve time — how an image selects one machine out of a multi-host launch tree |
| `rmw` | Backend for this image. Absent ⇒ `[system] rmw` |
| `panic` | Panic policy: `platform` \| `halt` \| `own` |
| `profile` | cargo / CMake build profile name |
| `os_config` | Layer-1 files, in order: Zephyr `prj-*.conf` / `.overlay` fragments, or a board network file (`.toml`). `os_config` is read as its old alias |
| `variant` | Framework build-variant name (Zephyr `prj_<buildtype>.conf`) |
| `features` | Capability axes for this image, over `[system]`'s list |
| `serdes`, `ros_edition` | Serialization provider and ROS edition. Absent ⇒ the `[system]` value |

Folding is per-key: scalars are replaced by the specific block, `args` merges,
`os_config` and `features` concatenate base-then-specific. `[system] default_images
= ["native"]` is what a bare `nros build` builds; without it, `nros build`
lists the declared images and stops.

Build one, all, or ask what would run:

```console
$ nros build linux
$ nros build --all
$ nros build linux --dry-run     # print the handoff, run nothing
```

`[image.<id>]` says what is BUILT; `[host.<name>]` says where nodes RUN —
which machine a node runs on. An image needs no host and a host needs no image
The old `[deploy.<id>]` table conflated the two and is refused by
name, pointing at both.

## The generated entry

The entry package is derived from `(launch, args, board)` and generated: a
workspace contains no entry packages. On the cargo driver it
lands at `<ws>/build/<coordinate>/<image_id>_entry/`, and that package's own
`Cargo.toml` is the cargo root: with no workspace root to walk up to, a
generated entry is simply a package with path dependencies. On
the cmake driver the generated `<ws>/build/<coordinate>/CMakeLists.txt` is the
root, and emits a `nano_ros_add_executable(…)` call per image on that
coordinate. Beside a cargo entry sits `nros-cargo.toml`, the per-image settings
file the build writes — triple, link flags, `[env]`, patch rows — which cargo
reads through `--config`. `nros build` writes all of it before it hands off;
`nros configure` writes it without building, for an IDE or a hand-run cargo.

All of it is build output under `build/`, and none of it is written beside your
source: no root build file, no `.cargo/config.toml` beside a package. The tracked marker
that a directory is a workspace root is `.colcon_workspace`.

Three parts, all derivable, which is why adding a board adds a descriptor
rather than a branch in the emitter:

| part | derived from |
| --- | --- |
| the `no_std` / `no_main` shell | the board's `entry_kind` |
| board boilerplate (`use panic_semihosting as _;`, `esp_app_desc!()`) | the board descriptor's `entry.crate_root_extra` |
| `nros::main!(launch = …, args = …)` | the image |

The generated entry calls `nros::main!` rather than an expanded form on
purpose: the macro reads the launch XML at *expansion* time, so adding a node
to the launch file is picked up by the next compile with nothing regenerated.

### `nros::main!()` forms

```rust,ignore
// RETIRED: `nros::main!(board = X)` is a compile error — an
// out-of-tree board states `[package.metadata.nros.board] type` instead.
nros::main!(launch = "demo_bringup");                   // multi-node (CANONICAL): the bringup's default launch
nros::main!(launch = "demo_bringup:sim.launch.xml");    // multi-node, a named launch file
nros::main!(                                            // multi-host: per-host slice via launch args
    launch = "demo_bringup:multihost.launch.xml",
    args = [("host", "robot1")],
);
// DEPRECATED (expert override): name the resolved model ARTIFACT directly
// instead of the input — skips the freshness contract; bake-time warning.
nros::main!(model = "demo_bringup");
```

The proc-macro reads the launch file at compile time, walks the workspace
pkg-index for each `<node pkg=…>` entry, and expands to a `fn main()` that
delegates to `<Board as BoardEntry>::run(...)`, dispatching one
`<pkg>::register(runtime)?` call per launch row.

The `launch` forms take `<bringup>[:<file>]` against
`launch/<file>.launch.xml` + `system.toml [system] default_launch`; the build
resolves that input into a SystemModel under `<ws>/build/nros/models/<bringup>/`
(a build artifact — never committed; gate `check-no-tracked-models`), and the
macro bakes from it. `launch` and `model` are mutually exclusive.

**Escape hatches, smallest first:** an `[image.*]` key (`panic`, `profile`,
`os_config`, `features`); `before_boot = "<pkg>::<fn>"` for one step before the
runtime starts; the board's `init_hardware` for hardware every image on that
board needs; and, last, `nros materialize <bringup>:<image>`, which copies the generated
entry to `src/<image_id>_entry/` for you to own. See
[Customising an entry](growing-your-workspace.md#customising-an-entry).

**C++ / C analogue:** the generated cmake root emits one
`nano_ros_add_executable(<name> BOARD … BRINGUP … LAUNCH … LANG … TYPED DEPLOY …)`
per image — no `SOURCES`, because the verb generates the translation unit that
carries `main`. `nano_ros_entry(NAME … LANG … BRINGUP … LAUNCH …)` is the
hand-written spelling, which you meet only in an entry you have taken over
with `nros materialize`. `MODEL` on either is the deprecated artifact-naming override,
mirroring the macro. Metadata flows through `${BUILD}/nros-metadata.json`
rather than a sidecar TOML.

## Zephyr images

A Zephyr image is no exception. Its entry is a Zephyr application — the `app`
target `find_package(Zephyr)` creates — and `nros build` generates it under
`build/` like any other entry, then runs `west build` on it. The board comes
from the image's `board` key, a nano-ros board id, which the descriptor it
resolves to turns into the string `west build -b` receives.

What Zephyr adds is authored Kconfig (`prj.conf`, `prj-<rmw>.conf`,
`boards/*.overlay`), which nothing derives. It lives in the Bringup pkg, one
directory per board, and the image names the fragments it adds:

```
src/demo_bringup/boards/native_sim_native_64/
├── prj.conf
└── prj-zenoh.conf      # named by [image.<id>] os_config
```

A change the image keys cannot express goes through the same last resort as on
every other platform: `nros materialize <bringup>:<image>` writes the generated
application into `src/<image_id>_entry/` as a package you own.
[The Zephyr chapter](../targets/zephyr.md#owning-the-application-nros-materialize)
has the details.

**Migrating off a hand-written entry is a deletion.** `nros build` keeps
`src/<image_id>_entry/` when it carries a `Cargo.toml` or `CMakeLists.txt`;
remove the directory and the next build generates its replacement. Move the
board / launch / RMW / panic facts to the `[image.*]` row and the RTOS overlays
to the Bringup pkg's `boards/<board>/` first.

## Workspace shape

```
my_ws/
├── .colcon_workspace   # tracked marker; there is no root Cargo.toml / CMakeLists.txt
└── src/
    ├── talker_pkg/         # Node pkg (lib, nros::node!)
    ├── listener_pkg/       # Node pkg
    └── demo_bringup/       # Bringup pkg (declarative; system.toml carries [image.*])
```

`nros build <image>` writes the generated message crates and the per-image
build settings, generates the entry and its build root under `build/`, and runs
the platform's own tool; there is no step to run before it. `nros plan` takes the sole
bringup when there is one, or you name it: `nros plan demo_bringup`.

`examples/workspaces/{c,cpp,rust,mixed}/` are the four canonical workspaces —
each is exactly this shape, Zephyr images included.

The language workspaces (`rust`, `c`, `cpp`) also hold the paired examples,
one bringup per behaviour: `pubsub_bringup` (talker and listener),
`service_bringup` (service server and client) and `action_bringup` (action
server and client). Each holds one `[image.<platform>_<role>]` row per
platform and role, with a `_<rmw>` suffix for any RMW other than zenoh, so
every role stays its own image. Run one role by naming its bringup and image:

```bash
nros run pubsub_bringup:linux_listener
```

A variant with different code (a callback-style client, an RTIC listener) is
its own node package with its own image row; a configuration (a serial link, a
custom transport) is an image row that states the knob over the same packages.

## One node, one bringup

The smallest project is still this shape, with one node package. The scaffold
writes both packages:

```bash
nros new host_ws --board linux --template talker
```

```text
host_ws/
└── src/
    ├── host_node/              # Node pkg: package.xml, Cargo.toml (a library), src/lib.rs
    └── host_node_bringup/      # Bringup pkg: package.xml and system.toml; no launch/
```

```toml
# src/host_node_bringup/system.toml
[system]
name = "host_node"
rmw = "zenoh"
domain_id = 0

[[component]]
pkg = "host_node"
class = "host_node::Talker"
name = "host_node"

[image.linux]
board = "linux"
```

There is no `src/main.rs` and no selection or board crate in the node's
manifest: the image's generated entry names the board. `nros run host_node`
builds and runs it. The node spelling means "the bringup whose only node is
`host_node`, with its only image"; once the bringup has a second node or a
second image, name `<bringup>:<image>` instead. `--lang c` or `--lang cpp`
writes a C or C++ node package; the bringup is the same.

The copy-out example for each platform and language,
`examples/<platform>/<lang>/talker/`, has this shape too: one node package and
one bringup with no `launch/`, so its launch file is generated. It is the
project to start from; the paired examples are the image rows above.

`nros build` (and `nros configure`) refuse a `system.toml` in a directory that also
holds a `Cargo.toml` or `CMakeLists.txt`, and the message says where to move
it. Projects that own their build and have no `system.toml` (a CMake
`add_subdirectory` consumer, a vendor tree, a Zephyr application you build
with west) are not affected: see
[Build as a CMake subdirectory](cmake-subdirectory.md),
[Vendored tree](vendored-tree.md) and
[A Zephyr App You Build with West](../targets/zephyr-west-app.md).

## Quick reference

| You want… | Use |
|---|---|
| Reusable node logic, board-independent | Node pkg (`nros::node!()`) |
| A runnable program: N nodes on one board | An `[image.<id>]` row + `nros build <id>` |
| A single-node project | `nros new <ws> --board <board> --template <t>`: one Node pkg plus its Bringup pkg |
| Same nodes on multiple boards | One Node pkg set + one `[image.*]` row per board |
| Launch topology + the image table | Bringup pkg (declarative — the one package a workspace must have) |
| A Zephyr image | An `[image.<id>]` row with a Zephyr board; `nros build` generates the application |
| One step before the runtime starts | `[image.<id>] before_boot = "<pkg>::<fn>"` |
| Startup the declarations cannot express | `nros materialize <bringup>:<image>`, then edit `src/<image>_entry/` |
| Board hardware bringup | `Board` trait family (see [porting chapter](../extending/board-trait.md)) |

## Design background

- [RFC-0121: workspace-first project layout](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0121-workspace-first-project-layout.md)
- [RFC-0065: colcon-like workspace builder (images, hosts, generated entries)](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0065-colcon-like-workspace-builder.md)
- [RFC-0098: generated leaf build config](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0098-generated-leaf-build-config.md)
- [RFC-0057: ament-parity component registration](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0057-ament-parity-component-registration.md)
- [RFC-0060: launch toolchain](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0060-launch-toolchain-three-layers.md)
- [RFC-0026: example directory layout](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)
- [RFC-0122](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0122-behaviour-bringups-road-witnesses-cadence-backed-ci.md): behaviour bringups, single-node witnesses, and cadence-backed CI.
  The paired examples as image rows of behaviour bringups, and the single-node
  talker kept per platform and language.
