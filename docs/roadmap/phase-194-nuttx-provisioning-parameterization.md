# Phase 194 — NuttX provisioning de-hardcode (board/arch parameterization)

**Goal.** Let a user target a **new NuttX architecture/board** (riscv32/64,
cortex-m, xtensa-esp, a real board) by adding a board crate + its NuttX
defconfig + cross-toolchain — **without editing ARM-specific code**. Keep NuttX
**source-built** (`make export`, the canonical out-of-tree-app SDK); only the
arch-specific knobs become per-board parameters.

**Status.** 194.1 + 194.2 + 194.3a + 194.3b + 194.4 + 194.5 done — the shared
NuttX provisioning carries **no arm literal** (all arch-specifics env-driven,
arm defaults); a full **riscv NuttX export builds** via `nros setup`'s toolchain
+ the existing flow; the marker is board-aware; and the export **self-provisions
under cmake** (`nros build`/`deploy`/raw cmake auto-run `make export`, no manual
kernel build). Following 194.4, **`just nuttx setup` + `just nuttx build` no
longer pre-build the kernel** — the export self-provisions at the first
example/fixture build (`nros_nuttx_build_example`); the now-orphaned
`build-kernel` recipe was deleted and the provisioning script moved to the shared
build-script dir (`scripts/nuttx/build-nuttx.sh`, board defconfig supplied via
`NUTTX_DEFCONFIG` / the board overlay's `NROS_NUTTX_DEFCONFIG`) so the builders
are self-contained — force an out-of-band provision by running that script
directly (and `just nuttx doctor` reports an unconfigured kernel as informational
`[--]`, not a failure). Remaining: **194.3c** — the `nros-board-nuttx-qemu-riscv`
crate (the 2nd-arch end-to-end proof), now **in progress** on branch
`feat/194.3c-nuttx-riscv-board`, expanded into waves 194.3c.1–.8 below (the design
found 194.3a's "no arm literal" claim was incomplete — four arm literals survive
in the FFI linker-script step + the per-board platform build.rs).

**Priority.** P2 — extensibility/correctness of the NuttX path; today only
`nuttx-qemu-arm` (cortex-a7) is reachable because the provisioning hardcodes ARM.

**Depends on.** Phase 192.3 (build.rs walk-ups → env; NuttX includes already
read `NROS_*` env) + the `nros setup` cross-toolchain index (191/192.x — ships
`arm-none-eabi-gcc`/`riscv-none-elf-gcc`/… per arch).

## Overview

NuttX is provisioned the canonical way: build the submodule from source →
`make export` → link the app against the export (libs + headers + linker script
+ flags). That source-built model is exactly what makes *any* NuttX-supported
arch possible (no per-arch prebuilt — the export is built locally). **But the
provisioning hardcodes ARM in two spots**, so a new arch needs code edits, not
just a board crate:

- `scripts/nuttx/build-nuttx.sh` requires `arm-none-eabi-gcc` (line ~60) + ARM
  cmd in help.
- `packages/boards/nros-board-nuttx-qemu-arm/nros-nuttx-ffi/build.rs` bakes
  `-mcpu=cortex-a7 -mfloat-abi=hard -mfpu=vfpv3-d16` (lines ~41-43, ~226).

The defconfig is already per-board (`$BOARD_DIR/nuttx-config/defconfig`). The
NuttX platform port (`platform.c`/`net.c`/`timer.c`) + the cffi shim are
arch-agnostic (POSIX-ish over NuttX libc) and should stay reused.

## Architecture

Arch-agnostic (reuse): the NuttX platform port, the cffi shim, the FFI build.rs
logic, `make export` itself.
Arch-specific (parameterize, per board): **defconfig** (done), **cross-toolchain**
(`arm-none-eabi-gcc` / `riscv-none-elf-gcc` / …), **cc flags** (`mcpu`/`march`/
float-abi/fpu), the **linker script** (comes *from* the export — already generic).

The per-board CMake overlay (`cmake/board/nano-ros-board-<board>.cmake`, the
established glue) — or board cache-vars / env — supplies the toolchain + flags;
`build-nuttx.sh` and `nros-nuttx-ffi` *read* them instead of hardcoding ARM.
`nros setup <new-nuttx-board>` resolves that arch's cross-gcc (host tool, already
in the index); the kernel source-builds against it.

