---
id: 26
title: nros-c posix platform headers fail to compile under gcc 14 (riscv NuttX C build)
status: open
type: bug
area: c-api
related: [phase-194]
---

The C/C++ message-lib + example compile breaks on the **riscv NuttX** board
(Phase 194.3c, `nros-board-nuttx-qemu-riscv`) because `riscv-none-elf-gcc`
is **14.2.0**, four majors newer than the arm NuttX toolchain
(`arm-none-eabi-gcc` 10.3.1). NuttX reuses the **posix** platform layer
(`packages/core/nros-c/include/nros/platform/posix.h`), so this is a pure
nros-c portability bug, not a board-wiring issue — the board overlay, FFI
crate, cross toolchain, defines (`-DNROS_PLATFORM_NUTTX`), and host codegen
all resolve correctly; the build dies inside nros-c headers.

**Symptoms** (compiling `builtin_interfaces__nano_ros_c` for
`riscv32imac-unknown-nuttx-elf`):

1. **implicit `nanosleep`** — `posix.h` `#include <time.h>` then calls
   `nanosleep()`, but NuttX's `<time.h>` gates `nanosleep` behind a feature
   test macro (`_POSIX_C_SOURCE`/`__USE_POSIX`) that this compile doesn't set.
   gcc 14 makes `-Werror=implicit-function-declaration` the default, so what
   was a warning under gcc 10 is now a hard error.

2. **conflicting types for `nros_platform_atomic_{store,load}_bool`** — a
   genuine signature mismatch: the hand-written `posix.h` declares
   `static inline void nros_platform_atomic_store_bool(volatile bool*, bool)`
   / `bool nros_platform_atomic_load_bool(volatile bool*)`, while the
   cbindgen-emitted `nros_generated.h` declares
   `extern void nros_platform_atomic_store_bool(bool*, bool)` /
   `bool nros_platform_atomic_load_bool(const bool*)`. The `volatile` /
   `const` qualifiers differ. gcc 14 errors on conflicting types; gcc 10
   tolerated it.

**Impact**: blocks the Phase 194.3c riscv NuttX C example end-to-end link
(wave 194.3c.7). The arm NuttX path is unaffected (older, lenient toolchain),
but this is latent for *any* gcc-14-class build of the posix/NuttX C path.

**To fix** (nros-c, not the board):
- Reconcile the `nros_platform_atomic_*_bool` signatures so the hand-written
  `posix.h` inline defs and the generated extern decls agree on the
  `volatile`/`const` qualifiers (single source of truth — the cbindgen input).
- Ensure `nanosleep` is declared: set the POSIX feature macro before
  `<time.h>` in `posix.h`, or use a declaration NuttX exposes unconditionally.
- Add a gcc-14 / riscv NuttX C compile to CI so the regression is caught.

Discovered: 2026-06-11 building `examples/qemu-riscv-nuttx/c/talker` against a
freshly `nros setup qemu-riscv-nuttx`-provisioned soft-float rv32imac export.
