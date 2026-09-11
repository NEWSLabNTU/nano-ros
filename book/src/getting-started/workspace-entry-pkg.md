# Images

An **image** is one buildable program: a topology, a board, and a backend.
You declare it as a row in your Bringup pkg's `system.toml` —

```toml
[image.native]
board = "native"
```

— and build it by name:

```sh
nros sync
nros build native
```

There is no package to write. `nros build` discovers your Node pkgs, reads the
`[image.*]` table, and **generates** the entry that boots the topology, under
`build/`. An image is a row, not a directory.

That is the whole model. The rest of this page is what goes in the row, where
the generated pieces land, and the one case that still needs a package of its
own.

## Prereqs

Pick one path from a fresh checkout:

**A. Front door** (bare machine OK — no Rust):
```sh
./scripts/bootstrap.sh
```
Installs rustup if needed and builds the in-tree `nros` CLI from
source at `packages/cli/target/release/nros`, leaving it on PATH for
this shell (in a checkout, the tree's own build is the binary this tree
accepts — RFC-0090 / phase-431).

**B. Already have cargo** (equivalent — same build, same binary):
```sh
git submodule update --init packages/cli/third-party/play_launch
cargo build --release --manifest-path packages/cli/Cargo.toml --bin nros
export PATH="$PWD/packages/cli/target/release:$PATH"
```

Every subsequent shell sources the workspace env via one of:
```sh
direnv allow                  # if you use direnv
source ./activate.sh          # bash / zsh
source ./activate.fish        # fish
```

Then provision the native host (the canonical first image target; for Zephyr /
FreeRTOS / ESP32 swap in the matching `nros setup` board):
```sh
nros setup native --rmw zenoh
nros sync                     # generated message bindings, once per workspace
```

## What a workspace looks like

The four reference workspaces in `examples/workspaces/` all have the same
shape. Node pkgs, one Bringup pkg, and nothing else:

```text
examples/workspaces/c/
├── .colcon_workspace         # the tracked marker: this directory IS a workspace root
├── .gitignore
├── README.md
└── src/
    ├── talker_pkg/           # Node pkg
    ├── listener_pkg/         # Node pkg
    ├── service_server_pkg/   # Node pkg
    ├── … three more …
    ├── demo_bringup/         # Bringup pkg: package.xml + system.toml + launch/
    └── zephyr_entry/         # the one exception — see below
```

No `CMakeLists.txt`, no `Cargo.toml` at the root — and none is generated
there either. A workspace is a directory of packages, exactly as a colcon
workspace is, and everything the build produces lives under `build/`, `dist/`
and `log/` (RFC-0098 D9). The tracked marker that this directory *is* a
workspace root is `.colcon_workspace`.

And no entry packages, except the Zephyr one. Fourteen images ship out of that
directory. `nros build` with no argument lists them:

```console
$ cd examples/workspaces/c
$ nros build
Error: this workspace declares 14 images and no default.

  demo_bringup:freertos
  demo_bringup:freertos_posix
  demo_bringup:native
  demo_bringup:native_action_client
  demo_bringup:native_action_server
  demo_bringup:native_cyclonedds
  demo_bringup:native_robot1
  demo_bringup:native_robot2
  demo_bringup:native_service_client
  demo_bringup:native_service_server
  demo_bringup:native_xrce
  demo_bringup:nuttx
  demo_bringup:threadx
  demo_bringup:zephyr

  build one:   nros build freertos
  build all:   nros build --all
  or declare:  [system] default_images = ["freertos"]
```

Fourteen programs, one launch tree, six node packages, **one** entry
directory. Under the old shape that was fourteen directories.

## Declaring an image

Images live in the Bringup pkg beside the topology they boot —
`examples/workspaces/c/src/demo_bringup/system.toml`:

