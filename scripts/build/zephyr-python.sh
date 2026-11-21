# The interpreter Zephyr's tooling runs under — resolved when a Zephyr lane
# needs it, never for the whole session.
#
# WHY THIS IS NOT IN activate.sh ANY MORE
#
# `activate.sh` used to prepend `scripts/zephyr/.venv/bin` to PATH whenever that
# directory held an executable `west`. Two things were wrong with it.
#
# 1. SCOPE. The venv exists for one lane — it provides `west` and Zephyr's build
#    imports — but prepending it to PATH replaces `python3` for EVERYTHING in
#    the session: all 37 `check-*.py` gates, colcon, rosidl_adapter, the
#    cyclonedds descriptor codegen. The 4.4 line already knew this and worked
#    around it by invoking west through the venv interpreter explicitly rather
#    than prepending (just/zephyr-dev.just), precisely because the ROS msg2idl
#    step needs the SYSTEM ROS python and must not see the venv. The 3.7 line
#    did the thing 4.4 refused to do, globally.
#
# 2. THE TEST WAS PRESENCE, NOT USABILITY — `[ -x .venv/bin/west ]`. A venv is
#    not portable: its `bin/python3` is a SYMLINK to a system interpreter and
#    its packages sit in `lib/python3.<minor>/site-packages`. Copy the tree to a
#    host whose python has a different minor version and the symlink still
#    resolves, the file is still executable, and the packages are invisible.
#    Measured in the ROS distrobox against the mirrored checkout: the venv was
#    built by Arch python 3.14, the symlink resolved to Ubuntu's 3.10, and
#    `activate.sh` happily put it first on PATH — so `west --version` died with
#    `ImportError: from west.app.main import main`, and every `python3` in that
#    shell was the venv shim. Presence said yes; usability said no.
#
# So: resolve on demand, and decide by ASKING the interpreter, not by stat().

# nros_zephyr_workspace_venv <workspace>
#
# The venv a Zephyr WORKSPACE carries for its own line, whether or not it
# exists. One spelling: `scripts/zephyr/provision-py312-venv.sh` creates it,
# and every reader asks here instead of joining the name by hand.
nros_zephyr_workspace_venv() {
    printf '%s' "${1:?usage: nros_zephyr_workspace_venv <workspace>}/.venv312"
}

# nros_zephyr_python_floor [<workspace>]
#
# The oldest Python the line's OWN Zephyr tree accepts, read from the
# `PYTHON_MINIMUM_REQUIRED` its `cmake/modules/python.cmake` sets (3.10 on the
# 3.7 line, 3.12 on 4.4). Read, never authored: a table of floors per line
# would be a second statement of what the tree already says, and wrong on the
# next line. Prints nothing when the tree cannot be found or states none.
nros_zephyr_python_floor() {
    local ws="${1:-}" f
    [ -n "$ws" ] || ws="$(nros_zephyr_python_line_workspace)"
    f="$ws/zephyr/cmake/modules/python.cmake"
    [ -f "$f" ] || return 0
    sed -n 's/^[[:space:]]*set(PYTHON_MINIMUM_REQUIRED[[:space:]]\{1,\}\([0-9][0-9.]*\)).*/\1/p' "$f" | head -n 1
}

# The selected line's workspace, through the one resolver ladder
# (scripts/lib/zephyr-workspace.sh). Empty when none is provisioned.
nros_zephyr_python_line_workspace() {
    local root
    root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
    "$root/scripts/lib/zephyr-workspace.sh" --version "${NROS_ZEPHYR_VERSION:-3.7}" resolve 2>/dev/null || true
}

# nros_zephyr_python [<workspace>]
#
# Prints the interpreter a Zephyr build should run under, or nothing when none
# is usable. <workspace> defaults to the selected line's
# (`NROS_ZEPHYR_VERSION`, 3.7 when unset). Order:
#
#   NROS_PYTHON               the one knob, honoured everywhere (the checker,
#                             `just doctor`, `scripts/zephyr/setup.sh`)
#   <workspace>/.venv312      the venv the workspace carries for its line
#   scripts/zephyr/.venv      the conventional in-repo venv
#   python3 on PATH           the user's own environment, which is where this
#                             belongs by default
#
# A candidate is taken only when it can import west AND is at least the
# line's floor (`nros_zephyr_python_floor`) — usability, never presence. The
# floor is what lets one resolver serve both lines: the 4.4 tree needs 3.12,
# so a 3.10 system python3 that imports west is passed over there and taken on
# 3.7. Before the floor was asked, the 4.4 line was answered by a just variable
# keyed on the version string, and a recipe that used this resolver instead
# configured 4.4 under 3.10 and stopped at `find_package(Python3)`.
#
# nano-ros never creates any of these. `scripts/check-python-deps.py` says what
# is missing; choosing between a distro package, `pip --user` and a venv is a
# decision about the host.
nros_zephyr_python() {
    local root ws floor cand

    # An explicitly-set NROS_PYTHON WINS, usable or not. Falling through to
    # something else because the named interpreter cannot import west would
    # substitute silently — and "which interpreter is this actually running
    # under" is the question this whole file exists to keep answerable. If it
    # is the wrong one, the caller's `command -v west` guard skips and
    # `scripts/check-python-deps.py` names what is missing.
    if [ -n "${NROS_PYTHON:-}" ]; then
        printf '%s' "$NROS_PYTHON"
        return 0
    fi

    root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
    ws="${1:-}"
    [ -n "$ws" ] || ws="$(nros_zephyr_python_line_workspace)"
    floor=""
    [ -z "$ws" ] || floor="$(nros_zephyr_python_floor "$ws")"

    for cand in \
        "${ws:+$(nros_zephyr_workspace_venv "$ws")/bin/python3}" \
        "$root/scripts/zephyr/.venv/bin/python3" \
        "$(command -v python3 2>/dev/null || true)"; do
        [ -n "$cand" ] || continue
        [ -x "$cand" ] || continue
        # Usability, not presence: a venv copied between hosts passes `-x` and
        # still cannot import its own packages; and an interpreter below the
        # tree's floor imports west and still cannot configure it.
        if "$cand" -c '
import sys
import west  # noqa: F401
floor = sys.argv[1]
if floor:
    want = tuple(int(x) for x in floor.split("."))
    sys.exit(0 if sys.version_info[:len(want)] >= want else 1)
' "$floor" >/dev/null 2>&1; then
            printf '%s' "$cand"
            return 0
        fi
    done
    return 0
}

# nros_zephyr_activate
#
# Put the resolved interpreter's bin directory FIRST on PATH, for this process
# only. Call it at the top of a lane that shells `west`; do not call it from a
# shell profile — that is the scope bug this file replaces.
#
# Returns 0 even when nothing resolved, because every caller already guards on
# `command -v west` and reports its own skip (issue 0650's protocol). Silent so
# it can be called unconditionally.
nros_zephyr_activate() {
    local py bindir
    py="$(nros_zephyr_python)"
    [ -n "$py" ] || return 0
    bindir="$(dirname "$py")"
    case ":$PATH:" in
        *":$bindir:"*) ;;
        *) PATH="$bindir:$PATH"; export PATH ;;
    esac
}
