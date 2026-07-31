# packages/cli — the `nros` CLI sub-workspace

Agent guide for the in-tree nano-ros CLI. This directory is its **own Cargo
workspace** (own `Cargo.toml`/`Cargo.lock`, separate from the repo root
workspace). The parent repo's `CLAUDE.md` + `AGENTS.md` practices apply here
too; this file only adds what is CLI-specific.

> History: this rewrite replaces the retired "colcon-cargo-ros2 Development
> Guide" that previously lived here (issue #210) — that text described the
> archived standalone `nros-cli` repo, not this tree.

## Build & test

- **Build via `just setup-cli`** (from the repo root) — produces
  `packages/cli/target/release/nros`. `source ./activate.sh` puts it on PATH.
  Never install a `nros` to `~/.nros/bin` and let it shadow the in-tree one
  (stale-CLI drift breaks workspace planning; the fixture builder errors on
  a stale binary).
- A `nros` rebuild **stales every workspace fixture** in the parent repo
  (the codegen tool is part of the fixture input signature) — after
  rebuilding, rebuild affected fixture families rather than debugging
  "runtime" failures.
- Tests: `cargo test`/`cargo nextest run` inside `packages/cli/`. E2E
  workspaces for orchestration tests live in `testing_workspaces/`.
- Version is **lockstep** with the root workspace (`workspace.package.version`
  here MUST equal the root's; `scripts/check-version-lockstep.sh` gates it;
  bump only via `just release-bump <X.Y.Z>`).

## Crate map (workspace members)

| Crate | Role |
| --- | --- |
| `nros-cli` | the `nros` binary (thin main over `nros-cli-core`) |
| `nros-cli-core` | all verb implementations: `cmd/` (setup, init, new, generate*, codegen, metadata, plan, check, explain, config, sync, ws, doctor, board), `codegen/` (entry emitters, metadata seam), `orchestration/` (workspace planning, sdk store, cmake presets), `abi_guard.rs` (CLI↔lock version check) |
| `rosidl-parser` / `rosidl-codegen` / `rosidl-bindgen` | ROS interface parsing + Rust/C/C++ message generation |
| `nros-msg-to-idl` | msg → Cyclone IDL lowering |
| `nros-pkg-index` | `nros-sdk-index.toml` model (boards → toolchain/SDK package sets) |
| `nros-launch-parser` | launch-file → plan resolution (consumes the resolver's SystemModel; does NOT shell out to `play_launch_parser` — RFC-0060) |
| `nros-launch-resolve` | the `nros-launch-resolve` binary: launch tree → SystemModel (RFC-0060 layer 2, wraps `third-party/ros-launch-resolve`). Built by `just setup-launch-resolve`; `nros sync` invokes it by ABSOLUTE path, never via `$PATH` (issue 0285) |
| `nros-build` | build-script codegen library for Entry pkgs |
| `cargo-nano-ros` | cargo subcommand front-end |
| `colcon-cargo-ros2/` | the `colcon_nano_ros` Python colcon extension (pyproject; not a Rust member) |

Nested submodules: `third-party/ros-launch-resolve` (which itself nests
`ros-launch-manifest` + `parser`, so a scoped init must be `--recursive`) and
`testing_workspaces/ros2_rust_examples`. RFC-0060 retired the direct
`third-party/{play_launch_parser, ros-launch-manifest}` pins — there is now ONE
copy of the rlm spec, reached through the resolver.

## Design rules

- **`nros` is a generic tool** — it must not learn the nano-ros directory
  layout. Board/toolchain/source knowledge lives in the parent repo's
  `nros-sdk-index.toml`; fixes are index edits, not CLI special cases.
- The CLI is consumed by the parent repo's CMake (`nano_ros_*` functions
  shell out to it) and by `nros sync` — changes to verb output formats are
  cross-repo API changes; check `cmake/` consumers before changing them.
- Design rationale lives in the parent repo's RFCs (RFC-0014 provisioning,
  RFC-0023 message generation, RFC-0048 ament/CMake + presets); don't
  duplicate it here.
