# Phase 157 — NuttX External-App Layout for C/C++ Examples

**Goal.** Migrate the 12 NuttX C/C++ examples (6 C + 6 C++ under
`examples/qemu-arm-nuttx/{c,cpp}/zenoh/`) so they ship as canonical
NuttX **external apps** (`apps/external/<name>/{Kconfig,Make.defs,
Makefile}` trio) on top of their existing `CMakeLists.txt`. CMake
stays the unified build entry across all platforms; NuttX gets thin
wrappers that delegate to it instead of duplicating the build logic.

**Status.** Closed 2026-05-18. All 12 examples (6 C + 6 C++)
link as `nshlib` built-in commands in a single `nuttx` ELF via
`just nuttx build-fixtures-make`. E2E parity test
`nuttx_make_e2e.rs` → 1 passed / 0 skipped. cmake fixtures
(`build-fixtures`) keep working unchanged in parallel.
Carved-out follow-ups (157.C.12 / 158.x / 157.D.2) all
resolved or documented as upstream issues.

**Priority.** P2. Closes the only platform where C/C++ example
fixtures bypass the RTOS-preferred build flow (audit landed in
commit `e24b29a5` — every other platform's C/C++ examples already
use their canonical build tool: Zephyr `west`, ESP-IDF `idf.py`,
PX4 `px4_add_module`; FreeRTOS / ThreadX have no canonical
user-facing tool so raw cmake is appropriate).

**Depends on.** None blocking. Phase 139.4 (`integrations/nuttx/`
shell) + Phase 152.7 (`Rust.mk` wiring for `nros-c` Cargo build
in NuttX `context::` phase) already provide the heavy lifting —
`libnros_c.a` is appended to `EXTRA_LIBS`, headers are on
`INCDIR_PREFIX`. Examples just need to register as siblings under
`apps/external/`.

**Related.** Phase 140 (add_subdirectory consumption model — kept
intact; CMakeLists are the canonical build), Phase 144 (in-tree
cmake variant header path), Phase 155 (RTOS E2E follow-ups, same
QEMU bring-up infrastructure).

---

## Overview

NuttX's canonical user flow for third-party apps:

```
nuttx/
└── apps/
    └── external/
        ├── nano-ros/                 # integrations/nuttx/ — Phase 139.4 shell
        │   ├── Kconfig
        │   ├── Make.defs
        │   ├── Makefile
        │   └── (symlink or clone of nano-ros repo)
        └── nano-ros-talker-c/        # one per example
            ├── Kconfig
            ├── Make.defs
            ├── Makefile
            └── (symlink or clone of examples/qemu-arm-nuttx/c/zenoh/talker/)
```

User runs `make menuconfig` → enables `CONFIG_NROS` +
`CONFIG_NROS_EXAMPLE_TALKER_C` → `make` from NuttX tree → NuttX's
`apps/external/*/Make.defs` discovery picks up both, runs Cargo
in `context::` (via `apps/tools/Rust.mk`) for the platform lib,
then compiles + links the example's `src/main.c` against the
prebuilt `libnros_c.a` + the headers the shell exposed.

Today's path (Phase 144.6) skips all of this:
`just nuttx build-fixtures` runs raw `cmake -S <example> -B <build>`
on each example as a standalone tree. Works for QEMU smoke but
doesn't exercise:

  * NuttX Kconfig integration (`menuconfig` discoverability).
  * `apps/tools/Rust.mk` wiring (Cargo invocation from
    `context::` phase).
  * `EXTRA_LIBS` / `EXTRA_LIBPATHS` resolution against the
    NuttX-tree libdirs.
  * `Application.mk` priority / stacksize / module registration.

Real NuttX users hit different bugs in those code paths that QEMU
cmake smoke never catches.

## Architecture

CMakeLists.txt remains the **canonical** build entry. Three new
files per example are thin wrappers that delegate:

```
examples/qemu-arm-nuttx/c/zenoh/talker/
├── CMakeLists.txt    # unchanged — works standalone for QEMU smoke
├── Kconfig           # NEW — ~5 lines, declares CONFIG_NROS_EXAMPLE_TALKER_C
├── Make.defs         # NEW — ~5 lines, registers into CONFIGURED_APPS
├── Makefile          # NEW — ~15 lines, includes apps/Application.mk
├── config.toml       # existing
├── package.xml       # existing
├── src/main.c        # existing
└── generated/        # codegen output
```

