# Phase 147 — `install-local` Per-Platform Fragility Triage (radar)

> **Archived 2026-05-18 — moot.** Phase 140 deleted `just
> install-local`, the `build/install/` layout, every `install(...)`
> rule, and every `Config.cmake.in` template. Every issue listed
> below — the 7 inline fixes on the Phase 144 branch AND the two
> Open sections (`app_define.c` picolibc vs nxd_bsd.h collision,
> threadx_linux / freertos install-recipe untested ground) —
> exists only because the legacy install path tried to build
> complete staticlibs from a CMake configure that didn't know
> which TUs a downstream consumer would link. The
> `add_subdirectory(<repo-root>)` shape (Phase 137/138/139/144)
> only builds what `nros_platform_link_app(target)` actually
> needs, so the include-composition collisions can't surface.
> Verification: `grep -rn install-local justfile just/*.just`
> returns only "Phase 140 — removed" / "Phase 140 — `install`
> recipe deleted" comments; no live recipe references the path.
> Keeping the doc around as historical reference only — do not
> iterate.

**Goal.** Document the per-platform install-local issues surfaced
by Phase 144's CI runs. These are NOT migration bugs — Phase 144's
structural work (83 example CMakeLists migrated to
`add_subdirectory`) is correct and `just check` passes. The
failures are install-local-stage breakage that Phase 144's new
add_subdirectory composition exposed in the legacy install path.

**Status.** Documented for triage. Phase 140 supersedes this work
by deleting `install-local` entirely — the issues become moot.
This phase exists ONLY so that if Phase 140 is delayed and someone
needs to iterate on install-local in the meantime, they have the
issue list in one place.

**Priority.** P4 — every issue logged here becomes irrelevant the
moment Phase 140 lands. Do not invest in fixing them unless Phase
140 slips for an unrelated reason.

**Depends on.** None.

**Related.** Phase 140 (install-local rip-off — supersedes this
work), Phase 144 (the migration whose CI runs surfaced these
issues).

---

## Overview

Phase 144 migrated 83 examples to `add_subdirectory` consumption.
`just check` (fmt, clippy, embedded, features, abi-mirror, C, C++,
Python) passes green. `just test-all` is blocked at the
`install-local` orchestration step inside `test-all`'s setup chain
(Phase 135.1's hoisted dep).

`install-local` iterates `for rmw in zenoh xrce dds cyclonedds`
and for each runs `cmake -S . -B build/cmake-$rmw -DNANO_ROS_RMW=$rmw
-DNANO_ROS_PLATFORM=posix`. Then per-RTOS `install` recipes do the
same for freertos / threadx-{linux,riscv64} (NuttX install is a
no-op per upstream libc gap).

Each of these CMake invocations goes through the root
`CMakeLists.txt` → Phase 138 platform module → Phase 144 board
overlay. The new composition pulls headers / build setup that the
legacy install path didn't.

---

## Fixed inline during Phase 144 CI iteration

These landed as fix commits on the `phase-144-example-migration`
branch:

| # | Fix commit | Issue |
|---|------------|-------|
| 1 | `c5e784df` | nightly fmt sweep on 7 files (pre-existing drift) |
| 2 | `576912c8` | clippy: `.extend(v.drain(..))` → `.append(&mut v)`, `for (k,_) in &map` → `for k in map.keys()` (zpico-sys + manifest.rs) |
| 3 | `850c6d00` | nros-platform-posix INTERFACE_INCLUDE_DIRECTORIES embedded source-tree path; split BUILD_INTERFACE / INSTALL_INTERFACE |
| 4 | `26c6a700` | Root CMake cyclonedds branch (was FATAL_ERROR stub); now `add_subdirectory(packages/dds/nros-rmw-cyclonedds)` |
| 5 | `c264683e` | RTOS install recipes missing `-DNANO_ROS_BOARD=…`; Phase 144 platform modules require it for staticlib build (FreeRTOSConfig.h etc.) |
| 6 | `c6896b9f` | Board overlays' `if(NOT DEFINED VAR AND NOT DEFINED ENV{VAR})` skipped `set(VAR …)` when ENV-only set, leaving CMake var empty for downstream `if(EXISTS "${NETX_DIR}/…")` checks |
| 7 | `41b87e6d` | `app_define.c` `int errno;` collided with `nxd_bsd.h`'s `#define errno (tx_thread_identify() -> bsd_errno)` macro once Phase 144's NetX-BSD include landed in the threadx_glue compile unit |

---

## Open — `app_define.c` picolibc vs nxd_bsd.h collision

**Surfaces in:** `just threadx_riscv64 install` (and likely
`threadx_linux install` once it gets that far).

`packages/boards/nros-board-threadx-qemu-riscv64/c/app_define.c`
gets compiled as part of `threadx_glue` target. Phase 144.7-8's
platform module composes NetX BSD addon headers (`nxd_bsd.h`) into
the include set for every threadx_glue TU. `nxd_bsd.h` includes
`<sys/types.h>` and `<sys/time.h>` which collide with picolibc's:

1. `suseconds_t` typedef redeclaration (picolibc `sys/types.h:243`
   vs nxd_bsd.h's own).
2. `fd_set`, `timeval` likely follow once (1) is fixed.
3. `errno` macro vs plain global (fixed in `41b87e6d` but only for
   app_define.c — same shape likely in other glue TUs).

**Real fix:** stop composing NetX BSD addon includes into TUs that
don't use them. The cmake/platform/nano-ros-threadx.cmake module
currently treats all threadx_glue TUs uniformly. It should
partition: NetX BSD socket consumers (`net.c`) get the addon;
kernel-only consumers (`app_define.c`, `startup.c`) don't.

**Whack-a-mole fix:** `#undef errno`, `#define _SYS_TYPES_H_`
guards, etc. per-TU. Already started in `41b87e6d` but rabbit-hole.

**Phase 140 supersedes:** ripping install-local removes the failing
recipe class. `add_subdirectory`-consumed users (post-Phase 144)
build a single app with `nros_platform_link_app(my_app)` and only
get the headers their app uses. The legacy install path needs
every staticlib to be link-complete; the source-distribution path
doesn't.

---

## Open — other install recipes not yet exercised post-144

CI v8 stopped at threadx_riscv64. Untested post-Phase-144:

- `just threadx_linux install` — likely same `errno` / `suseconds_t`
  class as riscv64 (different `app_define.c` per board but same
  NetX-BSD composition pattern).
- `just freertos install` — passes NANO_ROS_BOARD now; not yet
  reached due to upstream failure.
- `just cyclonedds build-rmw` — exercised early in install-local-posix
  loop and passed (cyclonedds CMake project doesn't go through
  Phase 138 platform modules).

Expected: same kind of include-composition collision per board, OR
they pass because their board overlays don't add NetX BSD addon
to non-network glue TUs.

---

## Notes

- **Why P4.** Every issue here exists because the legacy install
  path needs to produce complete staticlibs from a CMake configure
  that doesn't know which TUs the consumer will actually link. The
  add_subdirectory model (Phase 137/138) only builds what
  `nros_platform_link_app(target)` actually needs for the user's
  target. Phase 140 eliminates the failing surface entirely.
- **Phase 144 is structurally complete.** `just check` green; all
  83 example CMakeLists migrated; platform / board modules in
  place; native posix examples build end-to-end. The above issues
  do NOT block merging Phase 144 to main.
- **If Phase 140 slips beyond two months,** this triage doc becomes
  the work plan for keeping install-local alive. Until then it's
  reference material — DO NOT iterate on the open issues.
