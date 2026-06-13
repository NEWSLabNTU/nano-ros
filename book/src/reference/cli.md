# `nros` CLI reference

The `nros` binary is the user entry point for nano-ros: scaffolding,
message codegen, SDK provisioning, topology planning, static checks,
diagnostics, and board introspection.

The old `cargo nano-ros …` cargo subcommand has been removed. Use `nros`
directly.

## Install

The `nros` CLI ships from the in-tree sub-workspace at `packages/cli/`
(Phase 218). Build it per checkout, then activate the workspace to put
it on PATH:

```sh
./scripts/bootstrap.sh base
source ./activate.sh        # OR: direnv allow / source ./activate.fish
```

The resulting binary lives at `packages/cli/target/release/nros`. One
checkout = one CLI version = one runtime ABI; contributors with
multiple nano-ros worktrees get per-tree CLIs with no global PATH
skew. Once on PATH, `nros --help` lists every verb. The transitional
`${NROS_HOME:-~/.nros}/bin/nros` install location remains supported
for users mid-migration from the pre-218 release-fetch shape.

## Verbs

### `nros setup [<board>] [--rmw <rmw>] [--tool <name>] [--source <name>] [--prefix <dir>] [--list] [--licenses] [--dry-run]`

Provision the toolchain/SDK for a board. `nros setup` is the single
canonical provisioning command — it ships **prebuilt toolchains per
platform per RMW** (cross-compiler, emulator, RMW host daemon, SDK
sources) from a pinned index into a shared store (`${NROS_HOME:-~/.nros}/sdk`). No
hand-installed cross-toolchains; no ROS distro required.

```sh
nros setup native --rmw zenoh            # host build + zenoh router
nros setup qemu-arm-freertos --rmw xrce  # arm-none-eabi-gcc, qemu, FreeRTOS, XRCE agent
nros setup zephyr                        # Zephyr west workspace + SDK bits
```

| Flag | Effect |
|---|---|
| `<board>` | resolve + fetch this board's package set (see [Supported Boards](supported-boards.md)) |
| `--rmw <zenoh\|xrce\|cyclonedds>` | also provision the RMW's host daemon/tool (default `zenoh`); resolves `board ∪ rmw` packages |
| `--tool <name>` | install a single tool by name (e.g. `--tool qemu`) |
| `--source <name>` | provision a single source package by name (repeatable) |
| `--prefix <dir>` | install a `--tool` here instead of the shared store |
| `--list` | list every package in the index + its version |
| `--licenses` | show license-gated packages + how to install them |
| `--dry-run` | resolve + print the plan without fetching anything |

Board names: `native`, `posix`, `qemu-arm-baremetal`, `mps2-an385`,
`stm32f4`, `qemu-arm-freertos`, `qemu-arm-nuttx`, `qemu-riscv64-threadx`,
`threadx-linux`, `esp32`, `qemu-esp32-baremetal`, `zephyr`, and more —
run `nros setup --list` or `nros setup <board> --dry-run`.

### `nros new <name> --platform <plat> [--rmw <rmw>] [--lang <lang>] [--use-case <case>] [--force]`

Scaffold a new nano-ros project. Emits a colcon-compatible
`package.xml` plus a hello-world Rust / C / C++ skeleton tuned for the
chosen platform.

| Flag | Values | Default |
|---|---|---|
| `--platform` | `native`, `freertos`, `nuttx`, `threadx`, `zephyr`, `esp32`, `posix`, `baremetal` | (required) |
| `--rmw` | `zenoh`, `xrce`, `cyclonedds` | `zenoh` |
| `--lang` | `rust`, `c`, `cpp` | `rust` |
| `--use-case` | `talker`, `listener`, `service`, `action` | `talker` |
| `--force` | overwrite an existing directory | off |

**Deploy mode:** `nros new --deploy <name> [--kind <runner>] [--target <triple>] [--board <b>] [--bringup <pkg>] [--from-launch <path>] [--from-profile <name>]` scaffolds a `[deploy.<name>]` target into the bringup package's `system.toml` (RFC-0004 §4 — the deploy-target SSOT), instead of a project. `--kind` is a free-form runner key (`self`, `qemu`, `flash`, …). The bringup package is discovered automatically when the workspace exposes exactly one; pass `--bringup <pkg>` to pick one when there are several. `--from-launch` also seeds the bringup `[system].default_launch`; `--from-profile` forks an existing `[deploy.<name>]` in the same `system.toml`.

### `nros generate <lang> [--manifest <path>] [--output <dir>] [--ros-edition <edition>] [--force] [--verbose] [--generate-config]`

