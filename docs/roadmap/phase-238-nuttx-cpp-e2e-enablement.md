# Phase 238 — NuttX C/C++ E2E enablement (bootable-ELF wiring)

**Goal.** Run the NuttX C and C++ example E2E tests (pub/sub, service, action) in
QEMU ARM virt, the same way the NuttX **Rust** examples already do. The compile
blocker that gated this is gone; what remains is producing a bootable ELF from the
C/C++ build.

**Status.** Planned. Created 2026-06-12 after verifying the `_SC_HOST_NAME_MAX`
blocker is resolved. Off the critical path (NuttX is a secondary platform).

**Depends on.** `nros-board-nuttx-qemu-arm` (kernel staging + link), the NuttX
submodule (`nuttx-12.13.0-4`), `cmake/NanoRosNodeRegister.cmake`, the rtos_e2e
harness.

## Background / what was verified

The 6 NuttX C++ build tests in `nuttx_qemu.rs` are `#[ignore]`'d with reason
"CMake build blocked by upstream libc missing `_SC_HOST_NAME_MAX`". **That blocker
is resolved** in the current submodule:

- `_SC_HOST_NAME_MAX` is defined (`third-party/nuttx/nuttx/include/unistd.h:170`),
  `HOST_NAME_MAX 32` (`limits.h:329`).
- All 6 NuttX C++ examples now **compile clean** — each produces its component lib
  `libnuttx_cpp_<name>_<name>_component.a` with no error
  (`just nuttx build-fixtures` + a direct `cmake --build` both confirmed).

The z_open hang and the `tcp_update_timer`/`z_clock_t` issues are likewise long
resolved (Apr 2026, see the `project_nuttx_investigation` memory).

## The actual gap

The C/C++ NuttX examples are **component-lib only** — `NanoRosNodeRegister` emits a
`STATIC` `<pkg>_<name>_component` library and stops there (Phase 194.3c scoped the
C path as "build-coverage, no e2e"). There is **no executable / bootable-ELF
target** for C/C++ NuttX. The rtos_e2e cases
(`test_rtos_*_e2e::platform_*_Nuttx::lang_2_C` / `lang_3_Cpp`) and the
`build_nuttx_{c,cpp}_*` resolvers expect a bootable ELF at
`examples/qemu-arm-nuttx/<lang>/<name>/build-zenoh/nuttx_<lang>_<name>`, which the
build never produces → `require_prebuilt_binary` fails.

Contrast the **Rust** path (works): the example deps `nros-board-nuttx-qemu-arm`,
whose build.rs links the NuttX kernel staging, and `cargo build` emits a complete
bootable ELF at `target/armv7a-nuttx-eabihf/release/nuttx-rs-<name>`.

## Approaches

### A — cmake executable target (recommended; mirrors the Rust board crate)
Add a NuttX bootable-ELF target to the C/C++ example build: an `add_executable`
(or a custom link command) that links
- the example's `<pkg>_<name>_component` static lib,
- `nros-c` / `nros-cpp` (already corrosion-built),
- the NuttX kernel staging: `third-party/nuttx/nuttx/staging/libc.a` (+ the kernel
  objects the Rust board link pulls in),
- the NuttX linker script,

producing `build-zenoh/nuttx_<lang>_<name>`. The exact link line is the SSoT in the
board crate's `nuttx_platform_build` / `nros-board-common` — extract it into a
reusable cmake fragment (or a small `link-nuttx-elf.sh` the cmake invokes) so the
C/C++ link matches the Rust one byte-for-byte (same staging, same `.ld`, same
`arm-none-eabi-gcc` flags). Self-contained per example; no apps-tree.

### B — NuttX apps-tree integration (`stage-external-apps.sh`)
Restructure each C/C++ example as a NuttX external app (Make.defs + Kconfig + an
NSH-registered `main`), stage them with `stage-external-apps.sh`, and rebuild the
kernel (`build-nuttx.sh`) with the apps enabled — one kernel ELF carries all apps
as NSH commands. More idiomatic NuttX, but needs the examples reshaped as apps and
the rtos_e2e harness changed to "boot the shared kernel, run `<name>` via NSH"
instead of per-example boot. Larger test-harness change.

**Recommendation: A** — keeps the per-example bootable-ELF model the harness +
resolvers already assume, reuses the proven Rust kernel-staging link, and needs no
harness change.

## Work items (Approach A)

1. **Extract the kernel-ELF link recipe** from the Rust board path
   (`nros-board-common::nuttx_platform_build` + the example's effective rustc link
   args) into a reusable form: staging libs, kernel objects, linker script, flags.
2. **Add the executable target** to the C/C++ NuttX example cmake (via
   `NanoRosNodeRegister` NuttX branch or a sibling `nano_ros_nuttx_executable`):
   link component + nros-c/cpp + staging → `build-zenoh/nuttx_<lang>_<name>`.
3. **Wire it into the fixture build** (`just nuttx build-fixtures` / the cpp/c
   fixture leaves) so the ELF is produced alongside the component lib.
4. **Un-ignore** the 6 `nuttx_qemu.rs` C++ `*_builds` markers (now they resolve a
   real ELF) and confirm the rtos_e2e `Nuttx` × `{C,Cpp}` cases boot + pass in QEMU
   (pub/sub, service, action — expect ~90–140 s each, like the Rust/C cases).
5. **Regression:** the Rust NuttX E2E + the C/C++ component build stay green.

## Acceptance

- `test_rtos_{pubsub,service,action}_e2e::platform_*_Nuttx::lang_{2_C,3_Cpp}` boot
  the example in QEMU and exchange data over zenoh (matching the Rust cases).
- The 6 C++ `*_builds` `#[ignore]` markers removed; the ELF builds in CI.
- NuttX Rust E2E unaffected.

## Notes

- The link is the finicky part — a NuttX bootable ELF is sensitive to the exact
  staging libs / linker script / toolchain flags; mismatches manifest as a silent
  QEMU reboot loop (cf. the Phase 177.8.c rust cross-CGU miscompile). Build the
  link from the Rust path's known-good inputs, don't hand-roll.