## Work items

- [x] **194.1 — `build-nuttx.sh` toolchain-agnostic.** DONE. Reads `NUTTX_CROSS`
      (default `arm-none-eabi-gcc`) for the presence check; hint names the
      resolved toolchain. (NuttX's `make` still picks the actual compiler from the
      defconfig's `CONFIG_ARCH_TOOLCHAIN` + PATH — this was only a guard.)
      Verified `just nuttx build-kernel`.
- [x] **194.2 — `nros-nuttx-ffi` arch flags from env.** DONE. App cc flags →
      `NUTTX_ARCH_CFLAGS` (default `-mcpu=cortex-a7 -mfloat-abi=hard
      -mfpu=vfpv3-d16`); cross-compiler → `NUTTX_CROSS`; libgcc probe →
      `NUTTX_CROSS` + `NUTTX_LIBGCC_FLAGS` (default neon-vfpv4 — kept distinct,
      it selects `v7ve+simd/hard` vs the compile flags' `v7-a+fp/hard`). Defaults
      = qemu-arm; `just nuttx build-examples` green.
- [x] **194.3a — Shared FFI fully arch-agnostic.** DONE (`6cbed2dce`). The last
      arm-locks (`arch/arm/src/board` link path + `arm_vectortab.o`) →
      `NUTTX_ARCH` (default `arm` → `arch/<arch>/src`) + `NUTTX_VECTORTAB_OBJ`
      (default `arm_vectortab.o`; empty skips it). `nros-nuttx-ffi` + `build-nuttx.sh`
      now carry **no arm literal** in the provisioning path.

  **New-arch NuttX board recipe** (the unit of support): a board crate
  `nros-board-nuttx-<board>` whose overlay sets the per-arch env —
  `NUTTX_CROSS` (e.g. `riscv-none-elf-gcc`), `NUTTX_ARCH` (e.g. `risc-v`),
  `NUTTX_ARCH_CFLAGS`, `NUTTX_LIBGCC_FLAGS`, `NUTTX_VECTORTAB_OBJ` (often empty),
  its `nuttx-config/defconfig`, and the cargo target triple — reusing the
  arch-agnostic platform port + the shared `build-nuttx.sh`/FFI. `nros setup`
  ships that arch's cross-gcc.
- [x] **194.3b — riscv NuttX export proven end-to-end** (2026-05-29):
      - ✅ `nros setup --tool riscv-none-elf-gcc` installs the xPack riscv
        toolchain — the cross-toolchain provisioning scales by arch.
      - ✅ **A full riscv NuttX export builds with it**: `rv-virt:flats`
        configure → `make` → `make export` → `nuttx-export-12.13.0.tar.gz`
        (8.1 MB, riscv), exit 0. (Needed `genromfs`, a host tool the stock
        rv-virt board's `etc/` requires — user-installed; a nano-ros riscv
        defconfig would drop it, as the arm one does.) Confirms the existing
        provisioning *flow* (configure + `make export`) is arch-agnostic given a
        defconfig + `NUTTX_CROSS`.
      - **Examined the scripts without a board crate:** the generic NuttX flow is
        arch-agnostic; the only board-bound inputs `build-nuttx.sh` needs are the
        `DEFCONFIG` + `BOARD_MAKEDEFS` (still hardcoded to the arm board's paths)
        — those are exactly what a board crate supplies.
### 194.3c — `nros-board-nuttx-qemu-riscv` board crate (2nd-arch end-to-end) — IN PROGRESS (branch `feat/194.3c-nuttx-riscv-board`, 2026-06-11)

Validates the arch-agnostic NuttX platform port + FFI on a 2nd arch end-to-end:
a riscv (rv-virt) qemu board reachable with **only** a board crate + overlay +
`nros setup`'d cross-toolchain — no edits to ARM-specific code. Targets the
**C/C++ example path** (`nuttx_build_example`, the 194.4-proven lane), NOT the
225.O-walled Rust `nros::main!` workspace-entry cargo path.

**Design (explored 2026-06-11).**

- **Rust target is builtin — no custom JSON.** `rustc` ships
  `riscv32imac-unknown-nuttx-elf` / `riscv64gc-unknown-nuttx-elf` (et al.),
  mirroring arm's `armv7a-nuttx-eabihf`; same `build-std = ["std","panic_abort"]`
  + `compiler-builtins-mem` path. `rv-virt:flats` is rv32 → `riscv32imac-unknown-nuttx-elf`.
- **Toolchain already provisioned.** `nros setup --tool riscv-none-elf-gcc` + a
  full `rv-virt:flats` `make export` are proven (194.3b).
- **Precedent.** `nros-board-threadx-qemu-riscv64` gives the naming /
  `target_contains` disambiguation + riscv board-crate shape to copy.
- **cmake auto-wires.** NuttX board dispatch is
  `cmake/board/nano-ros-board-${NANO_ROS_BOARD}.cmake` — dropping the overlay
  file is enough; no central edit.

**194.3a's "no arm literal" was INCOMPLETE.** Four ARM literals survive in the
FFI/build path (194.3a de-armed `build-nuttx.sh` arch/vectortab + the FFI
arch/cflags, but NOT the FFI **linker-script step** nor the per-board platform
build.rs):

| Seam | Location | Fix |
|---|---|---|
| linker script `boards/arm/qemu/qemu-armv7a/scripts/dramboot.ld` | `nros-board-common/src/nuttx_ffi_build.rs:226` | env `NUTTX_LINKER_SCRIPT` |
| preprocess uses `arm-none-eabi-gcc` | `nuttx_ffi_build.rs:228` | use existing `nuttx_cross` |
| preprocess `-I arch/arm/src/{chip,common,armv7-a}` | `nuttx_ffi_build.rs:237-239` | env `NUTTX_ARCH_INCLUDES` |
| `BOARD_MAKEDEFS=boards/arm/qemu/qemu-armv7a/scripts/Make.defs` | `scripts/nuttx/build-nuttx.sh:134` | env `NUTTX_BOARD_MAKEDEFS` |
| fn literally named `run_qemu_arm` | `nuttx_ffi_build.rs:3` | generalize → `run_nuttx` |
| arm platform.c/net.c compile (compiler+cflags+includes arm-hardcoded) | `nros-board-nuttx-qemu-arm/build.rs:22-32` | extract shared parameterized helper |

**Waves (each wave: implement → `just ci` / scoped build → commit):**

**Validation state (2026-06-11, branch `feat/194.3c-nuttx-riscv-board`):**
Waves .1–.3 landed + `cargo check -p nros-board-common` clean; **.3 proven
functionally** — `just nuttx build` ran the de-armed `build-nuttx.sh` and
`make export` produced `staging/libc.a` (arm). The **setup-config fix is
provision-verified**: `nros setup qemu-riscv-nuttx` fetched the full set
(riscv-none-elf-gcc + qemu-riscv64 + nuttx kernel/apps/libc + zenoh stack) with
no apt/manual-submodule. Waves .4–.6 are **code-complete** (riscv crate +
overlay + cmake makedefs-forward patch), a faithful mirror of the known-good arm
crate. **Not yet build-validated end-to-end:** the arm full-example regression
and the riscv example build (.7) need a host that can sustain the NuttX cross
`build-std` matrix — this dev box's rustc **SIGSEGVs** under the parallel
build-std compiles (tiny crates like `quote`/`encoding_rs` crash, signal 11), an
environment/toolchain instability unrelated to the refactor. Host `cargo check`
of the board crates is not a valid gate (cross-only: `build-std` +
`target_os="nuttx"`; host pulls no_std staticlibs that fail without a
panic_handler — the arm crate fails identically).

**Update — riscv export GREEN + C-build blocker found (2026-06-11).** With the
toolchains provisioned (`nros setup qemu-riscv-nuttx`) and on PATH (activate
fix), the **riscv NuttX export builds end-to-end**: `build-nuttx.sh` with the
riscv defconfig/makedefs reconfigures the shared tree arm→rv-virt (**.8 marker
swap proven live**) and `make export` produces a **soft-float rv32imac NuttX
ELF** (`file nuttx` → "RISC-V, RVC, soft-float ABI"). This validated three
defconfig deltas found by build feedback (apps-dir, RV32 toolchain, FPU-off for
ilp32) and the `[tool.genromfs]` setup-config addition (the rv-virt board bakes
an etc/ ROMFS the arm board doesn't). The riscv C example (.7) gets all board
wiring right — cmake configures, host codegen runs, the message lib compiles
with `-DNROS_PLATFORM_NUTTX` + the riscv cross toolchain — but the **cross-link
is blocked downstream of the board** by an nros-c gcc-14 portability bug
([issue-0027]): riscv-none-elf-gcc is 14.2 vs arm's 10.3, and nros-c's posix
platform header (NuttX reuses it) hits `-Werror=implicit-function-declaration`
on `nanosleep` + a `nros_platform_atomic_*_bool` `volatile`/`const` signature
mismatch vs the cbindgen-generated decls. That is a separate nros-c fix, not
194.3c board wiring. Defconfig is the validated `rv-virt:netnsh` + 3 deltas.

- [x] **194.3c.1 — Generalize `run_qemu_arm` → `run_nuttx`.** Parameterize the
      linker-script path (`NUTTX_LINKER_SCRIPT`), the preprocess compiler (reuse
      `nuttx_cross`, not literal `arm-none-eabi-gcc`), and the preprocess arch
      includes (`NUTTX_ARCH_INCLUDES`). Arm defaults byte-for-byte unchanged
      (`run_qemu_arm` kept as a thin wrapper). Verify `just nuttx build-examples`
      still green.
- [x] **194.3c.2 — Shared NuttX platform-lib build helper.** Extract the
      `nros-board-nuttx-qemu-arm/build.rs` platform.c/net.c compile into a
      `nros_board_common::nuttx_platform_build::run()` parameterized by
      `NUTTX_CROSS`/`NUTTX_ARCH_CFLAGS`/`NUTTX_ARCH_INCLUDES`; arm root build.rs
      delegates (removes the last 3 arm literals in the arm crate). Arm build green.
- [x] **194.3c.3 — `build-nuttx.sh` BOARD_MAKEDEFS param.** Add
      `NUTTX_BOARD_MAKEDEFS` (board overlay supplies; default = arm
      `boards/arm/qemu/qemu-armv7a/scripts/Make.defs`). Arm provisioning unchanged.
- [x] **194.3c.4 — riscv defconfig.** `rv-virt:flats` + nano-ros feature set
      (zenoh-pico + **virtio-net** for the e2e), no board etc-ROMFS (drops the
      `genromfs` host-tool dep 194.3b hit). Lands under the new crate's
      `nuttx-config/defconfig`.
- [x] **194.3c.5 — `nros-board-nuttx-qemu-riscv` crate.** `nros-board.toml`
      (`names=["nuttx-riscv"]`, `target_contains="riscv"`, `cargo_config` = riscv
      target + `riscv-none-elf-gcc` linker + riscv cflags + build-std);
      `src/{lib,config,entry,node}` with a `QemuRvVirt` board ZST
      (`BoardInit`/`BoardPrint`/`BoardExit`); `nros-nuttx-ffi` subcrate whose
      build.rs calls the 194.3c.1 helper with riscv env (`NUTTX_ARCH=risc-v`,
      `NUTTX_VECTORTAB_OBJ=""`, riscv `NUTTX_LINKER_SCRIPT` + `NUTTX_ARCH_INCLUDES`
      + `NUTTX_LIBGCC_FLAGS`); riscv toolchain cmake file.
- [x] **194.3c.6 — cmake overlay.** `cmake/board/nano-ros-board-nuttx-qemu-riscv.cmake`
      mirroring the arm overlay (FFI crate dir, provision script + riscv defconfig,
      `nros_nuttx_set_cargo_target("riscv32imac-unknown-nuttx-elf")`,
      `nros_board_link_app`). Auto-wired by the board-name dispatch.
- [~] **194.3c.7 — riscv qemu example + e2e.** Mirror
      `examples/qemu-arm-nuttx/c/.../talker` as a riscv C example; add the
      `fixtures.toml` row + a `qemu-system-riscv` run that asserts cross-process
      `/chatter` delivery to an external native listener (the real acceptance,
      à la 225.O esp32).
- [x] **194.3c.8 — Shared-tree marker (194.5 tail).** Confirm the board-aware
      marker reconfigures on the arm↔riscv `.config` swap (it keys on
      `CONFIG_ARCH_BOARD`); full out-of-tree per-board build-dir retirement stays
      deferred.

**Open risks (resolve during the relevant wave):**

1. **riscv flat-build entry/vectortab** — `--entry=__start` is expected to hold;
   riscv has no `arm_vectortab.o` (`NUTTX_VECTORTAB_OBJ=""`). Confirm the riscv
   chip dir name for `NUTTX_ARCH_INCLUDES` (`arch/risc-v/src/{chip,common}`).
2. **riscv libgcc multilib flags** (`-march=rv32imac -mabi=ilp32`) for the
   `-print-libgcc-file-name` probe.
3. **std on rv-virt** — arm NuttX ships `std` (resolves from NuttX `libc.a`);
   riscv NuttX `std` is tier-3, same build-std path. 225.O kept `std` on NuttX —
   a good sign.
4. **rv-virt virtio-net** — confirm the flat defconfig brings up networking so the
   cross-process e2e can deliver (the genuine 2nd-arch proof).
- [x] **194.5 — board-aware marker.** DONE (`ff3eef3b7`). `build-nuttx.sh` keys
      `.nros-nuttx-build-head` on HEAD + `sha256(defconfig)` and self-validates
      the in-tree `.config`'s `CONFIG_ARCH_BOARD` vs this board's — reconfigures
      on either mismatch. Verified: an rv-virt-configured tree → `just nuttx
      build-kernel` detected `'rv-virt' != 'qemu-armv7a'` → reconfigured to arm.
      **Necessity:** the marker is a workaround for the *single shared in-tree*
      NuttX config. 194.4 **kept and leaned on** it — the marker is both the
      cache-invalidation key and the up-to-date guard for the build-once-link-many
      no-op, plus a `flock` to serialize concurrent provisions of the shared tree.
      Full marker *retirement* (the cleaner end state) needs per-board
      (out-of-tree) build dirs + CMake `ExternalProject` stamps so no two boards
      share one `.config` — that is **deferred** with 194.3c (a 2nd-arch board is
      what makes the shared-tree contention real).
- [x] **194.4 (optional) — Self-provision the export via CMake.** DONE. The
      board overlay (`cmake/board/nano-ros-board-nuttx-qemu-arm.cmake`) exposes
      `NROS_NUTTX_PROVISION_SCRIPT` (→ the shared `scripts/nuttx/build-nuttx.sh`); the generic
      `nros_nuttx_build_example` (`nros-c/cmake/nros-nuttx.cmake`) prepends a
      provision `COMMAND` (run the script in `NUTTX_DIR` with `NUTTX_DIR` +
      derived `NUTTX_APPS_DIR`) to each example's FFI `add_custom_command`, before
      `cargo build`. So `nros build`/`deploy` + raw `cmake --build` auto-`make
      export` with no manual kernel pre-build. **Concurrency- + idempotency-
      hardened** (the shared in-tree tree is hit by many parallel example builds):
      `build-nuttx.sh` now (a) `flock`s the provision (serialize concurrent
      `make export` — was racing `mkdir nuttx-export-<ver>`/`.version.tmp`),
      (b) `rm`s any stale `nuttx-export-*` before `make export` (it isn't
      idempotent — fails if the dir exists), and (c) short-circuits to a true
      no-op when the marker is fresh AND a completed export is present
      (build-once-link-many; a direct `scripts/nuttx/build-nuttx.sh` run is idempotent too).
      Verified end-to-end: removing the export + building a nuttx C example via
      cmake **rebuilt the export from nothing** before the cargo link; a fresh
      tree no-ops with `NuttX export up-to-date — skipping`. **`just nuttx
      build-fixtures` is fully green** (all C + C++ examples) after fixing the
      orthogonal nros-cpp layout-mirror break (`6d71ce17d`): `CppQosLayout`
      mirrored the qos `#[repr(C)]` enums as `c_int`, but ARM EABI `-fshort-enums`
      makes them 1 byte (not 4) — over-sizing `CppActionServer` by 12 bytes on ARM
      and tripping the size assert (E0080). Mirror now uses a `#[repr(C)]` enum
      (tracks short-enum width per target); real `CppActionServer`/`CppActionClient`
      also pinned to `#[repr(C)]`.

## Acceptance criteria

- [x] No `arm-none-eabi-gcc` / `cortex-a7` literal in the NuttX provisioning path;
      both come from the board overlay/env. **Done (194.3c.1/.2/.3)** — the FFI
      linker-script step, the platform-port compile, and `build-nuttx.sh`
      BOARD_MAKEDEFS are env-driven; the riscv export builds with only the
      board's `NUTTX_*` env, no shared-script arm literals on the path.
- [~] A second-arch NuttX board crate builds its export + links an example using
      only its overlay + `nros setup`'d cross-toolchain — no edits to shared
      scripts/build.rs. **Export half: DONE** (riscv soft-float rv32imac export
      builds via overlay + `nros setup qemu-riscv-nuttx`). **Link half: blocked**
      on [issue-0027] (nros-c posix headers under gcc 14) — board wiring is
      correct, the failure is downstream in nros-c.
- [x] `just nuttx build` (arm-qemu) still green (behavior-preserving for the
      existing board). **Verified** — `just nuttx build` runs the de-armed
      `build-nuttx.sh` → arm `make export` → `staging/libc.a` (4.2 MB).

## Remaining to close 194 (all 194.3c.7)

**issue-0027 RESOLVED 2026-06-11** (cbindgen `[export] exclude` + NuttX sysroot
include on the NanoRos umbrella). The riscv C compile blockers are gone:
**both the riscv (rv32imac) and arm (cortex-a7) NuttX C talkers compile their
generated std_msgs message libs + component archive clean** (serial build,
provisioned exports). That validates the de-arm refactor + the riscv overlay/FFI
on the *compile* path (194.3c.5/.6 compile side). What remains:

**Scope correction.** The NuttX **C** examples are **build-coverage only** — the
arm C nuttx examples are never run as kernels (the runnable nuttx kernels are the
**rust** standalone examples, `build_rust_example`; arm's C coverage is the
component/bringup *build* test `nuttx_qemu_arm_2_component_bringup_builds`). So a
runnable riscv C kernel + a virtio-net e2e are **not** part of the C-path design
(they'd belong to a separate riscv *rust* standalone example, the
`nros::main!`-style path 194.3c deliberately did not target). The earlier
"final entry ELF / e2e" checklist items were over-scoped against the arm
baseline and are dropped. The C-path proof is: **the riscv C component builds**
(✅ validated), exactly mirroring the arm C build-coverage.

- [x] **Harness wiring — DONE + validated 2026-06-11.** `examples/fixtures.toml`
      gained a `platform = "nuttx-riscv"` C-talker row; `just nuttx build-riscv-c`
      (new recipe) provisions the rv-virt export (reconfiguring the shared tree
      arm→rv-virt via the marker) and builds the riscv C example via the harness
      with the riscv toolchain + riscv FFI crate; `build-all` includes it; the
      riscv FFI crate gained its `rust-toolchain.toml` (nightly pin). **Verified
      green:** `just nuttx build-riscv-c` → std_msgs message lib + component
      archive built ("NuttX riscv C examples built!"). The arm `nuttx`-platform
      rows + recipe are untouched (riscv is a distinct platform tag + recipe, so no
      shared-tree concurrency). CI runs `build-riscv-c` after the arm fixtures.
- [ ] (Optional) a `qemu-riscv-nuttx` cell in the cmake `platform`/`board` smoke
      matrix (`cmake_platform_matrix.rs`) — nice-to-have; the build recipe already
      gives the coverage.
- [ ] (Optional, separate from 194.3c) a riscv **rust** standalone example
      (`examples/qemu-riscv-nuttx/rust/talker`, mirror of the arm rust talker) for
      a runnable rv-virt kernel + virtio-net e2e — only if a runnable riscv demo is
      wanted; not required to close the de-hardcode goal.
- [x] ~~Cosmetic: `build-nuttx.sh` arch-aware run hint~~ — done (`05e159217`).

**Env note:** the cross `build-std` link must run **serially** here — the parallel
`just nuttx build-fixtures` matrix SIGSEGVs host rustc under memory pressure
(not a missing SDK; everything provisions via `nros setup`). CI / a larger host
runs it parallel.

## Notes

- **Keep source-built** — decided 2026-05-28. A prebuilt-nuttx tier would *cap*
  arches to what we pre-build; source-built + per-arch cross-gcc (via `nros
  setup`) covers any NuttX-supported target. (A target-scoped prebuilt SDK tier
  for the canonical config, to lower the first-image flash floor, remains a
  separate deferred idea — see the Phase 187 comparison doc.)
- `make export` is NuttX's canonical out-of-tree-app mechanism (PX4, micro-ROS
  use it) — this phase parameterizes it, doesn't replace it.