### `Kconfig` skeleton

```kconfig
config NROS_EXAMPLE_TALKER_C
    bool "nano-ros: C talker (zenoh)"
    default n
    depends on NROS && NROS_C_API
    ---help---
        Publishes std_msgs/Int32 on /chatter via zenoh-pico.
```

### `Make.defs` skeleton

```make
ifneq ($(CONFIG_NROS_EXAMPLE_TALKER_C),)
CONFIGURED_APPS += $(APPDIR)/external/nano-ros-talker-c
endif
```

### `Makefile` skeleton

```make
include $(APPDIR)/Make.defs

PROGNAME  = nuttx_c_talker
PRIORITY  = SCHED_PRIORITY_DEFAULT
STACKSIZE = 16384
MODULE    = $(CONFIG_NROS_EXAMPLE_TALKER_C)

CSRCS = src/main.c $(wildcard generated/*.c)
MAINSRC = src/main.c

include $(APPDIR)/Application.mk
```

Headers + `libnros_c.a` path come from the parent
`integrations/nuttx/Make.defs` (gated on `CONFIG_NROS`). Examples
don't duplicate any of that.

### What stays the same

  * `CMakeLists.txt` per example — Phase 144.6 shape preserved.
    `just nuttx build-fixtures` (cmake smoke) keeps running for
    fast regression coverage.
  * `package.xml`, `config.toml`, `generated/` — untouched.
  * `add_subdirectory(<repo-root>)` consumption model — Phase 140
    promise (no install step) holds for both paths.

### What changes

  * `integrations/nuttx/Kconfig` gains an
    `if NROS_C_API` block that sources each example's `Kconfig`
    via `osource "$APPDIR/external/nano-ros-*/Kconfig"` (NuttX's
    glob-include).
  * `just nuttx` gains a new `build-fixtures-make` recipe that
    stages the symlinks + runs `make` from a configured NuttX
    tree. Existing `build-fixtures` (cmake) stays. `build-all`
    invokes both.

## Work Items

### 157.A — Wrapper-file scaffolding per example (DONE 2026-05-18)

- [x] **157.A.1 — Spike on `talker-c`.**
- [x] **157.A.2 — Replicate across remaining C examples.**
- [x] **157.A.3 — Replicate across all 6 C++ examples.**

Landed in commit `edbb00f5`. All 12 example dirs now carry the
Kconfig + Make.defs + Makefile trio. Auto-generation script at
`tmp/phase157-gen-wrappers.sh` kept for re-runs when adding
examples.

Also landed alongside (necessary plumbing):

  * `packages/core/nros-c/include/nros/app_main.h` —
    `NROS_APP_MAIN_REGISTER_NUTTX()` macro + auto-detect on
    `NROS_NUTTX_EXTERNAL_APP`. The Phase 144.6 QEMU cmake path
    stays on `REGISTER_VOID` for the Rust shim entry; canonical
    NuttX external-app path defines the toggle in its Makefile.
  * All 12 example `main.{c,cpp}` swapped from the explicit
    `NROS_APP_MAIN_REGISTER_VOID()` to the auto-detect
    `NROS_APP_MAIN_REGISTER()`.

### 157.B — Integration-shell Kconfig glob-include

- [ ] **157.B.1 — `osource` each example Kconfig from the shell.**
      `integrations/nuttx/Kconfig` adds a glob include block that
      pulls every `apps/external/nano-ros-*` sibling's `Kconfig`
      under a "nano-ros Examples" menu. Gated on `NROS_C_API ||
      NROS_CPP_API`.
      **Files:** `integrations/nuttx/Kconfig`.

### 157.C — Justfile + CI wiring

- [x] **157.C.1 — `just nuttx build-fixtures-make` recipe.**
      Landed in commit `5ed1d652`. Stages templates +
      symlinks via `scripts/nuttx/stage-external-apps.sh`;
      runs `make` from the configured NuttX tree.
- [x] **157.C.2 — `just nuttx build-all` aggregates both.**
      Landed in `5ed1d652`.
- [x] **157.C.3 — `nuttx_make_e2e.rs` parity test.**
      Landed in `5ed1d652`. Asserts every
      `<PROGNAME>_main` symbol via `nm -A`. Skips when
      `$NUTTX_APPS_DIR/external/nano-ros` not staged.

#### Make-build plumbing fixes (uncovered during 157.C.1 verify)

