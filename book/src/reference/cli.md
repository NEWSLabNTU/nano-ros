# `nros` CLI reference

The `nros` binary is the user entry point for nano-ros: scaffolding,
message codegen, configuration, build, run, diagnostics, and
workflow orchestration on top of ROS 2 launch files.

The old `cargo nano-ros …` cargo subcommand has been removed. Use `nros`
directly.

## Install

The `nros` CLI ships from the in-tree sub-workspace at `packages/cli/`
(Phase 218). Build it per checkout, then activate the workspace to put
it on PATH:

```sh
source ./activate.sh        # OR: direnv allow / source ./activate.fish
just setup-cli              # builds packages/cli/target/release/nros
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

**Deploy mode:** `nros new --deploy <name> --kind <self|vendor-lib|vendor-module> [--target <triple>] [--board <b>] [--from-launch <path>] [--from-profile <name>]` scaffolds a `[deploy.<name>]` target into the root `nros.toml` (and, for vendor kinds, a `deploy/<name>/` glue dir), instead of a project. `--from-launch` also seeds `[system].launch`; `--from-profile` forks an existing deploy target.

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
missing parameters, and SchedContext binding errors before `nros build`
runs. A `.toml` argument is instead validated as a **root `nros.toml`**
(the workspace deployment SSOT) — `[system]`/`[deploy.<name>]` shape,
default-deploy + system references, bridge endpoints, etc.

### `nros deploy [name] [--config <root.toml>] [--nano-ros-workspace <path>] [--dry-run]`

Run a `[deploy.<name>]` target from the root `nros.toml`. Omit
`name` to use `[workspace].default`. The runner asserts the vendor pin →
generates + builds the **entry lib** (the system wiring as a library) →
runs the target's `build[]` then `package[]` shell steps, substituting
`{self}` / `{entry_lib}` / `{entry_src}` / `{entry_header}` / `{board}` /
`{target}` / `{vendor.dir}`. Three build-ownership kinds: `self` (nano-ros
builds the binary), `vendor-lib` (links a vendor static lib), `vendor-module`
(the vendor's `make`/`west`/`idf.py` compiles the entry source). `--dry-run`
prints the resolved steps without generating/building. No per-vendor code
lives in nano-ros — vendor knowledge is the user's `build[]`/`package[]` lines.

### `nros config show [--config <path>]` / `nros config check [--config <path>]`

`show` parses the project's `nros.toml` (and any Kconfig overlay
on Zephyr) and pretty-prints it, plus reports any `ROS_DOMAIN_ID`
env override.

`check` validates `nros.toml` syntactically and warns when the
locator or domain are missing. Exits non-zero on warnings.

### `nros build [--project <path>] [-- ...]`

Auto-detect the project flavor and delegate. Detection precedence:

1. `prj.conf` present → Zephyr → `west build`
2. `CMakeLists.txt` + Cargo `staticlib` → cmake configure + build
3. `Cargo.toml` present → `cargo build`
4. plain `CMakeLists.txt` → `cmake -B build && cmake --build build`

Trailing arguments after `--` forward verbatim to the underlying tool.
Builds consume nano-ros via `add_subdirectory(<repo-root>)`
— there is no install layout to find first.

`nros build <name>` (in a workspace with a root `nros.toml`) is an alias
for `nros deploy <name>`; bare `nros build` there builds
`[workspace].default`. A component `nros.toml` (direct-mode `[node]`) is
not a workspace root, so it falls through to the project-flavor autodetect
above.

### `nros run [--project <path>] [--env <name>] [-- ...]`

Build → flash → monitor in one verb.

| Detected target | Action |
|---|---|
| Cargo + native | `cargo run` (default bin) |
| Cargo + `xtensa-esp32*` / `riscv32imc*` (from `.cargo/config.toml`) | `espflash flash --monitor` |
| Zephyr / cmake / QEMU multi-target | not yet wired — see `just <plat> run` recipes |

### `nros monitor [--env <name>] [-- ...]`

v1 surfaces `espflash monitor`. ARM RTT (`defmt-print`) and QEMU
semihosting decoders land (`nros-log`).

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
| Scaffold / build / run a single project | `nros …` |
| Generate Rust bindings | `nros generate-rust` |
| Plan + check a multi-component system from a ROS 2 launch file | `nros metadata` → `nros plan` → `nros check` → `nros build` |
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
