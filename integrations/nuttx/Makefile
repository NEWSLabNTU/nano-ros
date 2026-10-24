############################################################################
# Phase 139.4 / 152.7 — NuttX integration shell (Makefile).
#
# NuttX app Makefile. Invokes `cargo build` in the NuttX `context::`
# phase via `apps/tools/Rust.mk` macros, then leaves the link to
# `Application.mk` (the staticlib path is appended to `EXTRA_LIBS`
# by `Make.defs`).
############################################################################

include $(APPDIR)/Make.defs

############################################################################
# issue 0488 residue 4 — build OUT OF the nano-ros source tree.
#
# `Application.mk` names objects `$(PREFIX)<src>$(SUFFIX)$(OBJEXT)` and stamps
# `$(PREFIX).built` / `$(PREFIX)Make.dep` / `$(PREFIX).depend`. With the default
# empty PREFIX those land relative to `$(CWD)` — and `$(CWD)` is `$(CURDIR)`,
# which make resolves through the staging SYMLINK to the PHYSICAL directory,
# i.e. `<nano-ros>/integrations/nuttx`. `CSRCS` below also names an absolute
# path into `packages/platform/`. So a user running `make` in THEIR NuttX tree
# wrote object files into the nano-ros checkout — a hard failure when nano-ros
# is a read-only SDK install, and cross-user pollution when it is shared.
#
# `$(APPDIR)` is the user's own apps tree (NuttX exports it), so it is the only
# root here that belongs to the person running the build. `$(CURDIR)` is not,
# for the symlink reason above.
#
# The ARCH is in the coordinate deliberately. One apps tree serves both
# `qemu-armv7a` and `rv-virt` (issue 0525), `SUFFIX` is `$(CWD)` dotted and so
# identical for both, and the kernel `distclean` does not touch the apps tree —
# so a shared object dir would let one arch's objects reach the other's
# `libapps.a`. That is issue 0511's failure class, which is what this whole
# cluster is about.
#
# Upstream requires the directory to exist and PREFIX to end with the delimiter
# ("Apps compilation can achieve out-of-tree intermediate products by specifying
# PREFIX to a directory in its own Makefile" — Application.mk).
############################################################################

NROS_APPS_ARCH  := $(patsubst "%",%,$(CONFIG_ARCH))
NROS_APPS_BUILD := $(APPDIR)$(DELIM)external$(DELIM).nros-build$(DELIM)$(NROS_APPS_ARCH)
PREFIX          := $(NROS_APPS_BUILD)$(DELIM)

# Issue 0551 — `$(shell …)` at the top level runs on EVERY PARSE of this file,
# and not every parse has a configured tree. `make olddefconfig` runs
# `apps_preconfig`, which walks the apps tree before `DELIM` and `CONFIG_ARCH`
# are defined; both then expand EMPTY, `NROS_APPS_BUILD` collapses to
# `$(APPDIR)external.nros-build`, and the mkdir creates a junk directory
# alongside the apps tree (observed as `third-party/nuttx/nuttx-appsexternal.nros-build/`).
# An empty ARCH is also the shared-object-dir hazard this coordinate exists to
# prevent, so building into it would be worse than not creating it.
NROS_APPS_BUILD_OK := $(if $(and $(strip $(DELIM)),$(strip $(NROS_APPS_ARCH))),1,)
ifeq ($(NROS_APPS_BUILD_OK),1)
$(shell mkdir -p $(NROS_APPS_BUILD))
endif
# `CSRCS` names an ABSOLUTE path, so `$(PREFIX)<src>` mirrors that whole path
# under the build root and upstream's compile rule does NOT create the parent
# ("error: can't create …/.nros-build/arm//mnt/…/platform.c….o"). The two
# upstream users of PREFIX (audioutils/lame, videoutils/x264) never hit this:
# their sources are RELATIVE to the app dir, so the mirrored path is shallow and
# already exists.

# Upstream's `clean::` deletes BARE `.built` / `Make.dep` / `.depend`, so it
# does not see the prefixed ones. `::` rules accumulate; this removes what we
# actually wrote.
clean::
	$(call DELDIR, $(NROS_APPS_BUILD))

