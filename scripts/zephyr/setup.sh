#!/bin/bash
# nros Zephyr Workspace Setup
#
# Creates a Zephyr workspace at $NROS_ZEPHYR_WORKSPACE, defaulting to
# $repo/zephyr-workspace/ (gitignored). Pre-existing sibling installs at
# $repo/../nano-ros-workspace/ are auto-detected and reused so contributors
# who set up before this change keep working.
#
# Installs:
#   - Python tools (west, etc.)
#   - Zephyr SDK (cross-compilers)
#   - Zephyr RTOS and modules
#   - zephyr-lang-rust for Rust support
#
# Prerequisites (install manually):
#   - Python 3.8+, pip
#   - cmake, ninja-build
#   - Rust toolchain (rustup)
#
# Usage:
#   ./scripts/zephyr/setup.sh [OPTIONS]
#
# Options:
#   --force            Overwrite existing workspace
#   --skip-sdk         Skip SDK installation (if already installed)
#
# Environment overrides:
#   NROS_ZEPHYR_WORKSPACE   Absolute path to install the workspace at.
#                           Default: $repo/zephyr-workspace/
#
# Example:
#   ./scripts/zephyr/setup.sh
#   source zephyr-workspace/env.sh
#   cd zephyr-workspace
#   west build -b native_sim/native/64 nros/examples/zephyr/rust/talker -- -DCONF_FILE="prj.conf;prj-zenoh.conf"

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NANO_ROS_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
NANO_ROS_PARENT="$(dirname "$NANO_ROS_ROOT")"
NANO_ROS_NAME="$(basename "$NANO_ROS_ROOT")"

# Workspace location. Priority:
#   1. $NROS_ZEPHYR_WORKSPACE explicit override
#   2. Pre-existing sibling install at $parent/${name}-workspace (legacy layout)
#   3. In-tree default at $repo/zephyr-workspace
LEGACY_SIBLING="$NANO_ROS_PARENT/${NANO_ROS_NAME}-workspace"
IN_TREE_DEFAULT="$NANO_ROS_ROOT/zephyr-workspace"
if [ -n "${NROS_ZEPHYR_WORKSPACE:-}" ]; then
    WORKSPACE_DIR="$NROS_ZEPHYR_WORKSPACE"
elif [ -d "$LEGACY_SIBLING/.west" ]; then
    WORKSPACE_DIR="$LEGACY_SIBLING"
else
    WORKSPACE_DIR="$IN_TREE_DEFAULT"
fi

# Normalize WORKSPACE_DIR to an absolute path while cwd is still the repo root.
# `install_sdk` later `cd`s into the SDK dir and does not return, so a *relative*
# WORKSPACE_DIR (e.g. "zephyr-workspace" / "../nano-ros-workspace-4.4" passed by
# the just recipe) would make the subsequent `cd "$WORKSPACE_DIR"` land inside
# the SDK tree (scripts/zephyr/sdk/...). That only triggers on a fresh install
# (the SDK build runs), which is why local cached-SDK runs pass but CI fails.
# `[ -d ]` guard: WORKSPACE_DIR may already be a directory (re-run) or a dev
# symlink to a sibling workspace — `mkdir -p` errors on a non-directory.
[ -d "$WORKSPACE_DIR" ] || mkdir -p "$WORKSPACE_DIR"
WORKSPACE_DIR="$(cd "$WORKSPACE_DIR" && pwd)"

# Phase 180.A — west manifest selector. west.yml = 3.7 LTS (default),
# west-4.4.yml = 4.4 rolling. Set via NROS_ZEPHYR_MANIFEST.
MANIFEST="${NROS_ZEPHYR_MANIFEST:-west.yml}"

SDK_INSTALL_DIR="$SCRIPT_DIR/sdk"

