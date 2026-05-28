# Phase 194 — NuttX provisioning de-hardcode (board/arch parameterization)

**Goal.** Let a user target a **new NuttX architecture/board** (riscv32/64,
cortex-m, xtensa-esp, a real board) by adding a board crate + its NuttX
defconfig + cross-toolchain — **without editing ARM-specific code**. Keep NuttX
**source-built** (`make export`, the canonical out-of-tree-app SDK); only the
arch-specific knobs become per-board parameters.

**Status.** 194.1 + 194.2 + 194.3a done — the shared NuttX provisioning
(`build-nuttx.sh` + `nros-nuttx-ffi`) carries **no arm literal**; every
arch-specific is env-driven with qemu-arm defaults (verified: `just nuttx
build-kernel` + `build-examples` green). Remaining: 194.3b (a real 2nd-arch
board to prove it — needs that arch's toolchain/defconfig) + 194.4 (CMake
self-provision).

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
- [ ] **194.3b — Prove with a real 2nd-arch board.** A riscv (or other) NuttX
      board end-to-end. Needs that arch's NuttX defconfig + cross-toolchain
      (`riscv-none-elf-gcc` not yet provisioned here) — a NuttX-on-new-arch
      bring-up, tracked separately.
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
