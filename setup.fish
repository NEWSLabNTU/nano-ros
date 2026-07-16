#!/usr/bin/env fish
# nano-ros legacy activation shim (fish).
#
# DEPRECATED (issue #208): `activate.fish` is the activation SSoT
# (Phase 218.C) — this file survives only so old instructions keep working.
# It forwards to activate.fish and additionally exports the legacy NROS_ROOT
# name.
#
#   source ./activate.fish   # preferred
#   source ./setup.fish      # legacy — same effect

set -l _nros_script (status --current-filename)
set -l _nros_setup_dir (realpath (dirname $_nros_script))

source "$_nros_setup_dir/activate.fish"

# Legacy env name some older scripts/docs read; activate.fish exports
# NROS_REPO_DIR + nano_ros_ROOT.
set -gx NROS_ROOT $_nros_setup_dir
