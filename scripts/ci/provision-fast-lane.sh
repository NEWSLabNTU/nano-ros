#!/usr/bin/env bash
# Provision what the `fast` gate lane claims beyond the ci-base image
# (phase-543; `.config/gate-lanes.toml` [lane.fast]).
#
# ONE copy for every job that runs the `fast` lane. gate.yml carried this as a
# step in two jobs (`check-fast` and the main-only `warm-cache`), and they
# drifted: issue 1483 added the rosidl clone to the first only, so once a skip
# became a red under the gate harness, `python-entry-provenance` failed on
# main in the warm job and nowhere else (run 42346185440).
#
#   * the submodules the fast tier reads (the `# check::fast` rows of
#     `.config/worktree-provisioning.txt`, the same rows the lane preflight
#     checks for its `submodules` claim);
#   * Zephyr's C++ minimal-libc headers, as a sparse clone (`zephyr-tree`);
#   * the pinned rosidl source clone (`sources`; `python-entry-provenance`).
#
# Usage: scripts/ci/provision-fast-lane.sh   (from the checkout root)
set -euo pipefail
cd "$(dirname "$0")/../.."

mapfile -t subs < <(grep -E '# check::fast' .config/worktree-provisioning.txt \
    | grep -v '^[[:space:]]*#' | sed -e 's/#.*//' -e 's/[[:space:]]*$//' | sed '/^$/d')
if [ "${#subs[@]}" -eq 0 ]; then
    echo "provision-fast-lane: no \`# check::fast\` rows in .config/worktree-provisioning.txt" >&2
    exit 1
fi
git submodule update --init --depth 1 "${subs[@]}"

rev="$(awk '/- name: zephyr$/ { z = 1 } z && /revision:/ { print $2; exit }' west.yml)"
[ -n "$rev" ] || { echo "provision-fast-lane: no zephyr revision in west.yml" >&2; exit 1; }
if [ ! -d zephyr-workspace/zephyr/lib/cpp/minimal/include ]; then
    git clone --quiet --depth 1 --filter=blob:none --sparse --branch "$rev" \
        https://github.com/zephyrproject-rtos/zephyr.git zephyr-workspace/zephyr
    git -C zephyr-workspace/zephyr sparse-checkout set lib/cpp/minimal/include
fi

# shellcheck disable=SC1091
source ./activate.sh
nros setup --source rosidl