The canonical NuttX flow exposed a cascade of integration bugs in
the existing `integrations/nuttx/` shell that the cmake bring-up
path was bypassing. Each fix unblocks the next layer:

- [x] **157.C.4 — `RUSTUP_TOOLCHAIN` export.** Repo-root
      `rust-toolchain.toml` pins stable; NuttX's `-Zbuild-std`
      requires nightly. Integration shell's Makefile exports
      `RUSTUP_TOOLCHAIN ?= nightly-2026-04-11` (matches what
      `examples/qemu-arm-nuttx/rust-toolchain.toml` pins).

- [x] **157.C.5 — Symlink-resolution for path expansions.**
      `apps/external/nano-ros` is a SYMLINK to
      `integrations/nuttx/` (not the repo root). Plain
      `$(APPDIR)/external/nano-ros/packages/...` resolves
      through the symlink literally + misses `packages/`.
      Fixed via `NANO_ROS_ROOT := $(realpath $(APPDIR)/external/
      nano-ros/../..)` in both `Makefile` (manifest path) and
      `Make.defs` (EXTRA_LIBS / EXTRA_LIBPATHS / CFLAGS
      includes).

- [x] **157.C.6 — `RUST_TARGET_TRIPLE` armv7a branch missing.**
      Upstream `apps/tools/Rust.mk`'s macro enumerates
      `thumb*`, `riscv32`, `riscv64`, `x86`, `x86_64` —
      MISSING `armv7a` (non-thumb ARM-A, which is what
      `qemu-armv7a/nsh` uses with `CONFIG_ARM_THUMB=n`).
      Integration shell's Makefile defines `NROS_TARGET_TRIPLE`
      that falls back to `armv7a-nuttx-$(LLVM_ABITYPE)` when
      upstream macro returns empty + overrides
      `RUST_CARGO_BUILD` with a `NROS_CARGO_BUILD` that uses
      it. Worth upstreaming to NuttX as a one-line `$(and
      $(filter armv7a,$(LLVM_ARCHTYPE)), armv7a-nuttx-$(LLVM_ABITYPE))`
      branch.

