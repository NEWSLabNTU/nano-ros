---
id: 2426
title: "The NuttX make road diverges from the board and the other roads: restated cargo settings, an armv7a-only archive path, a fallback that cannot build, and a CMake shell nothing reaches"
status: open
type: bug
area: [nuttx, build]
severity: low
found: 2026-11-10
related: [issue-2236, phase-530, phase-550, rfc-0098]
---

**Home: [phase-550](../roadmap/phase-550-nuttx-native-build-infrastructure.md)**
(W1 to W5, W8, W12), beside [issue 2236](2236-nuttx-make-app-as-project.md), whose
design write-up found it while working on
[phase-530](../roadmap/phase-530-nros-configure-single-pass.md).

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

## Further divergences on the same road (2026-11-10)

A second read of the same files, checking seven reported defects against the
tree. Five hold, in full or in part, and are recorded here; two did not hold
and are listed after them. Line numbers are as of the day of filing.

3. **The archive path names armv7a whatever the arch.** `Make.defs:65` sets
   `NROS_NUTTX_TRIPLE := armv7a-nuttx-eabihf`, and `Make.defs:67-68` passes it
   as `--target` to `nros locate artifact`, so `EXTRA_LIBS` and the `-L` path
   (`Make.defs:81-86`) always name `<target dir>/armv7a-nuttx-eabihf/release/`.
   Cargo writes under the triple the Makefile resolved
   (`Makefile:201-206`, `--target $(NROS_TARGET_TRIPLE)` at `Makefile:262`),
   and the image root is per `CONFIG_ARCH` (`Make.defs:52`). On rv-virt the
   two triples differ, so the link names an archive that the rv-virt build
   never writes. This is item 2's literals one file over: the triple is
   stated a second time instead of being taken from the build that made it.
4. **The empty-triple fallback cannot build.** When `NROS_TARGET_TRIPLE` is
   empty, `context::` calls upstream's `RUST_CARGO_BUILD`
   (`Makefile:274-276`). At the pinned nuttx-apps commit
   (e4b84b29d47b98a666df524d6f2d38904c972de5), that macro passes
   `--target $(call RUST_TARGET_TRIPLE)` (`tools/Rust.mk:90-106`), the same
   expression that came back empty at `Makefile:201`. So the branch runs
   `cargo build ... --target` with no value, the error the comment at
   `Makefile:194-196` quotes. Even with a triple it would build the wrong
   thing in the wrong place: its manifest is the API crate
   `packages/api/nros-<flavor>` (which names no backend, `Makefile:85-90`),
   not the generated image crate; it passes no `--target-dir`, so the output
   would go to that manifest's workspace target dir inside the nano-ros
   checkout (the issue 0488 class); and it skips the header placer, while
   `Make.defs:53` and `:105` look under the image root. The branch exists to
   cover "an arch upstream does not know", and every such arch reaches it.
5. **A second copy of the nightly pin, ungated.** `Makefile:144` sets
   `RUSTUP_TOOLCHAIN ?= nightly-2026-04-11`. It agrees today with
   `tools/rust-toolchain.toml:18` (and the generated
   `config/rust-toolchains.txt`), but `check-rust-targets-covered`'s
   nightly-literal scan reads `*.cmake`, `*.cmake.in`, `CMakeLists.txt` and
   `examples/fixtures.toml` only (`scripts/check-rust-targets-covered.py:407-410`),
   so a nightly bump leaves this line behind silently: issue 2312's class,
   which `cmake/NanoRosRustToolchain.cmake` fixed for the cmake files. Its
   comment (`Makefile:137-143`) justifies the nightly by
   `-Zbuild-std=std`, which is item 1. The output directory, by contrast, does
   NOT diverge on the live branch: `NROS_CARGO_BUILD` passes
   `--target-dir $(NROS_IMAGE_TARGET_DIR)` (`Makefile:261`), which is
   `$(APPDIR)/external/.nros-build/<arch>/image/target` (`Make.defs:52-53`),
   the user's apps tree as issue 0488 wants. Only the dead branch of item 4
   writes into the checkout.
6. **The book's workspace steps have no consumer on this road.** The book
   (`book/src/guide/platforms/nuttx.md:95-100`) says the Kconfig RMW "must
   name the same RMW as your `system.toml`" and tells the user to run
   `nros configure` "so the generated message bindings exist before anything
   compiles against them". Neither the `Makefile` nor `Make.defs` names a
   workspace, a `system.toml` or anything `nros configure` / `nros sync`
   writes; the only optional input beyond Kconfig is `extra_libs.mk`
   (`Make.defs:94`), which nothing in-tree writes. So step 4 changes nothing
   the shell builds, and the RMW agreement step 3 asks for is checked by
   nothing. This is issue 2236's guest road seen from the book.
