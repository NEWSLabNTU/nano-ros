# `nros` CLI reference

The `nros` binary is the user entry point for nano-ros: scaffolding,
message codegen, SDK provisioning, topology planning, static checks,
diagnostics, and board introspection.

The old `cargo nano-ros …` cargo subcommand has been removed. Use `nros`
directly.

## Install

Two front doors, and which one is right depends on who you are.

**Using nano-ros** — install a release, with no checkout and no cargo:

```sh
curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install.sh | sh
```

It verifies the sha256, installs into `~/.nros/sdk/nros/<version>/` and points
`~/.nros/bin/nros` at the newest installed version — one command, however many
versions accumulate. `--version <v>` installs a named release instead of the
newest; the repository's GitHub Releases page lists them.

**Working on nano-ros** — build the CLI from the checkout. That is not a
fallback: in a checkout the tree's own build is the only binary this tree
accepts, because a released `nros` emits its own generated code and this
runtime is what compiles it (RFC-0090, phase-431 W1). `just doctor` fails when
another `nros` shadows it. The CLI ships from the in-tree sub-workspace at
`packages/cli/`; bootstrap builds it (installing rustup if needed), then
activate the workspace to put it on PATH:

```sh
./scripts/bootstrap.sh
source ./activate.sh        # OR: direnv allow / source ./activate.fish
```

Equivalent, if you already have cargo (same build, same binaries):
`git submodule update --init packages/cli/third-party/play_launch`,
then `cargo build --release --manifest-path packages/cli/Cargo.toml --bin nros`
and `cargo build --release --manifest-path packages/cli/nros-launch-resolve/Cargo.toml`
— the launch resolver the workspace flow needs (bootstrap runs all three for
you; `just setup-cli` + `just setup-launch-resolve` are the internal aliases).

The resulting binary lives at `packages/cli/target/release/nros`. One
checkout = one CLI version = one runtime ABI; contributors with
multiple nano-ros worktrees get per-tree CLIs with no global PATH
skew. Once on PATH, `nros --help` lists every verb. `${NROS_HOME:-~/.nros}/bin/nros`
is where `install.sh` puts a release; inside a checkout `just doctor` fails
when that one shadows the tree's own build.

## Verbs

### `nros setup [<board>] [--rmw <rmw>] [--tool <name>] [--source <name>] [--prefix <dir>] [--list] [--licenses] [--system] [--workspace [<path>]] [--check] [--sudo] [--dry-run]`

Provision the toolchain/SDK for a board. `nros setup` is the single
canonical provisioning command. Most components are **prebuilt per platform
per RMW** (cross-compiler, emulator, RMW host tools, SDK sources), fetched
from a pinned index into a shared store (`${NROS_HOME:-~/.nros}/sdk`). No
hand-installed cross-toolchains.

