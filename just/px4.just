set working-directory := '..'

default:
    @just --list --justfile {{ source_file() }}

# Symlink third-party/px4-rs → $PX4_RS_DIR (default: ~/repos/px4-rs).
# px4-rs is not packaged on crates.io yet; the symlink keeps Cargo path
# dependencies stable while letting users override the source location.
setup:
    #!/usr/bin/env bash
    set -e
    SRC="${PX4_RS_DIR:-$HOME/repos/px4-rs}"
    DST="third-party/px4-rs"
    if [ ! -d "$SRC/crates/px4-uorb" ]; then
        echo "ERROR: px4-rs not found at $SRC"
        echo "Set PX4_RS_DIR or clone https://github.com/.../px4-rs to $SRC"
        exit 1
    fi
    if [ -L "$DST" ] && [ "$(readlink "$DST")" = "$SRC" ]; then
        echo "px4-rs already linked: $DST -> $SRC"
        exit 0
    fi
    ln -sfn "$SRC" "$DST"
    echo "Linked $DST -> $SRC"

# Diagnose px4-rs install status.
doctor:
    #!/usr/bin/env bash
    set +e
    fail=0
    DST="third-party/px4-rs"
    if [ -L "$DST" ] && [ -d "$DST/crates/px4-uorb" ]; then
        TARGET=$(readlink "$DST")
        echo "  [OK] px4-rs symlink: $DST -> $TARGET"
    else
        echo "  [MISSING] $DST symlink (run: just px4 setup)"
        fail=1
    fi
    if [ -d "${PX4_AUTOPILOT_DIR:-}/Tools/setup" ]; then
        echo "  [OK] PX4-Autopilot: $PX4_AUTOPILOT_DIR"
    else
        echo "  [INFO] PX4-Autopilot not set — SITL integration tests will be skipped (set PX4_AUTOPILOT_DIR)"
    fi
    exit $fail

# Build the px4 RMW + board crates (host-mock std mode).
build:
    cargo build -p nros-rmw-uorb -p nros-px4 --features std

# Check formatting + clippy on px4 crates.
check:
    cargo fmt --check -p nros-rmw-uorb -p nros-px4
    cargo clippy -p nros-rmw-uorb -p nros-px4 --features std --all-targets -- -D warnings

# Run unit tests against the px4-rs std mock broker (no SITL required).
test:
    cargo test -p nros-rmw-uorb --features std --lib

# Run E2E test against PX4 SITL. Requires PX4_AUTOPILOT_DIR.
test-sitl:
    #!/usr/bin/env bash
    set -e
    if [ -z "${PX4_AUTOPILOT_DIR:-}" ]; then
        echo "ERROR: PX4_AUTOPILOT_DIR must be set for SITL tests"
        exit 1
    fi
    cargo nextest run -p nros-tests --features px4-sitl --test px4_e2e

# Combined CI: check + unit tests (SITL gated separately).
ci: check test
