# `nros` CLI reference

The `nros` binary is the user entry point for nano-ros: it provisions
toolchains, scaffolds workspaces, generates message bindings, builds and runs
images, and inspects boards, configuration and plans.

`nros --help` prints the verbs below, in the same groups. This page documents
those verbs: the user tier. The verbs that build systems call (the build-system tier)
and the older spellings that still parse are not part of it; `nros help --all`
lists the first and `nros help --aliases` the second.

## Install

Install a release, with no checkout and no cargo:

```sh
curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install.sh | sh
```

It verifies the sha256, installs into `~/.nros/sdk/nros/<version>/` and points
`~/.nros/bin/nros` at the newest installed version. `--version <v>` installs a
named release instead of the newest; the repository's GitHub Releases page
lists them. The release carries the CLI and its SDK root, the library source
your build compiles (`share/nano-ros` in the install). There is no crates.io,
Arduino zip or ESP-IDF binary channel. See
[Installation](../get-started/installation.md).

## Start

### `nros new`

Scaffold a workspace, a package, an image or a host. Outside a workspace it
writes a new one, for one board:

```sh
nros new sensor_ws --board mps2-an385-baremetal --template talker [--lang c|cpp] [--rmw <rmw>]
```

```text
sensor_ws/
└── src/
    ├── sensor_node/            # node package: a library, no main(), names no board
    └── sensor_node_bringup/    # package.xml and system.toml: one component row, one image
```

The node package is named from the workspace (`<x>_ws` gives `<x>_node`), and
the bringup is `<node>_bringup`. Its `system.toml` names the board, the RMW, the
one component and one image, so `nros run <node>` runs it with nothing edited
([First Project](../get-started/first-project.md)).

Inside a workspace, `nros new <name>` adds a node package under `src/` instead:

```sh
nros new monitor_node --template listener --to sensor_node_bringup
```

`--to <bringup>` also adds the node's `[[component]]` row and its
`<exec_depend>` to that bringup. Without it the package is written and no
bringup runs it yet.

The node is the `--template` role, the in-tree example of that role under the
package's names:

| `--template` | Node | Interface |
|---|---|---|
| `talker` (default) | publishes `Hello World: N` once a second | `std_msgs/String` on `/chatter` |
| `listener` | logs what `/chatter` carries | `std_msgs/String` on `/chatter` |
| `server` | adds two integers | `example_interfaces/srv/AddTwoInts` on `/add_two_ints` |
| `client` | calls the server once it is up | `example_interfaces/srv/AddTwoInts` on `/add_two_ints` |
| `action-server` | computes a Fibonacci sequence, with feedback | `example_interfaces/action/Fibonacci` on `/fibonacci` |
| `action-client` | sends one goal, logs feedback and the result | `example_interfaces/action/Fibonacci` on `/fibonacci` |

The roles and interfaces are the ROS 2 demos' (`demo_nodes_cpp`'s talker and
listener, `add_two_ints_server` / `_client`, the Fibonacci action pair), so a
scaffolded node talks to the demo it is named after.

The package flags follow `ros2 pkg create`:

| Flag | Writes | Default |
|---|---|---|
| `--node-name <name>` | the node's ROS name | the package name (`-` spelled `_`) |
| `--dependencies <pkg>...` | extra `<depend>` rows, after the role's interface package | none |
| `--license <id>` | `<license>` | `Apache-2.0` |
| `--description <text>` | `<description>` | a one-line summary |
| `--maintainer-name` / `--maintainer-email` | `<maintainer>` | `git config user.name` / `user.email` |
| `--destination-directory <dir>` | where the package directory is created | the current directory (`src/` inside a workspace) |

`ros2 pkg create --build-type` has no counterpart: `--lang` decides the
package's build type (cargo for Rust, CMake for C/C++). `--library-name` has
none either: every node package is a library. `--use-case` is retired and
refused, naming `--template`.

| Flag | Values | Default |
|---|---|---|
| `--board` | a board id ([Supported Boards](supported-boards.md)) | (required for a new workspace) |
| `--lang` | `rust`, `c`, `cpp` | `rust` |
| `--rmw` | `zenoh`, `xrce`, `cyclonedds` | `zenoh` |
| `--ros-edition` | `humble`, `iron`, `jazzy` | `humble` |
| `--template` | `talker`, `listener`, `server`, `client`, `action-server`, `action-client` | `talker` |
| `--to` | a bringup package in this workspace | none |
| `--force` | overwrite an existing directory | off |

The scaffold names the image after the board's platform: `[image.baremetal]`,
`[image.linux]`, `[image.freertos]`, `[image.zephyr]`, each with
`board = "<board>"`.

