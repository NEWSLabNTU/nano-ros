# `nros` CLI reference

The `nros` binary is the canonical user entry point for nano-ros. It
wraps scaffolding, message codegen, configuration, build, run, and
diagnostics into a single command. Phase 111 (Pillar A) ships the
verbs documented below; Pillar B (release pipeline) is a follow-up.

The `cargo nano-ros …` cargo subcommand keeps working — it dispatches
through the same `nros-cli-core` library, so the two front-ends stay
in lockstep.

## Install

`nros` is a workspace binary; until the crates.io release pipeline
lands (Phase 111 Pillar B), build it from a clone:

```sh
cd packages/codegen/packages
cargo install --path nros-cli
```

Once installed, run `nros --help` to list every verb.

## Verbs

### `nros new <name> --platform <plat> [--rmw <rmw>] [--lang <lang>] [--use-case <case>] [--force]`

Scaffold a new nano-ros project. Emits a colcon-compatible
`package.xml` plus a hello-world Rust / C / C++ skeleton tuned for the
chosen platform.

| Flag | Values | Default |
|---|---|---|
| `--platform` | `native`, `freertos`, `nuttx`, `threadx`, `zephyr`, `esp32`, `posix`, `baremetal` | (required) |
| `--rmw` | `zenoh`, `xrce`, `dds` | `zenoh` |
| `--lang` | `rust`, `c`, `cpp` | `rust` |
| `--use-case` | `talker`, `listener`, `service`, `action` | `talker` |
| `--force` | overwrite an existing directory | off |

> RMW choice + use-case template diversification surface in the
> "Next steps" banner today; full per-use-case template trees land
> alongside the Phase 112 example sweep.

### `nros generate <lang> [--manifest <path>] [--output <dir>] [--ros-edition <edition>] [--force] [--verbose] [--generate-config]`

Generate ROS 2 message bindings from a `package.xml`. Wraps the
existing `cargo nano-ros generate-{rust,c}` surface byte-for-byte.

| Argument | Values | Default |
|---|---|---|
| `<lang>` | `rust`, `c`, `cpp`, `all` | (required) |
| `--manifest` | path to `package.xml` | `package.xml` |
| `--output` | output directory | `generated` |
| `--ros-edition` | `humble`, `iron` | `humble` |
| `--generate-config` | emit `.cargo/config.toml` patches (Rust only) | off |

### `nros config show [--config <path>]` / `nros config check [--config <path>]`

`show` parses the project's `config.toml` and pretty-prints it,
plus reports any `ROS_DOMAIN_ID` env override.

`check` validates `config.toml` syntactically and warns when
`zenoh.locator` or `zenoh.domain_id` are missing. Exits non-zero on
warnings.

> Kconfig (Zephyr) values + the auto-generated `nros_app_config.h`
> struct land with Phase 112.D.

### `nros build [--project <path>] [-- ...]`

Auto-detect the project flavor and delegate. Detection precedence:

1. `prj.conf` present → Zephyr → `west build`
2. `CMakeLists.txt` + Cargo `staticlib` → cmake configure + build
3. `Cargo.toml` present → `cargo build`
4. plain `CMakeLists.txt` → `cmake -B build && cmake --build build`

Trailing arguments after `--` forward verbatim to the underlying tool.

### `nros run [--project <path>] [--env <name>] [-- ...]`

Build → flash → monitor in one verb.

| Detected target | Action |
|---|---|
| Cargo + native | `cargo run` (default bin) |
| Cargo + `xtensa-esp32*` / `riscv32imc*` (from `.cargo/config.toml`) | `espflash flash --monitor` |
| Zephyr / cmake / QEMU multi-target | not yet wired — see `just <plat> run` recipes |

### `nros monitor [--env <name>] [-- ...]`

v1 surfaces `espflash monitor`. ARM RTT (`defmt-print`) and QEMU
semihosting decoders land alongside Phase 88 (`nros-log`).

### `nros doctor [--platform <name>] [--workspace <path>]`

Shells out to `just doctor` from the auto-detected workspace root.
The justfile orchestrates every per-module doctor recipe (`just nuttx
doctor`, `just zephyr doctor`, …) and is the source of truth for
"healthy". `--platform <name>` scopes the check to a single module
(`just <platform> doctor`).

### `nros board list [--workspace <path>]`

Enumerate every `nros-board-*` crate under `packages/boards/`. Output
columns today are `name | description`; structured `chip | flash | ram |
supported_rmw` fields are deferred to UX-42 (board descriptor TOML).

### `nros version`

Print toolchain and library versions.

### `nros completions <shell>`

Emit shell completion scripts to stdout.

| Shell | Install snippet |
|---|---|
| `bash` | `nros completions bash > ~/.local/share/bash-completion/completions/nros` |
| `zsh` | `nros completions zsh > "${fpath[1]}/_nros"` |
| `fish` | `nros completions fish > ~/.config/fish/completions/nros.fish` |
| `powershell` | `nros completions powershell > $PROFILE.parent\nros.ps1` |

### `nros release …` (maintainer-only)

Compiled in only when `nros` is built with `--features release`;
hidden from `--help` otherwise. Phase 111 Pillar B fills in
`detect`, `publish`, `tag`, `c-libs`.

## Comparison: `nros` vs. `cargo nano-ros` vs. `just`

| Want to … | Use |
|---|---|
| Scaffold / build / run a single project | `nros …` |
| Generate bindings inside an existing Cargo workflow | `cargo nano-ros generate-{rust,c}` |
| Orchestrate the workspace (setup, doctor, CI, multi-platform sweeps) | `just …` |

`nros` and `cargo nano-ros` route through the same `nros-cli-core`
library, so they always agree on output. `just` recipes that wrap
user-flow operations call into `nros` for consistency; internal
recipes (build matrices, CI orchestration) keep their current shape.
