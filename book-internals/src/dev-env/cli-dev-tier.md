# The CLI's developer tier

The user-facing `nros` verbs are documented in the user book's
[CLI reference](../../reference/cli.html). This page holds the rest: building
the CLI from a checkout, the hidden `nros dev …` verbs that cmake, west,
`build.rs` and `just` call, the contributor-only `nros update`, and how `nros`
relates to the `just` recipes. `nros help --all` and `nros dev --help` list
the dev tier (RFC-0117 D1, D3).

## Building the CLI from the checkout

Inside a nano-ros checkout, build the CLI from the checkout. That is not a
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

`nros doctor` run inside a checkout also forwards to `just doctor` (or
`just <platform> doctor` with `--platform`) after its own native checks; when
`just` is not on PATH it says it skipped those contributor checks and still
exits on the native result.

## Dev-tier verbs

### `nros dev codegen rust [--manifest <path>] [--output <dir>] [--ros-edition <e>] [--codegen-config <f>] [--rename <old=new>] [--force] [--verbose]`

The Rust-only codegen **primitive**. Same binding generation as
`nros interface generate rust`, and with **no side-effects**: it writes `generated/` and
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

### `nros dev codegen <resolve-deps|cyclonedds-descriptors|entry> …`

**Internal build-tool interface** — invoked by the CMake integration and
`build.rs` scripts, not meant for direct use. Subcommands resolve
message-package dependency closures, emit CycloneDDS type descriptors,
and generate C/C++ entry glue. (`nros dev codegen system` below is the
user-facing system bake.)

### `nros dev metadata <system_pkg> [--workspace <path>] [--out-dir <dir>] [--metadata <existing.json>] [--build [--nano-ros-workspace <path>]]` — walk a colcon-style workspace under `<workspace>/src/`
collecting component source metadata into
`build/<system_pkg>/nros/source-metadata.json`. The result feeds
`nros plan`. With `--build`, any declared component (`component_nros.toml`)
missing its source-metadata is produced by the **metadata-mode build**:
nano-ros compiles + runs the component against an in-memory recorder to
emit the JSON. `--build` needs the nano-ros workspace
(`--nano-ros-workspace` or `NROS_WORKSPACE`).

### `nros ws <subcommand>`

Workspace-level message-package utilities (the `env` / `status` / `list` /
`clean` / `doctor` helpers; `sync` was promoted to top-level `nros sync`).

```sh
eval "$(nros dev ws env)"   # add src/ to NROS_INTERFACE_SEARCH_PATH
nros ws status          # freshness check (non-fatal): n up-to-date / n stale / n missing
nros list            # list discovered msg + Rust-consumer pkgs
nros clean           # remove generated/ + auto-managed patch entries
nros ws doctor          # lint workspace pkgs (package.xml markers, stale patches, …)
nros dev lock managed-deps    # write the `# nros-managed` message rows from package.xml
```

| Subcommand | Description |
|---|---|
| `env` | Print shell export adding `<dir>` (default `./src`) to `NROS_INTERFACE_SEARCH_PATH` |
| `status` | Non-fatal freshness check — sibling of `nros sync --check` |
| `list` | List discovered msg + Rust-consumer pkgs (kind, name, dir per row) |
| `clean` | Remove `generated/` + auto-managed `[patch.crates-io]` entries from each `.cargo/config.toml`; leaves user-written keys + sections alone |
| `managed-deps [--check]` | The one step `nros sync` and `nros build` both run: write each Rust package's `# nros-managed` `[dependencies]` rows — one per `<depend>` on a message package — without codegen. `--check` writes nothing and exits non-zero if a manifest would change |
| `doctor` | Lint: warn on missing `<member_of_group> rosidl_interface_packages</member_of_group>`, malformed `package.xml`, missing nros-managed `[patch.crates-io]` entries in the authority `.cargo/config.toml` |

### `nros dev codegen system [--bringup <pkg>] [--target <id> | --for-entry <pkg>] [--out <dir>] [--launch <file>] [--ahead-of-vendor <pio|px4>]`

Host-time system bake: reads `<bringup>/system.toml` +
`<bringup>/launch/system.launch.xml` and emits the baked compile-time C
config + component registration glue consumed by every embedded RTOS
adapter.