`--board native` and `--board posix` are refused with a hint naming
`linux` — the host board's one name. There is no standalone package shape: a
`system.toml` lives only in a bringup, and `nros build` refuses one beside a
`Cargo.toml` or `CMakeLists.txt`, naming where to move it. Zephyr is no
exception: `nros build` and `nros run` generate the Zephyr application under
`build/` from the image ([the Zephyr chapter](../targets/zephyr.md)), and
[`nros materialize`](#nros-materialize) turns it into a package of yours when
you need to edit it.

**Image / host mode:** `nros new --image <name> [--board <b>]` scaffolds an `[image.<name>]` — a buildable image — into the bringup package's `system.toml`; `nros new --host <name>` scaffolds a `[host.<name>]`, a machine nodes run on. Both accept `[--bringup <pkg>] [--from-launch <path>] [--from-profile <name>]`. The bringup package is discovered automatically when the workspace exposes exactly one; pass `--bringup <pkg>` to pick one when there are several. `--from-launch` also seeds the bringup `[system].default_launch`; `--from-profile` forks an existing block of the same table.

An image names a `--board` and derives its rustc triple from that board's descriptor, so there is no `--target`. A host takes no board: a host says WHERE nodes run, and a board is what an image is BUILT for — passing one is refused rather than written.

`nros new --deploy <name>` is DEPRECATED: `[deploy.*]` split into those two tables, and a `system.toml` that still carries one is refused by name. The flag still works, dispatched by the `--kind` it always took — `self` scaffolds a host, anything else an image — and says which flag to use instead. It never writes `[deploy.*]`.

### `nros setup`

Provision the toolchain/SDK for a board. `nros setup` is the single
canonical provisioning command. Inside a workspace, bare `nros setup`
provisions what the workspace's images declare. Most components are **prebuilt per platform
per RMW** (cross-compiler, emulator, RMW host tools, SDK sources), fetched
from a pinned index into a shared store (`${NROS_HOME:-~/.nros}/sdk`). No
hand-installed cross-toolchains.

Packages are prebuilt where the index has a binary for your host and built
from source otherwise; the vendored submodules (`zenoh-pico`, `mbedtls`,
RTOS sources) are the source builds. `--dry-run` prints the plan without
fetching. Full explanation:
[Installation](../get-started/installation-details.md#provisioning-with-nros-setup).

Note `setup` does **not** provision a zenoh router: the router is
ROS's `rmw_zenoh_cpp/rmw_zenohd`, so a zenoh run needs a ROS 2 install;
Cyclone DDS needs no daemon. See
[Do I need ROS 2 installed?](../get-started/installation.md#do-i-need-ros-2-installed).

```sh
nros setup linux --rmw zenoh            # host build (router comes from ROS)
nros setup mps2-an385-freertos --rmw xrce  # arm-none-eabi-gcc, qemu, FreeRTOS, XRCE agent
nros setup zephyr                        # the Zephyr SDK + RMW transport sources
```

| Flag | Effect |
|---|---|
| `<board>` | resolve + fetch this board's package set (see [Supported Boards](supported-boards.md)) |
| `--rmw <zenoh\|xrce\|cyclonedds>` | also provision the RMW's host tools — the XRCE agent, Cyclone's `idlc`; zenoh has none here (default `zenoh`); resolves `board ∪ rmw` packages |
| `--tool <name>` | install tools by name (e.g. `--tool qemu`); repeatable — `--tool ninja --tool make` is one plan, with one system-package ask for the union |
| `--source <name>` | provision a single source package by name (repeatable) |
| `--prefix <dir>` | install a `--tool` here instead of the shared store |
| `--list` | list every package in the index + its version |
| `--licenses` | show license-gated packages + how to install them |
| `--system` | resolve the OS-package closure for this host's package manager and PRINT the one install command (it never runs it without `--sudo`) |
| `--workspace [<path>]` | provision what the workspace at `<path>` (default `.`) declares: every `[image.*]`'s board and RMW, then the OS packages its build types and `<depend>`s name (installed with `--sudo`, otherwise printed). An unresolvable `<depend>` stops it before any fetch. Bare `nros setup` at a workspace root is the same; `--check` only reports |
| `--role <role>` | narrow `--system` to entries of this role (repeatable) |
| `--check` | run presence probes instead of installing; exits 1 when anything is missing. With `--system`, the OS packages only; alone, every declared class |
| `--sudo` | EXECUTE the plan's composed OS-package install command instead of printing it — with a `<board>` (its tools' and sources' `system = [..]`, before any download), `--workspace`, `--system`, or a `--tool` that declares OS build deps. Any other selector refuses it, naming these |
| `--full` / `--shallow` | full git history / force `--depth 1` for provisioned sources |
| `-j, --jobs <n>` | packages installed at once (default: CPU count; `NROS_SETUP_JOBS`) |
| `--index <path>` | the SDK index to read (default `nros-sdk-index.toml`) |
| `--dry-run` | resolve + print the plan without fetching anything |

`nros setup board <name> --zephyr-workspace <dir> [--workspace <nano-ros>]`
provisions a downstream Zephyr consumer's tree for a nano-ros board.

`--tool` and `--source` combine: `nros setup --tool arm-none-eabi-gcc
--source freertos-kernel --source lwip` installs the tool, then the
sources, in one command. Selectors that do not combine — a `<board>`
beside `--tool`/`--source`, anything beside `--list`/`--licenses`/
`--system`, `--source` with `--check`, `--rmw` without a `<board>` — are
**refused**, naming the flag, never silently ignored.

Board names (the full set — the index's `[board.*]` keys; each board is named for the reach of
its build):
<!-- board-names:begin (generated by scripts/check-book-board-names.py --write) -->
`freertos-posix`, `fvp-aemv8r-smp`, `linux`, `mps2-an385-baremetal`, `mps2-an385-freertos`, `mps2-an385-zephyr`, `mps3-an536-freertos`, `nucleo-f767zi-zephyr`, `qemu-armv7a-nuttx`, `rpi-pico-w-zephyr`, `rv-virt-nuttx`, `rv-virt-threadx`, `s32z270-freertos`, `threadx-linux`, `zephyr`.
<!-- board-names:end -->
A wrong one lists the rest; `nros setup <board> --dry-run` resolves without
fetching. `native` and `posix` are refused with a hint naming `linux`
([Retired names](../migrating/retired-names.md#retired-board-names)); older retired
board names are in the
[retired board names](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/reference/retired-board-names.md)
ledger.

#### `nros setup presets`

Write a project `CMakePresets.json` (into `[<dir>]`, default the current
directory) that `include`s the per-board preset fragments `nros setup
<board>` wrote under `~/.nros/presets/`. Each preset names the SDK root, the
board's toolchain file (for a cross board) and the build directory
`build/<board>`, so `cmake --preset <board>` configures a nano-ros CMake
package with no hand-set `-DCMAKE_TOOLCHAIN_FILE` or `-Dnano_ros_ROOT`.
Idempotent: re-run it after provisioning another board. Zephyr boards have no
preset, because west is their build tool.

```sh
nros setup qemu-armv7a-nuttx
nros setup presets                 # writes ./CMakePresets.json
cmake --preset qemu-armv7a-nuttx
```

## Build

### `nros build`

Build the workspace. The colcon-like build verb: discover packages, resolve the image, preflight,
generate the build root, then hand off to the native tool. The final stage is an
`exec`, so compiler diagnostics are byte-identical to running cargo / cmake /
west yourself. With no image argument it builds `[system] default_images`;
`--all` builds every declared image. An image is named `<bringup>:<image>`,
or `<image>` when exactly one bringup declares it.

Each build first prepares what the image reads, as `nros configure` does
([below](#nros-configure)): message bindings, the resolved launch model, the
generated entry and the tool's settings. A part whose inputs did not change is
not redone, so a build with nothing changed costs a digest comparison. There is
no step to run before it.

```sh
cd <workspace>
nros build                       # every default image
nros build linux                # one image by id
nros build linux -- --release   # arguments after `--` go to the native tool
```

`--board <board>` builds the image whose `[image.*] board` is that board —
the authored word in any case, another alias of the same board, or the
`nros setup` key it provisions through (`--board mps2-an385-baremetal` finds a
`mps2-an385-baremetal` image). One match is the same as naming the image; several
or none is refused with the image ids listed, so name one. It cannot be
combined with image arguments or `--all`.

After a successful build (not `--dry-run`, not `--check`) `nros build` prints,
per image, where the artifact is and the platform's own next command — it runs
nothing:

```text
nros build: next for `demo:qemu`:
  artifact: <ws>/build/…/qemu_entry/target/thumbv7m-none-eabi/debug/qemu_entry
  run:
    <store>/sdk/qemu/<ver>/bin/qemu-system-arm -cpu cortex-m3 -machine mps2-an385 … -kernel <artifact>
```

A host board prints the binary; a cargo board with a descriptor `runner` prints
that runner; NuttX and ThreadX riscv64 print the QEMU line the tests use; a
Zephyr image prints `west build -d <dir> -t run` and/or `west flash -d <dir>`,
read from what its build supports. QEMU is the store's patched build
(`nros locate tool qemu-system-arm`), never whatever `$PATH` holds. A board
with no derivable command (S32Z270) gets a pointer to its platform chapter.

colcon's flags work with colcon's names: `--packages-select`,
`--packages-up-to`, `--packages-above`, `--packages-skip` and `--base-paths`
choose the packages (`nros list -n` with the same flags prints the set),
and `--continue-on-error`, `--cmake-args` and `--build-base` act as
colcon's do. The differences are refusals rather than
different answers: a package name that matches nothing is an error, and a
selection that leaves out a package a selected one depends on is refused,
because nano-ros has no install prefix for it to come from. In a workspace
with images the unit of `--continue-on-error` is the image, and
`--build-base` is refused there.

`-j N` / `--jobs N` (default: the number of CPUs, as cargo) caps the TOTAL
number of jobs. `nros build` runs one GNU make jobserver of N tokens and
every cargo, make and ninja below it draws from it, so independent packages
building side by side never run more than N compilers between them. Run
under a `make -jN` that already has a jobserver, it joins that one. Ninja
older than 1.13 cannot read a jobserver; when one would run, `nros build`
says so. colcon's `--parallel-workers` is refused with a pointer to `-j`:
colcon counts packages because it has no shared job pool, and `nros build`
does.

```sh
nros build --packages-up-to talker_pkg          # it and its dependencies
nros build --packages-above msgs_pkg            # it and what depends on it
nros list -t -n --packages-skip other_pkg      # the set, in build order
```

A `runner` or `command` package is a host tool. `nros build`
builds it for the host triple, in release, into
`build/nros/host/<package>/`, whatever board the images beside it target. It
has its own target directory and never joins an image's generated root, so
editing it rebuilds only it, and editing an image does not rebuild it.
`--build-base` does not move it, because `nros run` and extension verbs look
for its executable there. An image depends on the workspace package providing
its runner, so `nros build --packages-up-to <bringup>` builds that runner too.
With no selection every host tool in the workspace builds, and a selection
that keeps only host tools builds no image:

```sh
nros build --packages-select acme_tools         # one command package, alone
```

A workspace package may replace one of nano-ros's own providers by announcing
the same kind and name: the workspace overlays the nano-ros packages, as a
colcon overlay does its underlay. For a runner or a command, which run host
code and flash hardware, the replacement must be declared in the workspace's
`nros.toml`, pinned to the package's path; otherwise the build or run that
resolves the name refuses it and prints the line to add.
`--allow-overriding <name>` approves it for one invocation, as colcon's flag
does. Other kinds (an rmw, a board) only warn when undeclared. The
replacement must speak the runner protocol of the one it replaces, and an
entry whose path no longer provides the name is an error rather than a quiet
return to ours.

The workspace's own source trees, `src/` and every `package_paths` entry, are
peers, as colcon's base paths are: the same provider in two of them is an
error for every kind, and the message names both paths. Declare the one you
mean, or pass `--allow-overriding <name>` to let the later root win for one
run. `NROS_PACKAGE_PATH` overlays everything, with a warning.

`--base-paths <path>…` names those source trees for one invocation, and means
the same on `nros build`, `nros list`, `nros configure` and
`nros run`/`flash`/`monitor` (whose build and runner lookup get it). It
replaces the workspace (default `.`) and its `package_paths`, relative to the
workspace, and nothing else: nano-ros's own packages and `NROS_PACKAGE_PATH`
stay on the search path, so a runner nano-ros provides still resolves. As in
colcon, the paths are peers, their order does not matter, a path repeated or
inside another counts once, and a package name found in two of them is an
error naming both. A path that does not exist is an error; one holding no
package is a warning. A path outside the workspace is allowed, and `nros doctor` and
`nros run --dry-run --why` say so.

```sh
nros build --base-paths src ../vendor   # src/ and a vendor tree beside the workspace
nros list -n --base-paths src           # what that build would see
```

```toml
# nros.toml, at the workspace root
[workspace]
package_paths = ["../vendor"]

[workspace.overrides]
qemu = "src/my_qemu"   # this workspace's package answers the qemu runner
```

Overrides are the workspace's to declare: a `system.toml` under `src/` that
carries `[overrides]` is refused. `nros doctor` lists every override, whether it was declared, and which file the
declarations came from.

`-v` / `--verbose` shows the configure's detail notes: why a re-configure
happened, which message bounds were refused and why. Without it each is one
line. A raw cmake run gets the same with `--log-level=VERBOSE`.

`--dry-run` prints the stages and the command that would run, and performs no
I/O.

`--check` type-checks a cargo image instead of building it: every stage runs as
for a build (the image's settings file, its lock), and only the final command
becomes `cargo check`, or `cargo clippy` with `--check=clippy`. Lint flags go
after a second `--`. A cmake or west image is refused, since neither tool has a
check that is not a build. Like a build, a check prepares the image first.

```sh
nros build linux --check                          # cargo check
nros build linux --check=clippy -- -- -D warnings # cargo clippy, warnings denied
```

Zephyr keeps `west build` as its build verb: `nros build` prepares the image,
then runs `west build` on the application it generated.

Driving the native tool yourself is supported: `nros build <image> --dry-run`
prints the exact `cargo` / `cmake` / `west` line, against the generated entry
under `build/`. Run `nros configure` first to write that entry without
building.

### `nros configure [<workspace>] [--image <id>] [--generated-base <d>] [--ros-edition <e>] [--dry-run] [--check] [--force] [--offline] [--no-metadata] [--no-provider-index] [--model-dir <d>] [--nano-ros-path <p>] [--nano-ros-root <p>] [--base-paths <p>…] [-v]`

Configure a project without building it: codegen every `*.msg` package the
declared deps name, write the generated build settings for each declared image,
then resolve and generate the image's build inputs (discover, preflight, census,
resolve, generate). It works on a **standalone package** or a **colcon-style
workspace** (it picks single-pkg vs workspace mode by layout) and runs no target
build tool.

You rarely type it. `nros build` and `nros run` run it first when it is stale,
the way `cargo build` fetches what it needs. Run it yourself when you want the
files without a build (an IDE, a CI step), after editing `*.msg`,
`package.xml` or `system.toml`.

```sh
nros configure               # generated/ msg crates + build/<image>/nros-cargo.toml
nros build                   # or: nros build <image-id>
```

The interface search path is computed inside configure, so no environment
step is needed before it.

What it writes into `build/<image>/nros-cargo.toml`: the board's `cargo_config` (its `[build] target`, `[target.*]`
rustflags/linker, `[unstable]`), this image's `target-dir`, its entity facts
and derived pool knobs as `[env]`, the in-repo `[patch.crates-io]` rows, and the
nano-ros cargo profiles. Nothing generated is written beside the package, and
`[env]` never uses `force = true`, so a value you set in the environment wins.

Transitionally, configure also still maintains the `[patch.crates-io]` block in
each Rust consumer's patch-authority `.cargo/config.toml` (never in
`Cargo.toml`). That road is being removed, and it is what the `ws clean` /
`ws doctor` subcommands below still talk about.

**The stamp.** Each image gets a stamp at
`<project>/build/nros/configured/<image>/stamp.toml`, holding a digest of its
inputs: `package.xml`, `Cargo.toml`, `system.toml`, the launch files, the
environment variables configure reads (such as `AMENT_PREFIX_PATH`), the
`nros-launch-resolve` path, and the content of the ROS install the generated
message crates came from. A second run with nothing changed reads the stamps and
exits without doing any work. An image this host cannot build is recorded as
skipped, and is configured only when you name it. The flags that change what
configure writes (`--ros-edition`, `--generated-base`, `--model-dir`,
`--no-metadata`, `--base-paths`, `--nano-ros-path`, `--nano-ros-root`) and
`NROS_MODEL_DIR` are inputs too, and a deleted output (`generated/`, a model,
the selection package) makes the stamp stale. `--force` configures even when
every stamp is current. `--dry-run` records no stamp.

**Named images.** An image you did not name that fails to configure is reported
on stderr (`configure: <image>: not configured — …`) and the configure still
succeeds. An image you name with `--image`, in a workspace as in a
single-package leaf, must configure: its failure, a missing toolchain included,
is an error.

**The lock.** A per-project file lock serialises writers, so several configures
reaching one workspace at once (a parallel fixture build) queue instead of
racing.

| Flag | Meaning |
| --- | --- |
| `[<workspace>]` | Workspace root (the dir containing `src/`); defaults to the cwd |
| `--image <id>` | Single-package leaf only: configure this `[image.<id>]` (its selection package, `build/<id>/nros-cargo.toml`, message codegen for its edition) |
| `--generated-base <d>` | Output dir for the generated message crates, relative to the workspace (default `generated`) |
| `--ros-edition <e>` | `humble` \| `iron` \| `jazzy`; inherits `[system].ros_edition`, else `humble` |
| `--dry-run` | Don't write; print what would happen |
| `--check` | Exit non-zero if any patch block is missing or stale (CI hook; also used by `nros ws status`) |
| `--force` | Configure even when every image's stamp is current |
| `--offline` | Never touch the network. Reaches the `cargo update --workspace` that prunes a single-package leaf's freshly seeded `Cargo.lock`, the one step that resolves crates |
| `--no-metadata` | Skip the per-Node-package host probe; bakes fall back to the model's entity lower bound |
| `--no-provider-index` | Skip the provider index (`build/nros/providers.json`) |
| `--model-dir <d>` | Write resolved SystemModels here instead of each bringup's `config/` |
| `--nano-ros-path <p>` | The SDK root the probes and board lookups read |
| `--nano-ros-root <p>` | nano-ros source tree scanned as the first provider search root |
| `--base-paths <p>…` | Provider search roots, replacing the default (as colcon's `--base-paths`) |
| `-v`, `--verbose` | Verbose codegen output |

`--check` exits non-zero when anything is missing or stale — the CI hook, and
what `nros ws status` reports non-fatally.

### `nros clean`

Remove the generated message bindings (`generated/`, or `--build-dir <dir>`)
and anything `nros configure` wrote for them under `build/`. Sections you wrote are left alone. `--dry-run` prints
what it would remove.

```sh
nros clean --dry-run
```

### `nros materialize`

Take ownership of a generated entry package: the last resort, after which
`nros build` stops regenerating it. It writes the entry into
`src/<image>_entry/`, for a Zephyr image the whole west application. `panic`, `profile` and `before_boot` are
declarations on the image and need no materialising. A second run without
`--force` is refused, because it would discard the edits the first one existed
to enable. `--workspace <dir>` names the workspace root.

```sh
nros materialize linux            # or <bringup>:linux when two bringups share the id
```

## Run on a target

### `nros run`

Run an image on its target: a host process or an emulator. It builds the
image if it is stale, starts the router or agent its link needs, streams the
console and stops everything on exit. Arguments after `--` go to the target.

The target argument is tried in this order:

| You type | It means |
|---|---|
| `<bringup>:<image>` | that bringup's image; always works |
| `<image>` | the image of that name, when exactly one bringup declares it |
| `<node>` | the one bringup whose `[[component]]` rows name only that node package, with its only image |
| nothing | the workspace's only image |

Anything else refuses and lists the `<bringup>:<image>` spellings that would
resolve; a shortcut never picks one of several. `--why` names the rung that
resolved. The `<node>` rung is `nros run`'s alone; `nros build`, `nros flash`
and `nros monitor` take the other spellings.

| Flag | Effect |
|---|---|
| `--expect <text>` / `--expect-regex <re>` | exit 0 as soon as a console line matches; exit 1 if the target ends, or `--timeout` passes, first |
| `--timeout <seconds>` | stop the target after that long |
| `--router <auto\|none\|LOCATOR>` | `auto` starts the zenoh router or XRCE agent when the link needs one on this host; `none` starts nothing; a locator names one already running |
| `--port <device>` | the console device |
| `--prefix <cmd>` | run a host image under a command (`--prefix 'valgrind -q'`) |
| `--dry-run [--why] [--json]` | print the resolved run plan instead of running it; `--why` adds the rung that supplied each field |
| `--plan <file>` | execute a plan file exactly as written: resolve nothing, build nothing |
| `--workspace <dir>` | the workspace root (default: found from the current directory) |

```sh
nros run sensor_node                                   # the node rung
nros run linux --expect 'Received: 2' --timeout 30
```

A board with no host process or emulator is real silicon; `nros run` refuses
it and names the flashing step. See [Running an Image](../targets/run.md).

### `nros flash`

Flash an image onto a board. **Reserved: not available in this version.**
It exits with an error that says so. Until it ships, flash with the command
`nros build` prints after a successful build. See
[Flash and Monitor](../targets/flash-and-monitor.md).

### `nros monitor`

Attach to a running board's console. **Reserved: not available in this
version**, with the same error and the same advice as `nros flash`.

### `nros bench`

This machine's boards. `nros bench scan` records which attached USB device is
which board, matching each device against the USB id a board descriptor's
`[board.console] usb` states, in `$NROS_HOME/bench.toml` (`--dry-run` prints
what it would record). `nros bench list` prints that file and whether each
device is attached now. The bench file supplies ports and probes; it never
decides what is tested. See [Your Bench](../targets/bench.md).

```sh
nros bench scan --dry-run
nros bench list
```

### `nros debug`

Debug an image on its target. **Reserved: not available in this version.**

## Workspace

### `nros list`

List the workspace's packages: kind, name and directory per row. Any of
`-t` (build order, as `colcon list -t`), `-n` (names only), `-p` (paths only)
or the colcon selection flags (`--packages-select`, `--packages-up-to`,
`--packages-above`, `--packages-skip`, `--base-paths`) switches to
`colcon list`'s format, one `name<TAB>path<TAB>(type)` line per package,
found the way `nros build` finds them.

```sh
nros list -t -n --packages-up-to talker_pkg
```

### `nros graph`

Print the workspace's packages in dependency order, each with the workspace
packages it depends on. `--workspace <dir>` names the workspace (default: the
current directory).

```sh
nros graph
```

## Interfaces

### `nros interface`

Message interfaces, with three subcommands:

| Subcommand | Effect |
|---|---|
| `generate <rust\|c\|cpp\|all>` | generate bindings from a `package.xml` (`--manifest`, default `package.xml`) into `--output` (default `generated`), for `--ros-edition` (default `humble`); `--codegen-config` names an explicit per-field capacity file, `--rename old=new` renames a generated Rust package, `--force` overwrites |
| `list` | list the interface types the search path reaches; `-m`, `-s`, `-a` keep only messages, services or actions |
| `show <package>/<msg\|srv\|action>/<Name>` | show one type's definition with nested types expanded |

`nros build` generates the bindings a workspace needs (for Rust together with
the image's build settings; for C and C++ during the CMake configure). `interface generate` is the codegen step alone.

```sh
nros interface show std_msgs/msg/Header
```

### `nros px4 msgs`

Generate CDR-serializable `px4_msgs::msg::*` bindings from a PX4-Autopilot
`.msg` tree (`--px4 <dir>`, default `$PX4_AUTOPILOT_DIR`) into `--output`.
`--lang rust` (the default) writes a `px4_msgs` crate for an XRCE companion;
`--lang cpp` writes headers and FFI glue for an in-firmware PX4 module.
`--topics` keeps only the messages a bridge carries (by message name or uORB
topic name), pulling in nested types. See the
[PX4 starter](../targets/px4.md).

```sh
nros px4 msgs --output generated/px4_msgs --topics vehicle_status
```

`nros px4` is a command package (`nros_px4_tools`), not a built-in: `nros`
runs its executable with the arguments unchanged. In a checkout `just
setup-cli` builds it. The old spelling, `nros generate-px4-msgs`, still works
and runs the same package.

### `nros router config --model <model>`

Print the zenoh router `downsampling` rule `nros configure` wrote beside a
resolved model, as the `ZENOH_CONFIG_OVERRIDE` entry `rmw_zenohd` reads
(`--format json5` prints the artifact itself):

```sh
ZENOH_CONFIG_OVERRIDE="$(nros router config --model build/nros/models/<bringup>/<stem>.yaml)" rmw_zenohd
```

It fails when there is no rule: no subscriber declares an `ingress`, no image
links zenoh, or the model was never configured. Like `nros px4`, it is a
command package (`nros_zenoh_tools`); the old spelling `nros ws router-config`
runs the same package.

### Extension commands

A verb `nros` does not know is looked up as an announced `command` package
(`<nano_ros_provides kind="command" name="<verb>"/>`). `nros help --all` lists
every one this workspace reaches, with its summary and package. The command runs with
`NROS_WS_ROOT`, `NROS_TOOLCHAIN` and `NROS_CLI` set, and its exit status is
`nros`'s. A package announcing a built-in verb is refused.

## Inspect

### `nros doctor`

Health-check the install and the project. With no workspace it verifies the
install: the launcher, the SDK store, and that the pin in this directory
resolves. In a workspace it also checks the bringup's `[image.*]` and
`[host.*]` blocks (`--config <system.toml>` names another) and the
license-gated SDKs the images' boards need (`--board <board>` narrows that to
one board). `--platform <name>` restricts the check to one platform.
`--report` prints the support status of every board the bringup's images
name, from the board support registry, and stops: a read-only report you can
attach to a bug or a review.

```sh
nros doctor --report
```

### `nros board`

The boards this `nros` knows: the board packages in its SDK root, plus any
found through `NROS_EXTRA_BOARD_PATH`.

| Subcommand | Effect |
|---|---|
| `list` | every board package, with its support status and cadence |
| `show <board>` | a board's support evidence: status, where it runs, cadence, breadth and the book chapters that run it |
| `info <board>` | the board's resolved descriptor, as JSON |
| `new <name> --platform <platform>` | scaffold a board package; `--out-of-tree <dir>` writes it outside nano-ros and prints the `NROS_EXTRA_BOARD_PATH` that reaches it; `--west-board` for Zephyr; `--dry-run` |

The status and cadence come from the board support registry, the same source
as [Supported Boards](supported-boards.md).

```sh
nros board show mps2-an385-freertos
```

### `nros config`

The resolved configuration, with where each value came from.

`nros config show [--system [<bringup>]] [--workspace <dir>]` prints the
effective config of a bringup system (`rmw`, `domain_id`, `locator`, and the
`safety`, `param_services` and `lifecycle` capability axes) with a
provenance column: `system.toml [section]` or the built-in default. Without a
bringup it uses the workspace's `default_system`, or its only bringup. A
legacy per-package `nros.toml` overlay that still declares one of those blocks
is flagged as deprecated.

`nros config explain --platform <name> [--board-toml <file>]` prints the
build-time knob ladder for a platform: every knob's final value and the rung
that set it (`builtin`, `platform`, `board` or `env`). When a pool size or a
hardware budget is not what you expected, it says which rung decided.

```sh
nros config show
nros config explain --platform freertos
```

### `nros plan`

Resolve a bringup's launch file, its manifests and the components' source
metadata into a typed `nros-plan.json` under `build/<system_pkg>/nros/`:
the component graph, topic wiring, parameter remaps and scheduling-context
bindings. The arguments are the system package, then a launch file or a
package directory, then `name:=value` launch arguments.

| Flag | Use |
|---|---|
| `--model <system_model.yaml>` | plan from a resolved SystemModel |
| `--record <file>` | use a precomputed `record.json` instead of parsing the launch file |
| `--image <id>` | resolve per-image values (RMW, build tuning, target triple) against this `[image.<id>]` |
| `--rmw <rmw>` | override the RMW: `--rmw` beats `[image.<id>].rmw`, which beats `[system].rmw` |
| `--workspace <dir>` / `--out-dir <dir>` | the workspace root / the output root |

```sh
nros plan demo_bringup src/demo_bringup
```

### `nros check`

Validate a generated `nros-plan.json` (default `build/nros/nros-plan.json`)
before the platform build runs: unconnected required topics, conflicting QoS,
missing parameters and scheduling-context binding errors. A bringup's
`system.toml` (or its directory) is validated as a deployment instead.
`--bringup` lints a bringup package directory (it must be purely declarative),
and `--workspace [<dir>]` runs the workspace layout checks.

```sh
nros check
```

### `nros explain`

Render a generated `nros-plan.json` (default `build/nros/nros-plan.json`) in
human-readable form: the component graph, topic wiring, parameter bindings and
scheduling-context assignments.

```sh
nros explain path/to/nros-plan.json
```

## Toolchain

### `nros toolchain`

The installed toolchains and SDK packages in the store
(`${NROS_HOME:-~/.nros}/sdk`), which only grows: provisioning never
overwrites, so a new version lands beside the old one.

| Subcommand | Effect |
|---|---|
| `list [--root <dir>]` | every entry with its version, size and when it was last read |
| `gc --older-than <dur> [--delete] [--pin-file <f>…] [--ignore-pins] [--root <dir>]` | propose removing entries not read for `<dur>` (`90d`, `12h`, `2w`); removes nothing without `--delete`, and never an entry a pin names |
| `remove <version> [--dry-run] [--pin-file <f>…] [--ignore-pins] [--root <dir>]` | remove one installed nano-ros toolchain; refuses while a pin names it, or when no pin file could be consulted |

```sh
nros toolchain gc --older-than 90d
```

### `nros toolchain <uninstall|manifest|stage-packages>`

| Subcommand | Effect |
|---|---|
| `uninstall <version> [--dry-run] [--pin-file <f>…] [--ignore-pins] [--root <dir>]` | remove one installed nano-ros toolchain; refuses while a pin names it, or when no pin file could be consulted |
| `manifest [--store-version <v>] [--index <date>] [--nano-ros <commit>] [--write <path>]` | read (or, with `--write`, stamp) the `share/nros/manifest.toml` a release declares it is made of |
| `stage-packages <prefix> [--checkout <dir>]` | release-only: stage every runner and command package of `<prefix>/share/nano-ros` prebuilt into `<prefix>/share/nros/packages/<package>/` (`package.xml`, descriptor, `bin/`), the underlay a released `nros` resolves runners from |

### `nros pin [<version>] [--dir <dir>] [--root <dir>] [--dry-run]`

Report this project's `nros-toolchain.toml` pin, or move it to `<version>` (the
store version, e.g. `0.6.0-nros3`, not the crate version `nros --version`
prints). A move reports the codegen delta between the two toolchains first, so
an upgrade that would invalidate already-generated code warns before anything
is written.

```sh
nros pin 0.6.0-nros4 --dry-run
```

### `nros update`

Move one pin of a pin set and leave one diff to review: a registry crate
(`--crate <name> [--precise <v>]`), a vendored source
(`--source <name> --ref <sha>`, forward only unless `--allow-rewind`), a host
tool (`--tool <name> --to-version <v>`) or a Zephyr west project
(`--west <module> --revision <rev>`). `--pins [--prune]` regenerates the
registry pin set from the locked roots. It edits the pin set of a nano-ros
source tree (`--nano-ros-path`); an installed release's pin set is read-only,
so a project that only consumes a release never runs it. To move your
project to another release, use `nros pin`.

```sh
nros update --crate heapless --precise 0.8.0
```

### `nros locate`

Print where a resource is, and with `--why`, every rung of the lookup ladder
(argument, environment, the project's `system.toml` `[sources]`, a local
edit, the store) and what each saw. A name is an index `[source.<name>]`, or
`store` for the store root. `nros locate tool <name>` locates a host binary
(`qemu-system-<arch>` through the same QEMU lookup `nros build` uses,
`renode`, or `nros-launch-resolve`) or an SDK tool's directory (below). `--all` locates every `[source.*]` row, and
`--format sh|cmake` prints shell or CMake assignments instead of bare paths.

```sh
nros locate zenoh-pico --why
nros locate tool qemu-system-arm
```

**The SDK root.** `nros locate nano-ros` prints the nano-ros SDK root the
project in the current directory builds against: the directory that holds
nano-ros's CMake modules, its Zephyr module and its board descriptors. Use it
wherever a build tool needs that path, for example
`-DZEPHYR_EXTRA_MODULES="$(nros locate nano-ros)"`. It needs no network and no
index, so it is safe in a CMake `execute_process` or a script.

Each project can pin its own nano-ros version (`nros-toolchain.toml`, written by
`nros build` and changed with `nros pin`). Inside a pinned project, `nros` runs
the pinned toolchain, so `nros locate nano-ros` prints **that version's** SDK
root. Two projects pinned to two versions get two different answers, and a
script that asks in each project gets each one right. `--why` shows which
version answered and which pin file chose it:

```sh
cd ~/robots/arm-controller && nros locate nano-ros --why
cd ~/robots/old-gateway    && nros locate nano-ros     # a different pinned version
```

`--project <dir>` asks about another project. If that project is pinned to a
different version from the `nros` answering, the command refuses rather than
print the wrong version's root, and tells you to run it inside that project.

**An SDK tool.** `nros locate tool <name>` also prints the install directory
of a tool that `nros setup <board>` provisioned, such as the Zephyr SDK or a
cross compiler. The answer is the version this release of nano-ros pins,
never another version that happens to sit beside it, so a build that asks
keeps working when the pin moves:

```sh
nros setup zephyr --rmw zenoh
export ZEPHYR_SDK_INSTALL_DIR="$(nros locate tool zephyr-sdk)"
```

A tool that is not installed, or that nano-ros does not know, makes the
command fail with a message that names `nros setup <board>`. A project's
`system.toml` can point a tool somewhere else with a `[tools]` entry, and that
entry wins.

## Other

### `nros version`

Print the CLI version, its codegen version and the versions of the libraries
it was built with.

```sh
nros version
```

### `nros completions`

Emit shell completion scripts to stdout.

| Shell | Install snippet |
|---|---|
| `bash` | `nros completions bash > ~/.local/share/bash-completion/completions/nros` |
| `zsh` | `nros completions zsh > "${fpath[1]}/_nros"` |
| `fish` | `nros completions fish > ~/.config/fish/completions/nros.fish` |
| `powershell` | `nros completions powershell > $PROFILE.parent\nros.ps1` |

### `nros help`

Print the grouped verb list, or one verb's help (`nros help build`, the same as
`nros build --help`). `nros help --all` adds the build-system tier and
`nros help --aliases` lists the older spellings that still parse.

## Design background

- [RFC-0117: CLI command tiers](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0117-cli-command-tiers.md) — the user tier and its groups
- [RFC-0118: target execution model](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0118-target-execution-model.md) — `nros run`, `flash`, `monitor` and `bench`
- [RFC-0065: colcon-like workspace builder](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0065-colcon-like-workspace-builder.md) — images, hosts and `nros materialize`
- [RFC-0075: zenoh router provenance](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0075-zenoh-router-provenance-and-the-unstable-seam.md) — why `nros setup` does not ship a router
- [RFC-0093: board naming states reach](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0093-board-naming-states-reach.md)
- [RFC-0098: generated leaf build config](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0098-generated-leaf-build-config.md) — the generated `nros-cargo.toml`
- [RFC-0120: one resolver for every build road](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0120-nros-configure-one-resolver.md) — `nros configure`, and why `nros build` and `nros run` configure themselves
- [RFC-0121: workspace-first project layout](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0121-workspace-first-project-layout.md) — the scaffold, the bringup-only `system.toml` and the target rungs
- [RFC-0119: CLI extension packages](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0119-cli-extension-packages.md) — `runner` and `command` packages
