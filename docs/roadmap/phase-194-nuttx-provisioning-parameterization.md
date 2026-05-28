# Phase 194 — NuttX provisioning de-hardcode (board/arch parameterization)

**Goal.** Let a user target a **new NuttX architecture/board** (riscv32/64,
cortex-m, xtensa-esp, a real board) by adding a board crate + its NuttX
defconfig + cross-toolchain — **without editing ARM-specific code**. Keep NuttX
**source-built** (`make export`, the canonical out-of-tree-app SDK); only the
arch-specific knobs become per-board parameters.

**Status.** 194.1 + 194.2 + 194.3a + 194.3b + 194.5 done — the shared NuttX
provisioning carries **no arm literal** (all arch-specifics env-driven, arm
defaults); a full **riscv NuttX export builds** via `nros setup`'s toolchain +
the existing flow; the marker is board-aware. Remaining: **194.3c** (the
`nros-board-nuttx-qemu-riscv` crate — deferred) + **194.4** (CMake
self-provision, which retires the marker).

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

- `packages/boards/nros-board-nuttx-qemu-arm/scripts/build-nuttx.sh` requires
  `arm-none-eabi-gcc` (line ~56) + ARM cmd in help.
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
- [ ] **194.3c — `nros-board-nuttx-qemu-riscv` board crate (DEFERRED).** The full
      nano-ros riscv board: a custom defconfig (rv-virt + nano-ros features, no
      board etc-ROMFS), its `BOARD_MAKEDEFS` (`boards/risc-v/qemu-rv/rv-virt`),
      the per-board env (`NUTTX_CROSS=riscv-none-elf-gcc`, `NUTTX_ARCH=risc-v`,
      `NUTTX_ARCH_CFLAGS`, `NUTTX_LIBGCC_FLAGS`, `NUTTX_VECTORTAB_OBJ=""`), the
      riscv FFI link (entry symbol, lib group, cargo target triple), and a riscv
      qemu example. Validates the platform port + FFI on a 2nd arch end-to-end.
- [x] **194.5 — board-aware marker.** DONE (`ff3eef3b7`). `build-nuttx.sh` keys
      `.nros-nuttx-build-head` on HEAD + `sha256(defconfig)` and self-validates
      the in-tree `.config`'s `CONFIG_ARCH_BOARD` vs this board's — reconfigures
      on either mismatch. Verified: an rv-virt-configured tree → `just nuttx
      build-kernel` detected `'rv-virt' != 'qemu-armv7a'` → reconfigured to arm.
      **Necessity:** the marker is a workaround for the *single shared in-tree*
      NuttX config; once 194.4's self-provisioning uses per-board (out-of-tree)
      build dirs + CMake `ExternalProject` stamps, the marker is redundant —
      retire it with 194.4.
- [ ] **194.4 (optional) — Self-provision the export via CMake.** Wire `make
      export` (`build-nuttx.sh`) as a **marker-guarded** (`.nros-nuttx-build-head`),
      **shared** (build-once-link-many) CMake `ExternalProject`/custom-target that
      is a dependency of the nuttx example target — so `nros build`/`deploy` (and
      raw `cmake --build`) auto-provision NuttX with no manual `just nuttx
      build-kernel`, parameterized by the board overlay's defconfig/toolchain/flags.
      (Supersedes wiring `build-examples: build-kernel` in the justfile.)

## Acceptance criteria

- [ ] No `arm-none-eabi-gcc` / `cortex-a7` literal in the NuttX provisioning path;
      both come from the board overlay/env.
- [ ] A second-arch NuttX board crate builds its export + links an example using
      only its overlay + `nros setup`'d cross-toolchain — no edits to shared
      scripts/build.rs.
- [ ] `just nuttx build` (arm-qemu) still green (behavior-preserving for the
      existing board).

## Notes

- **Keep source-built** — decided 2026-05-28. A prebuilt-nuttx tier would *cap*
  arches to what we pre-build; source-built + per-arch cross-gcc (via `nros
  setup`) covers any NuttX-supported target. (A target-scoped prebuilt SDK tier
  for the canonical config, to lower the first-image flash floor, remains a
  separate deferred idea — see the Phase 187 comparison doc.)
- `make export` is NuttX's canonical out-of-tree-app mechanism (PX4, micro-ROS
  use it) — this phase parameterizes it, doesn't replace it.