# Zephyr SDK configuration.
#
# The VERSION is all that lives here — it names the install dir below. Which
# tarball that version means on THIS host, and its checksum, are
# `[tool.zephyr-sdk]` in `nros-sdk-index.toml`, fetched by `nros setup --tool`
# (issue 0610). They were hardcoded to x86_64 here, which does not fail at
# download — the x86_64 archive fetches and verifies happily anywhere — but dies
# later inside the SDK's own installer with `Installing host tools ... ERROR:
# Host tools installation failed`, naming neither the arch nor the tarball.
#
# THE SDK IS A PER-LINE FACT, NOT A CONSTANT. Each Zephyr tree states the SDK it
# needs in `zephyr/SDK_VERSION`, and `FindZephyr-sdk.cmake` refuses anything
# older: 3.7 LTS wants 0.16.8, 4.4 wants 1.0.1. The manifest and the patch set
# were already dispatched per line; this was not, so
# `NROS_ZEPHYR_VERSION=4.4 just zephyr setup` exited 0 and produced a workspace
# that could not build any Cortex-M target. The failure surfaced only at the
# first `west build`, as a bare `FindZephyr-sdk.cmake:160 find_package` error
# naming neither the SDK version nor the step that chose it.
#
# Each arm names an INDEX ENTRY; the index stays the SSOT for URLs, checksums
# and host keying. Adding a Zephyr line = a new `[tool.zephyr-sdk-*]` table
# plus an arm here.
case "$MANIFEST" in
    west-4.4.yml)
        ZEPHYR_SDK_VERSION="1.0.1"
        ZEPHYR_SDK_TOOL="zephyr-sdk-1-0-1"
        ;;
    *)
        ZEPHYR_SDK_VERSION="0.16.8"
        ZEPHYR_SDK_TOOL="zephyr-sdk"
        ;;
esac

# Parse arguments
FORCE=false
SKIP_SDK=false
# Default Zephyr SDK toolchains:
#   x86_64-zephyr-elf  — native_sim and POSIX-emulated boards.
#   arm-zephyr-eabi    — Cortex-M, Cortex-A 32-bit, Cortex-R52 (ARMv8-R AArch32).
# Phase 117 adds:
#   aarch64-zephyr-elf — Cortex-A 64-bit + ARMv8-R AArch64
#                        (e.g. fvp_baser_aemv8r_smp targeted by 117.10).
#                        Toggle via `--phase-117` (or `--targets 117`).
DEFAULT_TARGETS="x86_64-zephyr-elf arm-zephyr-eabi"
EXTRA_TARGETS=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --force|-f)
            FORCE=true
            shift
            ;;
        --skip-sdk)
            SKIP_SDK=true
            shift
            ;;
        --phase-117)
            EXTRA_TARGETS="$EXTRA_TARGETS aarch64-zephyr-elf"
            shift
            ;;
        --target)
            EXTRA_TARGETS="$EXTRA_TARGETS $2"
            shift 2
            ;;
        --help|-h)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Create nros Zephyr workspace at ../${NANO_ROS_NAME}-workspace/"
            echo ""
            echo "Options:"
            echo "  --force, -f          Overwrite existing workspace"
            echo "  --skip-sdk           Skip SDK installation"
            echo "  --phase-117          Add aarch64-zephyr-elf for the"
            echo "                       Cortex-A / ARMv8-R AArch64 boards"
            echo "                       targeted by Phase 117.10"
            echo "                       (fvp_baser_aemv8r_smp)."
            echo "  --target NAME        Add an extra Zephyr SDK target"
            echo "                       (e.g. mips-zephyr-elf). Repeatable."
            echo ""
            echo "Default SDK targets installed: $DEFAULT_TARGETS"
            echo ""
            echo "FVP simulator + NXP S32Z board files are NOT installed by"
            echo "this script — see docs/reference/zephyr-armv8r-setup.md"
            echo "for the manual steps."
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

ALL_TARGETS="$DEFAULT_TARGETS $EXTRA_TARGETS"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[OK]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

