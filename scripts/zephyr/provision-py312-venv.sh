#!/usr/bin/env bash
# CHECK (no longer provision) the Python 3.12 venv the Zephyr 4.4 line needs.
#
# Zephyr 4.4's `find_package(Python3)` requires >=3.12; the 3.7 LTS line is
# happy on 3.10, so this is 4.4-only. The venv also has to be separate from the
# 3.7 line's interpreter, or the two lines' Zephyr dependency sets collide.
#
# This script used to CREATE that venv: `uv venv --python 3.12`, then
# `uv pip install west pyelftools` and the whole of `requirements.txt`. It no
# longer installs anything. nano-ros does not provision Python environments —
# PEP 668, `--user` vs venv vs pipx, distro-specific package names and
# `python3-venv` being a separate package make it a host decision, and guessing
# it wrong hides the consequence until a build fails somewhere unrelated. The
# same change was made to `scripts/zephyr/setup.sh`.
#
# What is left is the part worth keeping: saying exactly what the 4.4 line needs
# and where, so a host can be made ready in one obvious step.
#
# `--create` is the ONE exception, and it is for a host that owns its own
# interpreter: a CI container. There the workflow IS the host, the image ships
# `uv` for exactly this ("uv (Python 3.12 for the Zephyr 4.4 line)"), and the
# alternative is pasting this venv path and package list into three workflow
# jobs — a second copy of the list that drifts from
# `scripts/check-python-deps.py`, which is the single source. It stays OPT-IN,
# so a developer machine still gets the refusal and the remedy above.
#
# Without it the 4.4 line could not set up at all: nothing in CI ever created
# the venv, so every 4.4 cell died at `Set up Zephyr 4.4 workspace` from the day
# provisioning was removed (2026-08-19), and twelve cells reported nothing.
#
# Usage: provision-py312-venv.sh <workspace-dir> [--create]
set -euo pipefail

WS="${1:?usage: provision-py312-venv.sh <workspace-dir> [--create]}"
CREATE=0
[ "${2:-}" = "--create" ] && CREATE=1
VENV="$WS/.venv312"
PY="$VENV/bin/python"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"

remedy() {
    cat >&2 <<EOF

  nano-ros does not create this venv. Either of these makes one:

      uv venv --python 3.12 "$VENV"
      "$VENV/bin/python" -m pip install west pyelftools PyYAML pykwalify packaging jsonschema

  or, with a python3.12 already on PATH:

      python3.12 -m venv "$VENV"
      "$VENV/bin/python" -m pip install west pyelftools PyYAML pykwalify packaging jsonschema

  4.4 builds run west THROUGH it:
      $VENV/bin/python -m west build ...    (or prepend $VENV/bin to PATH)
EOF
}

if [ ! -x "$PY" ] && [ "$CREATE" = 1 ]; then
    echo "[py312] --create: no venv at $VENV; making one."
    # The package list comes from `check-python-deps.py`, the single source the
    # checker below reads too — never a second copy in this file or in YAML.
    # NOTE `--list` ignores its group arguments and prints every group, so the
    # filtering is here: the header line names the group, and the block's SECOND
    # indented line is the packages.
    pkgs="$(python3 "$here/scripts/check-python-deps.py" --list \
        | awk 'BEGIN{split("west zephyr-build",w," "); for(i in w) want[w[i]]=1}
               /^[^ ]/ {g=$1; blk=0; next}
               {blk++; if (blk==2 && (g in want)) print}')"
    if [ -z "$pkgs" ]; then
        echo "ERROR: could not read the package list from check-python-deps.py --list." >&2
        exit 1
    fi
    echo "[py312] installing:$(echo " $pkgs" | tr -s ' ')"

    if command -v uv >/dev/null 2>&1; then
        uv venv --python 3.12 "$VENV"
        # `uv venv` makes a venv with NO pip in it — `$PY -m pip` here fails
        # with `No module named pip`, and the failure lands mid-script where it
        # reads as a broken interpreter. `uv pip` installs into it directly.
        # shellcheck disable=SC2086
        uv pip install --python "$PY" --quiet $pkgs
    elif command -v python3.12 >/dev/null 2>&1; then
        python3.12 -m venv "$VENV"
        "$PY" -m pip install --quiet --upgrade pip
        # shellcheck disable=SC2086
        "$PY" -m pip install --quiet $pkgs
    else
        echo 'ERROR: --create needs uv or python3.12 on PATH; neither is here.' >&2
        remedy
        exit 1
    fi
fi

if [ ! -x "$PY" ]; then
    echo "ERROR: the Zephyr 4.4 line needs a Python >=3.12 venv at $VENV — none found." >&2
    remedy
    exit 1
fi

ver="$("$PY" -c 'import sys; print("%d.%d.%d" % sys.version_info[:3])' 2>/dev/null || true)"
if [ -z "$ver" ]; then
    echo "ERROR: $PY exists but does not run." >&2
    remedy
    exit 1
fi
if ! "$PY" -c 'import sys; sys.exit(0 if sys.version_info[:2] >= (3, 12) else 1)'; then
    echo "ERROR: $PY is Python $ver; the Zephyr 4.4 line needs >=3.12." >&2
    remedy
    exit 1
fi

echo "[py312] interpreter: $PY (Python $ver)"

# One checker for both Zephyr lines, so "which modules does a Zephyr build
# need" has a single answer rather than one per script.
if ! python3 "$here/scripts/check-python-deps.py" --python "$PY" west zephyr-build; then
    echo "ERROR: the 4.4 venv is missing Python packages — see the report above." >&2
    remedy
    exit 1
fi

echo "[py312] ready. 4.4 builds run west THROUGH this venv:"
echo "[py312]   $VENV/bin/python -m west build ...   (or prepend $VENV/bin to PATH)"