Packages are prebuilt where the index has a binary for your host and built
from source otherwise; the vendored submodules (`zenoh-pico`, `mbedtls`,
RTOS sources) are the source builds. `--dry-run` prints the plan without
fetching. Full explanation:
[Installation](../getting-started/installation.md#provision-your-toolchain-with-nros-setup).

Note `setup` does **not** provision a zenoh router: since RFC-0075 that is
ROS's `rmw_zenoh_cpp/rmw_zenohd`, so a zenoh run needs a ROS 2 install;
Cyclone DDS needs no daemon. See
[Do I need ROS 2 installed?](../getting-started/installation.md#do-i-need-ros-2-installed).

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
| `--workspace [<path>]` | plan from what the workspace at `<path>` (default `.`) declares — its `package.xml` dependencies, build types and the boards its `[image.*]` rows name; prints the plan, installs nothing |
| `--role <role>` | narrow `--system` to entries of this role (repeatable) |
| `--check` | run presence probes instead of installing; exits 1 when anything is missing. With `--system`, the OS packages only; alone, every declared class |
| `--sudo` | EXECUTE the plan's composed OS-package install command instead of printing it — with a `<board>` (its tools' and sources' `system = [..]`, before any download), `--workspace`, `--system`, or a `--tool` that declares OS build deps. Any other selector refuses it, naming these |
| `--build-sources` | provision the whole build-stage source union the repo's tests need (contributors); with `--check`, verify only |
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
**refused**, naming the flag, never silently ignored (issue 1935).

Board names (the full set — the index's `[board.*]` keys; RFC-0093 names each
board for the reach of its build):
<!-- board-names:begin (generated by scripts/check-book-board-names.py --write) -->
`freertos-posix`, `fvp-aemv8r-smp`, `linux`, `mps2-an385-baremetal`, `mps2-an385-freertos`, `mps3-an536-freertos`, `qemu-armv7a-nuttx`, `rv-virt-nuttx`, `rv-virt-threadx`, `s32z270-freertos`, `threadx-linux`, `zephyr`.
<!-- board-names:end -->
A wrong one lists the rest; `nros setup <board> --dry-run` resolves without
fetching. `native` and `posix` are refused with a hint naming `linux`
([Retired names](retired-names.md#retired-board-names)); names retired by
phase-437 are in the
[retired board names](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/reference/retired-board-names.md)
ledger.

### `nros init [<dir>]`

Generate a project `CMakePresets.json` (into `<dir>`, default the current
directory) that `include`s the per-board preset fragments `nros setup
<board>` wrote under `~/.nros/presets/` (RFC-0048 §6). After it,
`cmake --preset <board>` cross-configures a nano-ros ament package with no
hand-set `-DCMAKE_TOOLCHAIN_FILE` / `-Dnano_ros_ROOT`. Idempotent — re-run
after provisioning a new board to pick up its fragment.

```sh
nros setup qemu-armv7a-nuttx
nros init                 # writes ./CMakePresets.json
cmake --preset qemu-armv7a-nuttx
```

### `nros new <name> --platform <plat> [--rmw <rmw>] [--lang <lang>] [--use-case <case>] [--ros-edition <e>] [--force]`

Scaffold a new nano-ros project. Emits a colcon-compatible
`package.xml` plus a hello-world Rust / C / C++ skeleton tuned for the
chosen platform.

**Workspace mode:** `nros new <name> --workspace [--lang <lang>] [--rmw <rmw>]`
scaffolds a multi-node workspace instead — two node packages (`talker_pkg`,
`listener_pkg`) and a `demo_bringup` with its `system.toml` and launch file,
no root build file ([First Project](../getting-started/first-project.md)). It
defaults to `--lang cpp --rmw cyclonedds`, the combination that needs no
router, and builds for the host (`--platform linux`, the only one it accepts);
add embedded targets afterwards with `nros new --image <name> --board <b>`.

| Flag | Values | Default |
|---|---|---|
| `--platform` | `linux`, `freertos`, `nuttx`, `threadx`, `zephyr`, `baremetal` | (required, except with `--workspace`) |
| `--rmw` | `zenoh`, `xrce`, `cyclonedds` | `zenoh` (`cyclonedds` with `--workspace`) |
| `--lang` | `rust`, `c`, `cpp` | `rust` (`cpp` with `--workspace`) |
| `--ros-edition` | `humble`, `iron`, `jazzy` | `humble` |
| `--workspace` | scaffold the multi-node workspace above | off |
| `--use-case` | `talker`, `listener`, `service`, `action` | `talker` |
| `--force` | overwrite an existing directory | off |

`--platform native` and `--platform posix` are refused with a hint naming
`linux` — the host board's one name (phase-500).

**Image / host mode:** `nros new --image <name> [--board <b>]` scaffolds an `[image.<name>]` — a buildable image (RFC-0065 D6) — into the bringup package's `system.toml`; `nros new --host <name>` scaffolds a `[host.<name>]`, a machine nodes run on. Both accept `[--bringup <pkg>] [--from-launch <path>] [--from-profile <name>]`. The bringup package is discovered automatically when the workspace exposes exactly one; pass `--bringup <pkg>` to pick one when there are several. `--from-launch` also seeds the bringup `[system].default_launch`; `--from-profile` forks an existing block of the same table.

An image names a `--board` and derives its rustc triple from that board's descriptor, so there is no `--target`. A host takes no board: a host says WHERE nodes run, and a board is what an image is BUILT for — passing one is refused rather than written.

`nros new --deploy <name>` is DEPRECATED (issue 0951): `[deploy.*]` split into those two tables, and a `system.toml` that still carries one is refused by name (phase-486 W8). The flag still works, dispatched by the `--kind` it always took — `self` scaffolds a host, anything else an image — and says which flag to use instead. It never writes `[deploy.*]`.

### `nros generate <lang> [--manifest <path>] [--output <dir>] [--ros-edition <edition>] [--force] [--verbose] [--generate-config]`

Generate ROS 2 message bindings from a `package.xml`. Rust users should
prefer the direct `nros generate-rust` command; C and C++ users normally
use the CMake integration.

| Argument | Values | Default |
|---|---|---|
| `<lang>` | `rust`, `c`, `cpp`, `all` | (required) |
| `--manifest` | path to `package.xml` | `package.xml` |
| `--output` | output directory | `generated` |
| `--ros-edition` | `humble`, `iron`, `jazzy` | `humble` |
| `--codegen-config` | explicit per-field capacity config (`nros-codegen.toml`) | discovered by walking up |
| `--generate-config` | **legacy** — emit a `.cargo/config.toml` patch table (Rust only). Use `nros sync` | off |

### `nros generate-rust [--manifest <path>] [--output <dir>] [--ros-edition <e>] [--codegen-config <f>] [--rename <old=new>] [--force] [--verbose]`

The Rust-only codegen **primitive**. Same binding generation as
`nros generate rust`, and with **no side-effects**: it writes `generated/` and
nothing else. Use it when you want bindings and only bindings; use `nros sync`
for an actual project, because a leaf also needs its generated build settings.

`--generate-config` (alias `--config`), `--nano-ros-path` and `--nano-ros-git`
still parse and are **legacy**. They wrote a leaf `.cargo/config.toml` — the
file [RFC-0098](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0098-generated-leaf-build-config.md)
retired. The patch rows now live in the generated
`build/<image>/nros-cargo.toml` that `nros sync` writes, alongside the board's
cargo settings and the image's derived `[env]`, and cargo reads that through
`--config`. (`nros sync`'s own `--nano-ros-path` is documented in its help as a
no-op kept for back-compat.)

### `nros generate-px4-msgs`

Generate CDR-serializable `px4_msgs::msg::*` bindings from a PX4
`.msg` tree — the XRCE/PX4 interop path (see the
[PX4 starter](../getting-started/integration-px4.md)).

### `nros codegen <resolve-deps|cyclonedds-descriptors|entry> …`

**Internal build-tool interface** — invoked by the CMake integration and
`build.rs` scripts, not meant for direct use. Subcommands resolve
message-package dependency closures, emit CycloneDDS type descriptors,
and generate C/C++ entry glue. (`nros codegen-system` below is the
user-facing system bake.)

### `nros metadata <system_pkg> [--workspace <path>] [--out-dir <dir>] [--metadata <existing.json>] [--build [--nano-ros-workspace <path>]]` — walk a colcon-style workspace under `<workspace>/src/`
collecting component source metadata into
`build/<system_pkg>/nros/source-metadata.json`. The result feeds
`nros plan`. With `--build`, any declared component (`component_nros.toml`)
missing its source-metadata is produced by the **metadata-mode build**:
nano-ros compiles + runs the component against an in-memory recorder to
emit the JSON. `--build` needs the nano-ros workspace
(`--nano-ros-workspace` or `NROS_WORKSPACE`).

### `nros plan <system_pkg> <launch_file> [LAUNCH_ARGS...] [options]` — resolve a ROS 2 launch file (or a precomputed
`play_launch_parser` `record.json`) plus the source metadata into a
typed `build/<system_pkg>/nros/nros-plan.json` IR. Picks per-component
SchedContext bindings, node graph wiring, parameter remaps, and
generated-package layout.

| Flag | Use |
|---|---|
| `--record <file>` | Skip launch-file parsing; consume an existing `record.json` |
| `--workspace <path>` | Override the workspace root (default `$PWD`) |
| `--out-dir <dir>` | Override `build/<system_pkg>/nros/` |
| `--metadata <file>` | Reuse a prior `source-metadata.json` |
| `--manifest <file>` | ROS launch manifest YAML overlay |
| `--nros-toml <file>` | nano-ros deployment overlay (`nros.toml`) |

### `nros check [plan]`

Validate an `nros-plan.json` (default `build/nros/nros-plan.json`):
static checker — catches unconnected required topics, conflicting QoS,
missing parameters, and SchedContext binding errors before the platform
build runs. A `.toml` argument is instead validated as a **root `nros.toml`**
(the workspace deployment SSOT) — `[system]`/`[image.<id>]`/`[host.<name>]` shape,
default-deploy + system references, bridge endpoints, etc.

### `nros explain [<plan>]`

Render a generated `nros-plan.json` in human-readable form.

```sh
nros explain                            # reads build/nros/nros-plan.json
nros explain path/to/nros-plan.json     # explicit path
```

Run after `nros plan` to inspect the resolved component graph, topic
wiring, parameter bindings, and SchedContext assignments before
committing to a platform build.

### `nros config show --system <pkg> [--workspace <dir>]`

Prints the **resolved effective config** for a bringup system (`rmw`,
`domain_id`, `locator`, and the `safety` / `param_services` / `lifecycle`
capability axes) with a **per-value provenance column** — `system.toml
[section]` vs the built-in `default`. If a legacy per-package `nros.toml`
overlay still declares any of those blocks, it is flagged DEPRECATED by
name (RFC-0004 §3.1). Omit the `<pkg>` value to default to the workspace's
`default_system` (or the sole bringup); `--workspace` defaults to the cwd.
The `nros check` command surfaces the same overlay warnings when validating
a `system.toml`.

> The legacy `nros config show/check --config <path>` reader for `config.toml`
> was removed: `config.toml` is retired (RFC-0004 §8) and no example
> ships one. Embedded runtime config — board, RMW, domain, locator, IP — lives
> in `system.toml`: `[image.<id>]` and `[system]`. The
> `[package.metadata.nros.deploy.<board>]` manifest table that used to hold it
> is retired by RFC-0098 D5 and refused wherever it still appears.

### `nros config explain --platform <name> [--board-toml <f>] [--platforms-dir <d>]`

Print the resolved BUILD-time knob ladder for a platform: every knob's final
value plus the rung that set it (`builtin` / `platform` / `board` / `env`,
RFC-0049). This is the provenance tool for anything generated — when a pool
size or a hardware budget is not what you expected, it says which rung decided.
`--board-toml` supplies the board rung's `[knobs]` deltas by explicit path.

### `nros sync [<workspace>] [--build-dir <d>] [--ros-edition <e>] [--dry-run] [--check] [--base-paths <p>…]`

The pre-build step. Codegen every `*.msg` package the declared deps name, and
write the generated build settings for each declared image — for a **standalone
package** or a **colcon-style workspace** (it picks single-pkg vs workspace mode
by layout). Run it after editing `*.msg`, `package.xml` or `system.toml`.

```sh
eval "$(nros ws env)"   # add src/ to NROS_INTERFACE_SEARCH_PATH
nros sync               # generated/ msg crates + build/<image>/nros-cargo.toml
nros build              # or: nros build <image-id>
```

What it writes into `build/<image>/nros-cargo.toml`
([RFC-0098](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0098-generated-leaf-build-config.md)
D1): the board's `cargo_config` (its `[build] target`, `[target.*]`
rustflags/linker, `[unstable]`), this image's `target-dir`, its entity facts
and derived pool knobs as `[env]`, the in-repo `[patch.crates-io]` rows, and the
nano-ros cargo profiles. Nothing generated is written beside the package, and
`[env]` never uses `force = true`, so a value you set in the environment wins.

Transitionally, sync also still maintains the `[patch.crates-io]` block in each
Rust consumer's patch-authority `.cargo/config.toml` (never in `Cargo.toml`) —
that is the road phase-445 W6 removes, and it is what the `ws clean` / `ws
doctor` subcommands below still talk about.

`--check` exits non-zero when anything is missing or stale — the CI hook, and
what `nros ws status` reports non-fatally. `--dry-run` prints what it would do.

`nros generate-rust` stays the low-level codegen-only primitive (no side
effects beyond `generated/`).

### `nros build [<image>…] [--board <board>] [--workspace <dir>] [--all] [--dry-run] [--offline] [-v] [--check[=clippy]] [--packages-select <p>…] [--packages-up-to <p>…] [-- <native args>]`

The colcon-like build verb: discover packages, resolve the image, preflight,
generate the build root, then hand off to the native tool. The final stage is an
`exec`, so compiler diagnostics are byte-identical to running cargo / cmake /
west yourself. With no image argument it builds `[system] default_images`;
`--all` builds every declared image.

```sh
cd <leaf-or-workspace>
nros sync
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
  artifact: <leaf>/build/qemu/target/thumbv7m-none-eabi/debug/demo
  run:
    <store>/sdk/qemu/<ver>/bin/qemu-system-arm -cpu cortex-m3 -machine mps2-an385 … -kernel <artifact>
```

A host board prints the binary; a cargo board with a descriptor `runner` prints
that runner; NuttX and ThreadX riscv64 print the QEMU line the tests use; a
Zephyr image prints `west build -d <dir> -t run` and/or `west flash -d <dir>`,
read from what its build supports. QEMU is the store's patched build
(`nros locate tool qemu-system-arm`), never whatever `$PATH` holds. A board
with no derivable command (S32Z270) gets a pointer to its platform chapter.

`-v` / `--verbose` shows the configure's detail notes: why a re-configure
happened, which message bounds were refused and why. Without it each is one
line. A raw cmake run gets the same with `--log-level=VERBOSE`.

A build without a sync fails at the first preflight with one line naming
`nros sync`, rather than four frames down in cargo's manifest parser. `--dry-run`
prints the stages and the command that would run, and performs no I/O.

`--check` type-checks a cargo image instead of building it: every stage runs as
for a build (the image's settings file, its lock), and only the final command
becomes `cargo check`, or `cargo clippy` with `--check=clippy`. Lint flags go
after a second `--`. A cmake or west image is refused, since neither tool has a
check that is not a build. Like a build, a check needs `nros sync` first.

```sh
nros build linux --check                          # cargo check
nros build linux --check=clippy -- -- -D warnings # cargo clippy, warnings denied
```

In a single-package C/C++ leaf, `nros build` configures the leaf's own
`CMakeLists.txt` into `build/<image>/cmake` with `-DNROS_IMAGE=<id>` and the
board's toolchain file, then runs `cmake --build`: the leaf's own cmake pair,
with the inputs a bare configure cannot know
([issue 1296](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/archived/1296-nros-build-c-leaf-bringup-name-mismatch.md)). Zephyr keeps `west build` as its build verb, with
`nros sync` still run first.

Driving cargo yourself is supported — point it at the generated file, from the
directory above the package:

```sh
cargo build --manifest-path <leaf>/Cargo.toml \
            --config <leaf>/build/<image-id>/nros-cargo.toml
```

Run that from the directory above the package: `nros sync` also wires the same
file into the package's gitignored `.cargo/config.toml` (so a plain `cargo build`
inside the package works), and naming it again with `--config` from inside would
join the board's `rustflags` onto themselves.

### `nros materialize <image> [--workspace <dir>] [--force]`

Take ownership of a generated entry package (RFC-0065 D5) — the last resort,
after which `nros build` stops regenerating it. `panic`, `profile` and
`before_boot` are declarations on the image and need no materialising. A second run without
`--force` is refused, because it would discard the edits the first one existed
to enable.

### `nros ws <subcommand>`

Workspace-level message-package utilities (the `env` / `status` / `list` /
`clean` / `doctor` helpers; `sync` was promoted to top-level `nros sync`).

```sh
eval "$(nros ws env)"   # add src/ to NROS_INTERFACE_SEARCH_PATH
nros ws status          # freshness check (non-fatal): n up-to-date / n stale / n missing
nros ws list            # list discovered msg + Rust-consumer pkgs
nros ws clean           # remove generated/ + auto-managed patch entries
nros ws doctor          # lint workspace pkgs (package.xml markers, stale patches, …)
nros ws managed-deps    # write the `# nros-managed` message rows from package.xml
```

| Subcommand | Description |
|---|---|
| `env` | Print shell export adding `<dir>` (default `./src`) to `NROS_INTERFACE_SEARCH_PATH` |
| `status` | Non-fatal freshness check — sibling of `nros sync --check` |
| `list` | List discovered msg + Rust-consumer pkgs (kind, name, dir per row) |
| `clean` | Remove `generated/` + auto-managed `[patch.crates-io]` entries from each `.cargo/config.toml`; leaves user-written keys + sections alone |
| `managed-deps [--check]` | The one step `nros sync` and `nros build` both run: write each Rust package's `# nros-managed` `[dependencies]` rows — one per `<depend>` on a message package — without codegen. `--check` writes nothing and exits non-zero if a manifest would change |
| `doctor` | Lint: warn on missing `<member_of_group> rosidl_interface_packages</member_of_group>`, malformed `package.xml`, missing nros-managed `[patch.crates-io]` entries in the authority `.cargo/config.toml` |

### `nros codegen-system [--bringup <pkg>] [--target <id> | --for-entry <pkg>] [--out <dir>] [--launch <file>] [--ahead-of-vendor <pio|px4>]`

Host-time system bake: reads `<bringup>/system.toml` +
`<bringup>/launch/system.launch.xml` and emits the baked compile-time C
config + component registration glue consumed by every embedded RTOS
adapter.

```sh
nros codegen-system --bringup demo_bringup
nros codegen-system --bringup demo_bringup --target zephyr_native_sim  # an [image.<id>]
nros codegen-system --bringup demo_bringup --for-entry zephyr_app      # the image claiming it
nros codegen-system --bringup demo_bringup --ahead-of-vendor pio   # + PlatformIO library.json
nros codegen-system --bringup demo_bringup --ahead-of-vendor px4   # + PX4 module dirs
```

| Flag | Description |
|---|---|
| `--workspace <path>` | Workspace root (default: cwd) |
| `--bringup <pkg>` | Bringup pkg name or path. Defaults to `[workspace.metadata.nros].default_system` |
| `--target <id>` | The `[image.<id>]` this bake is for — it selects the domain, RMW, locator, launch and tier sub-table. A triple or platform name that matches no block selects nothing and every one of those falls back to the `[system]` default (issue 1312) |
| `--for-entry <pkg>` | Name the ENTRY PACKAGE instead (a name, or a path to its directory) and the `[image.*]` that claims it supplies the target. What a framework configure can answer: west/ESP-IDF know the application directory, never the image id. Mutually exclusive with `--target` |
| `--nano-ros-path <dir>` | nano-ros checkout holding `packages/boards`, for the board catalog a resolved target needs. Default: a walk up from the workspace, then this `nros` binary's own SDK root (a release's `share/nano-ros`, or the checkout an in-tree `nros` was built in) |
| `--out <dir>` | Output directory; `nros-system/` subdir created inside. Default: `<workspace>/build/<bringup>/` |
| `--launch <file>` | Multi-launch disambiguation: pick `<bringup>/launch/<file>` (`--file` is an alias) |
| `--exec <exec>` | `<node exec="…">` override for synthesised launches |
| `--ahead-of-vendor` | `pio`: emit PlatformIO `library.json`; `px4`: emit one PX4-native `nros_<component>/` module dir per component |

### `nros doctor [--platform <name>] [--workspace <path>]`

Shells out to `just doctor` from the auto-detected workspace root.
The justfile orchestrates every per-module doctor recipe (`just nuttx
doctor`, `just zephyr doctor`, …) and is the source of truth for
"healthy". `--platform <name>` scopes the check to a single module.

### `nros board list [--workspace <path>]`

Enumerate every `nros-board-*` crate under `packages/boards/`. Output
columns are `name | description`; structured `chip | flash | ram |
supported_rmw` fields are deferred to a future board-descriptor TOML.

### `nros sdk-root [--workspace <dir>] [--nano-ros-path <dir>] [--explain]`

Print the nano-ros SDK root this `nros` resolves — the directory holding the
runtime sources and `nano_rosConfig.cmake` that a build compiles against: an
explicit `--nano-ros-path`, else a nano-ros checkout found by walking up from
`--workspace` (default: the current directory), else this binary's own SDK
(a release's `share/nano-ros`, or the checkout an in-tree `nros` was built in).
`--explain` names the rung that answered on stderr; stdout stays the bare path.

```sh
nros sdk-root --explain
```

### `nros locate <name>… [--all] [--why] [--format <path|sh|cmake>] [--project <dir>]`

Where a resource is, and with `--why`, every rung of the one ladder (argument >
env > the project's `system.toml` `[sources]` > local edit > store > checkout)
and what each saw. A name is an index `[source.<name>]`, or `store` / `nano-ros`
for the two roots; `nros locate artifact <name>…` (`c-archive`, `cpp-archive`,
`c-include`, …, with `--target`, `--profile`, `--target-dir`, `--placed-dir`)
locates a cargo-built artifact, and `nros locate tool <name>` a first-party host
binary (`nros-launch-resolve`). `--all` locates every `[source.*]` row.

```sh
nros locate zenoh-pico --why
```

### `nros store <list|gc>`

Inspect and shrink the SDK store, which only grows: provisioning never
overwrites, so a new version lands beside the old one.

| Subcommand | Effect |
|---|---|
| `list [--root <dir>]` | every entry with its version, size and when it was last read |
| `gc --older-than <dur> [--delete] [--dry-run] [--pin-file <f>…] [--ignore-pins] [--root <dir>]` | propose removing entries not read for `<dur>` (`90d`, `12h`, `2w`); removes nothing without `--delete`, and never an entry a pin names |

### `nros toolchain <uninstall|manifest>`

| Subcommand | Effect |
|---|---|
| `uninstall <version> [--dry-run] [--pin-file <f>…] [--ignore-pins] [--root <dir>]` | remove one installed nano-ros toolchain; refuses while a pin names it, or when no pin file could be consulted |
| `manifest [--store-version <v>] [--index <date>] [--nano-ros <commit>] [--write <path>]` | read (or, with `--write`, stamp) the `share/nros/manifest.toml` a release declares it is made of |

### `nros pin [<version>] [--dir <dir>] [--root <dir>] [--dry-run]`

Report this project's `nros-toolchain.toml` pin, or move it to `<version>` (the
store version, e.g. `0.6.0-nros3`, not the crate version `nros --version`
prints). A move reports the codegen delta between the two toolchains first, so
an upgrade that would invalidate already-generated code warns before anything
is written.

### `nros update (--pins [--prune] | --crate <n> [--precise <v>] | --source <n> [--ref <sha>] [--allow-rewind] | --tool <n> [--to-version <v>] | --west <m> [--revision <r>]) [--nano-ros-path <dir>]`

Move ONE pin of a nano-ros checkout's pin set — a registry crate, a vendored
source, a host tool or a west project — and leave one diff to review.
`--pins` regenerates the registry pin set from the locked roots. A contributor
verb: an installed SDK's pin set is read-only.

### `nros version`

Print toolchain + library versions.

### `nros completions <shell>`

Emit shell completion scripts to stdout.

| Shell | Install snippet |
|---|---|
| `bash` | `nros completions bash > ~/.local/share/bash-completion/completions/nros` |
| `zsh` | `nros completions zsh > "${fpath[1]}/_nros"` |
| `fish` | `nros completions fish > ~/.config/fish/completions/nros.fish` |
| `powershell` | `nros completions powershell > $PROFILE.parent\nros.ps1` |

## Comparison: `nros` vs. `just`

| Want to … | Use |
|---|---|
| Scaffold a project | `nros new …` |
| Generate bindings + the image's build settings | `nros sync` (primitive: `nros generate-rust`) |
| Build | `nros build` — or the platform tool pointed at the generated settings (`cargo --config …`, `cmake --build`, `west build`) |
| Plan + check a multi-component system from a ROS 2 launch file | `nros metadata` → `nros plan` → `nros check` |
| Flash / run | Platform tools: `espflash`, `probe-rs`, `qemu-system-*`, `idf.py` |
| Orchestrate the checkout (contributor: setup, doctor, CI, multi-platform sweeps) | `just …` |

`nros` routes through the `nros-cli-core` library. `just` recipes that
wrap user-flow operations call into `nros` for consistency; internal
recipes (build matrices, CI orchestration) keep their current shape.

## Release pipeline status

The `nros` CLI is released as a GitHub Releases asset (`nros-<host>.tar.zst`,
carrying the CLI and its SDK root) and installed with `scripts/install.sh` —
[Installation](../getting-started/installation.md). There is no crates.io,
Arduino zip or ESP-IDF binary channel. (A hidden, maintainer-only
`nros release` verb exists behind the `release` cargo feature; the default
build does not carry it.)
Downstream RTOS package managers (Zephyr `west`,
ESP-IDF `idf_component_yml`, NuttX `Kconfig`) consume the same source
tree via the integration shells under `integrations/<rtos>/`.