echo ""
echo "========================================"
echo "  nros Zephyr Workspace Setup"
echo "========================================"
echo ""
log_info "Workspace: $WORKSPACE_DIR"
log_info "SDK directory: $SDK_INSTALL_DIR"
log_info "nros: $NANO_ROS_ROOT"
echo ""

# =============================================================================
# Check Prerequisites
# =============================================================================

log_info "Checking prerequisites..."

check_command() {
    if command -v "$1" &> /dev/null; then
        log_success "$1 found"
        return 0
    else
        log_error "$1 not found"
        return 1
    fi
}

MISSING=0
check_command python3 || MISSING=1
check_command pip3 || MISSING=1
check_command cmake || MISSING=1
check_command git || MISSING=1
check_command ninja || { log_warn "ninja not found"; MISSING=1; }
check_command rustc || MISSING=1
check_command cargo || MISSING=1

if [ $MISSING -eq 1 ]; then
    echo ""
    log_error "Missing prerequisites."
    # phase-327 W3 — the OS-package remedy is DERIVED from the index (never a
    # hand-written apt line): the printer composes the native command for THIS
    # host's package manager. ninja has a sudo-less installer of its own.
    if command -v nros >/dev/null 2>&1; then
        nros setup --system || true
    else
        echo "  (build the CLI first — just setup-cli — then run: nros setup --system)"
    fi
    echo "  ninja (sudo-less): nros setup --tool ninja"
    echo "  rust:  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

# =============================================================================
# Check Python Tools — nano-ros does NOT provision them
# =============================================================================

# This section used to install: `pip3 install --user west pyelftools`, a venv
# fallback when PEP 668 refused, `catkin_pkg`/`empy`/`lark`/`tomli` into that
# venv, and finally Zephyr's `requirements-base.txt`. It is now a CHECK.
#
# Provisioning a Python environment is a distro-by-distro problem this project
# is in no position to solve: PEP 668 externally-managed interpreters refuse
# `pip --user` outright, the remedy differs across Arch/Fedora/Debian/Ubuntu,
# `python3-venv` is a separate package on Debian, and a venv must either inherit
# system site-packages or shadow the other tools the tree calls. Attempting it
# anyway meant up to three interpreters were in play (system, `--user`, fallback
# venv) with the script choosing between them silently — so "setup succeeded"
# did not say which Python `west build` would later use.
#
# What the project can own is a precise report. `scripts/check-python-deps.py`
# names the interpreter, the missing imports and their pip names, and leaves the
# choice of distro package / `--user` / venv to whoever owns the host.
#
# `activate.sh` and `activate.fish` ALREADY have the right shape here: they put
# `scripts/zephyr/.venv/bin` on PATH *if it exists* and create nothing. So a
# venv at that path is adopted automatically — which is why the remediation
# below names it.
log_info "Checking Python tools (nano-ros does not install them)..."

# Same resolver the lanes use, so this reports on the interpreter that will
# actually run `west build` (scripts/build/zephyr-python.sh).
# shellcheck source=scripts/build/zephyr-python.sh
source "$NANO_ROS_ROOT/scripts/build/zephyr-python.sh"
NROS_PY="$(nros_zephyr_python)"
[ -n "$NROS_PY" ] || NROS_PY="$(command -v python3 || true)"
if [ -z "$NROS_PY" ]; then
    log_error "no python3 on PATH — Zephyr's build scripts are Python"
    exit 1
fi

if ! python3 "$NANO_ROS_ROOT/scripts/check-python-deps.py" --python "$NROS_PY" \
        west zephyr-build; then
    log_error "Python prerequisites missing — see the report above."
    echo "" >&2
    echo "  This script deliberately does not install them. A venv at" >&2
    echo "  scripts/zephyr/.venv is picked up automatically by activate.sh:" >&2
    echo "" >&2
    echo "      python3 -m venv --system-site-packages scripts/zephyr/.venv" >&2
    echo "      scripts/zephyr/.venv/bin/pip install west pyelftools PyYAML pykwalify packaging" >&2
    echo "      source ./activate.sh" >&2
    echo "" >&2
    echo "  Then re-run this script. Use NROS_PYTHON=<path> to point it at a" >&2
    echo "  different interpreter." >&2
    exit 1