Generate ROS 2 message bindings from a `package.xml`. Rust users should
prefer the direct `nros generate-rust` command; C and C++ users normally
use the CMake integration.

| Argument | Values | Default |
|---|---|---|
| `<lang>` | `rust`, `c`, `cpp`, `all` | (required) |
| `--manifest` | path to `package.xml` | `package.xml` |
| `--output` | output directory | `generated` |
| `--ros-edition` | `humble`, `iron` | `humble` |
| `--generate-config` | emit `.cargo/config.toml` patches (Rust only) | off |

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
(the workspace deployment SSOT) — `[system]`/`[deploy.<name>]` shape,
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

### `nros config show [--config <path>]` / `nros config check [--config <path>]`

`show` parses the project's `nros.toml` (and any Kconfig overlay
on Zephyr) and pretty-prints it, plus reports any `ROS_DOMAIN_ID`
env override.

`check` validates `nros.toml` syntactically and warns when the
locator or domain are missing. Exits non-zero on warnings.

### `nros ws <subcommand>`

Workspace-level message-package utilities. Manages codegen for
`*.msg` packages in a colcon-style `src/` tree and keeps
`[patch.crates-io]` blocks in sync across Rust consumers.

```sh
eval "$(nros ws env)"   # add src/ to NROS_INTERFACE_SEARCH_PATH
nros ws sync            # run codegen for all msg pkgs + write [patch.crates-io]
nros ws status          # freshness check (non-fatal): n up-to-date / n stale / n missing
nros ws list            # list discovered msg + Rust-consumer pkgs
nros ws clean           # remove generated/ + auto-managed patch blocks
nros ws doctor          # lint workspace pkgs (package.xml markers, stale patches, …)
```

| Subcommand | Description |
|---|---|
| `env` | Print shell export adding `<dir>` (default `./src`) to `NROS_INTERFACE_SEARCH_PATH` |
| `sync` | Codegen workspace msg pkgs + write `[patch.crates-io]` into each Rust consumer's patch authority `Cargo.toml`. Pre-cargo step; run once after editing `*.msg` files |
| `status` | Non-fatal freshness check — sibling of `sync --check` |
| `list` | List discovered msg + Rust-consumer pkgs (kind, name, dir per row) |
| `clean` | Remove `generated/` + auto-managed `[patch.crates-io]` blocks; leaves user-written sections alone |
| `doctor` | Lint: warn on missing `<member_of_group> rosidl_interface_packages</member_of_group>`, malformed `package.xml`, stale patch blocks |

### `nros codegen-system [--bringup <pkg>] [--target <triple>] [--out <dir>] [--launch <file>] [--ahead-of-vendor <pio|px4>]`

Host-time system bake: reads `<bringup>/system.toml` +
`<bringup>/launch/system.launch.xml` and emits the baked compile-time C
config + component registration glue consumed by every embedded RTOS
adapter.

```sh
nros codegen-system --bringup demo_bringup
nros codegen-system --bringup demo_bringup --target thumbv7em-none-eabihf
nros codegen-system --bringup demo_bringup --ahead-of-vendor pio   # + PlatformIO library.json
nros codegen-system --bringup demo_bringup --ahead-of-vendor px4   # + PX4 module dirs
```

| Flag | Description |
|---|---|
| `--workspace <path>` | Workspace root (default: cwd) |
| `--bringup <pkg>` | Bringup pkg name or path. Defaults to `[workspace.metadata.nros].default_system` |
| `--target <triple>` | Target triple for cross-compile bake context |
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
| Generate Rust bindings | `nros generate-rust` |
| Plan + check a multi-component system from a ROS 2 launch file | `nros metadata` → `nros plan` → `nros check` |
| Build / flash / run | Platform tools: `cargo`, `cmake --build`, `west`, `idf.py`, `probe-rs`, or focused `just <platform> …` recipes |
| Orchestrate the workspace (setup, doctor, CI, multi-platform sweeps) | `just …` |

`nros` routes through the `nros-cli-core` library. `just` recipes that
wrap user-flow operations call into `nros` for consistency; internal
recipes (build matrices, CI orchestration) keep their current shape.

## Release pipeline status

There is no `nros release` verb and no crates.io / Arduino zip /
ESP-IDF binary / GitHub Releases tarball channel. Per the archive
decision (2026-05-19), nano-ros is consumed by
`git clone --branch=v<X.Y.Z>` plus the in-tree build recipes
documented at [Installation](../getting-started/installation.md).
Downstream RTOS package managers (Zephyr `west`,
ESP-IDF `idf_component_yml`, NuttX `Kconfig`) consume the same source
tree via the integration shells under `integrations/<rtos>/`.
