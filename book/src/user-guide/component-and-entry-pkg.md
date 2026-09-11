# Role Reference: Node pkgs, Bringup pkgs, and Images

A nano-ros workspace has **two kinds of package you write**, and a third thing
that is not a package at all:

- **Node pkg** — a reusable, board-agnostic node library. Defines what a node *does* (publishers, subscribers, timers, services, actions) and registers itself with the `nros::node!(T)` macro. No `main()`, no board pick, no deploy config. (Previously called a *component package*; renamed to *Node pkg* to match ROS 2 composable-node naming.)
- **Bringup pkg** — pure declarative: owns the launch topology and the images built from it. Contains a `package.xml`, `system.toml`, `launch/*.launch.xml`, and optional `config/` + `boards/`. **No** `Cargo.toml`, no compiled code. Named `<system>_bringup`.
- **Image** — a row, not a directory. `[image.<id>]` in that `system.toml` names a board, a launch file, and a backend; `nros build <id>` generates the entry package from it, under `build/`, and hands off to cargo / cmake / west / idf.py. The workspace itself has no root build file (RFC-0098 D9).

The split exists because a node's logic is portable across boards, but boot +
transport + deploy config is not. One Node pkg is reused across native POSIX,
FreeRTOS, and Zephyr by adding an `[image.*]` row per target — never a package
per target.

