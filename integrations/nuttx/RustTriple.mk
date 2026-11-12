############################################################################
# phase-550 W4 (issue 2426 items 3 and 4) — the ONE Rust target triple of a
# make-road build. Included by the shell's Make.defs (the archive path) and
# Makefile (cargo's `--target`), so the two cannot disagree.
#
# Upstream first: `RUST_TARGET_TRIPLE` (apps/tools/Rust.mk) answers every
# arch it knows. It has no branch for ARM-A without thumb
# (`LLVM_ARCHTYPE := armv7a`, which `qemu-armv7a:nsh` configures), so for
# that arch alone this file falls back to `armv7a-nuttx-<abi>`. The
# fallback goes when the nuttx-apps pin gains the branch upstream; no fork.
# `check-nuttx-rust-triple` holds the fallback to upstream's table: for
# every arch upstream knows, this file must answer what upstream answers.
#
# An arch neither answers is refused here, at parse time, naming
# LLVM_ARCHTYPE: cargo with an empty `--target` cannot build.
############################################################################

NROS_RUST_TRIPLE_FALLBACK = $(if $(filter armv7a,$(LLVM_ARCHTYPE)),armv7a-nuttx-$(LLVM_ABITYPE))

NROS_RUST_TRIPLE := $(strip $(or $(strip $(call RUST_TARGET_TRIPLE)),$(NROS_RUST_TRIPLE_FALLBACK)))

# Only in a CONFIGURED tree. `make olddefconfig` reads the apps Makefiles
# while it is still resolving `.config`: first with no CONFIG_ARCH at all
# (see the Makefile's PREFIX note), then, while it creates the dir links,
# with CONFIG_ARCH set but the arch symbols the toolchain keys on (rv-virt's
# CONFIG_ARCH_RV32) not yet derived, so every LLVM_* is empty (measured).
# `include/nuttx/config.h` is the mark of a configured tree: the `context`
# step writes it from the resolved `.config`, and `olddefconfig` deletes it.
ifneq ($(wildcard $(TOPDIR)/include/nuttx/config.h),)
ifeq ($(NROS_RUST_TRIPLE),)
$(error nano-ros: no Rust target for this NuttX configuration (LLVM_ARCHTYPE='$(LLVM_ARCHTYPE)', LLVM_ABITYPE='$(LLVM_ABITYPE)', LLVM_CPUTYPE='$(LLVM_CPUTYPE)'): apps/tools/Rust.mk names none, and nano-ros adds one only for armv7a)
endif
endif
