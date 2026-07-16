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
# RFC-0048 (phase-287): the ament shape's `find_package(nano_ros REQUIRED)`
# locates the in-tree `nano_rosConfig.cmake` via CMake's `<pkg>_ROOT` env var.
# Exporting it here means a sourced shell needs no `-Dnano_ros_ROOT`; a copy-out
# built outside a sourced shell passes `-Dnano_ros_ROOT=<checkout>` (or a `nros
# setup` CMakePreset carries it).
export nano_ros_ROOT="$_nros_root"

# Rustup-managed toolchain: `scripts/bootstrap.sh` installs rustup, but only
# FUTURE shells pick up `~/.cargo/bin` (rustup edits .bashrc/.profile). The
# book's fresh-machine flow stays in the bootstrap shell, so wire it here —
# otherwise `nros setup`'s source builds die with `cargo: not found`
# (issue #204 probe finding).
if [ -d "$HOME/.cargo/bin" ]; then
    case ":$PATH:" in
        *":$HOME/.cargo/bin:"*) ;;
        *) export PATH="$HOME/.cargo/bin:$PATH" ;;
    esac
fi

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

# `nros` CLI resolution: the in-tree per-checkout binary at
# `packages/cli/target/release/nros`, built by `just setup-cli`. Each
# nano-ros worktree carries its own CLI — no global PATH skew across
# trees. This is the sole source: the pre-218 `~/.nros/bin/nros` curl
# install (`scripts/install-nros.sh`) is retired, and the standalone
# `NEWSLabNTU/nros-cli` repo was merged in-tree at `packages/cli/`.
if [ -x "$_nros_root/packages/cli/target/release/nros" ]; then
    export PATH="$_nros_root/packages/cli/target/release:$PATH"
elif [ -z "${NROS_QUIET_ACTIVATE:-}" ] && ! command -v nros >/dev/null 2>&1; then
    # Phase 222.F.1 — first-run hint. The checkout has no built CLI AND
    # `nros` is not resolvable from any other PATH entry (e.g. ~/.nros/bin).
    # Tell the user how to fix it explicitly instead of letting a silent
    # "command not found" surprise them minutes later. Suppress with
    # NROS_QUIET_ACTIVATE=1 (CI lanes that build the CLI as a separate step).
    echo "[nano-ros] CLI not built yet. Run:" >&2
    echo "  ./scripts/bootstrap.sh           (builds the CLI from source; installs rustup if needed)" >&2
    echo "  Equivalent, if you have cargo:" >&2
    echo "  git submodule update --init packages/cli/third-party/ros-launch-manifest \\" >&2
    echo "    && cargo build --release --manifest-path packages/cli/Cargo.toml --bin nros" >&2
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
# (~/.nros/sdk/<tool>/<version>/bin). Unlike qemu — which the test harness
# resolves via a build/<tool> prefix and deliberately keeps OFF the global
# PATH — a cross-gcc MUST be on PATH for cargo's `linker=` and the
# NuttX/Zephyr `make` to find it (e.g. riscv-none-elf-gcc for the riscv NuttX
# board, Phase 194.3c). Scope to store bin dirs that hold a whitelisted tool
# so the build/<tool> convention for qemu is preserved. A system cross-gcc
# (e.g. /usr/bin/arm-none-eabi-gcc) still resolves when the store has none.
# zenohd joined the whitelist for the book's first-node flow (issue #204):
# `nros setup native` installs it to the store and the book tells the user
# to run `zenohd` — the harness is unaffected (it reads build/zenohd/zenohd
# by explicit path).
_nros_sdk="${NROS_HOME:-$HOME/.nros}/sdk"
if [ -d "$_nros_sdk" ]; then
    for _nros_tcbin in "$_nros_sdk"/*/*/bin "$_nros_sdk"/*/bin; do
        [ -d "$_nros_tcbin" ] || continue
        # Cross-gcc toolchains, plus build host tools the RTOS `make` invokes by
        # bare name (genromfs — the NuttX rv-virt etc/ ROMFS bake, Phase 194.3c),
        # and sccache (issue #74) — the justfile's `RUSTC_WRAPPER` + the zephyr
        # fixture CMake launcher auto-use it once it's on PATH.
        if ls "$_nros_tcbin"/*-gcc >/dev/null 2>&1 \
            || [ -x "$_nros_tcbin/genromfs" ] \
            || [ -x "$_nros_tcbin/sccache" ] \
            || [ -x "$_nros_tcbin/zenohd" ]; then
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
