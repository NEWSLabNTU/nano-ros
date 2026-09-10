---
id: 1277
title: "`nros build` preflight demands a prebuilt `rust-std` for a board whose
  `cargo_config` builds `core` from source (`[unstable] build-std`)"
status: open
type: bug
area: cli, build
severity: low
related: [phase-445]
---

## What happens

`nros build demo_bringup:esp32` (examples/workspaces/rust, board `esp32-qemu`)
stops at stage 3 on a host whose row toolchain (`RUSTUP_TOOLCHAIN =
nightly-2026-04-11`, from the fixture row's `env`) has no prebuilt std for the
triple:

```text
Error: missing prerequisites for this build:
  - Rust target `riscv32imc-unknown-none-elf` (board `esp32-qemu`)
      run: rustup target add riscv32imc-unknown-none-elf
```

Measured 2026-09-10 in a fresh worktree, BEFORE any phase-445 change, while
taking the W4 baseline. `rustup target add --toolchain nightly-2026-04-11
riscv32imc-unknown-none-elf` cleared it.

## Why it is wrong

The board descriptor's `cargo_config` says `[unstable] build-std = ["core",
"alloc"]`: the image compiles `core` from `rust-src` and never links the
prebuilt `rust-std` the remedy installs. `builder::preflight::check` asks
`target_provisioning(nano_ros_root, target)`, which answers `Rustup` whenever a
prebuilt std EXISTS for the triple (riscv32imc is tier 2) — it does not look at
whether this board uses it. So the requirement it states is one the build does
not have, and the one it does have (`rust-src` on the ROW's toolchain) is not
checked.

It also reads `rustup target list --installed` for the toolchain the preflight
process runs under, not for the `RUSTUP_TOOLCHAIN` the build will use.

## Fix direction

When the descriptor's `cargo_config` declares `[unstable] build-std` (now
rendered into the image's `build/<coord>/<entry>/nros-cargo.toml`, phase-445
W4), preflight should require `rust-src` on the build's toolchain and stop
asking for the target's `rust-std`.
