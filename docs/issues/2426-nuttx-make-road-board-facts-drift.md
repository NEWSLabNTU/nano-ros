---
id: 2426
title: "The NuttX make road restates the board's cargo settings, and two of them have drifted: `-Zbuild-std=std` against the board's `core,alloc`, and armv7a-only flags on a tree that also serves rv-virt"
status: open
type: bug
area: [nuttx, build]
severity: low
found: 2026-11-10
related: [issue-2236, phase-530, rfc-0098]
---

**Home: [phase-530](../roadmap/phase-530-nros-configure-single-pass.md)**, beside
[issue 2236](2236-nuttx-make-app-as-project.md), whose design write-up found it.

## What happens

The NuttX `apps/external` make road (`integrations/nuttx/Makefile`) builds the
runtime archive with its own cargo command, and does not read the board
descriptor's `cargo_config`
(`packages/boards/nros-board-nuttx-qemu/nros-board.toml`). So it restates
board facts by hand, and two of them disagree with the board.

1. **The standard library it builds.** `Makefile:254` passes
   `-Zbuild-std=std,panic_abort`. The board's `cargo_config` says
   `build-std = ["core", "alloc", "panic_abort"]` plus
   `build-std-features = ["compiler-builtins-mem"]` (`nros-board.toml:46-47`).
   The make road therefore compiles a full `std` for the target, which the
   CMake leaf road and the cargo road do not. The core crates have no `std`
   (CLAUDE.md "`std` is DELETED from the core crates"), so a hosted `std` in
   this archive comes only from this flag.
2. **The target flags.** The `--config` literals `NROS_RUSTFLAGS_ARMV7A` and
   `NROS_CFLAGS_ARMV7A` (`Makefile:229-231`), and the `CC_`/`CXX_`/`AR_`
   variables in `NROS_CARGO_BUILD` (`Makefile:242-248`), name only
   `armv7a-nuttx-eabihf` and `cortex-a7`. The same apps tree serves the
   `rv-virt` board (`Makefile:29`, issue 0525), whose triple is
   `riscv32imac-unknown-nuttx-elf` (`nros-board.toml:142-164`). Upstream's
   `RUST_TARGET_TRIPLE` resolves a non-empty triple there (`Makefile:201`),
   so `context::` takes the same `NROS_CARGO_BUILD` branch
   (`Makefile:274-279`). Every literal in it is keyed to the armv7a triple,
   so on rv-virt none applies, and the board's own link and C flags are not
   passed. The comment at `Makefile:223-225` says to "extend the
   conditional in the `context::` rule when adding more boards"; the only
   conditional there chooses between upstream's macro and this one, not
   between boards.

## Why nothing caught it

No CI lane runs the make road (`check-nuttx-integration-makefile.sh` checks
the Makefile's shape, not a build), so the archive this command produces is
never built or compared with the CMake leaf road's.

## Not measured

Read from the tree; nothing was built. Whether `std` builds and links for
`armv7a-nuttx-eabihf` at the pinned nightly, and what an rv-virt make build
does today, are open. The make road needs the NuttX toolchain and mutates the
shared `.config` (issue 0525).

## Fix

Held until issue 2236 is ruled. Its recommended knob channel (B1) passes the
image's `nros-cargo.toml` (the board's `cargo_config` included) with
`--config`, which deletes these hand-written lines. If the ruling keeps the
make road a guest road, the fix is the same carrier without a project: the
board's `cargo_config` replaces the inline literals and the `-Zbuild-std`
flag (2236's question 5). Either way the acceptance is a make-road build on
both boards, in a lane, whose cargo flags match the board's.
