# Default paths for external SDKs. Keep these repo-relative so copied
# checkouts remain position-independent; local overrides belong in `.env`.
#
# ## Why every line below carries a `shell(_NROS_REROOT, …)` — issues 1280, 1391
#
# `env(NAME, default)` is ENV-FIRST, and that order is load-bearing: it is how
# a real out-of-tree SDK gets used. What it also did was hand a build in a
# LINKED GIT WORKTREE the other checkout's trees. A worktree inherits its
# parent shell's environment, every one of these values is an ABSOLUTE path
# rooted at the `justfile_directory()` of the checkout that shell activated,
# and an inherited absolute path outranks a default computed from the justfile
# that is actually running. Measured: all 24 of them set, all naming the main
# checkout, in a session whose whole purpose was to test a change in the
# worktree. So a worktree build compiled the main checkout's FreeRTOS, ThreadX,
# NuttX, platform sources and public headers, wrote into its NuttX kernel, and
# reported the worktree's change as verified. CLAUDE.md prescribes worktrees
# for parallel sessions, so that is the default shape of agent work here.
#
# ## The rule is OWNERSHIP, and a lexical prefix rewrite is not it — issue 1391
#
# 1280's fix was a PREFIX REWRITE: one `replace(value, _NROS_OTHER, _NROS_HERE)`
# per line, with `_NROS_OTHER` the one other checkout the environment was
# inherited from. That is not the rule stated in
# `scripts/lib/checkout-paths.sh`, and the difference is measurable the moment
# two checkouts NEST — which is the normal shape here, because an agent
# worktree lives at `<main>/.claude/worktrees/<id>`. The main checkout's root
# is then a strict PREFIX of the worktree's, so the rewrite fired on the
# DEFAULTS too, which were already rooted at `_NROS_HERE`. Measured in such a
# worktree on 811f081a3: nine of the twenty-one came out doubled, e.g.
#
#   NROS_PLATFORM_CFFI_INCLUDE :=
#     "<main>/.claude/worktrees/<id>/.claude/worktrees/<id>/packages/platform/…"
#
# and builds died on `fatal error: nros/platform.h: No such file or directory`
# — naming a source file rather than the environment. Two agent sessions lost a
# full lane run each to it. No prefix can be right here: under nesting, EVERY
# `_NROS_HERE`-rooted value is also `_NROS_OTHER`-rooted, so "keep" and
# "re-root" are the same lexical test and only the DEEPEST owning checkout
# tells them apart.
#
# So each value goes through the rule itself, one process per value:
# `nros_reroot_checkout_path`, the same function `scripts/sdk-env.sh` and
# `build-root.sh` call, reached through the thin
# `scripts/lib/reroot-checkout-path.sh`. It resolves the value's OWNING
# checkout (deepest marker at or above it) and keeps the value when that owner
# is this checkout — so a worktree-rooted default is kept, a parent-rooted
# inherited value is re-rooted, and a path outside any checkout (a genuine
# out-of-tree SDK, which is what env-first exists for) is kept untouched. No
# list of which variable is which, and nothing to keep in step by hand.
#
# Cost, measured on this host: 26–28 ms for all 21 values, against ~24 ms for
# the single detector call 1280 used. `check-inherited-checkout-paths.py` gates
# the coverage of this file, pins the spellings of the rule to each other, and
# drives `just` itself in BOTH checkout shapes — side-by-side and nested.

_NROS_HERE := justfile_directory()

# The one invocation spelling, so the 21 lines below differ only in the name
# and the default. `$1` is the value, `$2` is this checkout.
_NROS_REROOT := '"$2"/scripts/lib/reroot-checkout-path.sh "$1" "$2"'

# ADVISORY ONLY, and deliberately kept: this prints "re-rooting inherited paths
# from X onto Y" on stderr once per `just` run, which is the line that tells a
# reader their environment came from another checkout at all. Nothing below
# consumes its value any more — the per-value rule above needs no prefix — but
# deleting the call silences the diagnostic, so `check-inherited-checkout-paths`
# asserts it is still here. It no longer has to pick ONE foreign root either:
# per-value ownership handles an environment naming several, so that case is
# now reported rather than refused (issue 1391).
_NROS_INHERITED_FROM := shell('"$1"/scripts/lib/foreign-checkout-root.sh "$1"', _NROS_HERE)

# The checkout a build is FOR. Not `env(...)`: `justfile_directory()` is
# definitionally the checkout this `just` is running in, so there is nothing an
# inherited value could be more right about — and it is the sharpest case of
# all, because every path derived from it inherits the error. Inherited from
# the main checkout into a worktree it sent four gates' fixtures into the main
# checkout's `build/`, so those gates RAN and measured the wrong tree, which is
# worse than failing (issue 1280, phase-454 W3).
export NROS_REPO_DIR := _NROS_HERE
# Same argument, same variable one layer over: CMake's `<pkg>_ROOT` for
# `find_package(nano_ros)`. `activate.sh` and `activate.fish` already export
# both unconditionally; this makes a `just` run that was never activated agree
# with them.
export nano_ros_ROOT := _NROS_HERE

