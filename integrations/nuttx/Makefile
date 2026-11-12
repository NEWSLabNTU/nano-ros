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

# phase-550 W7 — nothing in this file acts while make READS it: the object
# dirs below and the image crate are made by the `context::` recipe. A
# top-level `$(shell …)` runs on every parse, and not every parse has a
# configured tree: `make olddefconfig` walks the apps tree before `DELIM` and
# `CONFIG_ARCH` are defined, so `NROS_APPS_BUILD` collapsed to
# `$(APPDIR)external.nros-build` and a parse-time mkdir created a junk
# directory beside the apps tree (issue 0551), while a parse-time cmake ran
# the image-crate generator on every `make olddefconfig`. `context::` runs
# once per build, in a configured tree, before anything compiles.
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
# A recipe line (phase-550 W7): the generator writes the image crate and
# prints `<manifest> <feature,feature,…>`, which lands in
# $(NROS_IMAGE_SPEC) for the cargo line and `clean::` to read.
NROS_IMAGE_SPEC = $(NROS_IMAGE_ROOT)/image-$(NROS_IMAGE_FLAVOR).txt
define NROS_IMAGE_CRATE
	cmake --log-level=WARNING \
		-DNROS_IMAGE_FLAVOR=$(NROS_IMAGE_FLAVOR) -DNROS_IMAGE_RMW=$(NROS_RMW) \
		-DNROS_IMAGE_EDITION=$(NROS_ROS_EDITION) \
		-DNROS_IMAGE_OUT_DIR=$(NROS_IMAGE_ROOT)/nros-$(NROS_IMAGE_FLAVOR) \
		-DNROS_IMAGE_WORKSPACE_DIR=$(NROS_IMAGE_ROOT) \
		"-DNROS_IMAGE_API_FEATURES=$(NROS_API_FEATURES)" \
		-P $(NANO_ROS_ROOT_DEFS)/cmake/NanoRosImageCrate.cmake > $(NROS_IMAGE_SPEC)
endef
# ONE image per configuration: the one that LINKS. A C++ image links
# `libnros_cpp.a` alone, which bundles nros-c (issue 1636, `Make.defs`), so a C
# image built beside it reached nothing. Building both was also what broke the
# C++ configuration outright: each generator call runs in its OWN cmake
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

# Pass the feature set + NuttX include dir to Cargo via env.
# `apps/tools/Rust.mk` reads CARGO_BUILD_ARGS to inject extras.
export NUTTX_INCLUDE_DIR := $(TOPDIR)/include:$(TOPDIR)/include/arch

# phase-550 W5 — the nightly is the one every other road builds with: the
# first `nightly-*` row of `config/rust-toolchains.txt`, the generated fold of
# every tracked `rust-toolchain.toml` (the cmake road's `nros_pinned_nightly`
# reads the same row). `-Z build-std` needs a nightly, and this road runs
# cargo from the user's tree, where no `rust-toolchain.toml` picks one. Set on
# the cargo command, not exported, and read only when the recipe runs.
NROS_RUST_NIGHTLY = $(shell awk '/^nightly-/ { print $$1; exit }' $(NROS_REPO_DIR)/config/rust-toolchains.txt)

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
# Include the platform.h header in CFLAGS so the POSIX port's
# `#include <nros/platform.h>` resolves.
CFLAGS += ${INCDIR_PREFIX}$(NROS_REPO_DIR)/packages/platform/nros-platform-api/include


# phase-550 W4 — the Rust target triple is `$(NROS_RUST_TRIPLE)`, resolved
# ONCE in `RustTriple.mk` (included by this app's Make.defs, so the archive
# path and cargo's `--target` are the same word): upstream's
# `RUST_TARGET_TRIPLE` first, and an armv7a fallback for the one arch it
# lacks. An arch with neither is refused while the Makefiles are read.

# phase-550 W3a (issue 2426 item 2) — the C cross tools and CPU flags cargo's
# build scripts compile with are NuttX's, for whatever triple was resolved:
# `$(CROSSDEV)` is the toolchain prefix and `$(ARCHCPUFLAGS)` the CPU flags
# the kernel itself compiles with (arch/<arch>/src/*/Toolchain.defs). The
# armv7a-only literals that stood here gave rv-virt the HOST `cc`, which
# refuses `-mabi=ilp32`. cc-rs reads `CC_<triple>` and friends with the
# triple's dashes as underscores.
NROS_TRIPLE_ENV := $(subst -,_,$(NROS_RUST_TRIPLE))

