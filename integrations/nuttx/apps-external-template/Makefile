############################################################################
# Phase 212.H.2 — NuttX apps/external/<bringup>/ Makefile template.
#
# Per-bringup NuttX app. The `context::` rule runs the Phase 212.E
# `nros codegen-system` host-time bake (system.toml + launch/*.xml →
# baked C config + system_main.c) then shells `NROS_CARGO_BUILD` (the
# integration shell's macro from `integrations/nuttx/Makefile`) to
# cross-build the bringup Rust workspace for the NuttX target triple.
#
# The bringup tree ITSELF lives at $(NROS_BRINGUP_WORKSPACE) — the
# staging script writes `nros_bringup.mk` next to this Makefile with
# the workspace + bringup-pkg-name pinned, so the template stays
# bringup-agnostic. Symlink-friendly: re-pointing the symlink at a
# new workspace + re-running the staging script swaps the pinning
# without editing in place.
#
# ≤200 LoC HARD cap. Today this file is well under (see `tokei`).
############################################################################

include $(APPDIR)/Make.defs
include $(APPDIR)/external/nano-ros/Make.defs

# Per-bringup pinning fragment written by the staging script. Carries
# `NROS_BRINGUP_NAME`, `NROS_BRINGUP_WORKSPACE`, `NROS_BRINGUP_IMAGE`,
# and (optional) `NROS_BRINGUP_TARGET_TRIPLE` overrides.
-include $(CURDIR)/nros_bringup.mk

# Default to demo if the pinning fragment is absent (template-as-is).
NROS_BRINGUP_NAME      ?= demo_bringup
NROS_BRINGUP_WORKSPACE ?= $(NANO_ROS_ROOT)/packages/testing/nros-tests/fixtures/multi_pkg_workspace_nuttx
NROS_BRINGUP_BUILD     ?= $(CURDIR)/build
NROS_BRINGUP_OUT       ?= $(NROS_BRINGUP_BUILD)/nros-system

# Issue 1396 — which `[image.<id>]` of the bringup this app IS, pinned by
# `stage-external-apps.sh --image <id>` (which checks it against the
# bringup's `system.toml`). `--target` names that block, and a string that
# names none is taken silently: the tier resolver then answers with the
# HOST's `[tiers.*.posix]` table for a NuttX image. This used to pass a
# synthesised `--target nuttx`, which named no block in the one bringup
# anything in-tree stages here. Unset ⇒ pass no `--target` and say so;
# the Zephyr/ESP-IDF `--for-entry` spelling is unavailable to this shim
# because the app lives in the NuttX apps tree, where no `[image.*]` can
# claim it.
NROS_BRINGUP_IMAGE     ?=
NROS_CODEGEN_TARGET_ARG = $(if $(NROS_BRINGUP_IMAGE),--target $(NROS_BRINGUP_IMAGE))
# No commas in either branch: `$(if)` splits its arguments on them.
NROS_CODEGEN_IMAGE_NOTE = $(if $(NROS_BRINGUP_IMAGE),image $(NROS_BRINGUP_IMAGE),NO IMAGE PINNED - this bake takes the system-wide defaults including the HOST tier tables; re-stage with `--image <id>` (issue 1396))

# Where the host `nros` CLI lives. Mirrors scripts/build/cargo.sh::nros_cli_bin.
# Post-Phase-218 canonical path is `packages/cli/target/release/nros` (built
# by `just setup-cli`); fall back to `~/.nros/bin/nros` (transitional) then PATH.
NROS_BIN ?= $(or \
    $(wildcard $(NANO_ROS_ROOT)/packages/cli/target/release/nros), \
    $(wildcard $(HOME)/.nros/bin/nros), \
    nros)

# Build target triple — inherits the shared shell's resolution when present.
NROS_BRINGUP_TARGET_TRIPLE ?= $(NROS_TARGET_TRIPLE)

ifneq ($(wildcard $(APPDIR)/tools/Rust.mk),)
context::
	@mkdir -p $(NROS_BRINGUP_OUT)
	@echo "  [nros codegen-system] $(NROS_BRINGUP_NAME) → $(NROS_BRINGUP_OUT)"
	@echo '  [nros codegen-system] $(NROS_CODEGEN_IMAGE_NOTE)'
	$(Q) $(NROS_BIN) codegen-system \
		--bringup $(NROS_BRINGUP_NAME) \
		$(NROS_CODEGEN_TARGET_ARG) \
		--out $(NROS_BRINGUP_OUT) \
		--workspace $(NROS_BRINGUP_WORKSPACE)
ifeq ($(NROS_BRINGUP_TARGET_TRIPLE),)
	$(call RUST_CARGO_BUILD,$(NROS_BRINGUP_NAME),$(NROS_BRINGUP_WORKSPACE))
else
	$(call NROS_CARGO_BUILD,$(NROS_BRINGUP_NAME),$(NROS_BRINGUP_WORKSPACE),std platform-nuttx)
endif

clean::
	$(Q) rm -rf $(NROS_BRINGUP_BUILD)
ifneq ($(NROS_BRINGUP_TARGET_TRIPLE),)
	-$(call NROS_CARGO_CLEAN,$(NROS_BRINGUP_NAME),$(NROS_BRINGUP_WORKSPACE))
endif
endif

include $(APPDIR)/Application.mk