export FREERTOS_DIR := shell(_NROS_REROOT, env("FREERTOS_DIR", _NROS_HERE / "third-party/freertos/kernel"), _NROS_HERE)
export FREERTOS_PORT := env("FREERTOS_PORT", "GCC/ARM_CM3")
export LWIP_DIR := shell(_NROS_REROOT, env("LWIP_DIR", _NROS_HERE / "third-party/freertos/lwip"), _NROS_HERE)
export FREERTOS_CONFIG_DIR := shell(_NROS_REROOT, env("FREERTOS_CONFIG_DIR", _NROS_HERE / "packages/boards/nros-board-mps2-an385-freertos/config"), _NROS_HERE)
# First-party C the board build.rs compiles (Phase 191/192.3 — was a build.rs
# `../../../packages` walk-up; centralized here so build.rs reads an env var).
export NROS_PLATFORM_CFFI_INCLUDE := shell(_NROS_REROOT, env("NROS_PLATFORM_CFFI_INCLUDE", _NROS_HERE / "packages/platform/nros-platform-api/include"), _NROS_HERE)
export NROS_PLATFORM_FREERTOS_SRC := shell(_NROS_REROOT, env("NROS_PLATFORM_FREERTOS_SRC", _NROS_HERE / "packages/platform/nros-platform-freertos/src"), _NROS_HERE)
export NROS_PLATFORM_POSIX_SRC := shell(_NROS_REROOT, env("NROS_PLATFORM_POSIX_SRC", _NROS_HERE / "packages/platform/nros-platform-posix/src"), _NROS_HERE)
export NROS_PLATFORM_THREADX_SRC := shell(_NROS_REROOT, env("NROS_PLATFORM_THREADX_SRC", _NROS_HERE / "packages/platform/nros-platform-threadx/src"), _NROS_HERE)
export NROS_LAN9118_LWIP_DIR := shell(_NROS_REROOT, env("NROS_LAN9118_LWIP_DIR", _NROS_HERE / "packages/drivers/net/lan9118-lwip"), _NROS_HERE)
export NROS_VIRTIO_NET_NETX_DIR := shell(_NROS_REROOT, env("NROS_VIRTIO_NET_NETX_DIR", _NROS_HERE / "packages/drivers/net/virtio-net-netx"), _NROS_HERE)
export NROS_C_INCLUDE := shell(_NROS_REROOT, env("NROS_C_INCLUDE", _NROS_HERE / "packages/api/nros-c/include"), _NROS_HERE)
export NROS_CPP_INCLUDE := shell(_NROS_REROOT, env("NROS_CPP_INCLUDE", _NROS_HERE / "packages/api/nros-cpp/include"), _NROS_HERE)
# Tonbandgeraet trace lib (third-party SDK; only used under NROS_TRACE=1).
export TBAND_DIR := shell(_NROS_REROOT, env("TBAND_DIR", _NROS_HERE / "third-party/tracing/Tonbandgeraet/tband"), _NROS_HERE)
export NUTTX_DIR := shell(_NROS_REROOT, env("NUTTX_DIR", _NROS_HERE / "third-party/nuttx/nuttx"), _NROS_HERE)
export NUTTX_APPS_DIR := shell(_NROS_REROOT, env("NUTTX_APPS_DIR", _NROS_HERE / "third-party/nuttx/nuttx-apps"), _NROS_HERE)
export THREADX_DIR := shell(_NROS_REROOT, env("THREADX_DIR", _NROS_HERE / "third-party/threadx/kernel"), _NROS_HERE)
export THREADX_CONFIG_DIR := shell(_NROS_REROOT, env("THREADX_CONFIG_DIR", _NROS_HERE / "packages/boards/nros-board-threadx-linux/config"), _NROS_HERE)
export NETX_DIR := shell(_NROS_REROOT, env("NETX_DIR", _NROS_HERE / "third-party/threadx/netxduo"), _NROS_HERE)
export NETX_CONFIG_DIR := shell(_NROS_REROOT, env("NETX_CONFIG_DIR", _NROS_HERE / "packages/boards/nros-board-threadx-linux/config"), _NROS_HERE)
export PX4_AUTOPILOT_DIR := shell(_NROS_REROOT, env("PX4_AUTOPILOT_DIR", _NROS_HERE / "third-party/px4/PX4-Autopilot"), _NROS_HERE)
export NROS_ESP_IDF_WORKSPACE := shell(_NROS_REROOT, env("NROS_ESP_IDF_WORKSPACE", _NROS_HERE / "esp-idf-workspace/esp-idf"), _NROS_HERE)
export NROS_ESP_IDF_ENV_SHIM := shell(_NROS_REROOT, env("NROS_ESP_IDF_ENV_SHIM", _NROS_HERE / "esp-idf-workspace/env.sh"), _NROS_HERE)
export IDF_PATH := shell(_NROS_REROOT, env("IDF_PATH", NROS_ESP_IDF_WORKSPACE), _NROS_HERE)
