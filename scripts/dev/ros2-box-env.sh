# Per-box environment for a ROS 2 distrobox whose $HOME is shared with the host
# (see docs/development/ros2-on-non-ubuntu.md). Source before any nano-ros
# command INSIDE the box:
#
#     distrobox enter ros2 -- bash -c '. scripts/dev/ros2-box-env.sh; <cmd>'
#
# GLIBC DIRECTION IS THE WHOLE STORY. glibc is backward compatible: a binary
# linked against the box's OLDER glibc runs on a newer host, never the reverse.
# So artifacts built IN THE BOX work on both sides, and everything the host
# built is unusable here. Each override below exists because the host may have
# already written that location.
#
#   CARGO_TARGET_DIR   A shared target dir does not merely churn — it FAILS:
#                      cargo re-runs cached build-script EXECUTABLES, and a
#                      host-built `build-script-build` dies here with
#                      `GLIBC_2.xx not found`. Verified, not assumed.
#   NROS_HOME          A shared SDK store reports the host's zenohd "present" at
#                      the version the index pins, then hands the box a binary
#                      it cannot exec. The box provisions its own.
#   CARGO_INSTALL_ROOT ~/.cargo/bin holds host-built tools (`just`), which fail
#                      the same way; keeping the box's copies elsewhere leaves
#                      the host's tools intact.
#
# Shared safely: ~/.rustup (toolchains target an old glibc) and the cargo
# registry/git caches (sources, not objects).

_nros_box_root="$(cd -P "$(dirname "${BASH_SOURCE[0]:-$0}")/../.." && pwd -P)"

export NROS_HOME="${NROS_HOME:-$HOME/.nros-box}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/.cargo-target-box}"
export CARGO_INSTALL_ROOT="${CARGO_INSTALL_ROOT:-$HOME/.local-box}"

cd "$_nros_box_root" || return 1

# `nros` discovery expects packages/cli/target/release/nros — activate.sh puts
# that directory on PATH and cmake's find_program HINTS look there directly, so
# CARGO_TARGET_DIR alone hides the box's CLI from every consumer. Publish the
# box build to that path; being the older-glibc binary it keeps working on the
# host too. A host-side `cargo build` of the CLI overwrites it with a host
# binary and breaks the box again — re-run this after that happens.
nros_box_publish() {
    local built="$CARGO_TARGET_DIR/release/nros"
    if [ ! -x "$built" ]; then
        echo "box: no CLI at $built — build it first:" >&2
        echo "  cargo build --release --manifest-path packages/cli/Cargo.toml --bin nros" >&2
        return 1
    fi
    mkdir -p "$CARGO_INSTALL_ROOT/bin" "$_nros_box_root/packages/cli/target/release"
    install -m755 "$built" "$_nros_box_root/packages/cli/target/release/nros"
    install -m755 "$built" "$CARGO_INSTALL_ROOT/bin/nros"
}

# BEFORE activate.sh: it sources scripts/sdk-env.sh, which shells out to `just`,
# and the host's ~/.cargo/bin/just would otherwise win the PATH race and print
# `GLIBC_2.xx not found`. activate.sh prepends packages/cli/target/release after
# this, which is correct — nros_box_publish put a box binary there.
export PATH="$CARGO_INSTALL_ROOT/bin:$PATH"

# shellcheck disable=SC1091
. "$_nros_box_root/activate.sh"
unset _nros_box_root
