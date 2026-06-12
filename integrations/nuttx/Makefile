############################################################################
# Phase 139.4 / 152.7 — NuttX integration shell (Makefile).
#
# NuttX app Makefile. Invokes `cargo build` in the NuttX `context::`
# phase via `apps/tools/Rust.mk` macros, then leaves the link to
# `Application.mk` (the staticlib path is appended to `EXTRA_LIBS`
# by `Make.defs`).
############################################################################

include $(APPDIR)/Make.defs

# Cargo features assembled from Kconfig knobs. The Kconfig sibling
# exposes RMW + ROS-edition as `choice` blocks so `make menuconfig`
# drives Cargo via this var rather than free-form strings.
#
# Phase 157.C.11 — feature names mirror what
# `packages/core/nros-c/CMakeLists.txt` passes to Corrosion. nros-c
# carries `cffi-zenoh-cffi` / `cffi-dds-cffi` / `cffi-xrce-c` as
# Cargo features that select the RMW + flip on `rmw-cffi`. Kconfig
# RMW choice maps onto those names.
# nros-c features. Names mirror what `packages/core/nros-c/CMakeLists.txt`
# passes to Corrosion. nros-c uses `cffi-zenoh-cffi` etc.
NROS_C_FEATURES := std ros-humble platform-nuttx
ifneq ($(CONFIG_NROS_RMW_ZENOH),)
NROS_C_FEATURES += cffi-zenoh-cffi
endif
ifneq ($(CONFIG_NROS_RMW_XRCE),)
NROS_C_FEATURES += cffi-xrce-c
endif
ifneq ($(CONFIG_NROS_RMW_CYCLONEDDS),)
NROS_C_FEATURES += rmw-cffi
endif
ifneq ($(CONFIG_NROS_ROS_IRON),)
NROS_C_FEATURES := $(filter-out ros-humble,$(NROS_C_FEATURES)) ros-iron
endif

# nros-cpp features. Different per-backend feature names from nros-c
# (rmw-{zenoh,xrce,cyclonedds}-cffi instead of cffi-{...}). Mirrors
# `packages/core/nros-cpp/CMakeLists.txt`.
NROS_CPP_FEATURES := std ros-humble platform-nuttx
ifneq ($(CONFIG_NROS_RMW_ZENOH),)
NROS_CPP_FEATURES += rmw-zenoh-cffi
endif
ifneq ($(CONFIG_NROS_RMW_XRCE),)
NROS_CPP_FEATURES += rmw-xrce-cffi
endif
ifneq ($(CONFIG_NROS_RMW_CYCLONEDDS),)
NROS_CPP_FEATURES += rmw-cffi
endif
ifneq ($(CONFIG_NROS_ROS_IRON),)
NROS_CPP_FEATURES := $(filter-out ros-humble,$(NROS_CPP_FEATURES)) ros-iron
endif

# Pass the feature set + NuttX include dir to Cargo via env.
# `apps/tools/Rust.mk` reads CARGO_BUILD_ARGS to inject extras.
export NUTTX_INCLUDE_DIR := $(TOPDIR)/include:$(TOPDIR)/include/arch

# Phase 157.C — nano-ros's repo-root `rust-toolchain.toml` pins
# stable. The NuttX `-Zbuild-std=std,panic_abort` flags need
# nightly. Pin via env var so the Cargo invocation from
# `RUST_CARGO_BUILD` picks the same nightly the rest of the
# NuttX examples use (`examples/qemu-arm-nuttx/
# rust-toolchain.toml`). Override via env before `make` if you
# need a different nightly.
export RUSTUP_TOOLCHAIN ?= nightly-2026-04-11

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
CSRCS = $(NANO_ROS_ROOT)/packages/core/nros-platform-posix/src/platform.c
# Include the platform.h header in CFLAGS so the POSIX port's
# `#include <nros/platform.h>` resolves.
CFLAGS += ${INCDIR_PREFIX}$(NANO_ROS_ROOT)/packages/core/nros-platform-api/include

