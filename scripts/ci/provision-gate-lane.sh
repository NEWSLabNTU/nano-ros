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
# Per claim:
#   submodules   the `.config/worktree-provisioning.txt` rows tagged with one
#                of the lane's tiers (`# check::fast`, `# check::build`), the
#                same rows the lane preflight checks; depth 1.
#   git-history  for the gates that ask about the COMMIT GRAPH
#                (`doc-commit-citations`, `roadmap-commit-refs`): on a shallow
#                clone a dead hash and an unfetched one look the same. In the
#                merge queue this is what catches a doc citing its own
#                branch's pre-rebase hashes. The BASE branch's commits,
#                treeless (`--filter=tree:0`):
#                measured 1.8 s and +13 MiB from a depth-1 clone, against 49 s
#                and ~1 GiB for a full unshallow (issue 1476). The base branch
#                only, never a bare `origin`: with checkout's `+refs/heads/*`
#                refspec that pulled every branch and ran 85 minutes, twice.
#   zephyr-tree  Zephyr's C++ minimal-libc headers, a sparse clone at the
#                pinned west revision (issue 2308 keeps it from reading as a
#                west workspace).
#   sources      the pinned rosidl source clone (`python-entry-provenance`).
# Every other claim (cli, cmake, cc, ros2, ...) is the image's or an earlier
# step's, and the lane preflight checks it.
#
# Usage: scripts/ci/provision-gate-lane.sh <lane>   (from the checkout root)
set -euo pipefail
lane="${1:?usage: provision-gate-lane.sh <lane>}"
cd "$(dirname "$0")/../.."

# "tiers: a b" and "claims: x y" for the lane, or a refusal naming it.
lane_decl="$(python3 - "$lane" <<'PY'
import sys
try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib
lane = sys.argv[1]
with open(".config/gate-lanes.toml", "rb") as f:
    decl = tomllib.load(f).get("lane", {}).get(lane)
if decl is None:
    sys.exit(f"provision-gate-lane: no [lane.{lane}] in .config/gate-lanes.toml")
print("tiers:", " ".join(decl.get("tiers", [])))
print("claims:", " ".join(decl.get("claims", [])))
PY
)"
read -r -a tiers <<<"$(sed -n 's/^tiers: //p' <<<"$lane_decl")"
read -r -a claims <<<"$(sed -n 's/^claims: //p' <<<"$lane_decl")"
claims_has() { local c; for c in "${claims[@]}"; do [ "$c" = "$1" ] && return 0; done; return 1; }
echo "provision-gate-lane: lane \`$lane\`, tiers: ${tiers[*]}, claims: ${claims[*]}"

if claims_has submodules; then
    tier_re="$(IFS='|'; echo "${tiers[*]}")"
    mapfile -t subs < <(grep -E "# check::(${tier_re})([^a-z-]|$)" .config/worktree-provisioning.txt \
        | grep -v '^[[:space:]]*#' | sed -e 's/#.*//' -e 's/[[:space:]]*$//' | sed '/^$/d')
    if [ "${#subs[@]}" -eq 0 ]; then
        echo "provision-gate-lane: no rows for tiers ${tiers[*]} in .config/worktree-provisioning.txt" >&2
        exit 1
    fi
    git submodule update --init --depth 1 "${subs[@]}"
fi

if claims_has git-history; then
    base="${GITHUB_BASE_REF:-main}"
    if [ "$(git rev-parse --is-shallow-repository)" = true ]; then
        # The depth-1 base fetch FIRST is load-bearing: without the base tip
        # present, git 2.34's treeless unshallow fetches trees one at a time
        # and stalls (measured: past a 240 s cap; with it, 5 s).
        git fetch --no-tags --quiet --depth=1 origin \
            "+refs/heads/$base:refs/remotes/origin/$base"
        # shellcheck disable=SC1091
        . scripts/lib/deadline.sh
        "${NROS_DEADLINE[@]}" 300 git fetch --no-tags --quiet --filter=tree:0 --unshallow \
            origin "+refs/heads/$base:refs/remotes/origin/$base"
    fi
    test "$(git rev-parse --is-shallow-repository)" = false
    echo "provision-gate-lane: history: $(git rev-list --count HEAD) commits"
fi

if claims_has zephyr-tree; then
    rev="$(awk '/- name: zephyr$/ { z = 1 } z && /revision:/ { print $2; exit }' west.yml)"
    [ -n "$rev" ] || { echo "provision-gate-lane: no zephyr revision in west.yml" >&2; exit 1; }
    if [ ! -d zephyr-workspace/zephyr/lib/cpp/minimal/include ]; then
        git clone --quiet --depth 1 --filter=blob:none --sparse --branch "$rev" \
            https://github.com/zephyrproject-rtos/zephyr.git zephyr-workspace/zephyr
        git -C zephyr-workspace/zephyr sparse-checkout set lib/cpp/minimal/include
    fi
fi

if claims_has sources; then
    # shellcheck disable=SC1091
    source ./activate.sh
    nros setup --source rosidl
fi