# Our cargo command (issue 2236 Q12 b), not upstream's `RUST_CARGO_BUILD`:
# it names the generated image crate and places headers.
#
# phase-550 W5 (issue 2426 items 1 and 5; 2236 Q5) — and it builds with the
# BOARD's cargo settings, the one carrier every road uses (RFC-0126 D2):
# `[build] target` (rv-virt's is its own target spec), `[unstable]
# build-std` (`core, alloc, panic_abort`: no `std`), and the rest of the
# board's `cargo_config`. `nros locate nuttx-cargo-config` writes them with
# configure's renderer and cargo reads the file with `--config`. The board is
# the kernel's: the NuttX board whose target is the triple this tree
# resolved, narrowed by CONFIG_ARCH_BOARD; a tree whose board that does not
# pick is refused there, naming a project as the way to choose. The inline
# std build, the `libc` patch literal and `--target` went with it.
NROS_CARGO_CONFIG = $(NROS_IMAGE_ROOT)/make/nros-cargo.toml

# phase-550 W6a (issue 2236 Q1, Q2 A2) — an app may name a nano-ros PROJECT,
# in its own Make.defs beside its `CONFIGURED_APPS +=` line:
#
#     NROS_PROJECTS += $(APPDIR)/external/<app>
#     NROS_IMAGE := <bringup>:<id>     # optional
#
# None: a guest, built exactly as before. One: `nros locate
# nuttx-cargo-config` checks it against this kernel (its image is
# NROS_IMAGE, else the one image whose board is the kernel's; the board,
# Kconfig's RMW and edition, and declared entities must agree), writes the
# image key beside the settings file, and `nros configure` configures that
# image in the project. More than one is refused: one NuttX build takes one
# project.
NROS_PROJECT_LIST := $(strip $(NROS_PROJECTS))
NROS_IMAGE_KEY = $(NROS_IMAGE_ROOT)/make/image-key
NROS_PROJECT_ARGS = $(if $(NROS_PROJECT_LIST),--project $(NROS_PROJECT_LIST) \
	$(if $(NROS_IMAGE),--image $(NROS_IMAGE)) --rmw $(NROS_RMW) \
	--ros-edition $(NROS_ROS_EDITION) --kconfig $(TOPDIR)/.config)