# Cargo features assembled from Kconfig knobs. The Kconfig sibling
# exposes RMW + ROS-edition as `choice` blocks so `make menuconfig`
# drives Cargo via these vars rather than free-form strings.
#
# issue 0947 — derived, so a new edition needs no edit here. The Kconfig choice
# guarantees the value is one of the three; `patsubst "%",%` strips the quotes
# Kconfig puts around a string symbol, the same way NROS_APPS_ARCH does above.
NROS_ROS_EDITION := $(patsubst "%",%,$(CONFIG_NROS_ROS_EDITION))
ifeq ($(NROS_ROS_EDITION),)
# Not a default. The Kconfig choice always yields a value when
# integrations/nuttx/Kconfig is sourced, so an empty one means it is NOT — and
# quietly picking humble there is the RFC-0056 wire mismatch: it builds clean
# and fails on the wire. Say so instead.
$(error CONFIG_NROS_ROS_EDITION is unset — is integrations/nuttx/Kconfig sourced?)
endif

# RFC-0071 D9 (phase-506 W4c) — the API crates name no backend. The image's
# staticlib is a generated IMAGE CRATE (`cmake/NanoRosImageCrate.cmake`, the
# generator every CMake road uses, run here in script mode) that depends on
# the API crate with `rmw-cffi` and on the backend crate the RMW's descriptor
# names, anchoring its `register`. So this file names no backend either: the
# RMW is the Kconfig choice that is set, `CONFIG_NROS_RMW_<NAME>=y` -> `<name>`.
NROS_RMW := $(shell echo $(patsubst CONFIG_NROS_RMW_%,%,$(foreach v,$(filter CONFIG_NROS_RMW_%,$(.VARIABLES)),$(if $(filter y,$($(v))),$(v)))) | tr A-Z a-z)

# phase-359 W10 stage C took `std` off nros-c / nros-cpp, and this line kept
# asking for it, so every build here stopped at `nros-c-image does not contain
# this feature: std` (found by issue 1824's D2 build — no lane runs this file).
# The NuttX port SUPPLIES the heap (`platform-nuttx` -> `global-allocator`) and
# this archive is the image's runtime, so it owns the panic handler: the same
# set the board's FFI crate builds (`nros-nuttx-ffi/Cargo.toml`).
NROS_API_FEATURES := alloc;panic-platform;ros-$(NROS_ROS_EDITION);platform-nuttx;rmw-cffi

# The C and C++ images are members of ONE generated root, so one target dir
# serves one root (issue 0616); `Make.defs` locates the archives under it.
# `$(shell …)` runs at parse time, so it is guarded like the mkdir above
# (issue 0551). Prints `<manifest> <feature,feature,…>`.
nros_image_crate = $(shell cmake --log-level=WARNING \
	-DNROS_IMAGE_FLAVOR=$(1) -DNROS_IMAGE_RMW=$(NROS_RMW) \
	-DNROS_IMAGE_EDITION=$(NROS_ROS_EDITION) \
	-DNROS_IMAGE_OUT_DIR=$(NROS_IMAGE_ROOT)/nros-$(1) \
	-DNROS_IMAGE_WORKSPACE_DIR=$(NROS_IMAGE_ROOT) \
	"-DNROS_IMAGE_API_FEATURES=$(NROS_API_FEATURES)" \
	-P $(NANO_ROS_ROOT_DEFS)/cmake/NanoRosImageCrate.cmake)
# ONE image per configuration: the one that LINKS. A C++ image links
# `libnros_cpp.a` alone, which bundles nros-c (issue 1636, `Make.defs`), so a C
# image built beside it reached nothing. Building both was also what broke the
# C++ configuration outright: each `nros_image_crate` call runs in its OWN cmake
# process and writes the shared root's `members` from that process alone, so
# the second call left the first crate outside its root and cargo refused it
# ("current package believes it's in a workspace when it's not").
# $(NROS_IMAGE_HEADER_PACKAGES) are the crates whose build scripts write the
# headers the image's TUs include: the C++ image's C header comes from nros-c's
# build script, not nros-cpp's (issue 1824).
ifneq ($(CONFIG_NROS_CPP_API),)
NROS_IMAGE_FLAVOR := cpp
NROS_IMAGE_HEADER_PACKAGES := nros-cpp nros-c
else
NROS_IMAGE_FLAVOR := c
NROS_IMAGE_HEADER_PACKAGES := nros-c
endif
ifeq ($(NROS_APPS_BUILD_OK),1)
NROS_IMAGE := $(call nros_image_crate,$(NROS_IMAGE_FLAVOR))
endif

# Pass the feature set + NuttX include dir to Cargo via env.
# `apps/tools/Rust.mk` reads CARGO_BUILD_ARGS to inject extras.
export NUTTX_INCLUDE_DIR := $(TOPDIR)/include:$(TOPDIR)/include/arch