# Phase 157.C — resolve the real nano-ros repo root via the
# `apps/external/nano-ros` symlink (which points at
# `integrations/nuttx/`; `../..` from there lands on the repo
# root). `realpath` follows the symlink so the cargo manifest path
# is absolute + correct. Plain `$(APPDIR)/external/nano-ros/...`
# would resolve through the symlink target literally + miss the
# packages/core/ dir.
NANO_ROS_ROOT := $(realpath $(APPDIR)/external/nano-ros/../..)

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
# matches what `examples/qemu-arm-nuttx/rust/zenoh/*/.cargo/
# config.toml` declares.
# Phase 157.C — `--config` overrides for the NuttX Rust build.
# nano-ros's root workspace doesn't carry a `[patch.crates-io]
# libc = ...` entry (it would affect every build, not just the
# NuttX cross). Inject the same libc patch + per-target rustflags
# the standalone examples carry in their `.cargo/config.toml`
# (e.g. `examples/qemu-arm-nuttx/rust/zenoh/talker/.cargo/
# config.toml`) via cargo's `--config <inline-toml>` flag so the
# integration shell builds match those examples exactly.
#
# CPU / FPU flags are armv7a-nuttx-eabihf-specific (cortex-a7 +
# hard-float ABI + neon-vfpv4). Extend the conditional in the
# `context::` rule when adding more boards.
# Use literal `"` inside the dotted-key TOML values; cargo's
# `--config` parser expects a TOML expression. The single-quoted
# shell arg in the recipe below preserves the `"` chars verbatim.
NROS_LIBC_PATCH := patch.crates-io.libc.path="$(NANO_ROS_ROOT)/third-party/nuttx/libc"
NROS_RUSTFLAGS_ARMV7A := target.armv7a-nuttx-eabihf.rustflags=["-C","link-arg=-mcpu=cortex-a7","-C","link-arg=-mfloat-abi=hard","-C","link-arg=-mfpu=neon-vfpv4"]
NROS_CFLAGS_ARMV7A := env.CFLAGS_armv7a_nuttx_eabihf.value="-mcpu=cortex-a7 -mfloat-abi=hard -mfpu=neon-vfpv4"

# $(3) = feature list (per-crate, since nros-c + nros-cpp have
# different per-backend feature names).
define NROS_CARGO_BUILD
	NUTTX_INCLUDE_DIR=$(TOPDIR)/include:$(TOPDIR)/include/arch \
	CC_armv7a_nuttx_eabihf=arm-none-eabi-gcc \
	CC_armv7a_nuttx_eabi=arm-none-eabi-gcc \
	CXX_armv7a_nuttx_eabihf=arm-none-eabi-g++ \
	CXX_armv7a_nuttx_eabi=arm-none-eabi-g++ \
	AR_armv7a_nuttx_eabihf=arm-none-eabi-ar \
	AR_armv7a_nuttx_eabi=arm-none-eabi-ar \
    cargo build --release \
		-Zbuild-std=std,panic_abort \
		--no-default-features \
		--features "$(strip $(3))" \
		--config '$(NROS_LIBC_PATCH)' \
		--config '$(NROS_RUSTFLAGS_ARMV7A)' \
		--config '$(NROS_CFLAGS_ARMV7A)' \
		--manifest-path $(2)/$(1)/Cargo.toml \
		--target $(NROS_TARGET_TRIPLE)
endef

define NROS_CARGO_CLEAN
	cargo clean --manifest-path $(2)/$(1)/Cargo.toml
endef

ifneq ($(wildcard $(APPDIR)/tools/Rust.mk),)
# NuttX-blessed Rust path — Cargo runs in `context::` for every
# crate listed. `clean::` mirrors. Phase 157.C overrides to the
# armv7a-aware NROS_CARGO_BUILD when LLVM_ARCHTYPE=armv7a falls
# through upstream's macro.
context::
ifeq ($(NROS_TARGET_TRIPLE),)
	$(call RUST_CARGO_BUILD,nros-c,$(NANO_ROS_ROOT)/packages/core)
else
	$(call NROS_CARGO_BUILD,nros-c,$(NANO_ROS_ROOT)/packages/core,$(NROS_C_FEATURES))
endif
# Phase 157.C.11 — also build nros-cpp staticlib when CONFIG_NROS_CPP_API=y.
# `libnros_cpp.a` rides on the same Cargo workspace target dir as
# `libnros_c.a`; Make.defs adds it to EXTRA_LIBS conditionally.
ifneq ($(CONFIG_NROS_CPP_API),)
ifeq ($(NROS_TARGET_TRIPLE),)
	$(call RUST_CARGO_BUILD,nros-cpp,$(NANO_ROS_ROOT)/packages/core)
else
	$(call NROS_CARGO_BUILD,nros-cpp,$(NANO_ROS_ROOT)/packages/core,$(NROS_CPP_FEATURES))
endif
endif

clean::
	$(call NROS_CARGO_CLEAN,nros-c,$(NANO_ROS_ROOT)/packages/core)
ifneq ($(CONFIG_NROS_CPP_API),)
	$(call NROS_CARGO_CLEAN,nros-cpp,$(NANO_ROS_ROOT)/packages/core)
endif
endif

include $(APPDIR)/Application.mk
