---
id: 2299
title: "Every Rust host-tool build warns \"patch … was not used\" three times per cargo call, because sync's central patch set assumes every consumer names `nros`"
status: resolved
type: bug
area: [cli, build]
severity: low
found: 2026-11-06
resolved: 2026-11-09
resolved_in: "host_tool::plan runs a host tool's cargo outside the workspace on the toolchain resolved at the package (preflight::toolchain_at)"
related: [phase-527, phase-287, 0457, phase-489]
---

**Home: [phase-489](../../roadmap/phase-489-ci-build-speedup.md).**

## What happens

`nros configure` writes a `.cargo/config.toml` into every `nros_cargo` package. Its
one `include` names the central `<checkout>/nros-patch.toml`, which patches
`nros`, `nros-core` and `nros-serdes` (`cmd/ws.rs` `CENTRAL_PATCH_CRATES`).
A runner or command package (phase-527, RFC-0119) is a host tool written
against `nros-extension`. It names none of the three. So every cargo call that
builds it prints:

    warning: patch `nros v…` was not used in the crate graph

once per crate, three per call. The phase-527 W7 e2e run printed 12 such lines.
None of them names `nros-extension`: `builder/host_tool.rs` already adds that row
only for a package that depends on it.

## Why

`CENTRAL_PATCH_CRATES` is admitted by the rule in its doc comment: "a crate may
live centrally only if it is registry-named in EVERY Rust consumer's dependency
graph". That held while every `nros_cargo` package was an image or a node. A
host package breaks it, and the rule had no way to see that, because the central
file is shared by construction.

## Direction

Don't write the central `include` into a host package's config. A host package
is identified by `nros-runner.toml` / `nros-command.toml`, the same test
`scripts/stage-sdk-root.sh` uses. Or narrow the row set by what the package's
own graph names registry-style, as `registry_patches` does for images. Fix the
class rather than the site: list every writer of a sync-managed
`.cargo/config.toml` and check each against a host package.

Acceptance: `nros build` of a scaffolded runner package (`nros new runner`)
prints no "was not used in the crate graph" line, and an image in the same
workspace still resolves `nros` through the central patch.

## Resolved (2026-11-09)

Neither direction above fixes it. Leaving the include out of a host package's
config cannot work: in a colcon-layout workspace the patch authority is the
workspace ROOT (`find_patch_authority`, RFC-0098 D9), so the images and the host
tools share one `<ws>/.cargo/config.toml`. Dropping the trio from it breaks the
images' probe, west and plain-cargo builds, which are why it exists. Narrowing
the rows per package would need per-package configs, which D9 removed.

The lever is where cargo RUNS. Cargo finds config files from the current
directory upward, never from `--manifest-path`. Measured on a toy workspace
whose config patches an unused crate to a missing path: run inside, `cargo
metadata` exits 101; run outside with `--manifest-path`, it exits 0.

rustup also reads `rust-toolchain.toml` from the current directory, so moving
cargo's directory alone would also move the toolchain. The toolchain is
therefore resolved AT the package first and passed by name:

- `preflight::toolchain_at(dir)` returns a set `RUSTUP_TOOLCHAIN`, else the pin
  `default_pin_for` applies when nothing at `dir` chose one (issue 2370's
  rule), else what `rustup show active-toolchain` answers at `dir`
  (`rustup_in`). So a package's own `rust-toolchain.toml` is honoured, never
  dropped. Its rustup call ignores an inherited `RUSTUP_TOOLCHAIN`, which
  rustup's proxy exports under `cargo test`.
- `host_tool::plan` with a known toolchain runs both cargo handoffs from
  `CONFIG_FREE_DIR` (`/`), with `--manifest-path` and `RUSTUP_TOOLCHAIN`. With
  no toolchain, it keeps the old directory, which brings back the warnings but
  keeps the same toolchain.

Tests:

- `a_host_tool_with_a_known_toolchain_runs_cargo_outside_the_workspace`;
- `a_package_rust_toolchain_file_names_the_host_tool_toolchain`, where a
  package `rust-toolchain.toml` names the toolchain resolved for it;
- `the_active_toolchain_is_read_by_name`.

Acceptance, measured: a copy of `examples/templates/multi-node-workspace`, plus
`nros new my_runner --kind runner`. The workspace config patches `nros`,
`nros-core` and `nros-serdes`. Running the tool's cargo the old way (in the
package) prints 3 "was not used" lines. `nros build` of the runner, and of the
whole workspace (runner and `demo_bringup:linux`), prints 0, exits 0, and the
image resolves `nros v0.6.0` from the tree through that patch.