# The image crate's manifest and features are the two words of
# $(NROS_IMAGE_SPEC), read when the line runs; $(3) = the image flavor (`c` /
# `cpp`); $(4) = $(NROS_IMAGE_HEADER_PACKAGES).
#
# Issue 1824 — cargo runs under the header placer, which reads the build's
# JSON stream and copies each package's headers out of ITS `$OUT_DIR` into
# the image's placed dir (`Make.defs`). One placer for every road (the Zephyr
# and cmake lanes run the same script).
#
# Phase-550 W2 (issue 2426 item 8) -- the kernel tree reaches cargo. On this
# road the kernel IS $(TOPDIR), the tree make is building, and the apps tree
# is $(APPDIR). Build scripts find both through the ONE ladder
# (`nros_build_paths::locate`, RFC-0103 D3), whose env rung is the
# `[source.nuttx-kernel]` / `[source.nuttx-apps]` `env` names in
# `nros-sdk-index.toml`: `NUTTX_DIR` / `NUTTX_APPS_DIR`, the channel `nros
# build` and the cmake road use too. Without them the ladder fell through to
# the checkout's `third-party/nuttx/nuttx` ("nuttx-kernel: not provisioned
# anywhere this build looks"), or, where that submodule is initialised,
# compiled zenoh-pico against nano-ros's kernel instead of this one.
#
# Set on the command, so they win over an exported value. This road STATES
# the tree (the ladder's arg rung) rather than looking for one: the archive
# links into $(TOPDIR)'s image, so headers from any other tree are issue
# 0525's split (config.h from one kernel, the link from another).
#
# `NUTTX_EXPORT_DIR` is the include-root override that
# `nros_build_paths::nuttx_include_root` consults BEFORE the per-arch
# snapshot under the kernel tree. Pinned to $(TOPDIR), the headers are
# $(TOPDIR)/include, the live tree's: a `nros-nuttx-export-<arch>` snapshot
# left there by `build-nuttx.sh` describes the configuration that script
# built, not this one, and an exported value names some other tree. Its
# `libs/` half (`nuttx_export::snapshot_root`) is read only by board crates,
# which this image does not build, and falls through when `libs/` is absent.
define NROS_CARGO_BUILD
	$(NROS_CLI) locate nuttx-cargo-config \
		--index $(NROS_REPO_DIR)/nros-sdk-index.toml \
		--target $(NROS_RUST_TRIPLE) \
		--nuttx-board $(patsubst "%",%,$(CONFIG_ARCH_BOARD)) \
		--target-dir $(NROS_IMAGE_TARGET_DIR) \
		--out $(NROS_CARGO_CONFIG) $(NROS_PROJECT_ARGS) >/dev/null
	$(if $(NROS_PROJECT_LIST),cd $(NROS_PROJECT_LIST) && $(NROS_CLI) configure --image "$$(cat $(NROS_IMAGE_KEY))")
	NUTTX_DIR=$(TOPDIR) \
	NUTTX_APPS_DIR=$(APPDIR) \
	NUTTX_EXPORT_DIR=$(TOPDIR) \
	NUTTX_INCLUDE_DIR=$(TOPDIR)/include:$(TOPDIR)/include/arch \
	CC_$(NROS_TRIPLE_ENV)=$(CROSSDEV)gcc \
	CXX_$(NROS_TRIPLE_ENV)=$(CROSSDEV)g++ \
	AR_$(NROS_TRIPLE_ENV)=$(CROSSDEV)ar \
	CFLAGS_$(NROS_TRIPLE_ENV)="$(ARCHCPUFLAGS)" \
	CXXFLAGS_$(NROS_TRIPLE_ENV)="$(ARCHCPUFLAGS)" \
	RUSTUP_TOOLCHAIN=$(or $(NROS_RUST_NIGHTLY),$(error nano-ros: no nightly-* row in $(NROS_REPO_DIR)/config/rust-toolchains.txt)) \
    python3 $(NROS_REPO_DIR)/scripts/build/cargo-out-dir-headers.py \
		$(foreach p,$(4),--package $(p)) \
		--dest $(call NROS_PLACED_DIR,$(3)) -- \
    cargo build --release \
		--message-format=json-render-diagnostics \
		--config $(NROS_CARGO_CONFIG) \
		--no-default-features \
		--features "$$(cut -d' ' -f2 $(NROS_IMAGE_SPEC))" \
		--manifest-path "$$(cut -d' ' -f1 $(NROS_IMAGE_SPEC))" \
		--target-dir $(NROS_IMAGE_TARGET_DIR)
endef

define NROS_CARGO_CLEAN
	if [ -s $(NROS_IMAGE_SPEC) ]; then cargo clean --manifest-path "$$(cut -d' ' -f1 $(NROS_IMAGE_SPEC))" --target-dir $(NROS_IMAGE_TARGET_DIR); fi
endef

ifneq ($(wildcard $(APPDIR)/tools/Rust.mk),)
# NuttX-blessed Rust path — cargo runs in `context::`, and `clean::` mirrors
# it. phase-550 W4 deleted the branch that called upstream's
# `RUST_CARGO_BUILD` with an empty `--target`: it could not build, and an
# arch with no triple is now refused by `RustTriple.mk` instead.
context::
	$(if $(word 2,$(NROS_PROJECT_LIST)),$(error nano-ros: more than one app names a project in NROS_PROJECTS: $(NROS_PROJECT_LIST). One NuttX build takes one project; keep a single `NROS_PROJECTS +=` line across the apps' Make.defs files))
	$(Q) mkdir -p $(NROS_APPS_BUILD) $(dir $(addprefix $(NROS_APPS_BUILD),$(CSRCS))) $(NROS_IMAGE_ROOT)
	$(NROS_IMAGE_CRATE)
	$(call NROS_CARGO_BUILD,,,$(NROS_IMAGE_FLAVOR),$(NROS_IMAGE_HEADER_PACKAGES))

clean::
	$(NROS_CARGO_CLEAN)
endif

include $(APPDIR)/Application.mk
