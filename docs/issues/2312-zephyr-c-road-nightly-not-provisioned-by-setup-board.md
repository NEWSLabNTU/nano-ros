---
id: 2312
title: "`nros setup board <zephyr-board>` provisions the SDK root's stable toolchain, but the Zephyr C road builds `nros-c` on a nightly hardcoded in two cmake files, so a fresh host's first `nros run` dies in rustup"
status: open
type: bug
area: cli, setup, zephyr, toolchain, cmake
related: [phase-537, 2151, 2285, 2288, 1447, 1672]
found: 2026-11-06
---

**Home: none yet.** Found by the phase-537 book session's renode-nucleo
replay, on a release built from 2026-11-06 `main`.

## Symptom

```
$ nros setup board nucleo-f767zi-zephyr --zephyr-workspace "$PWD"   # passes
$ nros run nucleo_node        # C talker, west build -b nucleo_f767zi
warn: the missing active toolchain `nightly-2026-04-11-x86_64-unknown-linux-gnu` has been auto-installed
error: the 'cargo' binary, normally provided by the 'cargo' component, is not applicable to the 'nightly-2026-04-11-x86_64-unknown-linux-gnu' toolchain
```

## What pins the nightly

Neither a `rust-toolchain.toml` in the user's tree nor zephyr-lang-rust
pins it. nano-ros's own Zephyr cmake pins it, as a LITERAL, in two places:

- `zephyr/cmake/nros_cargo_build.cmake`, around line 2340. The `nros-c` /
  `nros-cpp` build for an `armv7a|thumbv|riscv32` triple sets
  `RUSTUP_TOOLCHAIN=nightly-2026-04-11` and adds
  `-Z build-std=core,alloc,compiler_builtins`. This is the step that ran
  `cargo` in the symptom: a C image on a Cortex-M7 still builds the C ABI
  crate with cargo.
- `zephyr/cmake/nros_generate_interfaces.cmake`, around line 777. This
  writes a `rust-toolchain.toml` with `channel = "nightly-2026-04-11"`,
  `components = ["rust-src", "rustfmt"]` and the board triple next to the
  generated FFI crate.

Neither file reads the channel from a source of truth. The tree already has
one: `tools/rust-toolchain.toml`, mirrored into `config/rust-toolchains.txt`
(read by `builder::preflight::pinned_nightly`) and into
`nros-sdk-index.toml`'s `[rust.toolchain.nightly-pinned]`, where
`check-rust-targets-covered` keeps the index matching the file. The two
cmake literals sit outside that gate, so a nightly bump leaves them behind.
`examples/zephyr/rust-toolchain.toml` is a third copy, used by the Rust
example leaves.

## What `nros setup board` installs

`packages/cli/nros-cli-core/src/cmd/setup.rs`, `run_board()`, step (c). It
runs `rustup target add <rust_targets>` through
`builder::preflight::rustup_in(&root)`, so the target goes onto whatever
toolchain the SDK ROOT resolves to. That is `rust-toolchain.toml` =
`1.99.0` in a checkout, or rustup's default in an installed SDK. It never
installs `nightly-2026-04-11`, `rust-src` (which `-Z build-std` needs), or
the triple on the nightly. `nros setup <board>` (`board_rust_needs`) does
the same: a Zephyr board's `rust_target` and `rust_components` go to
channel `None`, because a Zephyr descriptor states `toolchain = "stable"`.
The nucleo descriptor states `toolchain = "stable"`, and the nightly the
build uses is a cmake decision invisible to every descriptor reader.

So the build reaches a toolchain that nothing installed, and rustup's
auto-install is the only provisioner.

## Why rustup failed (not reproduced here)

On this host, rustup 1.29.1 with a fresh `RUSTUP_HOME` and the `minimal`
profile auto-installs the toolchain WITH cargo in three shapes:

- a `cargo --version` inside the generated `rust-toolchain.toml`;
- `rustup target add <triple> --toolchain nightly-2026-04-11` first, then
  cargo;
- `rustup component add rust-src --toolchain …` first, then cargo.

rustup prints `warn: to install the active toolchain, use rustup install
instead` (rust-lang/rustup#4836) in the last two shapes. The reader's
"`cargo` … not applicable" means rustup's record of that toolchain has no
cargo component. That points to a partial install left by an earlier
command on the reader's host. It does not change the fix: a build should
never depend on what rustup's auto-install happens to do.

## Fix direction

Pick ONE source for the channel, `tools/rust-toolchain.toml` (as
`config/rust-toolchains.txt` / the index's `nightly-pinned`), and use it on
both sides:

1. **The build reads it.** Both cmake sites take the channel from
   `config/rust-toolchains.txt` (the first `nightly-` row) through one
   helper, never a literal. Add the two files to the coverage of
   `check-rust-targets-covered` (or a sibling gate) so a third literal
   cannot appear.
2. **Setup installs it.** For a Zephyr board whose Rust triple is
   `armv7a|thumbv|riscv32` (the same predicate the cmake uses; better,
   one predicate that both read), `nros setup board` and `nros setup
   <board>` run `rustup toolchain install <nightly> --profile minimal
   --component rust-src` plus the board's `rust_components`, then `rustup
   target add <triple> --toolchain <nightly>`. The pattern exists already:
   `rust_toolchain::ensure_needs` installs a pinned channel, minimal
   profile, before adding to it. The Zephyr arm passes `None` instead of
   the channel.
3. **The build's preflight asks it.** `nros build` / `nros run` should
   refuse before cmake when that nightly lacks cargo, rust-src or the
   triple, and name `nros setup board` as the fix. Today the first signal
   is rustup's error from inside a cmake custom command.

Acceptance: on a host whose `RUSTUP_HOME` has only the SDK's stable
toolchain, `nros setup board nucleo-f767zi-zephyr` then `west build -b
nucleo_f767zi` of the C talker builds, and no rustup auto-install line
appears.
