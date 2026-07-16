#!/usr/bin/env bash
# nano-ros legacy activation shim (bash / zsh).
#
# DEPRECATED (issue #208): `activate.sh` is the activation SSoT (Phase 218.C)
# — this file survives only so old instructions keep working. It forwards to
# activate.sh and additionally exports the legacy NROS_ROOT name.
#
#   source ./activate.sh     # preferred
#   source ./setup.bash      # legacy — same effect

if [ -n "${BASH_SOURCE[0]:-}" ]; then
    _nros_script="${BASH_SOURCE[0]}"
elif [ -n "${(%):-%x}" ]; then
    # zsh
    _nros_script="${(%):-%x}"
else
    echo "nano-ros setup.bash: cannot resolve script path. Source from bash or zsh." >&2
    return 1 2>/dev/null || exit 1
fi

_nros_setup_dir="$(cd "$(dirname "${_nros_script}")" && pwd)"
unset _nros_script

# shellcheck source=/dev/null
source "${_nros_setup_dir}/activate.sh"

# Legacy env name some older scripts/docs read; activate.sh exports
# NROS_REPO_DIR + nano_ros_ROOT.
NROS_ROOT="${_nros_setup_dir}"
export NROS_ROOT
unset _nros_setup_dir
