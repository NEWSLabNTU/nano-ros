# Phase 550 -- NuttX builds on NuttX's own infrastructure

**Status (2026-11-11). IN PROGRESS: W2 and W12 done; the rest not started.**
Candidate work for
[RFC-0126](../design/0126-nuttx-builds-on-nuttx-infrastructure.md), whose
decisions are not ruled yet. Each item names the question in
[issue 2236](../issues/2236-nuttx-make-app-as-project.md) that gates it
("Gate:"); an item gated on an unruled question does not start. Items with
"Gate: none" fix measured defects that hold whatever is ruled. Each item is
claimed with `just claim phase-550-wN` before it starts.

Homes:

- [issue 2426](../issues/2426-nuttx-make-road-board-facts-drift.md), the make
  road's verified defects (items 1 to 7): W1 to W5, W8, W12;
- [issue 2236](../issues/2236-nuttx-make-app-as-project.md), the design
  questions: W0, W6, W7, and the gates below.

Builds on [phase-530](phase-530-nros-configure-single-pass.md) (W5, Ruling
R36: the make road is a guest road today).

---

## Why

Issue 2236's study measured, at our 12.13.0 pins: the make road builds
neither board as shipped (armv7a lacks `NUTTX_DIR`; rv-virt compiles
zenoh-pico with the host `cc`); upstream's own Rust support builds neither
board with our nightly; the arm CPU flags exist in four copies, each equal to
NuttX's `ARCHCPUFLAGS`; and no lane builds the make road, which is why all of
this was found by reading. The direction this phase would implement: every
NuttX fact comes from NuttX's configured tree, and the copies go.

## Work items

### W0 -- The ruling (gate for the rest)

The maintainer rules issue 2236 Q1 to Q20, and RFC-0126 is rewritten from a
decision shape to decisions, with RFC-0120 D5/D8 and phase-530 R36 amended to
match (Q1, Q7).

- **Acceptance:** each question has a recorded answer in 2236; every "Gate:"
  line below is either cleared or the item is dropped, in this doc.

### W1 -- A lane that builds the make road on both boards

A recipe (successor of `just nuttx build-integration-app`) that, for armv7a
and rv-virt each, configures its OWN copy of the pinned kernel and apps trees
under the build dir (never `third-party/nuttx/nuttx`, issue 0525), stages the
shell, sets `CONFIG_NROS=y`, adds a minimal C app that calls `nros_*` (so the
link is not vacuous), and runs `make`.