```sh
nros dev codegen system --bringup demo_bringup
nros dev codegen system --bringup demo_bringup --target zephyr_native_sim  # an [image.<id>]
nros dev codegen system --bringup demo_bringup --for-entry zephyr_app      # the image claiming it
nros dev codegen system --bringup demo_bringup --ahead-of-vendor pio   # + PlatformIO library.json
nros dev codegen system --bringup demo_bringup --ahead-of-vendor px4   # + PX4 module dirs
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

### `nros dev sdk root [--workspace <dir>] [--nano-ros-path <dir>] [--explain]`

Print the nano-ros SDK root this `nros` resolves — the directory holding the
runtime sources and `nano_rosConfig.cmake` that a build compiles against: an
explicit `--nano-ros-path`, else a nano-ros checkout found by walking up from
`--workspace` (default: the current directory), else this binary's own SDK
(a release's `share/nano-ros`, or the checkout an in-tree `nros` was built in).
`--explain` names the rung that answered on stderr; stdout stays the bare path.

```sh
nros dev sdk root --explain
```

## Dev-tier verbs the user book no longer names

The user book teaches only the user tier (RFC-0117 D2, RFC-0123 D9). These
dev-tier verbs used to appear in user chapters; the user step that replaced
each is named beside it.

| Dev-tier verb | What it does | What the user book says instead |
|---|---|---|
| `nros dev sdk root [--explain]` | print the SDK root this `nros` resolves (section above) | `nros locate nano-ros`, the same ladder; a raw `cmake` can also use `nros setup presets` + `cmake --preset <board>` |
| `nros dev sdk path <tool> [--require]` | print a store tool's prefix (`qemu`, `zephyr-sdk`, `arm-fvp`) | `nros locate tool qemu-system-<arch>` for QEMU; the store path `~/.nros/sdk/<tool>/<version>` for the rest |
| `nros dev sdk manifest` (old `nros toolchain manifest`) | read, or with `--write` stamp, the `share/nros/manifest.toml` a release declares; the release workflow calls it | nothing: a release-pipeline verb |
| `nros dev codegen rust` | Rust bindings only, no side effects (section above) | `nros interface generate rust`, or `nros sync` |
| `nros dev rmw session-properties <bringup> --domain <name> --symbol <ident> --out <header>` | lower a bridge's session config file to a C/C++ header without CMake | the CMake road only |
| `nros dev lock managed-deps [--check]` | write the `# nros-managed` message rows into each Rust package, alone | `nros sync` (which runs it) |
| `nros dev image entity-inventory --metadata <json> --model <yaml>` | print the composed entity inventory an image is sized from | nothing |
| `nros dev image entity-census take --image <bringup>:<image>` | take a cross image's census before a hand-run `west build` / `cmake` | `nros build` takes it; a hand-run configure refuses and prints this command |
| `nros dev image facts` | per-image facts, including `cmake_build_dir` | the command `nros build --dry-run` prints |
| `nros dev image model-path` | where a resolved SystemModel landed | `build/nros/models/<bringup>/` (default) |
| `nros dev profile resolve --build-type <T>` / `nros dev profile dir <name>` | the cargo profile for a CMake build type, and its `target/` subdirectory | the configure's own `-- nano-ros: cargo profile …` line |
| `nros dev board cmake-vars` | project a board descriptor into cmake variables (`nano_ros_use_board()` runs it) | "asks the `nros` CLI" |
| `nros dev metadata [--build]` | collect component source metadata for `nros plan` (section above) | nothing |
| `nros ws qemu-nic <leaf> --model <nic-model>` (hidden `ws` subcommand) | print the QEMU `-nic` value for a leaf's image | `nros run` passes it to QEMU |

## Old spellings

`nros help --aliases` prints every older spelling that still parses and the
spelling it stands for. The table lives in
`packages/cli/nros-cli-core/src/cmd/user.rs` (`ALIASES`, user tier) and
`dev.rs` (dev tier). An old *user* spelling prints one note on stderr naming
the new one, only when stderr is a terminal; a dev-tier alias prints nothing.

| Old spelling | Now |
|---|---|
| `nros init` | `nros setup presets` |
| `nros store list` / `nros store gc` | `nros toolchain list` / `nros toolchain gc` |
| `nros toolchain uninstall` | `nros toolchain remove` |
| `nros toolchain manifest` | `nros dev sdk manifest` |
| `nros generate` | `nros interface generate` |
| `nros generate-rust` | `nros dev codegen rust` |
| `nros ws list` / `nros ws clean` | `nros list` / `nros clean` |
| `nros sdk-root`, `nros sdk-path`, `nros sdk-front` | `nros dev sdk root`, `nros dev sdk path`, `nros dev sdk front` |
| `nros codegen`, `nros codegen-system` | `nros dev codegen`, `nros dev codegen system` |
| `nros image-facts`, `nros model-path` | `nros dev image facts`, `nros dev image model-path` |
| `nros ws leaf-system`, `board-facts`, `sizing-descriptor`, `west-leaf-sizing`, `entity-inventory`, `entity-census`, `model-dims`, `message-bound-knobs` | `nros dev image <same>` |
| `nros ws rmw-dispatch`, `rmw-config`, `session-properties` | `nros dev rmw dispatch`, `config`, `session-properties` |
| `nros ws seed-lock`, `check-lock`, `managed-deps`, `central-patch` | `nros dev lock seed`, `check`, `managed-deps`, `central-patch` |
| `nros ws env`, `order`, `providers` | `nros dev ws env`, `order`, `providers` |
| `nros profile` | `nros dev profile` |
| `nros board cmake-vars`, `nros board net` | `nros dev board cmake-vars`, `nros dev board net` |
| `nros metadata` | `nros dev metadata` |

