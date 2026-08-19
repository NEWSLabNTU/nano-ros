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

# nros_zephyr_python
#
# Prints the interpreter a Zephyr build should run under, or nothing when none
# is usable. Order:
#
#   NROS_PYTHON               the one knob, honoured everywhere (the checker,
#                             `just doctor`, `scripts/zephyr/setup.sh`)
#   scripts/zephyr/.venv      the conventional in-repo venv — used ONLY when it
#                             can actually import west, never merely because it
#                             exists
#   python3 on PATH           the user's own environment, which is where this
#                             belongs by default
#
# nano-ros never creates any of these. `scripts/check-python-deps.py` says what
# is missing; choosing between a distro package, `pip --user` and a venv is a
# decision about the host.
nros_zephyr_python() {
    local root cand

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

    for cand in \
        "$root/scripts/zephyr/.venv/bin/python3" \
        "$(command -v python3 2>/dev/null || true)"; do
        [ -n "$cand" ] || continue
        [ -x "$cand" ] || continue
        # Usability, not presence: a venv copied between hosts passes `-x` and
        # still cannot import its own packages.
        if "$cand" -c 'import west' >/dev/null 2>&1; then
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
