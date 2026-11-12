# Phase 550 -- NuttX builds on NuttX's own infrastructure

**Status (2026-11-13). IN PROGRESS: W0 done (every question ruled), W1, W2, W5, W7, W6a
and W12 done; W3a and W4's code done (W3b and W4's upstream contribution
open); the rest not started.**
The work for
[RFC-0126](../design/0126-nuttx-builds-on-nuttx-infrastructure.md), all of
whose decisions are ruled (its "Decided" section). Each item names the question in
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
- **Status: DONE (2026-11-12).** All twenty questions are ruled, in two steps
  (issue 2236, "Ruling"). Every gate below is cleared. W8 and W13 are
  rewritten to the ruled road, and W14 leaves this phase (Q20).

### W1 -- A lane that builds the make road on both boards

A recipe (successor of the retired `build-integration-app` recipe) that, for armv7a
and rv-virt each, configures its OWN copy of the pinned kernel and apps trees
under the build dir (never `third-party/nuttx/nuttx`, issue 0525), stages the
shell, sets `CONFIG_NROS=y`, adds a minimal C app that calls `nros_*` (so the
link is not vacuous), and runs `make`.

- Gate: cleared (Q19: daily, ruled 2026-11-12).
- **Acceptance:** the lane runs both arches and reports red on today's tree
  (the two failures issue 2236's study measured); after W2 and W3 it is
  green, and the ELF holds `nros_*` text symbols; it is reachable from the
  cadence Q19 picks, and `scripts/check-nuttx-integration-makefile.sh`'s
  "no lane" note is deleted.
- **Status:** DONE (2026-11-13, PR #3646).
  `just nuttx build-make-road [arm|riscv|all]`
  runs `scripts/nuttx/make-road-build.sh <config>`. It clones its own copy of
  the kernel and apps trees the source ladder locates (`--shared`, at the
  tree's HEAD) into `build/nuttx-make-road/<config>/`, configures with the
  board's defconfig plus `CONFIG_APPS_DIR` (what `tools/configure.sh -a`
  writes), `CONFIG_NROS=y` and the smoke app `tests/nuttx-make-road-app/`,
  and runs `make` under the stall watchdog. It is green only when the ELF
  holds the app's entry, `rcl_publish` and `nros_*` text. The board
  `Make.defs` path is derived from the defconfig's `CONFIG_ARCH` /
  `CONFIG_ARCH_CHIP` / `CONFIG_ARCH_BOARD`, not listed. The kconfig frontend
  moved into `scripts/nuttx/kconfig-frontend.sh`, which `build-nuttx.sh` now
  sources too. Cadence (Q19): job `nuttx-make-road` in `nightly.yml`, on the
  daily 07:00 cron and on dispatch, one matrix cell per config. The recipe
  replaces `build-integration-app`, which built in the shared tree.
  Measured on today's tree (W2 already landed, so armv7a's
  `NUTTX_DIR` failure is gone):
  - `arm` is green: ELF 5,308,296 bytes, 29 `nros_*` functions.
  - `riscv` is red at 2426 item 2, as the study measured: the host `cc`
    gets `-mabi=ilp32` (`unrecognized argument in option '-mabi=ilp32'`),
    compiling zenoh-pico and nros-c. W3a's scope.
  - Until W5 drops the `libc` patch literal, the road also needs the
    `nuttx-libc` source, which `just setup nuttx` provisions.

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
  `scripts/Make.defs` on the leaf and workspace roads). Gate: cleared (Q13 yes, Q14 read the export's `scripts/Make.defs`, ruled 2026-11-12).
- **Acceptance:** W1 green on rv-virt; `git grep` finds `cortex-a7` /
  `neon-vfpv4` / `rv32imac` flag literals only where W3b's ruling allows; a
  gate refuses a new copy.
- **Status:** OPEN (W3a done 2026-11-13; W3b not started).
  **W3a.** `NROS_CARGO_BUILD` sets `CC_` / `CXX_` / `AR_<triple>` to
  `$(CROSSDEV)gcc` / `g++` / `ar`, and `CFLAGS_` / `CXXFLAGS_<triple>` to
  `$(ARCHCPUFLAGS)`. `<triple>` is the resolved triple (W4) with dashes as
  underscores, cc-rs's spelling. The six armv7a tool literals,
  `NROS_RUSTFLAGS_ARMV7A` (link args, which a staticlib never uses) and
  `NROS_CFLAGS_ARMV7A` are deleted.
  - rv-virt now builds on the make road: `just nuttx build-make-road riscv`
    is green (ELF 6,442,336 bytes, 29 `nros_*` functions), so the nightly
    `riscv` cell is back.
  - armv7a is unchanged: 5,308,332 bytes, 29 `nros_*` functions.
  - cc-rs still adds its own `-march=armv7-a+neon-vfpv4` beside NuttX's
    `-mcpu=cortex-a7`, which warns `switch '-mcpu=cortex-a7' conflicts with
    switch '-march=...'` (4 times per build). The old literals warned the
    same.
  - W3b and the acceptance's "no new copy" gate remain.

### W4 -- One triple (2426 items 3 and 4)

- The archive path in `Make.defs` uses the triple the Makefile resolved; the
  `NROS_NUTTX_TRIPLE := armv7a-nuttx-eabihf` literal goes. Gate: none.
- The empty-triple branch that calls upstream's `RUST_CARGO_BUILD` with an
  empty `--target` is deleted (it cannot build). Gate: none.
- The armv7a triple comes from ONE fallback, in one place, and a gate checks
  it against upstream's table (`RUST_TARGET_TRIPLE` /
  `nuttx_rust_target_triple`) for every triple upstream knows, so the two
  can never disagree. The armv7a branch is contributed upstream, and the
  fallback is deleted when the nuttx-apps pin gains it. No nuttx-apps fork.
  Our cargo command stays (Q12 b). Gate: cleared (Q11 b, Q12 b, ruled 2026-11-12).
