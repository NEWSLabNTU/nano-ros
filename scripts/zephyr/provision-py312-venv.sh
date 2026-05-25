#!/usr/bin/env bash
# Phase 180.A Task 2b — provision a Python 3.12 venv for the Zephyr 4.4
# line. Zephyr 4.4's find_package(Python3) requires >=3.12; the 3.7 LTS
# line is happy on 3.10, so this is 4.4-only. Idempotent.
#
# Why a venv (not system python): 4.4 needs 3.12 which many hosts lack,
# and installing west/zephyr deps must not collide with the 3.10 used by
# the 3.7 line. uv provides a standalone 3.12 without sudo.
#
# Usage: provision-py312-venv.sh <workspace-dir>
set -euo pipefail

WS="${1:?usage: provision-py312-venv.sh <workspace-dir>}"
VENV="$WS/.venv312"
PY="$VENV/bin/python"

if ! command -v uv >/dev/null 2>&1; then
    echo "ERROR: uv not found." >&2
    echo "  The Zephyr 4.4 line needs Python >=3.12 (host default is older)." >&2
    echo "  Install uv (https://docs.astral.sh/uv/) so it can fetch a" >&2
    echo "  standalone 3.12 without sudo, or put a python3.12 on PATH and" >&2
    echo "  create \$WS/.venv312 yourself." >&2
    exit 1
fi

if [ ! -x "$PY" ]; then
    echo "[py312] creating venv at $VENV"
    uv venv --python 3.12 "$VENV"
fi
echo "[py312] interpreter: $("$PY" --version)"

# uv venv has no pip; install into the venv via `uv pip` (NOT `pip`,
# which leaks to the host user-site — Phase 180.A Task 3 finding).
echo "[py312] installing west + pyelftools into venv"
uv pip install --python "$PY" -q west pyelftools
if [ -f "$WS/zephyr/scripts/requirements.txt" ]; then
    echo "[py312] installing Zephyr 4.4 python requirements into venv"
    uv pip install --python "$PY" -q -r "$WS/zephyr/scripts/requirements.txt"
fi

echo "[py312] west: $("$PY" -m west --version)"
echo "[py312] ready. 4.4 builds run west THROUGH this venv:"
echo "[py312]   $VENV/bin/python -m west build ...   (or prepend $VENV/bin to PATH)"
