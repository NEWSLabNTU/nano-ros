---
id: 2312
title: "`nros setup board <zephyr-board>` provisions the SDK root's stable toolchain, but the Zephyr C road builds `nros-c` on a nightly hardcoded in two cmake files, so a fresh host's first `nros run` dies in rustup"
status: resolved
type: bug
area: cli, setup, zephyr, toolchain, cmake
related: [phase-537, 2151, 2285, 2288, 1447, 1672]
found: 2026-11-06
resolved: 2026-11-07
resolved_in: "fix-issue-2312 branch: the Zephyr C road reads the pinned nightly, and setup installs it"
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

## Resolution

All three halves of the fix direction, on one source: the first `nightly-*`
row of `config/rust-toolchains.txt` (the fold that carries
`tools/rust-toolchain.toml`, and what an installed SDK root ships).

1. **The build reads it.** `cmake/NanoRosRustToolchain.cmake` holds
   `nros_pinned_nightly()` and `nros_rust_target_builds_std()` (the
   `armv7a|thumbv|riscv32` predicate). `zephyr/cmake/nros_cargo_build.cmake`
   sets `RUSTUP_TOOLCHAIN` from the first, and
   `zephyr/cmake/nros_generate_interfaces.cmake` writes the FFI crate's
   `rust-toolchain.toml` channel from it. The two NuttX toolchain files
   (`cmake/toolchain/armv7a-nuttx-eabi.cmake`,
   `riscv32imac-nuttx-elf.cmake`) carried the same literal for Corrosion's
   `Rust_TOOLCHAIN` and read it the same way now. `check-rust-targets-covered`
   refuses a `nightly-<date>` literal in any tracked cmake file outside a
   comment, and a fixture row's `RUSTUP_TOOLCHAIN` (TOML, which cannot read
   a file) that differs from the pin.
2. **Setup installs it.** `builder::preflight::zephyr_runtime_needs` is the
   Rust side of the same predicate: for a Zephyr board whose
   `[board.provisioning] rust_targets` or `[board.zephyr] rust_target` is an
   embedded triple, the pinned nightly with `cargo`, `rust-src`, `rustfmt`
   (the FFI crate's toolchain file names it) and the triple. `nros setup
   board` runs it as step (c2) through `rust_toolchain::ensure_needs`
   (minimal profile, then each component and target), and `nros setup
   <board>` adds the same rows to `board_rust_needs`. `cargo` is a need
   because the reader's toolchain was a partial install without it; `rustup
   component add cargo` repairs one.
3. **Preflight asks it.** `nros build` / `nros run` of a C/C++ image on a
   Zephyr board refuse before cmake when the nightly is absent, or lacks a
   component or the triple, naming `nros setup board <board>
   --zephyr-workspace <ws>` (`check_zephyr_runtime`).

```text
$ nros setup board nucleo-f767zi-zephyr --zephyr-workspace <ws> --dry-run
  (c) rust targets: rustup target add thumbv7em-none-eabihf
  (c2) runtime nightly: rustup toolchain install nightly-2026-04-11 --profile minimal --component cargo,rust-src,rustfmt --target thumbv7em-none-eabihf
```

Tests: `a_cortex_m_zephyr_board_needs_the_pinned_nightly_for_its_runtime`
and `a_missing_runtime_nightly_is_refused_naming_setup_board`
(`builder/preflight.rs`), `setup_for_a_cortex_m_zephyr_board_installs_the_runtime_nightly`
and `the_board_setup_plan_line_is_one_rustup_command` (`cmd/setup.rs`), and
the gate's self-test cases.

Verified on a host whose `RUSTUP_HOME` held only `1.99.0` (minimal), on
2026-11-07, with a scaffolded C talker:

- `mps2-an385-zephyr` (Cortex-M3, `thumbv7m-none-eabi`): `nros build`
  refused naming `nros setup board`; `nros setup board mps2-an385-zephyr`
  added the target to the nightly; `nros build` then ran `west build` to
  `zephyr.elf`, with the generated FFI `rust-toolchain.toml` and both
  `RUSTUP_TOOLCHAIN` settings on `nightly-2026-04-11`, no rustup
  auto-install line, and no toolchain in `RUSTUP_HOME` but the two.
- `nucleo-f767zi-zephyr`: the preflight refused before setup, `nros setup
  board` installed `nightly-2026-04-11` with `cargo`, `rust-src`, `rustfmt`
  and `thumbv7em-none-eabihf`, and the next `nros build` passed preflight and
  reached `west build`. That build stopped at Zephyr's devicetree
  (`st/f7/stm32f767zitx-pinctrl.dtsi`): the host's west workspace has no
  `hal_stm32`. The acceptance's Nucleo `west build` is NOT verified here.

The `renode-nucleo` probe row in `packages/boards/board-support.toml` still
`awaits` this; it can drop that once a release carries the fix and the
track replays green.
