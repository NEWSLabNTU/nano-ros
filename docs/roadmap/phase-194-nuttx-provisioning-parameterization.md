# Phase 194 — NuttX provisioning de-hardcode (board/arch parameterization)

**Goal.** Let a user target a **new NuttX architecture/board** (riscv32/64,
cortex-m, xtensa-esp, a real board) by adding a board crate + its NuttX
defconfig + cross-toolchain — **without editing ARM-specific code**. Keep NuttX
**source-built** (`make export`, the canonical out-of-tree-app SDK); only the
arch-specific knobs become per-board parameters.

**Status.** Not started (design 2026-05-28, split from the NuttX provisioning
discussion).

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

- [ ] **194.1 — `build-nuttx.sh` toolchain-agnostic.** Take the cross-compiler
      (e.g. `NUTTX_CROSS=arm-none-eabi-gcc`) + any arch knobs from env / the
      board overlay instead of the hardcoded `arm-none-eabi-gcc` check; defconfig
      stays the per-board path. Error hint names the resolved toolchain.
      **Files:** `nros-board-nuttx-qemu-arm/scripts/build-nuttx.sh` (relocate/
      generalize so it serves any nuttx board, or factor a shared script).
- [ ] **194.2 — `nros-nuttx-ffi` arch flags from the board.** The `-mcpu`/
      `-mfloat-abi`/`-mfpu` (+ the `-print-libgcc-file-name` probe) come from the
      board overlay / cache-vars (e.g. `NUTTX_ARCH_CFLAGS`), not baked cortex-a7.
      **Files:** `nros-board-nuttx-qemu-arm/nros-nuttx-ffi/build.rs`,
      `cmake/board/nano-ros-board-nuttx-qemu-arm.cmake`.
- [ ] **194.3 — Board crate = the unit of new-arch support.** Document + shape so
      a new NuttX board is `nros-board-nuttx-<board>` supplying {defconfig,
      toolchain, arch flags} via its overlay, reusing the arch-agnostic platform
      port. A second board (e.g. a riscv `nuttx-qemu-riscv`) proves it.
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
