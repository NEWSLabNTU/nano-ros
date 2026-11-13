# Phase 561 -- Retire the vendored NuttX `libc` fork

**Status (2026-11-14). IN PROGRESS: W0 to W2 done; W3 and W4 not started.**
The work moved out of
[phase-550](phase-550-nuttx-native-build-infrastructure.md) W14 by the
maintainer's ruling Q20 (2026-11-12): a separate phase that retires
`third-party/nuttx/libc` once no road compiles `std`. It implements
[RFC-0126](../design/0126-nuttx-builds-on-nuttx-infrastructure.md)'s
direction: every NuttX fact comes from NuttX, and nano-ros keeps no
second copy. Each item is claimed with `just claim phase-561-wN` before it
starts.

---

## Why

The fork exists because Rust's `std` for NuttX named `libc` symbols that
crates.io `libc` lacked (`_SC_HOST_NAME_MAX`). It also mirrored NuttX's
opaque C types as byte blobs. NuttX sizes several of those from Kconfig, and
two undersized mirrors corrupted callers' stack frames (issues 0167 and
0570). Phase-359 W7 took every NuttX image off `std`, so neither reason
holds any more.

Measured 2026-11-14, before any change: the fork is already unused in every
Rust NuttX image.

- A NuttX entry's lock resolves crates.io `libc 0.2.186`.
- cargo warns that the fork's patch "was not used in the crate graph".
- The only `libc` user in that graph is `cpufeatures`, which compiles it
  for x86 and aarch64 hosts only, not for arm or riscv.

What keeps the fork alive is about 89 references in configs, the CLI, the
SDK index, CI and two gates. There is also one stale setting: both C/C++ FFI
crates' `.cargo/config.toml` still say `build-std = ["std", …]`, although
both crates are `#![no_std]`. That compiles `std`, and with it the fork, for
nothing on the cmake road.

## Work items

### W0 -- Measure who reaches `libc`

For each NuttX image class, on both arches, record which crates in the graph
depend on `libc`, and whether any of them compiles it for the NuttX target:

- a Rust workspace entry;
- a hand-written Rust entry (the test bins);
- the cmake road's FFI crate;
- the make road's image crate.

- Gate: none.
- **Acceptance:** a table in this doc, one row per class and arch, from
  `cargo tree -i libc --target <triple>` on real roots.