# Phase 157.C — nano-ros's repo-root `rust-toolchain.toml` pins
# stable. The NuttX `-Zbuild-std=std,panic_abort` flags need
# nightly. Pin via env var so the Cargo invocation from
# `RUST_CARGO_BUILD` picks the same nightly the rest of the
# NuttX examples use (`examples/qemu-armv7a-nuttx/
# rust-toolchain.toml`). Override via env before `make` if you
# need a different nightly.
export RUSTUP_TOOLCHAIN ?= nightly-2026-04-11

# issue 0488 follow-up — this definition sits ABOVE its first use on
# purpose. It used to come 9 lines after the `CFLAGS +=` below, and NuttX's
# CFLAGS is simply-expanded, so `+=` expanded `$(NROS_REPO_DIR)` while it was
# still EMPTY: the include became `-I/packages/platform/...` and every build
# of this app failed with `nros/platform.h: No such file or directory`.
# `CSRCS` is recursively expanded, which is why the SOURCE path resolved and
# only the header went missing.
# Phase 157.C — resolve the real nano-ros repo root via the
# `apps/external/nano-ros` symlink (which points at
# `integrations/nuttx/`; `../..` from there lands on the repo
# root). `realpath` follows the symlink so the cargo manifest path
# is absolute + correct. Plain `$(APPDIR)/external/nano-ros/...`
# would resolve through the symlink target literally + miss the
# packages/api/ dir.
NROS_REPO_DIR := $(realpath $(APPDIR)/external/nano-ros/../..)

# Phase 157.C.17 + .C.18 — full `nros_platform_*` ABI from the
# POSIX C-port. NuttX provides POSIX threading + clocks +
# semaphores + sockets via its libc, so reuse the canonical C-port
# implementation. Compiles cleanly under arm-none-eabi-gcc against
# NuttX's POSIX headers + supplies every symbol nano-ros expects:
#   * `nros_platform_mutex_*` / `_task_*` / `_time_now_ms` /
#     `_clock_*` / `_alloc` etc. — used by zenoh-pico's
#     `platform_aliases.c` shim (Phase 157.C.17).
#   * `nros_platform_wake_*` — real `sem_t`-backed impl (POSIX
#     `nros_wake_t` wraps a `sem_t`). Supersedes the Phase
#     157.C.15 stubs (which returned `storage_size=0` to disable
#     the executor's wake-cb path). Phase 158.x — was carved out
#     as a follow-up but C.17's POSIX C-port reuse incidentally
#     provided the real impl, since NuttX's `sem_t` honors
#     `sem_timedwait` correctly.
# Obviates a dedicated `nros-platform-nuttx` Rust crate.
CSRCS = $(NROS_REPO_DIR)/packages/platform/nros-platform-posix/src/platform.c
ifeq ($(NROS_APPS_BUILD_OK),1)
$(shell mkdir -p $(dir $(addprefix $(NROS_APPS_BUILD),$(CSRCS))))
endif
# Include the platform.h header in CFLAGS so the POSIX port's
# `#include <nros/platform.h>` resolves.
CFLAGS += ${INCDIR_PREFIX}$(NROS_REPO_DIR)/packages/platform/nros-platform-api/include


# Phase 157.C — Rust target triple resolution.
#
# Upstream `apps/tools/Rust.mk`'s `RUST_TARGET_TRIPLE` macro
# enumerates thumb*, riscv32, riscv64, x86, x86_64 but is MISSING
# the `armv7a` (non-thumb ARM-A) branch — `qemu-armv7a/nsh`
# (the canonical QEMU NuttX board) configures CONFIG_ARM_THUMB=n
# and ends up with `LLVM_ARCHTYPE := armv7a`, which falls through
# every `$(filter ...)` clause and yields an empty triple. Cargo
# then errors with `--target takes a target architecture as an
# argument`.
#
# Fix: synthesize the triple ourselves when upstream returns
# empty. Today we only need to cover the `armv7a` case; richer
# fallbacks land alongside additional NuttX QEMU board configs.
NROS_TARGET_TRIPLE := $(strip $(call RUST_TARGET_TRIPLE))
ifeq ($(NROS_TARGET_TRIPLE),)
  ifeq ($(LLVM_ARCHTYPE),armv7a)
    NROS_TARGET_TRIPLE := armv7a-nuttx-$(LLVM_ABITYPE)
  endif
endif