- Gate: Q19 for the cadence only; the recipe itself is ungated.
- **Acceptance:** the lane runs both arches and reports red on today's tree
  (the two failures issue 2236's study measured); after W2 and W3 it is
  green, and the ELF holds `nros_*` text symbols; it is reachable from the
  cadence Q19 picks, and `scripts/check-nuttx-integration-makefile.sh`'s
  "no lane" note is deleted.

### W2 -- The kernel tree reaches cargo

`NROS_CARGO_BUILD` passes `NUTTX_DIR=$(TOPDIR)` (and the apps dir), so
`zpico-sys` builds against the kernel being built, not the nano-ros
checkout's (measured: `nuttx-kernel: not provisioned anywhere this build
looks`).

- Gate: none.
- **Acceptance:** W1's armv7a build passes with `third-party/nuttx/nuttx`
  uninitialised and `NUTTX_DIR` unset in the caller's environment.
- **Status: DONE (2026-11-11).** `NROS_CARGO_BUILD` sets
  `NUTTX_DIR=$(TOPDIR)`, `NUTTX_APPS_DIR=$(APPDIR)` and
  `NUTTX_EXPORT_DIR=$(TOPDIR)` on the cargo command. The first two are the
  `env` names of `[source.nuttx-kernel]` / `[source.nuttx-apps]`, the
  ladder's env rung (RFC-0103 D3), the channel `nros build` and the cmake
  road already use; no new spelling. They are set on the command, so they
  win over an exported value: this road states the tree it builds (the
  ladder's arg rung), and any other tree is issue 0525's headers-from-one,
  link-from-another split. `NUTTX_EXPORT_DIR` is the class sweep: it is
  `nros_build_paths::nuttx_include_root`'s override, read before the per-arch
  snapshot under the kernel tree, so pinning it keeps the headers on the live
  `$(TOPDIR)/include` rather than a `build-nuttx.sh` snapshot or an exported
  tree. `NUTTX_INCLUDE_DIR` stays (upstream `Rust.mk`'s spelling; nothing in
  nano-ros reads it). The empty-triple fallback still calls upstream's macro
  without these (W4 deletes it).
  E2E, W1's lane not yet existing: a scratch clone of the pinned kernel and
  apps trees (`/mnt/wd/nt/nx`, never the shared tree), board defconfig,
  shell staged, `CONFIG_NROS=y`, `NUTTX_DIR` unset, the checkout's
  `third-party/nuttx/nuttx` empty, `make -j` on 4 jobs. qemu-armv7a before:
  rc 2 in 16 s, `zpico-sys` stops with `nuttx-kernel: not provisioned
  anywhere this build looks`. After: rc 0 in 66 s (cargo 36.2 s, `std`
  compiled, item 1), `libnros_c.a` 2,314,740 bytes, `nuttx` ELF 4,500,260
  bytes linked; zenoh-pico compiled against the scratch kernel's `include/`.
  rv-virt after: rc 2 in 5 s, past the kernel lookup, at 2426 item 2's host
  `cc` (`unrecognized argument in option '-mabi=ilp32'`), W3's scope.

### W3 -- Cross tools and CPU flags from NuttX (2426 item 2)

- **W3a (make road).** `CC_` / `CXX_` / `AR_<triple>` from `$(CROSSDEV)` and
  `CFLAGS_<triple>` from `$(ARCHCPUFLAGS)`, for the triple the build
  resolved; the armv7a-only literal list and `NROS_RUSTFLAGS_ARMV7A` /
  `NROS_CFLAGS_ARMV7A` are deleted. Gate: none.
- **W3b (every other copy).** The board `cargo_config` rustflags `-mcpu` /
  `-mfloat-abi` / `-mfpu` / `-march` / `-mabi`, the `CFLAGS_*` and
  `NUTTX_*_CFLAGS` env rows, and `cmake/toolchain/*-nuttx-*.cmake`'s
  `CMAKE_*_FLAGS_INIT` take NuttX's value (the export's
  `scripts/Make.defs` on the leaf and workspace roads). Gate: Q13, Q14.
- **Acceptance:** W1 green on rv-virt; `git grep` finds `cortex-a7` /
  `neon-vfpv4` / `rv32imac` flag literals only where W3b's ruling allows; a
  gate refuses a new copy.

### W4 -- One triple (2426 items 3 and 4)

- The archive path in `Make.defs` uses the triple the Makefile resolved; the
  `NROS_NUTTX_TRIPLE := armv7a-nuttx-eabihf` literal goes. Gate: none.
- The empty-triple branch that calls upstream's `RUST_CARGO_BUILD` with an
  empty `--target` is deleted (it cannot build). Gate: none.
- The armv7a triple comes from the source Q11 picks (upstream PR plus a
  nuttx-apps patch branch, one checked fallback, or the board). Gate: Q11,
  Q12.
- **Acceptance:** W1 links on both arches with the archive found under the
  triple cargo wrote; an arch with no triple is refused at parse time,
  naming `LLVM_ARCHTYPE`.

### W5 -- The board's cargo config on the make road (2426 items 1 and 5)

The make road's cargo takes the board's `cargo_config` (build-std
`core, alloc, panic_abort`, build-std-features, the target spec) through the
image's generated cargo config (`--config`), and drops
`-Zbuild-std=std,panic_abort`, the `libc` patch literal and
`export RUSTUP_TOOLCHAIN ?= nightly-2026-04-11` (the toolchain comes from
`tools/rust-toolchain.toml` like every other recipe).

- Gate: 2236 Q3 and Q5.
- **Acceptance:** W1's cargo command line carries no literal from
  `Makefile:229-231`, `:254` or `:144`; `std` is not compiled (no
  `Compiling std` in the log); a nightly bump moves no line in
  `integrations/nuttx/`.

### W6 -- A make app as a nros project (issue 2236 A/B; 2426 item 6)

Project identity and `nros configure` inside the shell's `context::`, as
ruled; the book's NuttX steps 3 and 4 (`book/src/guide/platforms/nuttx.md`)
describe what the road then does.

- Gate: 2236 Q1 to Q4, Q6, Q7.
- **Acceptance:** 2236 section 5's acceptance list.

### W7 -- The image crate is generated in `context::`

`nros_image_crate`'s `$(shell cmake -P ...)` moves from parse time into the
`context::` recipe, and the issue 0551 parse guard it needs goes with it.

- Gate: Q9 (and W6's ordering, if a project is named).
- **Acceptance:** `make olddefconfig` in a staged tree runs no `cmake`; W1
  green.

### W8 -- NuttX's CMake flow (2426 item 7)

Either: staging writes `external/CMakeLists.txt`, and the shell becomes a
NuttX CMake app over `nuttx_add_application` / `nuttx_add_extra_library`
that reads `CONFIG_NROS_RMW_<X>` (no undeclared `CONFIG_NROS_RMW`), with W1
extended to a CMake build of both boards; or the CMake shell and the book's
sentence (`nuttx.md:103-104`) are deleted.

- Gate: Q16.
- **Acceptance:** either a CMake NuttX build of a staged tree configures and
  links on both arches in a lane, or `integrations/nuttx/CMakeLists.txt` is
  gone and no doc offers the flow.

### W9 -- The rv-virt target spec

Move or keep `riscv32imac-unknown-nuttx-elf.json` as Q15 rules. Measured
starting point: it equals rustc's built-in spec plus `eh-frame-header:
false` and `panic-strategy: abort`.

- Gate: Q15.
- **Acceptance:** the spec has one home; every road that links with rustc on
  rv-virt still links (`workspace-rust-nuttx-riscv-realtime`); the
  regeneration note names the pinned nightly it was compared with.

### W10 -- Configure the kernel with NuttX's configure

The board defconfigs become fragments over upstream's
`qemu-armv7a:nsh` and `rv-virt:netnsh` (`#include`, `process_config.sh`),
and `scripts/nuttx/build-nuttx.sh` calls `tools/configure.sh` instead of
copying the defconfig and running `olddefconfig` itself.

- Gate: Q17.
- **Acceptance:** the resolved `.config` of each board is byte-identical
  before and after (modulo `CONFIG_BASE_DEFCONFIG`); the kconfig frontend
  `configure.sh` needs is provisioned by `nros setup`, never by hand.

### W11 -- The boot entry

`CONFIG_INIT_ENTRYPOINT` names the image's entry, and the `nsh_main`
override in `nros-board-nuttx-qemu/src/entry.rs` goes.

- Gate: Q18.
- **Acceptance:** every NuttX fixture row boots and passes its test on both
  arches; no symbol named `nsh_main` is defined outside the kernel.

### W12 -- Retire what is dead or stale (ungated)

- `integrations/nuttx/external-Kconfig.in` (nothing installs it) and the
  `Kconfig:12` comment that names it.
- The `examples/qemu-armv7a-nuttx/rust-toolchain.toml` comment that says the
  examples build `std` against the patched `libc` (phase-359 W7 ended that).
- `cmake/toolchain/armv7a-nuttx-eabi.cmake`'s file name and header comment,
  which name `armv7a-nuttx-eabi` for a file that sets `armv7a-nuttx-eabihf`
  (issue 2426, "Checked and not holding").
- Issue 2236 section 1's citation of `external-Kconfig.in` as the discovery
  mechanism.
- Gate: none.
- **Acceptance:** `git grep external-Kconfig.in` finds only archived docs;
  every renamed path is updated in `nros-board.toml`, the docs and the SDK
  index; `just check fast` green.
- **Status: DONE (2026-11-11).** The template is deleted and
  `integrations/nuttx/Kconfig`'s comment names what writes
  `apps/external/Kconfig` (`stage-external-apps.sh`, one explicit `source`
  line). The rust-toolchain comment says why the nightly is pinned
  (`-Z build-std` of `core` / `alloc`) and that only the make road still
  builds `std` against the `libc` fork. The toolchain file is
  `cmake/toolchain/armv7a-nuttx-eabihf.cmake`, header fixed, every live
  reference updated (`nros-board.toml`, both board cmake files, `just nuttx`,
  the CLI's preset and descriptor docs, RFC-0048, RFC-0096, issues 2236 and
  2426; the SDK index names no toolchain file). The old name stays as a
  deprecated file that includes the new one: CMake records the toolchain path
  in `<build>/CMakeFiles/<version>/CMakeSystem.cmake` at the first configure
  and includes it on every later configure, even when
  `-DCMAKE_TOOLCHAIN_FILE` names a new file (measured, CMake 3.22.1). Deleting
  it stranded every NuttX build dir configured before the rename, which
  stopped tier 2 at the NuttX fixture stage on its first run after the merge.
  2236 section 1 now cites the
  generated `apps/external/Kconfig`. `git grep external-Kconfig.in` outside
  archived docs finds only this item's own text, which archives with the
  phase. `just check fast` green.

### W13 -- The roads (issue 2236 Q10)

As ruled: if the apps road becomes the only NuttX road, move the 35 NuttX
fixture rows and their tests to it and delete the leaf road's link
restatement (the board's `link-arg` lists, the export snapshot as a link
input); if both stay, each takes its facts from NuttX (W3b, W9, W11); if
only the leaf and workspace roads stay, delete `integrations/nuttx/`, the
staging script, the book's make-road section and this phase's W1 to W8.

- Gate: Q10.
- **Acceptance:** the support registry (RFC-0114) names exactly the roads
  that remain, and each has a lane.

### W14 -- The vendored `libc` fork

Once no road compiles `std` (W5), retire `third-party/nuttx/libc` and its
`[patch.crates-io]` rows, which the board descriptor already calls unused.

- Gate: Q20 (and W5).
- **Acceptance:** no tracked `[patch.crates-io] libc` row; the SDK index has
  no `nuttx-libc` source; both arches' fixtures pass.

## Order

W0 first. W1, W2, W3a, W4 (the ungated parts) and W12 can start before the
ruling; W1 lands first so the rest have a verdict. Then W5, W7, W6 in that
order; W3b, W8 to W11, W13 and W14 as ruled.

## What this phase does not do

- Touch the shared `third-party/nuttx/nuttx` tree's `.config` from a lane
  (issue 0525).
- Decide anything issue 2236 leaves to the maintainer.

## Acceptance for the phase

- The make road builds and links a real app on armv7a and rv-virt in a lane.
- No NuttX fact (triple, CPU or C flags, cross tool names, build-std, nightly)
  is written in `integrations/nuttx/`; each comes from NuttX's tree or the
  board's `cargo_config`.
- Issue 2426 is resolved; issue 2236 is resolved or ruled wontfix.