- **Status:** DONE (2026-11-14). `cargo tree --offline -e normal -i libc
  --target <triple>` on the real roots, after `just nuttx build-fixtures`
  and `build-make-road`:

  | Image class | arm (`armv7a-nuttx-eabihf`) | riscv (the board's `riscv32imac-unknown-nuttx-elf.json`) |
  | --- | --- | --- |
  | Rust workspace entry (`examples/workspaces/rust`, `realtime-rust`) | no `libc` | no `libc` |
  | Hand-written entry (the logging-smoke test bin) | no `libc` | (arm only) |
  | cmake road's FFI crate (`nros-nuttx-ffi`, `nros-nuttx-riscv-ffi`) | no `libc` | no `libc` |
  | make road's image crate (`apps/external/.nros-build/<arch>/image`) | no `libc` | no `libc` |

  With `--target all`, `libc` appears only through `cpufeatures` → `sha2`
  → `ros-launch-manifest-rlx` → `nros-orchestration-ir`. `cpufeatures`
  compiles `libc` for x86 and aarch64 HOSTS only: the macro and host side,
  never the image.
  - A workspace entry resolves crates.io `libc 0.2.186` there.
  - The FFI crates' own locks still route that host copy through the
    fork's patch (`libc v0.2.183 (third-party/nuttx/libc)`). Deleting the
    patch moves the host side to crates.io `libc`, which is the right
    crate for a host.
  - The other thing that reaches the fork is the FFI crates' stale
    `build-std = ["std", …]`, since `std` for NuttX takes `libc` from the
    `[patch]`. That is W1.

### W1 -- The FFI crates stop compiling `std`

`nros-nuttx-ffi` and `nros-nuttx-riscv-ffi` build `core,alloc,panic_abort`,
as the board's `cargo_config` does.

- Gate: W0 shows no FFI-crate dependency needs `std`.
- **Acceptance:** every cmake NuttX fixture row builds on both arches, and
  its tests pass on QEMU.
- **Status:** DONE (2026-11-14). Both FFI configs say
  `build-std = ["core", "alloc", "panic_abort"]`.
  `just nuttx build-fixtures` built every NuttX row on arm and riscv, and
  `just nuttx build-make-road` built both arches. The make road's arm
  project case was sized from the probe. On QEMU, 26 of the 29 NuttX tests
  passed in one batch; that batch also ran `realtime_sched_tasks`. The 3
  reds were not NuttX failures:
  - two needed a native Linux peer that a disk-full Linux fixture build had
    left stale;
  - `realtime_sched_tasks`'s rows need Zephyr, FreeRTOS and ThreadX
    fixtures this run did not build.

  After the Linux peers were rebuilt, both peer tests passed:
  `c_riscv_nuttx_talker_delivers_cross_process` and
  `nuttx_arm_carves_the_parameter_store_and_delivers`.

### W2 -- Delete the `libc` patch rows and the code that writes them

Delete:

- the `[patch.crates-io] libc` rows in both boards' `cargo_config` and in
  both FFI crates' configs;
- their renderers and finders: `builder/cargo_config.rs`, `cmd/ws.rs`,
  `orchestration/metadata_{build,refresh}.rs`,
  `nros-sizes-build`'s `find_nuttx_patched_libc`, and the
  `nros-cargo-profile` diagnostic.

- Gate: W1.
- **Acceptance:** `git grep` finds no tracked `[patch.crates-io] libc`
  row; every NuttX fixture builds on both arches.
- **Status:** DONE (2026-11-14). The rows are gone from both boards'
  `cargo_config` and both FFI configs, and the FFI locks re-resolved to
  crates.io `libc`. `nros-sizes-build` lost `find_nuttx_patched_libc`, its
  libc injection and the cargo-minor lockfile seeding that existed only for
  that patch.
  Measured, not as planned: three of the listed renderers were never
  fork code. They are generic mechanisms that the fork happened to use:
  - `cmd/ws.rs`'s `board_patch_rows` (a board descriptor's `[patch]`
    rows, the one spelling phase-351 W3 gave them);
  - `builder/cargo_config.rs`'s patch merge;
  - `orchestration/metadata_build.rs`'s build-std probe blocker.

  They stay, with their prose updated, so a board that needs a patch row
  has one place to say it. The remedy strings that name `nuttx-libc` in
  `metadata_{build,refresh}.rs` and the `nros-cargo-profile` diagnostic
  name the SOURCE, so they go with it in W3.

### W3 -- Retire the source

Delete:

- the index's `[source.nuttx-libc]` and its `build_sources` entry;
- the `.gitmodules` entry and the gitlink;
- the worktree-provisioning row;
- the CI provisioning steps;
- the fork's book page (`book/src/reference/forks/libc.md`) and its
  `forks.toml` row.

The fork's REMOTE repository is left as it is. The maintainer may archive
it; nothing in this tree needs it once this lands.

- Gate: W2.
- **Acceptance:** the index has no `nuttx-libc` source; a fresh
  `nros setup --build-sources` fetches no libc tree.
- **Status:** OPEN.

### W4 -- Retire the fork-only machinery, and gate the lesson instead

Retire:

- `scripts/build/nuttx-libc-pin-guard.sh`, and the pin hashing in
  `workspace-fixture-signature.sh` and `fixtures-build.sh`;
- `check-nuttx-libc-struct-sizes.py` (issue 0570's gate measures the
  fork's own mirrors, which go with it);
- the `check-leaf-lockfiles` references, and the gate baselines.

Issue 0570's lesson stays enforced by a new gate in two halves:

- **Static, on the fast line:** no tracked `[patch.crates-io] libc` row,
  and no `nuttx-libc` source in the index.
- **Graph, in a tier that builds what it resolves**
  (`check-lane-contracts`): `cargo metadata` on real NuttX roots finds no
  crate that compiles `libc` for a NuttX target.

The refusal states the lesson, not just an issue id. NuttX sizes several
opaque C types from Kconfig, so a Rust mirror of a fixed size corrupts its
caller when it is smaller than the kernel's. A NuttX graph that needs
`libc` again needs struct mirrors sized from the kernel's own
configuration, with a size gate, and never plain crates.io `libc`.

- Gate: W3.
- **Acceptance:** both halves fail their own negative controls; the
  struct-size and pin-guard scripts are gone.
- **Status:** OPEN.

## Acceptance (phase)

- No tracked `[patch.crates-io] libc` row.
- The SDK index has no `nuttx-libc` source.
- Every NuttX fixture builds on both arches, and the make road builds on
  both arches.
- The NuttX tests and every NuttX row of `realtime_sched_tasks` pass on
  QEMU.

## Order

W0, then W1 to W4 in order.
