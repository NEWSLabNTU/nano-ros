# nano-ros workspace activation — POSIX shell (bash / zsh).
#
# Phase 218.C — single source of truth for env exports + PATH wiring.
# Source this once after `git clone`:
#
#     source ./activate.sh
#
# direnv users get it for free via `.envrc` (which sources this file).
# fish users source `./activate.fish` instead — the two files stay
# manually mirrored.
#
# This file does NOT install anything. It WIRES paths to artifacts that
# `just setup` (or `just setup-cli` for the CLI alone) produces. If the
# corresponding binaries / SDKs are absent, the export is harmlessly
# skipped — the script never errors.

# Resolve the workspace root the way both bash and zsh agree on:
# ${BASH_SOURCE[0]} for bash, ${(%):-%N} for zsh, $0 as the fallback
# when the script is `source`d.
if [ -n "${BASH_SOURCE[0]:-}" ]; then
    _nros_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
elif [ -n "${ZSH_VERSION:-}" ]; then
    _nros_root="$(cd "$(dirname "${(%):-%N}")" && pwd)"
else
    _nros_root="$(cd "$(dirname "$0")" && pwd)"
fi
export NROS_REPO_DIR="$_nros_root"

# ROS 2 Humble — sourcing setup.bash exports AMENT_PREFIX_PATH,
# CMAKE_PREFIX_PATH, ROS_DISTRO, etc. Required by `nros generate-rust`
# (resolves .msg defs via rosidl_adapter) + the cyclonedds codegen +
# every rmw_zenoh interop test.
if [ -f /opt/ros/humble/setup.bash ]; then
    # shellcheck disable=SC1091
    . /opt/ros/humble/setup.bash
else
    echo "activate.sh: /opt/ros/humble/setup.bash not found — ROS-dependent recipes will fail" >&2
fi

# `nros` CLI resolution (Phase 218 monorepo merge):
#   1. packages/cli/target/release/nros — per-checkout binary, built by
#      `just setup-cli`. PREFERRED. Each nano-ros worktree carries its
#      own CLI, no global PATH skew across trees.
#   2. ~/.nros/bin/nros — transitional fallback for users mid-migration
#      from the pre-218 `scripts/install-nros.sh` curl install. Will be
#      removed once every active branch lands on 218.
# Order matters: each `export PATH="X:$PATH"` prepends X to the LEFT,
# so the LAST export wins on a shell PATH search. To make (1) win,
# (2) is exported FIRST, then (1) is exported LAST.
if [ -x "${NROS_HOME:-$HOME/.nros}/bin/nros" ]; then
    export PATH="${NROS_HOME:-$HOME/.nros}/bin:$PATH"
fi
if [ -x "$_nros_root/packages/cli/target/release/nros" ]; then
    export PATH="$_nros_root/packages/cli/target/release:$PATH"
elif [ -z "${NROS_QUIET_ACTIVATE:-}" ] && ! command -v nros >/dev/null 2>&1; then
    # Phase 222.F.1 — first-run hint. The checkout has no built CLI AND
    # `nros` is not resolvable from any other PATH entry (e.g. ~/.nros/bin).
    # Tell the user how to fix it explicitly instead of letting a silent
    # "command not found" surprise them minutes later. Suppress with
    # NROS_QUIET_ACTIVATE=1 (CI lanes that build the CLI as a separate step).
    echo "[nano-ros] CLI not built yet. Run one of:" >&2
    echo "  ./scripts/bootstrap.sh base      (bare machine)" >&2
    echo "  cargo build --release --manifest-path packages/cli/Cargo.toml --bin nros   (if you have cargo)" >&2
    echo "  ./scripts/install-nros-prebuilt.sh   (tagged checkout; downloads prebuilt)" >&2
    echo "  (set NROS_QUIET_ACTIVATE=1 to suppress this hint.)" >&2
fi

# `play_launch_parser` — installed by `just workspace install-play-
# launch-parser` to `~/.nros/sdk/play_launch_parser/bin/`. The Phase
# 212.L.6 launch-graph resolver shells out to it. The Phase 212.M-F.20
# pkg-index resolver inside the parser eats `AMENT_PREFIX_PATH` so the
# ROS source above must run FIRST.
if [ -x "${NROS_HOME:-$HOME/.nros}/sdk/play_launch_parser/bin/play_launch_parser" ]; then
    export PATH="${NROS_HOME:-$HOME/.nros}/sdk/play_launch_parser/bin:$PATH"
fi

# Cross-compiler toolchains installed by `nros setup` land in the SDK store
# (~/.nros/sdk/<tool>/<version>/bin). Unlike qemu/zenohd — which the test
# harness resolves via a build/<tool> prefix and deliberately keeps OFF the
# global PATH — a cross-gcc MUST be on PATH for cargo's `linker=` and the
# NuttX/Zephyr `make` to find it (e.g. riscv-none-elf-gcc for the riscv NuttX
# board, Phase 194.3c). Scope to store bin dirs that hold a `*-gcc` so the
# build/<tool> convention for qemu/zenohd is preserved. A system cross-gcc
# (e.g. /usr/bin/arm-none-eabi-gcc) still resolves when the store has none.
_nros_sdk="${NROS_HOME:-$HOME/.nros}/sdk"
if [ -d "$_nros_sdk" ]; then
    for _nros_tcbin in "$_nros_sdk"/*/*/bin "$_nros_sdk"/*/bin; do
        [ -d "$_nros_tcbin" ] || continue
        if ls "$_nros_tcbin"/*-gcc >/dev/null 2>&1; then
            export PATH="$_nros_tcbin:$PATH"
        fi
    done
    unset _nros_tcbin
fi
unset _nros_sdk

# Pinned ninja (>=1.13, GNU-jobserver client — Phase 176) installed by
# `just workspace install-ninja`. Wins over Ubuntu's apt 1.10 (no
# jobserver). Required by `just build-all-jobserver` to scale cargo +
# cmake + ninja under one token pool.
if [ -x "$_nros_root/third-party/ninja/ninja" ]; then
    export PATH="$_nros_root/third-party/ninja:$PATH"
fi

# Pinned GNU make (>=4.4, fifo jobserver — Phase 176) installed by
# `just workspace install-make`. Wins over Ubuntu's 4.3.
if [ -x "$_nros_root/third-party/make/make" ]; then
    export PATH="$_nros_root/third-party/make:$PATH"
fi

# Project `.env` overrides (runtime config, buffer tuning, SDK paths)
# + the just/sdk-env.just SSoT defaults. direnv loads `.env` via
# `dotenv_if_exists`; outside direnv we shell-source it here.
if [ -f "$_nros_root/.env" ]; then
    set -a
    # shellcheck disable=SC1091
    . "$_nros_root/.env"
    set +a
fi
# shellcheck disable=SC1091
. "$_nros_root/scripts/sdk-env.sh"

unset _nros_root
