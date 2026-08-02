# Per-box environment for a ROS 2 distrobox whose $HOME is shared with the host
# (see docs/development/ros2-on-non-ubuntu.md). Source before any nano-ros
# command INSIDE the box:
#
#     distrobox enter ros2 -- bash -c '. scripts/dev/ros2-box-env.sh; <cmd>'
#
# On a host with BOTH docker and podman, prefix that with
# `DBX_CONTAINER_MANAGER=docker` (or podman) — distrobox prefers podman when it
# is present, and against a docker-created box it reports `no such container`
# and offers to create an empty Fedora one instead of finding yours.
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

# distrobox mounts the whole host filesystem at /run/host AND translates the
# entry cwd into it, so the SAME checkout is reachable inside the box by two
# paths — /run/host/mnt/… and the bind-mounted /mnt/… — and which one you land
# on depends on how you entered. Either is a working tree; only one matches the
# host's own absolute path, and the mismatch is the issue-0375 hazard for real:
# `nros sync` writes absolute paths, and cargo/cmake caches key on them, so a
# box build under /run/host/… and a host build under /mnt/… silently disagree.
# Strip the prefix when the stripped path is the same checkout.
case "$_nros_box_root" in
    /run/host/*)
        _nros_box_stripped="${_nros_box_root#/run/host}"
        if [ -d "$_nros_box_stripped/packages/cli" ]; then
            _nros_box_root="$_nros_box_stripped"
            cd "$_nros_box_root" || return 1
        fi
        unset _nros_box_stripped
        ;;
esac

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
# The root is exported, NOT read from `$_nros_box_root`: that variable is unset
# at the end of this file, so a function closing over it silently resolved to
# "" and installed to /packages/cli/… — which fails, while the second install
# still returned 0 and the function reported success.
export NROS_BOX_REPO="$_nros_box_root"

nros_box_publish() {
    local built="$CARGO_TARGET_DIR/release/nros"
    if [ ! -x "$built" ]; then
        echo "box: no CLI at $built — build it first:" >&2
        echo "  cargo build --release --manifest-path packages/cli/Cargo.toml --bin nros" >&2
        return 1
    fi
    mkdir -p "$CARGO_INSTALL_ROOT/bin" "$NROS_BOX_REPO/packages/cli/target/release"
    install -m755 "$built" "$NROS_BOX_REPO/packages/cli/target/release/nros" || return 1
    install -m755 "$built" "$CARGO_INSTALL_ROOT/bin/nros" || return 1
    echo "box CLI published: $NROS_BOX_REPO/packages/cli/target/release/nros + $CARGO_INSTALL_ROOT/bin/nros"
}

# BEFORE activate.sh: it sources scripts/sdk-env.sh, which shells out to `just`,
# and the host's ~/.cargo/bin/just would otherwise win the PATH race and print
# `GLIBC_2.xx not found`. activate.sh prepends packages/cli/target/release after
# this, which is correct — nros_box_publish put a box binary there.
export PATH="$CARGO_INSTALL_ROOT/bin:$PATH"

# shellcheck disable=SC1091
. "$_nros_box_root/activate.sh"
unset _nros_box_root
