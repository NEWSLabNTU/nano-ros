# Default paths for external SDKs. Keep these repo-relative so copied
# checkouts remain position-independent; local overrides belong in `.env`.
#
# ## Why every line below carries a `replace(...)` — issue 1280
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
# The fix is a PREFIX REWRITE, not a list. `_NROS_OTHER` is the one other
# nano-ros checkout this environment was inherited from (empty when there is
# none, in which case it is set to `_NROS_HERE` and every `replace` below is a
# no-op). Because all the inherited values share that one prefix, one rewrite
# re-roots all of them — and a value pointing OUTSIDE any nano-ros checkout
# does not contain the prefix, so it survives untouched. That is the property
# that keeps a genuine out-of-tree SDK working, and it holds without anyone
# maintaining a list of which variables are which.
#
# The rule itself lives in `scripts/lib/checkout-paths.sh`; the detector is
# `scripts/lib/foreign-checkout-root.sh` (ONE subprocess per `just` run, ~24 ms,
# and it refuses rather than guesses when the environment names two other
# checkouts). `check-inherited-checkout-paths.py` gates the coverage of this
# file and pins the three spellings of the rule to each other.

_NROS_HERE := justfile_directory()
_NROS_OTHER_RAW := shell('"$1"/scripts/lib/foreign-checkout-root.sh "$1"', _NROS_HERE)
# Rewriting `_NROS_HERE` to `_NROS_HERE` is the no-op that lets every line
# below have exactly one shape, with no `if` per variable.
_NROS_OTHER := if _NROS_OTHER_RAW == "" { _NROS_HERE } else { _NROS_OTHER_RAW }

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

export FREERTOS_DIR := replace(env("FREERTOS_DIR", _NROS_HERE / "third-party/freertos/kernel"), _NROS_OTHER, _NROS_HERE)
export FREERTOS_PORT := env("FREERTOS_PORT", "GCC/ARM_CM3")
export LWIP_DIR := replace(env("LWIP_DIR", _NROS_HERE / "third-party/freertos/lwip"), _NROS_OTHER, _NROS_HERE)
export FREERTOS_CONFIG_DIR := replace(env("FREERTOS_CONFIG_DIR", _NROS_HERE / "packages/boards/nros-board-mps2-an385-freertos/config"), _NROS_OTHER, _NROS_HERE)
# First-party C the board build.rs compiles (Phase 191/192.3 — was a build.rs
# `../../../packages` walk-up; centralized here so build.rs reads an env var).
export NROS_PLATFORM_CFFI_INCLUDE := replace(env("NROS_PLATFORM_CFFI_INCLUDE", _NROS_HERE / "packages/platform/nros-platform-api/include"), _NROS_OTHER, _NROS_HERE)
export NROS_PLATFORM_FREERTOS_SRC := replace(env("NROS_PLATFORM_FREERTOS_SRC", _NROS_HERE / "packages/platform/nros-platform-freertos/src"), _NROS_OTHER, _NROS_HERE)
export NROS_PLATFORM_POSIX_SRC := replace(env("NROS_PLATFORM_POSIX_SRC", _NROS_HERE / "packages/platform/nros-platform-posix/src"), _NROS_OTHER, _NROS_HERE)
export NROS_PLATFORM_THREADX_SRC := replace(env("NROS_PLATFORM_THREADX_SRC", _NROS_HERE / "packages/platform/nros-platform-threadx/src"), _NROS_OTHER, _NROS_HERE)
export NROS_LAN9118_LWIP_DIR := replace(env("NROS_LAN9118_LWIP_DIR", _NROS_HERE / "packages/drivers/net/lan9118-lwip"), _NROS_OTHER, _NROS_HERE)
export NROS_VIRTIO_NET_NETX_DIR := replace(env("NROS_VIRTIO_NET_NETX_DIR", _NROS_HERE / "packages/drivers/net/virtio-net-netx"), _NROS_OTHER, _NROS_HERE)
export NROS_C_INCLUDE := replace(env("NROS_C_INCLUDE", _NROS_HERE / "packages/api/nros-c/include"), _NROS_OTHER, _NROS_HERE)
export NROS_CPP_INCLUDE := replace(env("NROS_CPP_INCLUDE", _NROS_HERE / "packages/api/nros-cpp/include"), _NROS_OTHER, _NROS_HERE)
# Tonbandgeraet trace lib (third-party SDK; only used under NROS_TRACE=1).
export TBAND_DIR := replace(env("TBAND_DIR", _NROS_HERE / "third-party/tracing/Tonbandgeraet/tband"), _NROS_OTHER, _NROS_HERE)
export NUTTX_DIR := replace(env("NUTTX_DIR", _NROS_HERE / "third-party/nuttx/nuttx"), _NROS_OTHER, _NROS_HERE)
export NUTTX_APPS_DIR := replace(env("NUTTX_APPS_DIR", _NROS_HERE / "third-party/nuttx/nuttx-apps"), _NROS_OTHER, _NROS_HERE)
export THREADX_DIR := replace(env("THREADX_DIR", _NROS_HERE / "third-party/threadx/kernel"), _NROS_OTHER, _NROS_HERE)
export THREADX_CONFIG_DIR := replace(env("THREADX_CONFIG_DIR", _NROS_HERE / "packages/boards/nros-board-threadx-linux/config"), _NROS_OTHER, _NROS_HERE)
export NETX_DIR := replace(env("NETX_DIR", _NROS_HERE / "third-party/threadx/netxduo"), _NROS_OTHER, _NROS_HERE)
export NETX_CONFIG_DIR := replace(env("NETX_CONFIG_DIR", _NROS_HERE / "packages/boards/nros-board-threadx-linux/config"), _NROS_OTHER, _NROS_HERE)
export PX4_AUTOPILOT_DIR := replace(env("PX4_AUTOPILOT_DIR", _NROS_HERE / "third-party/px4/PX4-Autopilot"), _NROS_OTHER, _NROS_HERE)
export NROS_ESP_IDF_WORKSPACE := replace(env("NROS_ESP_IDF_WORKSPACE", _NROS_HERE / "esp-idf-workspace/esp-idf"), _NROS_OTHER, _NROS_HERE)
export NROS_ESP_IDF_ENV_SHIM := replace(env("NROS_ESP_IDF_ENV_SHIM", _NROS_HERE / "esp-idf-workspace/env.sh"), _NROS_OTHER, _NROS_HERE)
export IDF_PATH := replace(env("IDF_PATH", NROS_ESP_IDF_WORKSPACE), _NROS_OTHER, _NROS_HERE)