- **Acceptance:** W1 links on both arches with the archive found under the
  triple cargo wrote; an arch with no triple is refused at parse time,
  naming `LLVM_ARCHTYPE`.
- **Status:** OPEN (the code is done, 2026-11-13; the armv7a branch is not
  yet offered upstream).
  - **The one triple.** `integrations/nuttx/RustTriple.mk` resolves
    `NROS_RUST_TRIPLE`: upstream's `RUST_TARGET_TRIPLE` first, then
    `armv7a-nuttx-<abi>` for `LLVM_ARCHTYPE=armv7a` alone. The shell's
    `Make.defs` includes it, so the archive path and the Makefile's
    `--target` are one word, and `NROS_NUTTX_TRIPLE` is gone.
  - **The empty-triple branch** (upstream's `RUST_CARGO_BUILD` with an
    empty `--target`) is deleted.
  - **The refusal.** A configured tree with no triple is refused while the
    Makefiles are read, naming `LLVM_ARCHTYPE`, `LLVM_ABITYPE` and
    `LLVM_CPUTYPE`. "Configured" means `$(TOPDIR)/include/nuttx/config.h`
    exists. `make olddefconfig` reads the apps Makefiles while `.config` is
    still unresolved: on rv-virt, `CONFIG_ARCH` is set but
    `CONFIG_ARCH_RV32` is not yet derived, so every `LLVM_*` is empty
    (measured). For the same reason, `Make.defs` asks for the archive only
    once a triple exists.
  - **The gate.** `just check nuttx-rust-triple` evaluates upstream's
    `tools/Rust.mk` and `RustTriple.mk` together with GNU make over 19
    cases: every arch upstream's macro knows, plus armv7a, armv7r, aarch64
    and xtensa. It requires:
    - the same answer wherever upstream answers;
    - the fallback for armv7a alone;
    - a refusal naming `LLVM_ARCHTYPE` for the rest.
  - **How the gate goes red.** It fails, saying to delete the fallback, as
    soon as the nuttx-apps pin answers armv7a itself. Its self-test (a
    fallback that answers for every arch) must fail first. It is a recorded
    skip without a nuttx-apps tree.
  - **Not covered.** The gate reads the make macro, not
    `nuttx_rust_target_triple` (upstream's CMake twin), which no nano-ros
    road calls since W8 deletes the CMake shell.
  - **Open:** the upstream contribution of the armv7a branch to nuttx-apps
    `tools/Rust.mk`. It is an outward-facing PR, so it is not filed from
    here without the maintainer's go-ahead.

### W5 -- The board's cargo config on the make road (2426 items 1 and 5)

The make road's cargo takes the board's `cargo_config` (build-std
`core, alloc, panic_abort`, build-std-features, the target spec) through the
image's generated cargo config (`--config`), and drops
`-Zbuild-std=std,panic_abort`, the `libc` patch literal and
`export RUSTUP_TOOLCHAIN ?= nightly-2026-04-11` (the toolchain comes from
`tools/rust-toolchain.toml` like every other recipe).

- Gate: cleared (Q3 B1 and Q5 yes, ruled 2026-11-12).
- **Acceptance:** W1's cargo command line carries no literal from
  `Makefile:229-231`, `:254` or `:144`; `std` is not compiled (no
  `Compiling std` in the log); a nightly bump moves no line in
  `integrations/nuttx/`.
- **Status:** DONE (2026-11-13, W5 PR).
  - **The board's cargo settings.** The shell's `context::` recipe runs
    `nros locate nuttx-cargo-config` first. It writes the board's
    `cargo_config` with configure's renderer (`builder::cargo_config`) to
    `<image root>/make/nros-cargo.toml`, and cargo reads that file with
    `--config`.
  - **Removed from the command:** `-Zbuild-std=std,panic_abort`, the `libc`
    patch literal, `--target` (the file's `[build] target` states it) and
    `export RUSTUP_TOOLCHAIN ?= nightly-…`. The nightly is now the first
    `nightly-*` row of `config/rust-toolchains.txt`, the row the cmake road's
    `nros_pinned_nightly` reads. It is set on the cargo command, so a nightly
    bump moves no line in `integrations/nuttx/`.
  - **Board identity on a build with no project** (manager ruling
    2026-11-13, option a: the board is a kernel fact, never stated). It is
    the one NuttX board whose `target` is the triple the tree resolved,
    narrowed by `CONFIG_ARCH_BOARD` against `[board.nuttx] board_config`
    only when several share it. Zero or several matches are refused, naming
    the candidates and "name a project" as the way to choose. That is W6's
    `NROS_PROJECTS`, so until W6 lands, the refusal names a mechanism not
    yet built. Unit tests `cmd::locate::nuttx_board_tests` cover the two
    in-tree boards, a target with no board, and a twin narrowed and then
    left ambiguous.
  - **Measured** with `just nuttx build-make-road`:
    - arm: `Compiling std` 0 times, `core` and `alloc` once each; ELF
      5,308,348 bytes, 29 `nros_*` functions.
    - riscv: the same counts; ELF 6,435,352 bytes. It built with the board's
      own `riscv32imac-unknown-nuttx-elf.json` as its target, so the make
      road already meets W9's "one spec on every road".

### W6 -- A make app as a nros project (issue 2236 A/B; 2426 item 6)

Project identity and `nros configure` inside the shell's `context::`, as
ruled; the book's NuttX steps 3 and 4 (`book/src/guide/platforms/nuttx.md`)
describe what the road then does.

- Identity is A2: the app declares itself in its OWN `Make.defs`, beside its
  `CONFIGURED_APPS +=` line:

      NROS_PROJECTS += $(APPDIR)/external/<app>
      NROS_IMAGE := <image id>          # optional; default_images otherwise

  The shell's `Makefile` reads `NROS_PROJECTS`. None means a guest, with a
  byte-identical build. One is the project. More than one is refused, naming
  them. The app template gains the idiom.
- A make-app project declares its entities (`entities = [...]` or
  `"defaults"`). No make census here; that is a separate issue (Q6).
- Gate: cleared (Q1 yes, Q2 A2, Q3 B1, Q4 yes, Q6 yes, Q7 yes, ruled 2026-11-12).
- **Acceptance:** 2236 section 5's acceptance list.
- **Status:** OPEN (W6a done 2026-11-13; W6b and W6c not started). Split
  into three PRs (manager, 2026-11-13).
  - **W6a, identity.** The shell's Makefile reads `NROS_PROJECTS`:
    - none: a guest, with the same command as before;
    - more than one: refused in `context::`, naming every path;
    - one: `nros locate nuttx-cargo-config --project <dir> [--image
      NROS_IMAGE] --rmw --ros-edition --kconfig` checks it against this
      kernel, and writes the chosen image's key beside the settings file.
      Then `nros configure --image <key>` runs in the project.
  - **The checks** (`check_nuttx_project`):
    - The image is `NROS_IMAGE`, or else the one image whose board is the
      kernel's (W5's derivation).
    - A named image whose board is not the kernel's is refused.
    - Kconfig's RMW and edition must match the image's (Q4).
    - Every `[[component]]` must declare `entities`, unless `[system]
      entities = "defaults"` (Q6).
    - Each refusal names the file, the field and the value to change, and
      no internal id. A unit test checks the last of these.
  - **Tests.** Six unit tests (`cmd::locate::nuttx_project_tests`), plus a
    manual run in a staged arm tree. With a copy of the C talker workspace
    as `apps/external/talker_proj`, the missing-entities refusal fired.
    With `entities = []` added, `nros configure` ran in `context::` and
    chose `nuttx_c_talker_bringup:qemu-armv7a-nuttx`. A second
    `NROS_PROJECTS` line was refused. The guest build is unchanged (5,308,348 B).
  - **What W6a does not do.** The configured image's knobs do not reach
    cargo yet; that is W6b, through the cmake road's own producer.
  - **Measured (for W6b and W6c): NuttX's apps `context` is NOT run on
    every build.** `tools/Unix.mk` runs it only while the `apps/.context`
    stamp is missing. After a first build, `make` runs neither the shell's
    `context::` nor its cargo build until a clean removes the stamp. So
    "edit a knob, and the next `make` changes the archive with no `clean`"
    (2236 section 5) needs an edge from `system.toml` that the stamp does
    not give. The same holds today for an edited nano-ros Rust source on
    this road. W6b has to answer this, not assume `context::` reruns.

### W7 -- The image crate is generated in `context::`

`nros_image_crate`'s `$(shell cmake -P ...)` moves from parse time into the
`context::` recipe, and the issue 0551 parse guard it needs goes with it.

- Gate: cleared (Q9 yes, ruled 2026-11-12). W6's ordering still applies if a project is named.
- **Acceptance:** `make olddefconfig` in a staged tree runs no `cmake`; W1
  green.
- **Status:** DONE (2026-11-13, W7 PR).
  - **What moved.** The Makefile now does nothing while make reads it.
    - The image-crate generator is a `context::` recipe line. It writes
      `<manifest> <features>` to `$(NROS_IMAGE_SPEC)`
      (`<image root>/image-<flavor>.txt`), which the cargo line and
      `clean::` read when they run.
    - The two `$(shell mkdir -p …)` object-dir lines moved into the same
      recipe, so the issue 0551 guard (`NROS_APPS_BUILD_OK`) has nothing
      left to guard, and it is deleted.
  - **Measured** with a logging `cmake` shim on `PATH`, against the pre-W7
    Makefile:
    - a full `just nuttx build-make-road arm` ran the generator 4 times
      before (once per parse of a configured tree) and once after;
    - `make clean` ran it 0 times both ways;
    - `make olddefconfig` in a staged tree runs no `cmake` both before and
      after, because the old guard already skipped the call on that
      unconfigured parse. That acceptance line holds, but it is not what
      changed.
  - **Results.** W1 is green on both boards (arm 5,308,348 B, riscv
    6,435,352 B), and no `external.nros-build` junk directory appears.

### W8 -- Delete NuttX's CMake shell (2426 item 7)

Ruled (Q16 b): `integrations/nuttx/CMakeLists.txt` and the book's sentence
offering NuttX's CMake flow (`nuttx.md:103-104`) are deleted. RFC-0130 Q-N1
holds the flow back until it is measured booting both boards and a lane
builds it, and only then is it offered again, as new work.

- Gate: cleared (Q16 b, ruled 2026-11-12).
- **Acceptance:** `integrations/nuttx/CMakeLists.txt` is gone, and no doc
  offers the flow.

### W9 -- The rv-virt target spec

Ruled (Q15 a): the board's `riscv32imac-unknown-nuttx-elf.json` is the one
statement, on EVERY road, including the make road through the board's
`cargo_config` (W5). Measured starting point: it equals rustc's built-in spec
plus `eh-frame-header: false` and `panic-strategy: abort`. Upstreaming it to
rustc or to NuttX stays possible later.

- Gate: cleared (Q15 a, ruled 2026-11-12).
- **Acceptance:** the spec has one home; every road that links with rustc on
  rv-virt still links (`workspace-rust-nuttx-riscv-realtime`); the
  regeneration note names the pinned nightly it was compared with.

### W10 -- Configure the kernel with NuttX's configure

The board defconfigs become fragments over upstream's
`qemu-armv7a:nsh` and `rv-virt:netnsh` (`#include`, `process_config.sh`),
and `scripts/nuttx/build-nuttx.sh` calls `tools/configure.sh` instead of
copying the defconfig and running `olddefconfig` itself.

- Gate: cleared (Q17 yes, ruled 2026-11-12).
- **Acceptance:** the resolved `.config` of each board is byte-identical
  before and after (modulo `CONFIG_BASE_DEFCONFIG`); the kconfig frontend
  `configure.sh` needs is provisioned by `nros setup`, never by hand.

### W11 -- The boot entry

`CONFIG_INIT_ENTRYPOINT` names the image's entry, and the `nsh_main`
override in `nros-board-nuttx-qemu/src/entry.rs` goes.

- Gate: cleared (Q18 yes, ruled 2026-11-12).
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

### W13 -- One road for nano-ros packages (issue 2236 Q10)

Ruled (Q10): nano-ros packages build on ONE road for NuttX, the unified
ROS-CMake road of RFC-0130 D7, building against NuttX's exported make tree.
Today's leaf and workspace roads become that one road: their NuttX fixture
rows and tests move to it, and what they restate is taken from NuttX (W3b,
W9, W11). The `apps/external` make app (W1 to W8) stays as a GUEST road,
for nano-ros entering a NuttX user's own tree. It is not a second way to
build a nano-ros package.

- Gate: cleared (Q10, ruled 2026-11-12). It is ordered after RFC-0130's own phase lands D7,
  which this item consumes rather than builds.
- **Acceptance:** the support registry (RFC-0114) names the one road and the
  guest road, and each has a lane. No NuttX fixture row builds through a
  road other than RFC-0130 D7's.

### W14 -- The vendored `libc` fork

**Moved out of this phase (Q20: a separate phase, ruled 2026-11-12).** Once no
road compiles `std` (W5), a separate phase retires `third-party/nuttx/libc`
and its `[patch.crates-io]` rows, which the board descriptor already calls
unused. It is opened with `just phase-new` after W5 lands and is measured.
Kept here as a pointer only.

- Gate: not this phase's.
- **Acceptance:** no tracked `[patch.crates-io] libc` row; the SDK index has
  no `nuttx-libc` source; both arches' fixtures pass.

## Order

W0 is done. W1 lands first so the rest have a verdict. Then W5, W7, W6 in
that order; W3, W4, W8 to W11 in any order; W13 after RFC-0130's D7 lands.
W14 is a separate phase.

## What this phase does not do

- Touch the shared `third-party/nuttx/nuttx` tree's `.config` from a lane
  (issue 0525).
- Build RFC-0130's unified road; W13 consumes it.

## Acceptance for the phase

- The make road builds and links a real app on armv7a and rv-virt in a lane.
- No NuttX fact (triple, CPU or C flags, cross tool names, build-std, nightly)
  is written in `integrations/nuttx/`; each comes from NuttX's tree or the
  board's `cargo_config`.
- Issue 2426 is resolved; issue 2236 is resolved (W6).
