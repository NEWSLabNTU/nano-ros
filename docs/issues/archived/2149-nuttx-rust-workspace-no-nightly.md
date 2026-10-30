---
id: 2149
title: "A NuttX Rust workspace image does not build from `nros build`: the cargo road ignored the board's nightly, and nothing configures the kernel first"
status: open
type: bug
area: [cli, nuttx, build]
severity: medium
found: 2026-10-29
related: [phase-517]
---

## What

`book/src/getting-started/integration-nuttx.md` says a Rust NuttX image
"starts as a workspace image (`nros new --image nuttx --board
qemu-armv7a-nuttx`, then `nros sync` and `nros build nuttx`)". Phase 517 W10
replayed that chapter's C package only. When W17 set out to replay the Rust
road too, the build failed twice, for two separate reasons.

Reproduction, with `nros setup qemu-armv7a-nuttx --rmw zenoh` done:

```bash
nros new nx_ws --workspace --lang rust --rmw zenoh
cd nx_ws
nros new --image nuttx --board qemu-armv7a-nuttx
nros sync
nros build nuttx
```

### 1. The cargo road built on the default toolchain (fixed)

```text
error[E0463]: can't find crate for `core`
  = note: the `armv7a-nuttx-eabihf` target may not be installed
```

The board says `toolchain = "nightly"`. Its `cargo_config` carries
`[unstable] build-std`, which a stable cargo ignores. The preflight already
asked the pinned nightly for `rust-src`, but the build ran `cargo` with no
channel at all, so the target's `core` was never built.

**Fixed.** `builder::preflight::board_rust_channel` is the one answer to
"which channel does this board build with". Both cargo handoffs set
`RUSTUP_TOOLCHAIN` from it:

* the generated entry of a workspace, and its `cargo update` prepare step;
* a single-package leaf, through `LeafImage::rust_channel`.

The preflight asks the same function. Measured: the command now reads
`RUSTUP_TOOLCHAIN=nightly-2026-04-11 cargo build …` and compiles past `core`.

### 2. Nothing configures the NuttX kernel before the Rust build (open)

```text
third-party/nuttx/nuttx/include/stdint.h:30:10: fatal error: nuttx/config.h: No such file or directory
```

`nros-board-nuttx-qemu`'s build script compiles the platform C against the
kernel's headers. `nuttx/config.h` exists only once the kernel has been
configured. The C/C++ road configures and builds the kernel inside its cmake
configure. The cargo road has no such stage, and `nros setup` fetches the
kernel source but does not configure it. The in-tree Rust NuttX fixtures get
a configured kernel from the fixture build's own kernel step, which no user
runs.

## Fix direction

* Give the cargo road for a NuttX board the same kernel configure (and build)
  the cmake road runs, from the same board facts.
* Or have the NuttX board's build script refuse with that remedy, instead of
  a missing header, and document the step in the chapter.

Either way, the chapter's Rust paragraph is not true until this lands. Phase
517 W17 adds no NuttX Rust probe track until it does.