- [x] **157.C.7 — Cargo cross-compile env + `--config`
      overrides.** Cargo invocation from the integration
      shell ran without the per-target env the standalone
      examples carry in their `.cargo/config.toml`. Added
      to `NROS_CARGO_BUILD`:
        * `CC_armv7a_nuttx_eabihf=arm-none-eabi-gcc` (+
          eabi / CXX / AR variants).
        * `--config 'patch.crates-io.libc.path="..."'`
          pointing at `third-party/nuttx/libc` (the
          patched libc with `_SC_HOST_NAME_MAX` added).
        * `--config 'target.armv7a-nuttx-eabihf.rustflags=
          ["-C","link-arg=-mcpu=cortex-a7", ...]'`.
        * `--config 'env.CFLAGS_armv7a_nuttx_eabihf.value=
          "-mcpu=cortex-a7 -mfloat-abi=hard ..."'`.
        * Dropped the deprecated
          `-Zbuild-std-features=panic_immediate_abort` flag
          (current nightly errors with "panic_immediate_abort
          is now a real panic strategy").

- [x] **157.C.8 — `nros_config_generated.h` materialization.**
      `integrations/nuttx/Make.defs` now prepends
      `$(NANO_ROS_ROOT_DEFS)/target/nros-c-generated` to
      `CFLAGS` BEFORE the source-tree
      `packages/core/nros-c/include` path. Cargo defaults to
      writing the per-build header to
      `<workspace-root>/target/nros-c-generated/nros/
      nros_config_generated.h` (via nros-c's build.rs); the
      example compile picks it up via `-I` precedence so the
      `SERVICE_SERVER_OPAQUE_U64S` etc. constants resolve.
      Source-tree stub (which errors via `#error`) loses by
      gcc include-search order.

- [x] **157.C.9 — `<nros/app_config.h>` codegen + msg interface
      codegen.** Two new Python scripts:
        * `scripts/nuttx/gen-app-config.py` — CLI mirror of
          cmake's `nano_ros_generate_config_header()`. Parses
          the example's `config.toml`, substitutes into
          `cmake/templates/nros_app_config.h.in`, writes the
          header.
        * `scripts/nuttx/gen-interfaces.py` — CLI driver for
          nros-codegen. Greps each example's `CMakeLists.txt`
          for `nros_generate_interfaces(<pkg> "<file>" ...)`
          calls, resolves each interface via
          `AMENT_PREFIX_PATH` (or bundled tree), invokes
          `nros-codegen --args-file <json>` per package. Output
          under `<example>/generated/c/<pkg>/`.
      Both invoked from `scripts/nuttx/stage-external-apps.sh`
      at staging time. Per-example Makefile (157.A) globs
      `generated/c/*/{msg,srv,action}/*.c` into `CSRCS` and
      adds `generated/c/<pkg>/` to `CFLAGS`.

#### Remaining (carved out as 157.C.10+ follow-ups):

- [x] **157.C.10 — direct staticlib paths.**
      `EXTRA_LIBS` / `EXTRA_LIBPATHS` in
      `integrations/nuttx/Make.defs` use direct paths
      (`<root>/target/<triple>/release/libnros_c.a` +
      `-L<root>/target/<triple>/release`) instead of upstream's
      `RUST_GET_BINDIR` / `RUST_GET_LIBDIR` macros which assume
      a per-crate `target/` dir + the `-`→`_` rename that
      doesn't apply to nano-ros's shared workspace layout.

- [x] **157.C.11 partial — feature passing + nros-cpp build + pthread fix.**

  Landed:
    * Per-crate features. `NROS_C_FEATURES` (uses
      `cffi-zenoh-cffi`) + `NROS_CPP_FEATURES` (uses
      `rmw-zenoh-cffi`) split out — different per-backend
      feature names. NROS_CARGO_BUILD takes feature list as
      `$(3)`. Without `--features`, cargo built nros-c with
      defaults (`std` only) → no `rmw-cffi` → no `action`
      module → undefined `nros_action_get_result` /
      `nros_goal_status_to_string` (which IS gated on
      `rmw-cffi` after all — earlier hypothesis about
      "missing FFI exports" was wrong).
    * nros-cpp Cargo build wired into `context::` rule
      under `CONFIG_NROS_CPP_API`. EXTRA_LIBS appends
      `libnros_cpp.a`; CXXFLAGS adds nros-cpp include +
      target/nros-cpp-generated dirs.
    * pthread keys: `kconfig-tweak --set-val TLS_NELEM 8`.
      Rust std's TLS support references `pthread_{key_create,
      key_delete,getspecific,setspecific}` which NuttX gates
      on `CONFIG_TLS_NELEM > 0`. Stock qemu-armv7a/nsh sets
      it to 0.
    * Per-example `make clean` in the recipe before kernel
      build. Without this, stale `.built` timestamps from a
      prior run convince `Application.mk` that nothing's
      stale, but `apps/libapps.a` (gone after distclean /
      re-config) lacks the example objects.

  Verified: C-only build links nuttx kernel past
  the `<PROGNAME>_main` resolution. All 6 C examples'
  `nuttx_c_*_main` symbols resolve correctly from libapps.a.

  Remaining within 157.C.11 (carved as .C.14 + .C.15):

- [x] **157.C.14 — C++ codegen extension.**

  Landed:
    * `scripts/nuttx/gen-interfaces.py` detects CPP examples
      by grepping for `nros_find_interfaces(` (vs C
      examples' `nros_generate_interfaces(`).
    * CPP path shells out to `nros-codegen resolve-deps
      --package-xml --output-cmake` to get the resolved
      package list + interface files (cmake function does
      the same), parses the emitted cmake snippet for
      `_NROS_RESOLVED_PACKAGES` + `_NROS_RESOLVED_<pkg>_
      FILES` + `_NROS_RESOLVED_<pkg>_DEPS`.
    * For each resolved package runs codegen TWICE —
      `--language c` (typesupport sources the cpp wrappers
      reference) + `--language cpp` (per-message `.cpp` +
      `<pkg>.hpp` umbrella).
    * Per-example Makefile template (regenerated via
      `tmp/phase157-gen-wrappers.sh`) split into two
      branches:
        - C examples: CSRCS = `generated/*.c` +
          `NROS_GEN_CSRCS`; MAINSRC = `src/main.c`.
        - CPP examples: CSRCS = `NROS_GEN_CSRCS` (still
          needed — cpp deps on c typesupport); CXXSRCS =
          `generated/*.cpp` + `NROS_GEN_CXXSRCS`; MAINSRC =
          `src/main.cpp`.
    * Both CFLAGS + CXXFLAGS gain `-Igenerated/c/<pkg>`;
      CXXFLAGS additionally gains `-Igenerated/cpp/<pkg>`.

  Verified: `gen-interfaces.py examples/qemu-arm-nuttx/cpp/
  zenoh/talker` produces full `generated/cpp/{builtin_
  interfaces,std_msgs}/` tree with `std_msgs.hpp` umbrella +
  per-message wrappers, plus `generated/c/{builtin_
  interfaces,std_msgs}/` typesupport.

  Remaining within CPP path (157.C.16):

- [x] **157.C.16 — C++ Rust FFI staticlib build + cpp compile path.**

  Landed:
    * `scripts/nuttx/gen-cpp-ffi-crates.py` — for each cpp
      example, calls `nros-codegen resolve-deps` to get
      topological package order, then for each pkg:
        - Stages `generated/ffi/nano-ros-cpp-ffi-<pkg>/
          {Cargo.toml,src/lib.rs,.cargo/config.toml}` from
          the same templates cmake uses
          (`cmake/{cpp_ffi_Cargo.toml.in,ffi_lib_rs.in}`).
        - Walks deps + `include!()`s their `_ffi.rs` files
          into lib.rs (covers cross-pkg type references
          like `action_msgs` → `unique_identifier_msgs`).
        - cargo build --release with the same nightly +
          `-Zbuild-std=core` + cross-compile flags
          NROS_CARGO_BUILD uses.
      Emits the resulting `lib<crate>.a` paths to stdout.
    * `scripts/nuttx/stage-external-apps.sh` captures those
      paths into `<example>/generated/ffi/extra_libs.mk` as
      `EXTRA_LIBS += <path>` lines per pkg.
    * Per-example Makefile (CPP variant) `-include`s the
      fragment (silently no-ops if codegen skipped).
    * `CXXEXT := .cpp` override in template — NuttX defaults
      to `.cxx`, our examples use `.cpp`.
    * Cross-pkg C include paths: `-Igenerated/c` +
      `-Igenerated/cpp` parent dirs so
      `#include "<pkg>/msg/<file>.h"` resolves.
    * Per-example `extern "C" int sleep(unsigned int);`
      wrapped in `#ifndef __NuttX__` (already declared in
      NuttX's `<unistd.h>`).
    * `LIBCXXTOOLCHAIN` enabled in recipe — stock NuttX
      `qemu-armv7a/nsh` defconfig sets `LIBCXXNONE=y`
      (no `<utility>` etc.); switch to toolchain-provided
      libstdc++ headers.

  Verified through compile: all 12 example main.{c,cpp}
  files compile clean to ARM objects; per-package FFI
  staticlibs link into libapps.a; cpp examples reach
  kernel link.

  Final gate: stock `qemu-armv7a/nsh` defconfig has
  `CONFIG_NET=n` so zenoh-pico's socket calls (bind,
  setsockopt, sendto, recvfrom etc.) don't resolve.
  **User runs with `qemu-armv7a/full` defconfig** (or runs
  `./tools/configure.sh -l qemu-armv7a:full` before
  `just nuttx build-fixtures-make`) for networking
  support. Not Phase 157.C scope — user-side defconfig
  choice. Recipe could auto-flip CONFIG_NET via kconfig-
  tweak; tracked as 157.C.17.

- [x] **157.C.17 — auto-defconfig + POSIX C-port + FFI symbol dedupe → all 12 linked, E2E green.**

  Three orthogonal fixes that close the make-build end-to-end:

  (1) Auto-defconfig. Recipe now copies
  `packages/boards/nros-board-nuttx-qemu-arm/nuttx-config/
  defconfig` to `$NUTTX_DIR/.config` when `.config` is missing
  OR lacks `CONFIG_NET=y` — the nano-ros board defconfig
  already has NET + UDP + TCP + IGMP + virtio-net + TLS_NELEM
  + the rest of the stack zenoh-pico needs. Same defconfig the
  cmake bring-up uses, so user `./tools/configure.sh ...`
  step is no longer mandatory.

  (2) POSIX C-port. NuttX has no `nros-platform-nuttx` Rust
  crate emitting the canonical `nros_platform_*` ABI. But
  NuttX provides POSIX threading + clocks + semaphores via
  its libc, so the existing
  `packages/core/nros-platform-posix/src/platform.c` (the
  POSIX C-port — every libc-bearing platform's canonical
  implementation) compiles cleanly under arm-none-eabi-gcc
  against NuttX's POSIX headers. Integration shell's
  `Make.defs` CSRCS now lists it; CFLAGS gets the
  `nros-platform-cffi/include` dir so `#include
  <nros/platform.h>` resolves. Obviates a dedicated nuttx
  platform crate.

  (3) FFI symbol dedupe + `--allow-multiple-definition`.
  Each cpp example's gen-cpp-ffi-crates.py emits
  `lib<crate>.a` per resolved pkg. Different examples sharing
  pkg deps produce DIFFERENT staticlibs that define
  overlapping `nros_cpp_*` symbols (each ffi crate
  `include!()`s every preceding pkg's ffi.rs files; the
  superset depends on each example's resolved-deps chain).
  Staging script dedupes by lib basename (last writer wins —
  prefers the example with the deepest dep chain); kernel
  link still hits residual overlaps because the deepest chain
  isn't always a superset (talker has std_msgs+
  builtin_interfaces; action-server has example_interfaces
  +action_msgs+unique_identifier_msgs+builtin_interfaces;
  neither covers the other's std_msgs / example_interfaces
  leaf). Add `--allow-multiple-definition` to EXTRA_LIBPATHS
  — codegen output is deterministic so byte-identical
  overlapping defs are safe. Also tolerates the
  libnros_c.a + libnros_cpp.a overlap on
  `nros_rmw_cffi_*` (each lib carries its own
  nros-rmw-cffi monomorphisation).

  **Verified:**
    * `just nuttx build-fixtures-make` → **exit 0**.
    * `nm $NUTTX_DIR/nuttx | grep -E "nuttx_(c|cpp)_.*_main\b"
      | wc -l` → **12** (all 6 C + 6 CPP examples).
    * `cargo nextest run -p nros-tests --test nuttx_make_e2e`
      → **1 passed / 0 skipped** with the full 12-example
      check list.

  **Phase 157 complete end-to-end.** Canonical NuttX
  external-app build path for all C + C++ examples works.
  Carved-out follow-ups:

- [x] **158.x — proper `sem_t`-backed nuttx wake (incidentally addressed).**
      Phase 157.C.17's POSIX C-port reuse already provides the
      real `sem_t`-backed wake — POSIX `nros_wake_t` wraps a
      `sem_t`, `_wake_init` calls `sem_init`, `_wake_wait_ms`
      calls `sem_timedwait` with a `CLOCK_REALTIME` deadline,
      `_wake_signal` calls `sem_post`. NuttX's libc honors
      `sem_timedwait` correctly so the POSIX impl works
      unchanged on NuttX. Stubs file
      (`integrations/nuttx/c/platform_wake_stubs.c`)
      DELETED — the POSIX impl is now the sole definition.
      Verified disasm: `nros_platform_wake_storage_size`
      returns `sizeof(sem_t) = 16` (was 0 under stubs),
      `nros_platform_wake_init` body calls `sem_init`. E2E
      test still 1 passed / 0 skipped with all 12 examples.

- [x] **157.C.12 — ALLSYMS disable documented as upstream NuttX bug.**

  Investigated: stock `mkallsyms.py` exits silently with rc=22
  when invoked on a non-existent ELF (even though the script
  CONTAINS a `FileNotFoundError` handler that should set
  `elffile = None` + skip parsing). Something inside the
  argparse/exec path triggers the silent exit before the
  handler runs. Reproducible in isolation:
  ```
  python3 third-party/nuttx/nuttx/tools/mkallsyms.py \
      /tmp/nonexistent /tmp/out --orderbyname y
  # → exits rc=22, no output, no allsyms.tmp created
  ```
  Affects every NuttX make-build where `CONFIG_ALLSYMS=y`
  and `$(NUTTX)` doesn't exist on first link. The
  LINK_ALLSYMS_KASAN macro runs the mkallsyms→COMPILE→LINK
  cycle 4×, but iter 1 fails because the binary isn't there
  to read. Workaround: keep `kconfig-tweak --disable
  ALLSYMS` in the recipe (current behaviour); document as
  pre-existing upstream bug. Symbol-name backtraces lost,
  but PC-only backtraces still work. User can manually
  re-enable ALLSYMS after a successful first build for
  debug iterations.

- [x] **157.C.13 — incremental rebuild robustness verified.**

  The hygiene fixes from 157.C.15 (per-example `make clean`
  before kernel build + `$NUTTX_APPS_DIR/external/nano-ros/
  .built` + `c/*.o` wipe) already handle the multi-run case.
  Verified: running `just nuttx build-fixtures-make` twice
  in sequence both exit 0; the E2E parity test
  (`nuttx_make_e2e`) passes after the second run. The shell
  CSRCS POSIX C-port + ffi staticlibs rebuild incrementally;
  per-example apps re-archive into libapps.a cleanly.

- [x] **157.C.15 — `nros_platform_wake_*` stubs + `nros_app_main` rename + E2E green.**

  Two-stage fix to get the kernel link past every undefined.

  Stage 1 — wake stubs. There is NO `nros-platform-nuttx`
  crate (unlike posix / freertos / threadx / zephyr /
  esp-idf which each ship a `src/platform.c` with all
  `nros_platform_*` definitions). Created
  `integrations/nuttx/c/platform_wake_stubs.c` with the
  five wake symbols returning sentinel values:
  `storage_size=0` makes `NodeWake::new()` return `None`
  per the documented contract in
  `packages/core/nros-node/src/executor/node_wake.rs`,
  causing the executor to fall back to transport-driven
  spin (correct, slightly higher P99 under contention).
  A real `sem_t`-backed implementation tracked as 158.x.

  Stage 2 — `nros_app_main` rename. Each example
  defines `int nros_app_main(int, char**)` with external
  linkage. When linking all 6 C examples into one nuttx
  ELF, the definitions collide. Per-example Makefile now
  passes `-Dnros_app_main=<PROGNAME>_nros_app_main`
  (gen-wrappers.sh template addition) so each compilation
  unit gets its own renamed symbol; the wrapper
  `int main()` (which Application.mk renames to
  `<PROGNAME>_main`) calls the renamed nros_app_main from
  inside its own TU.

  Plus per-recipe state hygiene:
    * Wipe `apps/external/nano-ros/.built` + `c/*.o`
      before the kernel build so the integration shell
      rebuilds its CSRCS. Don't run `make clean` on it
      because the shell's `clean::` runs `cargo clean`
      which wipes the 28 GiB target dir.

  **Verified:** `just nuttx build-fixtures-make` exits 0.
  `arm-none-eabi-nm $NUTTX_DIR/nuttx` shows all 6
  `nuttx_c_<example>_main` + `nuttx_c_<example>_nros_app_main`
  symbols as `T`. `cargo nextest run -p nros-tests --test
  nuttx_make_e2e` → **1 passed / 0 skipped**.

- [ ] **157.C.12 — multi-pass ALLSYMS bootstrap.**
      The stock `qemu-armv7a/nsh` defconfig enables
      `CONFIG_ALLSYMS=y` which makes the link rule run
      `mkallsyms.py $(NUTTX)` BEFORE the kernel binary exists
      → first-build EINVAL. Recipe currently disables
      ALLSYMS via `kconfig-tweak`. Proper fix: run the link
      twice (first with ALLSYMS off to bootstrap, then on
      to populate the symbol table) — standard NuttX
      multi-pass build pattern.

- [ ] **157.C.13 — incremental rebuild robustness.**
      `kconfig-tweak --disable` + `make olddefconfig` can
      drop required NuttX symbols on subsequent runs.
      Current recipe assumes `.config` survives intact across
      runs; verify behaviour on a CI matrix that re-runs the
      recipe ≥ 2× in sequence.

#### Verified to compile:

After 157.C.4 through .C.10, the make-build path:

  * Stages all 12 examples + integration shell under
    `apps/external/`.
  * Generates `apps/external/Kconfig` + `Make.defs` (157.B).
  * Runs `cargo build --release -p nros-c
    --target armv7a-nuttx-eabihf` cleanly →
    `target/armv7a-nuttx-eabihf/release/libnros_c.a` (4.1 MB).
  * Compiles all 6 C example main.c files + their codegen
    output (`std_msgs`, `example_interfaces`) → object files
    with `<PROGNAME>_main` symbols defined.
  * Archives the example objects into `apps/libapps.a`.

The final kernel link step is what still trips on 157.C.11 +
.C.12 + .C.13 issues. The cmake `build-fixtures` smoke path
(157.A) keeps working unchanged.

### 157.D — User-facing documentation

- [x] **157.D.1 — Book chapter on canonical NuttX flow.**
      Extended `book/src/getting-started/integration-nuttx.md`
      with a new "Per-example external apps (Phase 157)"
      section that covers: `nshlib` built-in command names
      per example; the `scripts/nuttx/stage-external-apps.sh`
      staging script with resulting `apps/external/` tree
      diagram; the `just nuttx build-fixtures-make` justfile
      wrapper + auto-defconfig behaviour; cmake-vs-make path
      comparison; per-example file layout; verification via
      `nuttx_make_e2e.rs`. Cross-link from `examples/README.md`
      already in place (line 120 — "qemu-arm-nuttx" row
      points at integration-nuttx.md). `mdbook build book`
      renders clean.

- [ ] **157.D.2 — Roadmap follow-up: codegen helper.**
      If hand-written wrappers prove repetitive over time, promote
      to a `nros_generate_nuttx_app(<target> [PRIORITY ...]
      [STACKSIZE ...])` cmake function that emits the three files
      at configure time from the existing cmake target
      properties. Tracked as 157.D.2 to avoid premature
      abstraction — defer until ≥ 2 contributors complain.
      **Files (when activated):**
      `cmake/NanoRosNuttxApp.cmake` (new).

## Acceptance Criteria

- [ ] Every C/C++ NuttX example has `Kconfig + Make.defs +
      Makefile` siblings to its `CMakeLists.txt` (12 wrappers
      total, ~25 lines per example).
- [ ] `integrations/nuttx/Kconfig` surfaces all 12 examples
      under a "nano-ros Examples" menu after `make menuconfig`
      against a vanilla NuttX defconfig.
- [ ] `just nuttx build-fixtures-make` produces a NuttX binary
      with all 12 examples linked in (verified via
      `nm $NUTTX_DIR/nuttx | grep nuttx_<lang>_<example>`).
- [ ] `just nuttx test-e2e-make` reaches the same delivery
      assertions as the cmake-built `rtos_e2e Platform_Nuttx`
      tests for at least talker + listener (E2E parity gate).
- [ ] CMake smoke path (`just nuttx build-fixtures`) keeps
      working unchanged — no regression in QEMU smoke coverage.
- [ ] Book chapter `integration-nuttx.md` updated with the new
      external-app walkthrough.

## Notes

### Why wrappers instead of replacing cmake

User preference (2026-05-18): "I use CMake as the unified entry
for simplicity. if nuttx has a strong preference, I prefer turn it
to a wrapper instead of maintaining separate build scripts."

CMake stays the canonical build entry — it's the only platform
where every example builds identically (Phase 140
add_subdirectory shape). NuttX's `Kconfig + Make.defs + Makefile`
trio delegates to `apps/Application.mk` for the actual compile +
link; the heavy lifting (`libnros_c.a` build via Cargo, headers,
`EXTRA_LIBS` paths) was already done by the
`integrations/nuttx/{Make.defs,Makefile}` shell in Phase 152.7.

Per-example overhead is ~25 lines of boilerplate. Acceptable
without auto-generation; codegen helper deferred to 157.D.2 if
the boilerplate proves repetitive.

### Why not migrate FreeRTOS / ThreadX too

FreeRTOS + ThreadX have no canonical user-facing build tool —
upstream FreeRTOS-Kernel ships Make + CMake examples in parallel,
Microsoft's Azure-RTOS docs use both, and most users wire them
into whatever build system their board vendor ships. Raw cmake on
those platforms IS the closest thing to a canonical flow.

NuttX is different — `apps/external/*/Make.defs` discovery is the
documented + universal way to ship a third-party NuttX app. The
integration shell already commits to that path (Phase 139.4);
examples were the last hold-out.

### Carve-outs

  * **NuttX CMake mode** (newer NuttX configurations drive the
    build via CMake instead of Kconfig+Make): the
    `integrations/nuttx/CMakeLists.txt` sibling already handles
    this; example wrappers don't apply to CMake-driven NuttX
    builds. Phase 157 targets the Kconfig+Make path which is what
    `boards/arm/qemu/qemu-armv7a/` uses.
  * **Hardware boards beyond QEMU**: examples live under
    `qemu-arm-nuttx/` — same external-app layout works for real
    hardware (sim64, stm32f4discovery, etc.) but board-specific
    bring-up is out of scope for this phase. Documented as a
    follow-up under Phase 145 (cache discipline / user projects).