7. **The CMake shell reads a symbol Kconfig never declares, and nothing
   stages it.** `integrations/nuttx/CMakeLists.txt:13-17` reads
   `CONFIG_NROS_RMW` and falls back to `zenoh` when it is undefined. The
   shell's Kconfig declares only the choice booleans `NROS_RMW_ZENOH`,
   `NROS_RMW_XRCE`, `NROS_RMW_CYCLONEDDS` (`Kconfig:31-58`) and no string
   `NROS_RMW`, so on the CMake lane the RMW is always zenoh. That is the
   defect `Kconfig:75-86` records fixing for the edition (issue 0947), left in
   place for the RMW. Separately, `scripts/nuttx/stage-external-apps.sh`
   writes `external/Make.defs` (`:76`), the `external/nano-ros` link (`:79`)
   and `external/Kconfig` (`:103`), and no `external/CMakeLists.txt`. The
   pinned nuttx-apps `CMakeLists.txt:76-77` runs
   `add_subdirectory(external)` whenever `external/` exists, and CMake refuses
   a directory without a `CMakeLists.txt`, so a CMake NuttX build over a
   staged apps tree would stop at configure rather than reach the shell. The
   book offers that build in one sentence (`nuttx.md:103-104`).

Checked and not holding:

- **The CMake toolchain file's triple.** It is named
  `cmake/toolchain/armv7a-nuttx-eabi.cmake`, and its header comment (`:6`)
  says `armv7a-nuttx-eabi`, but it sets
  `Rust_CARGO_TARGET "armv7a-nuttx-eabihf"` (`:52`), the board's
  `[build] target` (`nros-board.toml:32`). The file name and that comment are
  stale; the triple agrees.
- **The retired make-road test in the book.** The book no longer cites
  `nuttx_make_e2e.rs` or `EXPECTED_PROGNAMES`; only archived roadmap docs
  (phase-157, phase-177) name it, and `just/nuttx.just:390-395` records its
  retirement.

## Measured (2026-11-11)

Issue 2236's study built the make road in a scratch clone of the pinned
kernel and apps trees (never the shared tree, issue 0525), with the board's
defconfigs and `CONFIG_NROS=y`:

8. **The kernel tree does not reach cargo.** `NROS_CARGO_BUILD` sets only
   `NUTTX_INCLUDE_DIR`, so `zpico-sys`'s build script resolves the kernel
   through the source ladder and stops: `[port.nuttx] include_paths
   {nuttx_include}: nuttx-kernel: not provisioned anywhere this build looks`
   (`$NUTTX_DIR` unset, checkout `third-party/nuttx/nuttx` not initialised).
   In a checkout where that submodule is initialised it would instead compile
   zenoh-pico against nano-ros's kernel, not `$(TOPDIR)`.

With `NUTTX_DIR` exported to the scratch kernel:

- armv7a builds: `make -j` 69 s on 4 jobs, cargo 39.8 s of it with `std`
  compiled (item 1); a 2.3 MB `libnros_c.a`; the `nuttx` ELF links, but holds
  no `nros_*` text symbol, since no app references the archive.
- rv-virt stops in 19 s: the zenoh-pico C sources are compiled by the host
  `cc` (`cc: error: unrecognized argument in option '-mabi=ilp32'`), item 2.
- The empty-`--target` error item 4 quotes was reproduced through upstream's
  own `examples/rust/hello` on qemu-armv7a (`make context`).

## Why nothing caught it

No CI lane runs the make road (`check-nuttx-integration-makefile.sh` checks
the Makefile's shape, not a build), so the archive this command produces is
never built or compared with the CMake leaf road's.

## Not measured

Read from the tree; nothing was built. Whether `std` builds and links for
`armv7a-nuttx-eabihf` at the pinned nightly, and what an rv-virt make build
does today, are open. The make road needs the NuttX toolchain and mutates the
shared `.config` (issue 0525).

Items 3-7 are reads as well. Not measured: that an rv-virt make build fails
at the link (item 3), the exact cargo error of the fallback branch (item 4),
and that a CMake NuttX build over a staged apps tree stops at configure
(item 7). The upstream macro and `CMakeLists.txt` were read at the pinned
nuttx-apps commit, not at a NuttX release.

## Fix

Held until issue 2236 is ruled. Its recommended knob channel (B1) passes the
image's `nros-cargo.toml` (the board's `cargo_config` included) with
`--config`, which deletes these hand-written lines. If the ruling keeps the
make road a guest road, the fix is the same carrier without a project: the
board's `cargo_config` replaces the inline literals and the `-Zbuild-std`
flag (2236's question 5). Either way the acceptance is a make-road build on
both boards, in a lane, whose cargo flags match the board's.

Items 3-7 wait on the same ruling; this record proposes no fix for them.