```toml
[image_defaults]
rmw = "zenoh"

[image.native]
board = "native"

[image.freertos]
board = "mps2-an385-freertos"

[image.freertos_posix]
board = "freertos-posix"
# NOT the `[image_defaults]` zenoh. zenoh-pico's FreeRTOS backend is lwIP-only
# and this board has no lwIP.
rmw = "cyclonedds"

[image.native_service_server]
board = "native"
launch = "service_server.launch.xml"

[image.native_robot1]
board = "native"
launch = "multihost.launch.xml"
args = { host = "robot1" }

[image.zephyr]
board = "native_sim/native/64"
conf = ["prj-zenoh.conf"]
```

Read it as a table of variants over one topology. `native` and
`native_cyclonedds` differ in a backend; `native` and `native_service_server`
differ in a launch file; `native_robot1` and `native_robot2` differ in one
launch argument. Nothing is duplicated to express any of that.

| Key | Meaning |
|---|---|
| `board` | The nano-ros board id — resolved through `packages/boards/board-support.toml`, which supplies the rustc triple, the platform, and the framework's own board string |
| `launch` | The launch file this image bakes, relative to the Bringup pkg. Absent ⇒ `[system] default_launch` |
| `args` | Launch arguments bound at resolve time (`{ host = "robot1" }`) — how an image selects one machine out of a multi-host launch tree |
| `rmw` | Backend for this image. Absent ⇒ `[system] rmw` |
| `panic` | RFC-0077 panic policy (`platform` \| `halt` \| `own`), forwarded to the generated entry |
| `profile` | A cargo / CMake build profile name |
| `conf` | Extra framework config fragments, in order (Zephyr `prj-*.conf`) |
| `features` | Capability axes for this image, over `[system]`'s list |
| `entry` | The application package — **normally omitted**; see [the exception](#the-exception-when-a-framework-owns-the-application) |

Switching a workspace image to another board is that one `board` line and
`nros sync` — nothing in any package you wrote mentions a board crate, because
the entry that depends on it is generated (RFC-0098 D6). A single-package leaf
is its own entry and does not yet get that: see
[Role reference](../user-guide/component-and-entry-pkg.md#single-package-convenience).

`[image_defaults]` is the base every block folds over (RFC-0065 D5.1): scalars
are replaced by the specific block, `args` merges, `conf` and `features`
concatenate. Without it an eight-image workspace repeats its RMW eight times,
and eight copies of one fact is how they start disagreeing.

`[system] default_images = ["native"]` is what a bare `nros build` then builds.

## What `nros build` generates

Everything it generates lands under `build/` — build output, gitignored, none
of it yours to edit:

| driver | where the entry lands | what roots the build |
|---|---|---|
| **cargo** | a real package at `<ws>/build/<coordinate>/<image_id>_entry/`, path-depending on your Node pkgs | its own `Cargo.toml`. That IS the cargo root; the binary lands at `…/<image_id>_entry/target/[<triple>/]<profile>/<image_id>_entry` |
| **cmake** | a `nano_ros_add_executable(…)` call in `<ws>/build/<coordinate>/CMakeLists.txt`, one per image sharing the coordinate | that generated `CMakeLists.txt`; the binary lands in `<ws>/build/<coordinate>/cmake/` |

An earlier shape kept the cargo root at `<ws>/Cargo.toml`, on the grounds that
cargo resolves a package's workspace by walking *up* and refuses a member above
its root. That constraint binds only while a workspace *exists*: with no root
manifest anywhere there is nothing to walk up to, and a path dependency need not
sit below anything. So the generated entry is the root, and the workspace keeps
no build file of its own (RFC-0098 D9, superseding RFC-0065 D3).

Beside the generated entry sits `nros-cargo.toml`, the settings file `nros sync`
writes per image (RFC-0098 D1): the board's rustc triple and link flags, that
image's `target-dir`, the resolved `[env]`, and the in-repo patch rows. Cargo
reads it through `--config`, which is why each image compiles with its own
settings and nothing generated ever appears beside a package — no leaf
`.cargo/config.toml`, no board projection to commit.

The coordinate is the platform and the RMW, plus — for cmake, which pins one
compiler per configure — the board. So `[image.native]` in
`examples/workspaces/c` configures `build/posix-zenoh-native/` into
`build/posix-zenoh-native/cmake/` and leaves its binary at
`build/posix-zenoh-native/cmake/native_entry`. On the cargo driver `nros build`
names the entry package it wrote as it goes:

```text
nros build:   entry → …/build/posix/native_entry
```

The generated cmake root lists **every** image on that coordinate, not just the
one being built: what a coordinate contains is a property of the workspace, and
making it depend on which image you asked for would reconfigure on every switch.

The generated entry is the same few lines you would have written. RFC-0065 D4
measured them before deciding to derive them: every Rust entry in the tree was
**≤ 6 non-comment lines**, every embedded C/C++ entry had **zero** source
files. Three parts, all derivable —

| part | derived from |
| --- | --- |
| the `no_std` / `no_main` shell | the board's `entry_kind` |
| board boilerplate (`use panic_semihosting as _;`, `esp_app_desc!()`) | the board descriptor's `entry.crate_root_extra` |
| `nros::main!(launch = …, args = …)` | the image |

— which is why adding a board adds a descriptor rather than a branch in the
emitter, and why the generated entry calls `nros::main!` rather than its own
expansion: the macro reads the launch XML at *expansion* time, so adding a node
to the launch file is picked up by the next compile with nothing regenerated.

## The exception: when a framework owns the application

An entry package survives in exactly one situation — **an external build system
that demands a real application directory.** That is Zephyr.

A Zephyr app *is* the `app` target that `find_package(Zephyr)` creates, and it
carries authored Kconfig that nothing can derive: `prj.conf`, the per-RMW
`prj-<rmw>.conf` fragments, `boards/*.overlay`. RFC-0065 D5 draws the line
there — *"west and ESP-IDF apps keep their own files because those are Kconfig
overlays — user intent, not derivable."* So `nros build` generates no entry for
a Zephyr image; it resolves your application, applies the image's overlays, and
runs `west build`.

`examples/workspaces/c/src/zephyr_entry/` is the whole package — four files, no
sources:

```text
src/zephyr_entry/
├── CMakeLists.txt      # find_package(Zephyr) + nano_ros_add_executable(... DEPLOY zephyr)
├── package.xml
├── prj.conf
└── prj-zenoh.conf
```

and its `CMakeLists.txt` states the reason in its own words:

```cmake
# Unlike the FreeRTOS/NuttX/ThreadX entries (cmake-lane, add_executable +
# nros_platform_link_app), a Zephyr app IS the `app` target that find_package(Zephyr)
# creates — so this entry CMakeLists is itself a Zephyr application, built by west
```

For a Rust workspace you do not have to write it by hand:

```sh
nros new entry zephyr_entry --platform zephyr
```

That writes `Cargo.toml`, `CMakeLists.txt` and `prj.conf`, **and** the
`[image.*]` row — the two halves are one declaration, and every Zephyr build
failure worth having is the two disagreeing. (The C and C++ Zephyr entries in
`examples/workspaces/{c,cpp}` are authored; the shape above is all of it.)
Zephyr is the only platform the verb accepts, which is the rule stated by the
tool itself:

```text
`nros new entry` currently scaffolds Zephyr entries only (got --platform native).
Every other platform builds through cargo or cmake, where the entry is
GENERATED from the image and needs no package of its own (RFC-0065 D3).
```

### Running on Zephyr

The Zephyr image names its application and its RMW overlay; nothing else in
the row is Zephyr-specific:

```toml
[image.zephyr]
board = "native_sim/native/64"
conf  = ["prj-zenoh.conf"]
```

```sh
nros sync                              # generated message bindings, once
nros build demo_bringup:zephyr         # resolves the app + overlays, runs west build
nros build demo_bringup:zephyr -- -t run
```

Plain `west` keeps working and stays the primary flow — `nros build` is the
convenience that applies an image's overlays for you, never a required layer
between you and west:

```sh
west build -b native_sim/native/64 src/zephyr_entry \
    -- -DCONF_FILE="prj.conf;prj-zenoh.conf"
west build -t run                      # native_sim; `west flash` for hardware
```

**One Zephyr entry package covers every Zephyr board.** Zephyr owns its board
abstraction, so the board is chosen by the image's `board` key (which becomes
`west build -b`), not baked into the package. Swap
`native_sim/native/64` for `nrf52840dk/nrf52840` and `prj-zenoh.conf` for
`prj-xrce.conf`, and nothing in `src/` changes.

The `entry` key exists for the case where *several* packages could answer to
one image. An application package no longer declares the board it serves —
that direction is reversed: the **image claims the entry**, by naming it
(`entry = "zephyr_entry"`) or, when it names none, by being the image whose
generated entry would carry that name (RFC-0098 D5, phase-445 W5). So the
application reads its deployment — board, RMW, locator, domain — off the image
that claims it, and a `[package.metadata.nros.entry] deploy` left in a
`Cargo.toml` is now an error naming the file to write instead.

Two images claiming one entry is refused rather than resolved by first match:
`examples/workspaces/realtime-cpp` has `zephyr_entry` and `fvp_entry`, both
`DEPLOY zephyr`, both on the same board, for two images that differ in payload
— which image's locator the entry bakes is not a coin toss, so `nros build`
names the candidates instead of picking one. Everywhere else, leave `entry`
out and let the name match.

The full Zephyr path — west workspaces, freestanding applications, `west nros`,
sysbuild — is [Zephyr (west module)](./integration-zephyr.md).

## If you already have entry packages

`nros build` keeps a hand-written entry: if `src/<image_id>_entry/` carries a
`Cargo.toml` or a `CMakeLists.txt`, it is used as-is and nothing is generated
over it. Migration is therefore a **deletion**, one image at a time — remove
the directory, and the next build generates its replacement.

Move these out first, since deleting the package deletes them too:

| in the old entry | goes to |
|---|---|
| board, launch file, launch args, RMW, panic policy, profile | the `[image.*]` row |
| RTOS config overlays — `prj.conf`, `boards/*.overlay`, `sdkconfig.defaults` | the Bringup pkg, under `boards/<board>/` |
| genuinely hand-written startup you cannot express as a declaration | `nros materialize`, which writes the package back out for you to keep |

## `nros::main!()` — what the generated entry contains

You will read this in a generated entry, and write it by hand only in a
single-package project or a materialized entry.

```rust
// 1. Single-node self-bringup: reads the board from the `[image.*]` row in
//    the `system.toml` beside this package's Cargo.toml, and boots the Node
//    pkg that is the only member of this package.
nros::main!();

// 2. Single-node, explicit board type.
nros::main!(board = LinuxBoard);

// 3. Multi-node (CANONICAL): name your INPUT — the Bringup pkg, and
//    optionally a launch file inside it; the default is its default launch.
nros::main!(launch = "demo_bringup");
nros::main!(launch = "demo_bringup:variant_b.launch.xml");

// 4. Multi-host slice: the launch file gates nodes on a `host` launch
//    argument (`if=` conditions), and the model is resolved with
//    `host:=robot1`, so it already contains only this host's nodes.
nros::main!(launch = "demo_bringup:multihost.launch.xml", args = [("host", "robot1")]);

// 5. DEPRECATED (expert override): name the resolved model ARTIFACT
//    directly instead of the input. Skips the sync-freshness contract;
//    warns at bake time (phase-330 W7).
nros::main!(model = "demo_bringup");
```

`launch` and `model` are mutually exclusive. `launch` is the canonical
spelling: it names your *input* — the bringup package and launch file — and the
entry consumes the SystemModel that `nros sync` resolves from it. The same
resolved artifact drives the Linux runtime (play_launch) and every embedded
image, so contract budgets, tiers, and QoS never drift between runtimes.

```console
$ nros sync    # resolves models into <ws>/build/nros/models/<bringup>/
```

`nros sync` re-resolves a model whenever a bringup's launch XML or `system.toml`
is newer than it, and `nros build` runs it for you. **SystemModels are build
artifacts — never commit one** (the `check-no-tracked-models` gate enforces
this). Under the hood `nros sync` runs the pinned `nros-launch-resolve` helper
(RFC-0060 layer 2) by absolute path, never by a bare name on `$PATH`, because
an unrelated ROS 2 `play_launch` on `PATH` used to win that race.

The macro reads the image's `board` at compile time to select the board
crate and executor backend, and the same model resolves its tier table for
whichever RTOS that board targets. On Embassy / RTIC targets it emits the
framework-specific `#[embassy_executor::main]` or `#[rtic::app]` body, so the
crate root stays a single line — as in
`examples/mps2-an385-baremetal/rust/talker-rtic/src/main.rs`:

```rust
#![no_std]
#![no_main]

use panic_semihosting as _;

nros::main!();
```

## Escape hatch

If you need more control than the macro provides — custom startup ordering,
hardware init before the runtime, or a fully manual executor loop — take
ownership of the generated entry:

```console
$ nros build native          # generate it first — materialize copies, it does not re-emit
$ nros materialize native
wrote …/src/native_entry

This entry is YOURS now — `nros build` will not regenerate it.
```

It lands at `src/<image_id>_entry/`, carries a stamp recording the shape it was
cut for, and `nros build` warns if that shape later moves. Its derivation stays
live — `nros::main!` still reads the launch file at compile time, so adding a
node needs no change here; what is frozen is the *shell*: the panic policy, the
board boilerplate, the crate type. Inside it you can bypass `nros::main!()`
entirely:

```rust
// Option A: delegate init to the board crate, supply your own closure.
<LinuxBoard as BoardEntry>::run(|runtime| {
    let node = runtime.create_node("talker", "/", &Default::default())?;
    // ...
    Ok(())
});

// Option B: fully manual — no board crate.
let executor = nros::Executor::open(&ExecutorConfig::default())?;
// wire nodes, spin manually ...
```

Option A is the right choice when you need to run something before the first
spin (e.g. DMA setup, flash unlock). Option B is there for board-bringup
authors adding a new platform. Reach for either only after the declarative
escapes — an `[image.*]` key, a `conf` fragment, a support package — have
actually run out.

## Running a native image

Build it, start a router if your backend needs one, run the binary:

```bash
cd examples/workspaces/c
nros sync
nros build native

# in another shell, for the zenoh backend:
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false' ros2 run rmw_zenoh_cpp rmw_zenohd &

./build/posix-zenoh-native/cmake/native_entry
```

The binary opens the executor, registers `talker` + `listener` composed into a
single process, and runs the topology. The launch product *is* the binary —
there is no separate launch step.

`examples/workspaces/{c,cpp,rust,mixed}/` are the four canonical workspaces and
each carries a README with the commands verified green today.

## C / C++ images

Nothing above is Rust-specific: the driver is chosen by the **board**, not by
the language mix, and cmake wins whenever the package graph crosses languages
(corrosion makes cargo consumable from cmake; nothing makes cmake consumable
from cargo). `examples/workspaces/c` and `examples/workspaces/cpp` declare
their images exactly as shown here. See
[C / C++ multi-node workspaces](./workspace-cpp.md).

## Where to go next

- [Bringup packages](./workspace-bringup.md) — the `system.toml` and launch XML an image is declared in.
- [Node packages](./workspace-node-pkgs.md) — the Node pkgs an image links.
- [Zephyr (west module)](./integration-zephyr.md) — the exception, in full.
- [Role reference](../user-guide/component-and-entry-pkg.md) — reference for the package roles and the macro forms.