fi

# issue 0698 follow-up — the Zephyr venv is this lane's, not the session's.
source "$NANO_ROS_ROOT/scripts/build/zephyr-python.sh"
nros_zephyr_activate
if command -v west &> /dev/null; then
    log_success "west present: $(west --version)"
else
    log_error "west imports but is not on PATH — add its bin dir (e.g. \`source ./activate.sh\`)"
    exit 1
fi

# =============================================================================
# Install Rust Embedded Targets
# =============================================================================

log_info "Installing Rust embedded targets..."

rustup target add thumbv7m-none-eabi 2>/dev/null || true
rustup target add thumbv7em-none-eabi 2>/dev/null || true
rustup target add thumbv7em-none-eabihf 2>/dev/null || true
rustup target add x86_64-unknown-none 2>/dev/null || true

log_success "Rust embedded targets ready"

# =============================================================================
# Download and Install Zephyr SDK
# =============================================================================

SDK_PATH="$SDK_INSTALL_DIR/zephyr-sdk-$ZEPHYR_SDK_VERSION"



provision_sdk_via_nros() {
    # issue 0610 — the archive is host-keyed, so ASK THE INDEX rather than
    # composing a URL here. `[tool.zephyr-sdk]` carries a `dist.<host>` row per
    # host and `nros setup` picks the one matching `host_key()`, downloads,
    # verifies the sha256 and unpacks it. The tarball has no top-level `bin/`,
    # and `tar -xf` is run without `--strip-components`, so this lands exactly
    # where the rest of this script expects: `$SDK_INSTALL_DIR/zephyr-sdk-<ver>`.
    #
    # This replaces a hand-rolled aria2c + sha256sum + tar block whose tarball
    # name and checksum were hardcoded to x86_64 — which fetched 1.3 GiB, PASSED
    # verification, and then failed inside the SDK's own installer on any other
    # host, naming neither the architecture nor the tarball.
    #
    # Trade-off, deliberate: `nros` downloads with curl where this used aria2c
    # with 16 connections, so a cold fetch is slower. Worth it — a second
    # spelling of "which SDK does this host need" is what the bug was.
    local nros_bin
    # shellcheck source=../build/cargo.sh
    source "$NANO_ROS_ROOT/scripts/build/cargo.sh"
    nros_bin="$(nros_cli_bin)"
    log_info "Provisioning Zephyr SDK $ZEPHYR_SDK_VERSION via nros (index entry: $ZEPHYR_SDK_TOOL, host-keyed dist)..."
    "$nros_bin" setup --tool "$ZEPHYR_SDK_TOOL" \
        --prefix "$SDK_INSTALL_DIR" \
        --index "$NANO_ROS_ROOT/nros-sdk-index.toml"
}

install_sdk() {
    provision_sdk_via_nros

    log_info "Running SDK setup..."
    cd "$SDK_PATH"
    # Build the `-t <target>` list dynamically so callers can add
    # ARMv8-R AArch64 (Phase 117.10) or other arch toolchains
    # without forking this script.
    SDK_ARGS=()
    for t in $ALL_TARGETS; do
        SDK_ARGS+=("-t" "$t")
    done
    log_info "  Toolchains: $ALL_TARGETS"
    ./setup.sh "${SDK_ARGS[@]}" -h -c

    log_success "Zephyr SDK installed"
}

if [ "$SKIP_SDK" = true ]; then
    log_info "Skipping SDK installation (--skip-sdk)"
elif [ -d "$SDK_PATH" ] && [ -f "$SDK_PATH/setup.sh" ]; then
    log_info "Zephyr SDK already installed at $SDK_PATH"