`nros toolchain manifest`: with no arguments it reads the manifest of the
toolchain the binary belongs to; `--write <path>` (with `--store-version`,
`--index <date>`, `--nano-ros <commit>`) stamps one. The release workflow
calls the binary it just built, so a manifest cannot claim a codegen version
its own binary does not emit.

## Front-door gaps

A user step with no user-tier `nros` spelling. The user book either works
around each (named below) or says no command exists yet. Each wants a
front-door verb or an explicit decision that it stays out of the user tier.

- **The SDK root — closed (phase-532 W9).** `nros locate nano-ros` is the
  user spelling: it answers through the `nano_ros_root` ladder anywhere, and
  in a pinned project it is the pinned version's root. The user book uses
  `"$(nros locate nano-ros)"` wherever a host tool needs the path. The
  scaffolds `nros new` writes and the probe fences still spell
  `nros dev sdk root`, which gives the same answer.
- **A store tool's directory.** `nros locate tool` covers `qemu-system-*`
  only. The Zephyr SDK (`ZEPHYR_SDK_INSTALL_DIR`) and the Arm FVP model
  directory (`ARMFVP_BIN_PATH`) have no user-tier spelling.
- **The boot-report reader** (below).
- **The mem-report tool** ([mem-report](../realtime/mem-report.md)).
- **A QEMU `-nic` value for a script that starts QEMU itself**
  (`nros ws qemu-nic`).
- **The census, for a hand-run west/cmake build** (`nros dev image
  entity-census take`); the configure prints the command when it refuses.
- **The NuttX apps-tree staging** is a script in the SDK root
  (`scripts/nuttx/stage-external-apps.sh`), not a verb.

## Decoding a boot report

The user book's *Observing a Board With No Console* page saves the
`NROS_BOOT_REPORT` record with a debug probe; decoding it needs the
repository's reader:

```sh
read ADDR LEN <<< "$(python3 scripts/read-boot-report.py --addr-only build/zephyr/zephyr.elf)"
pyocd commander -t s32k344 -c halt -c "savemem $ADDR $LEN report.bin"
python3 scripts/read-boot-report.py build/zephyr/zephyr.elf report.bin
```

`--addr-only` prints the address and length from the ELF's own symbol table,
so a relinked image is never read at a stale address. `--heap-headroom
<dump>` is the heap-headroom gate; `--help` lists the fields.

## Contributor verbs

### `nros update (--pins [--prune] | --crate <n> [--precise <v>] | --source <n> [--ref <sha>] [--allow-rewind] | --tool <n> [--to-version <v>] | --west <m> [--revision <r>]) [--nano-ros-path <dir>]`

Move ONE pin of a nano-ros checkout's pin set — a registry crate, a vendored
source, a host tool or a west project — and leave one diff to review.
`--pins` regenerates the registry pin set from the locked roots. A contributor
verb: an installed SDK's pin set is read-only.

## `nros` and `just`

### `nros` vs. `just`

| Want to … | Use |
|---|---|
| Scaffold a project | `nros new …` |
| Generate bindings + the image's build settings | `nros sync` (primitive: `nros dev codegen rust`) |
| Build | `nros build` — or the platform tool pointed at the generated settings (`cargo --config …`, `cmake --build`, `west build`) |
| Plan + check a multi-component system from a ROS 2 launch file | `nros dev metadata` → `nros plan` → `nros check` |
| Flash / run | Platform tools: `espflash`, `probe-rs`, `qemu-system-*`, `idf.py` |
| Orchestrate the checkout (contributor: setup, doctor, CI, multi-platform sweeps) | `just …` |

`nros` routes through the `nros-cli-core` library. `just` recipes that
wrap user-flow operations call into `nros` for consistency; internal
recipes (build matrices, CI orchestration) keep their current shape.

### Release pipeline status

The `nros` CLI is released as a GitHub Releases asset (`nros-<host>.tar.zst`,
carrying the CLI and its SDK root) and installed with `scripts/install.sh` —
[Installation](../../get-started/installation.html). There is no crates.io,
Arduino zip or ESP-IDF binary channel. (A hidden, maintainer-only
`nros release` verb exists behind the `release` cargo feature; the default
build does not carry it.)
Downstream RTOS package managers (Zephyr `west`,
ESP-IDF `idf_component_yml`, NuttX `Kconfig`) consume the same source
tree via the integration shells under `integrations/<rtos>/`.