> **Single-package projects** (the copy-out examples under
> `examples/<platform>/<lang>/`) skip both *packages* but keep the declaration:
> one package carries the node *and* `nros::main!()`, and states its board in a
> `system.toml` beside its own `Cargo.toml` — the same file a bringup writes.
> See [Single-package convenience](#single-package-convenience) below.

## Node pkg

A Node pkg is a normal Rust library (or C++ static library) with a few nano-ros-specific knobs:

```
src/talker_pkg/
├── Cargo.toml          # [lib] crate-type = ["rlib", "staticlib"] — Rust-toolchain
│                       # facts only; the node is declared by a [[component]] row
│                       # in the bringup's system.toml
├── package.xml         # ROS 2 package manifest (<exec_depend> etc.)
├── src/
│   └── lib.rs          # impl Node for Talker { … }
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
- **`class` must be a namespace-qualified C++ name.** The old 212.L.4 prefix rule (`class` starting with the pkg dir name) is retired by RFC-0057 — `pkg` on the `[[component]]` row is the identity authority, so upstream namespaces (`autoware::x::Node`) port verbatim. `nros check` still rejects an unqualified `class`.
- **C++ / C analogue:** `nano_ros_auto_add_library(<lib> STATIC <srcs>)` + `nros_components_register_node(<lib> PLUGIN … EXECUTABLE …)` (RFC-0057, rclcpp_components keyword parity) + a typed component in the source — C++ a `configure(::rclcpp::Node&)` method, C a `NROS_C_COMPONENT(StateT, configure_fn)` seam (RFC-0043). Same conceptual shape, no Cargo.toml.
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
turtlebot3. A multi-node workspace needs one: `[image.*]` lives here, so it is
the package that gives a workspace images to build. (A workspace with no
bringup at all still builds — every package in dependency order, each into its
own `build/<pkg>/`, which is colcon's default; there are simply no images.
A single-package project has no bringup package either, but keeps the
`system.toml` — see [below](#single-package-convenience).)

`launch/*.launch.xml` is the ROS 2 launch schema verbatim — `<launch>`,
`<arg>`, `<node>`, `<param>`, `<remap>`, `<group>`, `<include>`, with
`$(find <pkg>)` / `$(var)` / `$(env)` substitutions. Stock nav2/Autoware
XML pastes in and Just Works; Python `.launch.py` files are resolved by
`nros sync` too (the RFC-0060 resolver executes them — see the bringup
tutorial).
See [the workspace bringup tutorial](../getting-started/workspace-bringup.md).

## Image

An image is one buildable program: a topology, a board, and a backend. It is a
table row in the Bringup pkg's `system.toml`:

```toml
[image_defaults]           # base every block folds over (RFC-0065 D5.1)
rmw = "zenoh"

[image.native]
board = "native"

[image.freertos]
board = "mps2-an385-freertos"
panic = "own"

[image.native_robot1]
board  = "native"
launch = "multihost.launch.xml"
args   = { host = "robot1" }
```

| Key | Meaning |
|---|---|
| `board` | nano-ros board id, resolved through `packages/boards/board-support.toml` (which supplies the rustc triple, the platform, and the framework's own board string) |
| `launch` | Launch file relative to the Bringup pkg. Absent ⇒ `[system] default_launch` |
| `args` | Launch arguments bound at resolve time — how an image selects one machine out of a multi-host launch tree |
| `rmw` | Backend for this image. Absent ⇒ `[system] rmw` |
| `panic` | RFC-0077 panic policy: `platform` \| `halt` \| `own` |
| `profile` | cargo / CMake build profile name |
| `conf` | Extra framework config fragments, in order (Zephyr `prj-*.conf`) |
| `variant` | Framework build-variant name (Zephyr `prj_<buildtype>.conf`) |
| `features` | Capability axes for this image, over `[system]`'s list |
| `serdes`, `ros_edition` | Serialization provider and ROS edition. Absent ⇒ the `[system]` value |
| `entry` | The application package — **only** for the framework-owned case below |

Folding is per-key: scalars are replaced by the specific block, `args` merges,
`conf` and `features` concatenate base-then-specific. `[system] default_images
= ["native"]` is what a bare `nros build` builds; without it, `nros build`
lists the declared images and stops.

Build one, all, or ask what would run:

```console
$ nros build native
$ nros build --all
$ nros build native --dry-run     # print the handoff, run nothing
```

`[image.<id>]` is a nano-ros table and is **not** `[deploy.<id>]`, which is
upstream `ros-launch-manifest`'s and keeps meaning placement — which machine a
node runs on. An image needs no deploy block and a deploy block needs no image
(RFC-0065 D6).

## The generated entry

The entry package is derived from `(launch, args, board)` and generated —
RFC-0065 D4, *"A workspace contains no entry packages."* On the cargo driver it
lands at `<ws>/build/<coordinate>/<image_id>_entry/`, and that package's own
`Cargo.toml` is the cargo root: with no workspace root to walk up to, a
generated entry is simply a package with path dependencies (RFC-0098 D9). On
the cmake driver the generated `<ws>/build/<coordinate>/CMakeLists.txt` is the
root, and emits a `nano_ros_add_executable(…)` call per image on that
coordinate. Beside a cargo entry sits `nros-cargo.toml`, the per-image settings
file `nros sync` writes — triple, link flags, `[env]`, patch rows — which cargo
reads through `--config`.

All of it is build output under `build/`, and none of it is written beside your
source: no root build file, no leaf `.cargo/config.toml`. The tracked marker
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
nros::main!();                                          // single-package: reads [image.*] board from system.toml
nros::main!(board = LinuxBoard);                        // single-package, explicit board type
nros::main!(launch = "demo_bringup");                   // multi-node (CANONICAL): the bringup's default launch
nros::main!(launch = "demo_bringup:sim.launch.xml");    // multi-node, a named launch file
nros::main!(                                            // multi-host: per-host slice via launch args
    launch = "demo_bringup:multihost.launch.xml",
    args = [("host", "robot1")],
);
// DEPRECATED (expert override): name the resolved model ARTIFACT directly
// instead of the input — skips the sync-freshness contract; bake-time warning.
nros::main!(model = "demo_bringup");
```

The proc-macro reads the launch file at compile time, walks the workspace
pkg-index for each `<node pkg=…>` entry, and expands to a `fn main()` that
delegates to `<Board as BoardEntry>::run(...)`, dispatching one
`<pkg>::register(runtime)?` call per launch row.

The `launch` forms take `<bringup>[:<file>]` against
`launch/<file>.launch.xml` + `system.toml [system] default_launch`; `nros sync`
resolves that input into a SystemModel under `<ws>/build/nros/models/<bringup>/`
(a build artifact — never committed; gate `check-no-tracked-models`), and the
macro bakes from it. `launch` and `model` are mutually exclusive.

**Escape hatch:** `nros materialize <image>` copies the generated entry to
`src/<image_id>_entry/` and stamps it; `nros build` then leaves it alone and
warns if the shape it was cut for moves. Inside it you can skip the macro and
call `<LinuxBoard as BoardEntry>::run(|runtime| { ... })`, or go fully manual
with `nros::Executor::open(&ExecutorConfig::default())`. Reach for it only
after the declarative escapes — an `[image.*]` key, a `conf` fragment, a
support package — have run out.

**C++ / C analogue:** the generated cmake root emits one
`nano_ros_add_executable(<name> BOARD … BRINGUP … LAUNCH … LANG … TYPED DEPLOY …)`
per image — no `SOURCES`, because the verb generates the translation unit that
carries `main`. `nano_ros_entry(NAME … LANG … BRINGUP … LAUNCH …)` is the
hand-written spelling, and you write it yourself only in a Zephyr application
(below). `MODEL` on either is the deprecated artifact-naming override,
mirroring the macro. Metadata flows through `${BUILD}/nros-metadata.json`
rather than a sidecar TOML.

## The one entry package that survives

An entry package is still **required** where an external build system demands a
real application *directory*. That is Zephyr: a Zephyr app *is* the `app`
target `find_package(Zephyr)` creates, and it carries authored Kconfig
(`prj.conf`, `prj-<rmw>.conf`, `boards/*.overlay`) that nothing derives —
RFC-0065 D5, *"west and ESP-IDF apps keep their own files because those are
Kconfig overlays — user intent, not derivable."*

```
src/zephyr_entry/
├── CMakeLists.txt      # find_package(Zephyr) + nano_ros_add_executable(… DEPLOY zephyr)
├── package.xml
├── prj.conf
└── prj-zenoh.conf      # no src/ — the entry TU is generated into `app`
```

Scaffold it with `nros new entry <name> --platform zephyr`, which writes the
package *and* its `[image.*]` row together. The verb accepts no other platform,
and says why:

```text
`nros new entry` currently scaffolds Zephyr entries only (got --platform native).
Every other platform builds through cargo or cmake, where the entry is
GENERATED from the image and needs no package of its own (RFC-0065 D3).
```

One Zephyr entry covers every Zephyr board — Zephyr owns its board
abstraction, so the board comes from the image's `board` key (which becomes
`west build -b`). Set the image's `entry` key only when *several* packages
claim one board: `examples/workspaces/realtime-cpp` has `zephyr_entry` and
`fvp_entry`, both `DEPLOY zephyr`, on the same board, for two images that
differ in payload. Deriving there is a coin flip, so `nros build` refuses and
names the candidates.

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

`nros sync` writes the generated message crates and the per-image build
settings; `nros build <image>` then generates the entry and its build root
under `build/` and runs the platform's own tool. `nros plan` takes the sole
bringup when there is one, or you name it: `nros plan demo_bringup`.

`examples/workspaces/{c,cpp,rust,mixed}/` are the four canonical workspaces —
each is exactly this shape, plus one `zephyr_entry` for the Zephyr images.

## Single-package convenience

For tiny fixtures, host-side dev loops, and the copy-out examples under
`examples/<platform>/<lang>/`, one package is both the node and the boot point.
There is no bringup *package* — but the declaration is the same file, written
beside the manifest (RFC-0098 D3), and `src/main.rs` collapses to one line:

```toml
# system.toml, beside Cargo.toml
[system]
name = "native_talker"
rmw = "zenoh"
domain_id = 0

[[component]]
pkg = "native_talker"
class = "native_rs_talker::Talker"
name = "talker"

[image.native]
board = "native"
```

```rust,ignore
// src/main.rs
nros::main!();
```

The macro reads the board from that `[image.*]` row, maps it to the right board
crate, and emits `fn main()` + `<this_pkg>::register(runtime)?;` — the latter
resolves through the companion `src/lib.rs` cargo auto-wires alongside the
binary target. No `build.rs`, no launch file (one is synthesised in-memory), no
hand-written boot glue.

Switching board here is **not** yet the one-line edit it is in a workspace. A
single-package leaf is its own entry, so it still names its board crate in its
own `[dependencies]`; the generated board-crate dependency (RFC-0098 D6) reaches
the *generated* workspace entry only. Change `[image.*] board` alone and `nros
sync` reports success while the build fails in `nros::main!` with `cannot find
nros_board_<new_board> in the crate root` — move the `[dependencies]` line in
the same edit. Tracked as
[issue 1305](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/1305-single-package-board-crate-dep-not-generated.md).

Build it the same two ways a workspace is built:

```bash
nros sync            # generated/ message crates + build/<image>/nros-cargo.toml
nros build           # or: nros build <image-id>
```

The binary lands at `build/<image-id>/target/[<triple>/]<profile>/<bin>`. To
drive cargo yourself, sync first and pass the generated settings file — and run
cargo from the directory *above* the package, because until phase-445 W6 deletes
the leaf's own `.cargo/` directory cargo would read it a second time and join
the two sets of link flags:

```bash
cd <leaf-parent>
cargo build --manifest-path <leaf>/Cargo.toml \
            --config <leaf>/build/<image-id>/nros-cargo.toml
```

Once W6 lands, the working directory stops mattering. `cargo run` through the
same file works for a QEMU board — the board's runner is in the generated file.

A single-package **C or C++** leaf is simpler still: its own CMake is unchanged,
and it needs no sync, because its message bindings are a CMake-time output.

```bash
cd <leaf>
cmake -B build
cmake --build build
```

What changed for it is where the facts come from: the board, RMW, domain and
network identity are in `system.toml`, not in a `package.xml` `<nano_ros …/>`
tuple and not on the `cmake` command line. `nros build` does not yet work in a
single-package C/C++ leaf —
[issue 1296](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/1296-nros-build-c-leaf-bringup-name-mismatch.md).

This is a *project shape*, not a workspace shortcut: one `system.toml`, one
package, and that is what makes a copy-out example copy out (RFC-0026). Two or
more nodes means a workspace with a Bringup pkg.

## Quick reference

| You want… | Use |
|---|---|
| Reusable node logic, board-independent | Node pkg (`nros::node!()`) |
| A runnable program: N nodes on one board | An `[image.<id>]` row + `nros build <id>` |
| A single-node project with no workspace | One package with its own `system.toml` — `[image.native] board = "native"` |
| Same nodes on multiple boards | One Node pkg set + one `[image.*]` row per board |
| Launch topology + the image table | Bringup pkg (declarative — the one package a workspace must have) |
| A Zephyr application directory | `nros new entry <name> --platform zephyr` |
| Startup the declarations cannot express | `nros materialize <image>`, then edit `src/<image>_entry/` |
| Board hardware bringup | `Board` trait family (see [porting chapter](../porting/board-trait.md)) |