else
    # Download + verify + unpack is `nros setup --tool zephyr-sdk` (issue 0610);
    # `install_sdk` then runs the SDK's own registration. The local
    # download-cache/checksum dance this replaces lived here only because the
    # fetch did — nros writes a provenance marker in the prefix and skips the
    # download when the tool is already present.
    install_sdk
fi

export ZEPHYR_SDK_INSTALL_DIR="$SDK_PATH"

# =============================================================================
# Create Environment Script
# =============================================================================

create_env_script() {
    log_info "Creating environment script..."
    cat > "$WORKSPACE_DIR/env.sh" << ENVEOF
#!/bin/bash
# nros Zephyr Environment
# Usage: source zephyr-workspace/env.sh (from nros dir)
#    or: source env.sh (from workspace dir)

WORKSPACE="\$(cd "\$(dirname "\${BASH_SOURCE[0]}")" && pwd)"

# Zephyr environment
source "\$WORKSPACE/zephyr/zephyr-env.sh"

# Zephyr SDK
export ZEPHYR_SDK_INSTALL_DIR="$SDK_PATH"
export ZEPHYR_TOOLCHAIN_VARIANT=zephyr

# nros paths
export NANO_ROS_ROOT="\$WORKSPACE/$NANO_ROS_NAME"

# Local bin
export PATH="\$HOME/.local/bin:\$PATH"

echo "nros Zephyr environment ready"
echo "  ZEPHYR_BASE: \$ZEPHYR_BASE"
echo "  ZEPHYR_SDK: $SDK_PATH"
echo "  NANO_ROS_ROOT: \$NANO_ROS_ROOT"
echo ""
echo "Build example:"
echo "  cd \$WORKSPACE"
echo "  west build -b native_sim/native/64 $NANO_ROS_NAME/examples/zephyr/rust/talker -- -DCONF_FILE=\"prj.conf;prj-zenoh.conf\""
ENVEOF
    chmod +x "$WORKSPACE_DIR/env.sh"
}

# =============================================================================
# Initialize Workspace
# =============================================================================

if [ -d "$WORKSPACE_DIR/.west" ]; then
    if [ "$FORCE" = true ]; then
        log_warn "Removing existing workspace..."
        rm -rf "$WORKSPACE_DIR"
    else
        log_info "Workspace exists, updating..."
        cd "$WORKSPACE_DIR"
        west update

        log_success "Update complete"

        # Regenerate env.sh
        create_env_script

        echo ""
        log_success "Workspace ready!"
        echo ""
        echo "Usage:"
        echo "  source $WORKSPACE_DIR/env.sh"
        echo "  cd $WORKSPACE_DIR"
        echo "  west build -b native_sim/native/64 $NANO_ROS_NAME/examples/zephyr/rust/talker -- -DCONF_FILE=\"prj.conf;prj-zenoh.conf\""
        exit 0
    fi
fi

log_info "Initializing workspace..."
mkdir -p "$WORKSPACE_DIR"
cd "$WORKSPACE_DIR"

# Create manifest directory with west.yml, then replace with symlink
# (west init -l follows symlinks during init, so we copy first)
mkdir -p "$WORKSPACE_DIR/$NANO_ROS_NAME"
cp "$NANO_ROS_ROOT/$MANIFEST" "$WORKSPACE_DIR/$NANO_ROS_NAME/$MANIFEST"

# Initialize west (--mf selects the 3.7 vs 4.4 manifest)
west init -l --mf "$MANIFEST" "$WORKSPACE_DIR/$NANO_ROS_NAME"

# Replace with symlink to real nros
rm -rf "$WORKSPACE_DIR/$NANO_ROS_NAME"
ln -sf "$NANO_ROS_ROOT" "$WORKSPACE_DIR/$NANO_ROS_NAME"

log_info "Fetching Zephyr and modules (this may take a while)..."
west update

