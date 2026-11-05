---
id: 2288
title: "Preflight asked rustup from the caller's directory, so a build run outside the checkout was refused on the default toolchain's targets"
status: resolved
type: bug
area: [cli, build, zephyr, ci]
severity: high
found: 2026-11-05
resolved: 2026-11-05
related: [2151, 2149, 2193, phase-466]
---

## Measured

Tier-2 nightly 41985062052 (2026-11-05 07:07Z, runner newslab-118) stopped in
its zephyr build stage, rc=2. Three Rust native_sim workspace entries
(`build-ws-rs-entry-zenoh`, `build-ws-rs-params-entry-zenoh`,
`build-ws-rs-lifecycle-entry-zenoh`) were refused at `nros build --dry-run`:

```
Error: missing prerequisites for this build:
  - Rust target `x86_64-unknown-none` (a Rust image on board `zephyr`)
      run: rustup target add x86_64-unknown-none
  - rustup component `clippy` (a Rust image on board `zephyr`)
      run: rustup component add clippy
nothing was built. ...
```

The refusal was false. The checkout's pinned toolchain (`1.99.0`,
`rust-toolchain.toml`) has both in the runner's `~/.rustup`; the default
`stable`, installed with the minimal profile, has neither. The runner
session read this on the host:

- inside the checkout, `rustup show active-toolchain` gives `1.99.0 (overridden by
  …/nros-fixture-tree/rust-toolchain.toml)`;
- from outside it, the same command gives `stable (default)`.

## Cause

`preflight::installed_rust_targets()` and `rust_component_installed()` ran a
bare `rustup` in the PROCESS's cwd. rustup picks its toolchain from that
directory (`RUSTUP_TOOLCHAIN`, then the nearest `rust-toolchain.toml`, then
the default). `scripts/build/zephyr-fixture-run-one.sh` runs
`cd "$workspace" && nros build … --workspace "$ws_dir" --dry-run` with the
Zephyr west workspace (`~/.nros/workspaces/zephyr/3.7`) as cwd. That
directory is outside every checkout, so the probe answered for a different
toolchain from the one the build's cargo uses in the image's workspace.

The checks were added by issue 2151 (b275e885b3, 2026-10-30). The tier-2
nightly had not reached this stage since then, because it stopped earlier on
issues 2229 and 2249.

`cargo test` cannot see this: rustup's proxy exports `RUSTUP_TOOLCHAIN` to
the test process, and that outranks the directory.

## Fix

One helper, `preflight::rustup_in(root)`, which sets `current_dir` to the
workspace root `nros build` was given (when it exists). Every rustup probe
goes through it: the target list and the component list, in `check()` and in
`check_zephyr_rust()`, which now takes the root. The target-list cache is
keyed by that directory, so two workspaces in one process that pin different
toolchains each get their own answer. Fixing the script's cwd instead would
have fixed one caller of the class.

Sibling, the same class on the installing side: `nros setup board`'s
`rustup target add` also ran in the caller's cwd, so it could install the
target onto a toolchain the build never uses. It now goes through
`rustup_in` with the catalog root. Sweep: `git grep -n 'Command::new("rustup")'
-- packages/cli` (no other site).

Test: `rustup_is_asked_from_the_workspace_it_builds` asserts the command's
directory, since an answer-level test passes with or without the fix under
`cargo test`.

Not fixed here: the driver's error-line matcher printed "no error line
matched" for this refusal format, so the job log never showed it. That is
issue 2193's class.
