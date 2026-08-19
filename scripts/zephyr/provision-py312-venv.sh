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
# Usage: provision-py312-venv.sh <workspace-dir>
set -euo pipefail

WS="${1:?usage: provision-py312-venv.sh <workspace-dir>}"
VENV="$WS/.venv312"
PY="$VENV/bin/python"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"

remedy() {
    cat >&2 <<EOF

  nano-ros does not create this venv. Either of these makes one:

      uv venv --python 3.12 "$VENV"
      "$VENV/bin/python" -m pip install west pyelftools PyYAML pykwalify packaging

  or, with a python3.12 already on PATH:

      python3.12 -m venv "$VENV"
      "$VENV/bin/python" -m pip install west pyelftools PyYAML pykwalify packaging

  4.4 builds run west THROUGH it:
      $VENV/bin/python -m west build ...    (or prepend $VENV/bin to PATH)
EOF
}

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