# Apply Cortex-A9 Rust patches (Phase 92.1 / 92.4 — required for
# qemu_cortex_a9 DDS interop builds; idempotent, no-op on re-runs).
# 3.7 LTS only: the Zynq-7000 SoC layout this patch targets moved in Zephyr
# 4.4 (soc/xlnx/zynq7000/xc7zxxxs/), and the 4.4 line applies its own
# line-specific patches via the `just zephyr setup` recipe. Gating here keeps
# setup.sh standalone-correct on both lines.
if [ "$MANIFEST" = "west.yml" ]; then
    log_info "Applying Cortex-A9 Rust patches..."
    bash "$NANO_ROS_ROOT/scripts/zephyr/cortex-a9-rust-patch.sh" "$WORKSPACE_DIR"
else
    log_info "Skipping Cortex-A9 Rust patch (manifest $MANIFEST is not the 3.7 line)"
fi

# Forward per-example EXTRA_CARGO_ARGS (per-RMW Cargo feature selection) into
# zephyr-lang-rust's cargo build — so non-default RMW examples compile only
# their backend. Arch/version-blind; idempotent.
log_info "Applying Rust cargo-features pass-through patch..."
bash "$NANO_ROS_ROOT/scripts/zephyr/cargo-features-patch.sh" "$WORKSPACE_DIR"

# Re-check Zephyr's Python dependencies now that the workspace exists.
#
# This used to `pip install -r requirements-base.txt` (issue 0078: base.txt
# only, because the full requirements.txt pulls `spsdk-mcu-link` on the 4.4 line
# and that ENOSPC'd a 14 GB CI container). nano-ros no longer installs Python
# packages at all — see "Check Python Tools" above for why.
#
# The check runs a SECOND time here on purpose: the first ran before `west
# update`, so `requirements-base.txt` did not exist yet and the earlier verdict
# was made against the module list this repo maintains. Now the real file is on
# disk and can be named in the remediation.
log_info "Re-checking Zephyr Python dependencies (build-only subset)..."
REQ_BASE="$WORKSPACE_DIR/zephyr/scripts/requirements-base.txt"
if ! python3 "$NANO_ROS_ROOT/scripts/check-python-deps.py" --python "$NROS_PY" \
        zephyr-build; then
    log_error "Zephyr's Python build dependencies are missing — see above."
    if [ -f "$REQ_BASE" ]; then
        echo "" >&2
        echo "  Upstream's full build-only set is:" >&2
        echo "      $REQ_BASE" >&2
        echo "  nano-ros imports a subset of it; install either." >&2
    fi
    exit 1
fi

# Create environment script
create_env_script

# =============================================================================
# Summary
# =============================================================================

echo ""
log_success "========================================"
log_success "  Workspace setup complete!"
log_success "========================================"
echo ""
echo "Workspace: $WORKSPACE_DIR"
echo "SDK:       $SDK_PATH"
echo ""
echo "Structure:"
echo "  $NANO_ROS_NAME/  -> $NANO_ROS_ROOT (symlink)"
echo "  zephyr/          - Zephyr RTOS v3.7.0"
echo "  modules/         - Zephyr modules (lang/rust, HALs)"
echo ""
echo "Next steps:"
echo ""
# Show a relative path when the workspace is in-tree, else show absolute.
if [ "$WORKSPACE_DIR" = "$IN_TREE_DEFAULT" ]; then
    REL_WS="zephyr-workspace"
else
    REL_WS="$WORKSPACE_DIR"
fi
echo "  1. Source the environment:"
echo "     source $REL_WS/env.sh"
echo ""
echo "  2. Build an example:"
echo "     cd $REL_WS"
echo "     west build -b native_sim/native/64 $NANO_ROS_NAME/examples/zephyr/rust/talker -- -DCONF_FILE=\"prj.conf;prj-zenoh.conf\""
echo ""
echo "  3. Run:"
echo "     ./build/zephyr/zephyr.exe"
echo ""
echo "  Networking: native_sim uses NSOS on the host loopback — no TAP"
echo "  bridge or sudo required. Start zenohd/MicroXRCEAgent on 127.0.0.1."
echo ""
