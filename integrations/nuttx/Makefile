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
CARGO_FEATURES :=
ifneq ($(CONFIG_NROS_RMW_ZENOH),)
CARGO_FEATURES += rmw-zenoh
endif
ifneq ($(CONFIG_NROS_RMW_DDS),)
CARGO_FEATURES += rmw-dds
endif
ifneq ($(CONFIG_NROS_RMW_XRCE),)
CARGO_FEATURES += rmw-xrce
endif
ifneq ($(CONFIG_NROS_RMW_CYCLONEDDS),)
CARGO_FEATURES += rmw-cyclonedds
endif
ifneq ($(CONFIG_NROS_ROS_HUMBLE),)
CARGO_FEATURES += ros-humble
endif
ifneq ($(CONFIG_NROS_ROS_IRON),)
CARGO_FEATURES += ros-iron
endif
CARGO_FEATURES += platform-nuttx

# Pass the feature set + NuttX include dir to Cargo via env.
# `apps/tools/Rust.mk` reads CARGO_BUILD_ARGS to inject extras.
export NUTTX_INCLUDE_DIR := $(TOPDIR)/include:$(TOPDIR)/include/arch
export CARGO_BUILD_ARGS := --features "$(strip $(CARGO_FEATURES))" --no-default-features

# Phase 157.C — nano-ros's repo-root `rust-toolchain.toml` pins
# stable. The NuttX `-Zbuild-std=std,panic_abort` flags need
# nightly. Pin via env var so the Cargo invocation from
# `RUST_CARGO_BUILD` picks the same nightly the rest of the
# NuttX examples use (`examples/qemu-arm-nuttx/
# rust-toolchain.toml`). Override via env before `make` if you
# need a different nightly.
export RUSTUP_TOOLCHAIN ?= nightly-2026-04-11

# No C sources of our own — the symbol-level integration goes
# through `libnros_c.a` (and optionally `libnros_cpp.a`) produced
# by the Cargo build in the `context::` phase.
CSRCS =

# Phase 157.C — resolve the real nano-ros repo root via the
# `apps/external/nano-ros` symlink (which points at
# `integrations/nuttx/`; `../..` from there lands on the repo
# root). `realpath` follows the symlink so the cargo manifest path
# is absolute + correct. Plain `$(APPDIR)/external/nano-ros/...`
# would resolve through the symlink target literally + miss the
# packages/core/ dir.
NANO_ROS_ROOT := $(realpath $(APPDIR)/external/nano-ros/../..)

ifneq ($(wildcard $(APPDIR)/tools/Rust.mk),)
# NuttX-blessed Rust path — Cargo runs in `context::` for every
# crate listed.  `clean::` mirrors.
context::
	$(call RUST_CARGO_BUILD,nros-c,$(NANO_ROS_ROOT)/packages/core)

clean::
	$(call RUST_CARGO_CLEAN,nros-c,$(NANO_ROS_ROOT)/packages/core)
endif

include $(APPDIR)/Application.mk