# Mirror upstream `RUST_CARGO_BUILD` but force --target to the
# resolved triple so the empty-expansion bug above can't bite.
# Includes the same `-Zbuild-std`/`-Zbuild-std-features` flags
# upstream uses for `CONFIG_DEBUG_NOOPT`-style release builds —
# matches what `examples/qemu-armv7a-nuttx/rust/zenoh/*/.cargo/
# config.toml` declares.
# Phase 157.C — `--config` overrides for the NuttX Rust build.
# nano-ros's root workspace doesn't carry a `[patch.crates-io]
# libc = ...` entry (it would affect every build, not just the
# NuttX cross). Inject the same libc patch + per-target rustflags
# the standalone examples carry in their `.cargo/config.toml`
# (e.g. `examples/qemu-armv7a-nuttx/rust/zenoh/talker/.cargo/
# config.toml`) via cargo's `--config <inline-toml>` flag so the
# integration shell builds match those examples exactly.
#
# CPU / FPU flags are armv7a-nuttx-eabihf-specific (cortex-a7 +
# hard-float ABI + neon-vfpv4). Extend the conditional in the
# `context::` rule when adding more boards.
# Use literal `"` inside the dotted-key TOML values; cargo's
# `--config` parser expects a TOML expression. The single-quoted
# shell arg in the recipe below preserves the `"` chars verbatim.
NROS_LIBC_PATCH := patch.crates-io.libc.path="$(NROS_REPO_DIR)/third-party/nuttx/libc"
NROS_RUSTFLAGS_ARMV7A := target.armv7a-nuttx-eabihf.rustflags=["-C","link-arg=-mcpu=cortex-a7","-C","link-arg=-mfloat-abi=hard","-C","link-arg=-mfpu=neon-vfpv4"]
NROS_CFLAGS_ARMV7A := env.CFLAGS_armv7a_nuttx_eabihf.value="-mcpu=cortex-a7 -mfloat-abi=hard -mfpu=neon-vfpv4"

# $(1) = the image crate's manifest, $(2) = its features (comma-separated) —
# the two words `nros_image_crate` printed; $(3) = the image flavor (`c` /
# `cpp`); $(4) = $(NROS_IMAGE_HEADER_PACKAGES).
#
# Issue 1824 — cargo runs under the header placer, which reads the build's
# JSON stream and copies each package's headers out of ITS `$OUT_DIR` into
# the image's placed dir (`Make.defs`). One placer for every road (the Zephyr
# and cmake lanes run the same script).
define NROS_CARGO_BUILD
	NUTTX_INCLUDE_DIR=$(TOPDIR)/include:$(TOPDIR)/include/arch \
	CC_armv7a_nuttx_eabihf=arm-none-eabi-gcc \
	CC_armv7a_nuttx_eabi=arm-none-eabi-gcc \
	CXX_armv7a_nuttx_eabihf=arm-none-eabi-g++ \
	CXX_armv7a_nuttx_eabi=arm-none-eabi-g++ \
	AR_armv7a_nuttx_eabihf=arm-none-eabi-ar \
	AR_armv7a_nuttx_eabi=arm-none-eabi-ar \
    python3 $(NROS_REPO_DIR)/scripts/build/cargo-out-dir-headers.py \
		$(foreach p,$(4),--package $(p)) \
		--dest $(call NROS_PLACED_DIR,$(3)) -- \
    cargo build --release \
		--message-format=json-render-diagnostics \
		-Zbuild-std=std,panic_abort \
		--no-default-features \
		--features "$(strip $(2))" \
		--config '$(NROS_LIBC_PATCH)' \
		--config '$(NROS_RUSTFLAGS_ARMV7A)' \
		--config '$(NROS_CFLAGS_ARMV7A)' \
		--manifest-path $(1) \
		--target-dir $(NROS_IMAGE_TARGET_DIR) \
		--target $(NROS_TARGET_TRIPLE)
endef

define NROS_CARGO_CLEAN
	cargo clean --manifest-path $(1) --target-dir $(NROS_IMAGE_TARGET_DIR)
endef

ifneq ($(wildcard $(APPDIR)/tools/Rust.mk),)
# NuttX-blessed Rust path — Cargo runs in `context::` for every
# crate listed. `clean::` mirrors. Phase 157.C overrides to the
# armv7a-aware NROS_CARGO_BUILD when LLVM_ARCHTYPE=armv7a falls
# through upstream's macro.
context::
ifeq ($(NROS_TARGET_TRIPLE),)
	$(call RUST_CARGO_BUILD,nros-$(NROS_IMAGE_FLAVOR),$(NROS_REPO_DIR)/packages/api)
else
	$(call NROS_CARGO_BUILD,$(word 1,$(NROS_IMAGE)),$(word 2,$(NROS_IMAGE)),$(NROS_IMAGE_FLAVOR),$(NROS_IMAGE_HEADER_PACKAGES))
endif

clean::
	$(call NROS_CARGO_CLEAN,$(word 1,$(NROS_IMAGE)))
endif

include $(APPDIR)/Application.mk
