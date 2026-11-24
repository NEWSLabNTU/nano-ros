# shellcheck shell=bash
# PROBE-OWNED prelude for the `upgrade-<lang>` tracks (issue 2912). Sourced at
# the head of the extracted script; run-bootstrap-probe.sh substitutes the
# release page's install line with a call to `probe_upgrade_before <release>`
# followed by that line, so this runs after installation.md has installed the
# candidate and before the page's Upgrade block.
#
# It makes the project a user upgrades: a linux talker in the track's
# language, scaffolded and built by the PREVIOUS release. The candidate is the
# installed front, so the project is pinned to the previous release first and
# every `nros` below hands off to it, fetching it on first use: the same
# hand-off the `pinned` tracks prove. It leaves the shell in the project,
# which is where the page's block says to run.

probe_upgrade_before() {
    local from="${1:?probe_upgrade_before: the previous release}"
    local lang="${PROBE_PLATFORM#upgrade-}"
    case "$lang" in
        rust|c|cpp) ;;
        *)
            echo "PROBE FAIL: track $PROBE_PLATFORM names no language (want upgrade-rust|upgrade-c|upgrade-cpp)" >&2
            exit 1
            ;;
    esac
    echo "=== probe upgrade: a $lang project made and built by $from ==="
    # Every first build generates messages from the reader's ROS 2 (issue
    # 2275), as the chapters' own build steps source it. The page's steps
    # inherit it from this shell, as a user's later terminal inherits the
    # configure that recorded it.
    set +u
    # shellcheck disable=SC1090
    source "/opt/ros/${ROS_DISTRO:-humble}/setup.bash" || {
        echo "PROBE FAIL: cannot source /opt/ros/${ROS_DISTRO:-humble}/setup.bash" >&2
        exit 1
    }
    mkdir -p "$HOME/upgrade_app" && cd "$HOME/upgrade_app" || exit 1
    printf '[toolchain]\nversion = "%s"\n' "$from" >nros-toolchain.toml
    nros new "up_$lang" --platform linux --lang "$lang" --rmw zenoh || {
        echo "PROBE FAIL: $from could not scaffold the $lang project" >&2
        exit 1
    }
    # A user's pin is in the project root, where its first `nros build` wrote
    # it; move it there so the page's `nros pin` moves that one.
    cd "up_$lang" || exit 1
    mv ../nros-toolchain.toml . || exit 1
    nros setup linux --rmw zenoh || {
        echo "PROBE FAIL: $from could not provision linux for the $lang project" >&2
        exit 1
    }
    nros build || {
        echo "PROBE FAIL: $from could not build its own $lang project" >&2
        exit 1
    }
    # The project builds with the candidate only once the page's `nros pin`
    # moves this pin.
    PROBE_UPGRADE_FROM="$from"
    PROBE_UPGRADE_LANG="$lang"
    PROBE_UPGRADE_DIR="$PWD"
    echo "=== probe upgrade: $from built up_$lang; the page's Upgrade block runs next ==="
}
