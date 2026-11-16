#!/usr/bin/env bash
# Provision what a gate lane CLAIMS beyond the CI image (phase-543;
# `.config/gate-lanes.toml` [lane.<name>]), for every job that runs that lane.
#
# ONE script, driven by the lane's own declaration, so a job cannot provide
# less than its lane claims without this script being wrong for every job at
# once. Two shapes of drift came before it:
#
#   * issue 1483: the fast lane's provisioning was a step in two jobs, and the
#     rosidl clone reached only one;
#   * issue 2382: the `build` lane claims `submodules` and `git-history`, and the
#     jobs that run it (gate.yml's build-tier shards, host-tests' tier-1 gates)
#     provided neither, so from phase-543 W5/W6 (2026-11-07) every one of them
#     stopped in the lane preflight and no gate ran.
#
# What each claim provisions, and the code that does it, is shared with
# `just setup-worktree` (phase-565 W2): `[provision.claim]` in
# .config/gate-lanes.toml says which claims nano-ros installs, and
# scripts/lib/claim-provision.sh holds one `claim_<name>` per provisioner.
# This script runs that library for a CI lane (`--ci`): submodules at depth 1,
# and the `cli` claim left to the job's own `setup-cli` step. Every other claim
# (cmake, cc, ros2, ...) is the image's, and the lane preflight checks it.
#
# Usage: scripts/ci/provision-gate-lane.sh <lane>   (from the checkout root)
set -euo pipefail
lane="${1:?usage: provision-gate-lane.sh <lane>}"
cd "$(dirname "$0")/../.."
exec bash scripts/lib/claim-provision.sh "$lane" --ci
