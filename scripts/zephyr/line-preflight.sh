#!/usr/bin/env bash
# Check a Zephyr line's host preconditions BEFORE anything is fetched (issue 2902).
#
# The 4.4 line needs a Python >=3.12 venv at `<workspace>/.venv312`. The patch
# set (`patches/4.4.sh`) checks for it, but the patch set runs after
# `west init` and the whole `west update`, so a host without the venv paid a
# ~6 min fetch and was left with a fetched, unpatched workspace before it heard
# about a precondition that needs nothing from the fetch. Both entry points,
# `just zephyr setup` and `scripts/zephyr/setup.sh` run on its own, call this
# first.
#
# The line is named by its west MANIFEST, the one per-line fact both callers
# hold before the tree exists (the tree's own Python floor,
# `nros_zephyr_python_floor`, cannot be read until it is fetched).
#
# Creating the venv stays opt-in, exactly as in the patch set:
# `NROS_ZEPHYR_CREATE_VENV=1` passes `--create` (see provision-py312-venv.sh for
# why nano-ros does not choose a host's Python environment by default).
#
# Usage: line-preflight.sh <manifest> <workspace-dir>
set -euo pipefail

MANIFEST="${1:?usage: line-preflight.sh <manifest> <workspace-dir>}"
WS="${2:?usage: line-preflight.sh <manifest> <workspace-dir>}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"

case "$MANIFEST" in
    west-4.4.yml)
        create=""
        [ "${NROS_ZEPHYR_CREATE_VENV:-0}" = "1" ] && create="--create"
        echo "[zephyr setup] 4.4 line: checking the Python 3.12 venv before fetching${create:+ (creating it if missing)}..."
        bash "$here/provision-py312-venv.sh" "$WS" $create
        ;;
    *) ;;
esac
