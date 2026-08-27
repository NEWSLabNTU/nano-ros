#!/usr/bin/env bash
#
# Make a self-hosted runner's LABELS true.
# phase-395 W6; design: docs/development/multi-agent-ci-workflow.md.
#
# WHY THIS IS A THIN CALLER AND NOT A PROVISIONER.
#
# Everything a runner needs, a contributor also needs, and this repo already has
# one way to install each of them: `nros setup` driven by `nros-sdk-index.toml`,
# reached through `just setup <platform>` / `just workspace <step>`. This script
# adds NO new provisioning logic. It maps label -> the existing verb and runs it.
#
# That is the whole design intent, from the doc:
#
#   "make the labels true - install the Zephyr SDK, QEMU, ROS 2, toolchains -
#    reusing `nros setup` so a runner and a contributor provision the same way"
#
# A second installation path is how a runner ends up with a subtly different
# toolchain from every developer, and then the runner's red is unreproducible
# locally — which is exactly the state phase-395 W0 found CI in. The repo has
# already paid for this lesson at smaller scale: issue 0833 (an installer list
# and a checker list that drifted), issue 0500 (two provisioning paths landing
# at one prefix), issue 0610 (a hand-rolled SDK fetch that hardcoded x86_64).
#
# WHAT IT WILL NOT DO
#
# It never runs `sudo` and never installs a system package. Two reasons, both
# load-bearing:
#
#   * this repo's convention (phase-327 W2 / issue 0368 F1) is that every
#     sudo-less installer runs FIRST and the system-package step only PRINTS —
#     one sudo failure used to cascade and abort the rest;
#   * ROS 2 cannot be installed any other way, so pretending otherwise would
#     make `nros-ros2` a label this script claims to provision and does not.
#     It prints the exact command and fails honestly instead.
#
# usage:
#   scripts/ci/runner-provision.sh <labels> [--check] [--no-base] [--no-verify]
#
#   --check      print the plan and the CURRENT doctor verdict; change nothing.
#                Exit 0 = nothing to do, 1 = provisioning is needed.
#   --no-base    skip `just setup base` (host toolchains, cargo tools, the nros
#                CLI). Only for a machine you know already has it.
#   --no-verify  skip the closing runner-doctor run. Discouraged: the point of
#                ending with a verification is that "provisioned" and "actually
#                has it" cannot drift.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# The label vocabulary and every capability probe come from runner-doctor.sh.
# Sourced, not re-implemented — see issue 0833 in that file's header for what a
# second copy of a list costs.
# shellcheck source=scripts/ci/runner-doctor.sh
. "$repo_root/scripts/ci/runner-doctor.sh"

CHECK=0
DO_BASE=1
DO_VERIFY=1
labels=""

for arg in "$@"; do
    case "$arg" in
        --check|--dry-run) CHECK=1 ;;
        --no-base)         DO_BASE=0 ;;
        --no-verify)       DO_VERIFY=0 ;;
        -h|--help)
            sed -n '2,45p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        -*)
            echo "runner-provision: unknown option '$arg'" >&2
            exit 2
            ;;
        *)
            if [ -n "$labels" ]; then labels="$labels,$arg"; else labels="$arg"; fi
            ;;
    esac
done

if [ -z "$labels" ]; then
    echo "runner-provision: usage: scripts/ci/runner-provision.sh <labels> [--check]" >&2
    echo "  Known labels: $NROS_RUNNER_LABELS" >&2
    exit 2
fi

cd "$repo_root"

# --- the plan ----------------------------------------------------------------
#
# Held as data rather than executed inline so `--check` prints EXACTLY what the
# real run would do. A dry-run that describes the plan in prose while the real
# run does something else is worse than no dry-run: it is a second spelling.
PLAN=()
_plan() { PLAN+=("$*"); }

_plan_for_label() {
    case "$1" in
        nros-sdk-zephyr)
            # `just setup zephyr` == `just setup-cli && just zephyr setup`:
            # provisions the sources via `nros setup --source`, fetches the
            # host-keyed SDK via `nros setup --tool zephyr-sdk` (issue 0610),
            # runs the SDK's own registration, applies the per-line patch set,
            # and adds the armv7a Rust target. All of that is already one verb;
            # re-deriving any part of it here would be a second spelling.
            _plan "just setup zephyr"
            ;;
        nros-qemu)
            # Three verbs, because "QEMU + the RTOS toolchains" is three things
            # and each has its own index entry:
            #   qemu             -> patched qemu-system-arm + arm-none-eabi-gcc
            #                       + the bare-metal zenoh-pico archive
            #   threadx_riscv64  -> qemu-system-riscv64 + riscv-none-elf-gcc
            #                       (the xPack dist; Ubuntu's package has a
            #                        different prefix entirely — issue 0657)
            #   rust-targets     -> every cross triple in config/rust-targets.txt
            # The third is separate on purpose: it is the one runner-doctor
            # asserts from a shared list, and issue 0833 is what happens when
            # the installer and the checker do not read the same file.
            _plan "just setup qemu"
            _plan "just setup threadx_riscv64"
            _plan "just workspace rust-targets"
            ;;
        nros-ros2)
            # Deliberately empty. See `_provision_ros2_notice` below.
            ;;
        nros-big)
            # Nothing to install: the label is a hardware fact. runner-doctor
            # verifies it; there is no verb that can make a 4-core box big.
            ;;
        *)
            : # unknown labels are runner-doctor's problem, not this script's
            ;;
    esac
}

_provision_ros2_notice() {
    # Only say it when it is TRUE. A host that already has ROS gets a
    # three-paragraph install lecture otherwise, and advice that fires whether
    # or not it applies is advice people learn to page past — which is how the
    # one case where it mattered goes unread.
    if NROS_RUNNER_QUIET=1 nros_runner_check_label nros-ros2 >/dev/null 2>&1; then
        echo "runner-provision: nros-ros2 already holds — no operator action needed."
        return 0
    fi
    cat >&2 <<'EOF'

  nros-ros2 cannot be provisioned by this script.

  ROS 2 is a system package. Installing it needs root, and this repo's
  convention (phase-327 W2 / issue 0368 F1) is that no provisioning step here
  runs sudo — one sudo failure used to abort every sudo-less step after it.

  On Ubuntu, as the operator:
      sudo apt install ros-<distro>-desktop ros-<distro>-rmw-zenoh-cpp
      # rmw-zenoh-cpp is NOT optional: without it every zenoh interop lane
      # reports [SKIPPED:capability], which reads as green rather than as
      # absent coverage.

  Then put ROS_DISTRO in the runner's own environment (its `.env` file), so
  jobs and `runner-doctor.sh` resolve the same install.

  Not on Ubuntu? docs/development/ros2-on-non-ubuntu.md — and read its warning
  first: when the distrobox is in play, EVERY job runs in the box on its OWN
  tree (issue 0759).

  Composed install command for this host:  nros setup --system
EOF
}

# --- build the plan ----------------------------------------------------------

wants_ros2=0
while read -r label; do
    [ -n "$label" ] || continue
    if ! nros_runner_label_known "$label"; then
        # Not fatal here — runner-doctor decides whether an unknown label is a
        # typo (`nros-*`) or somebody's site label. Provisioning simply has
        # nothing to do for it.
        continue
    fi
    if [ "$label" = "nros-ros2" ]; then
        wants_ros2=1
    fi
    _plan_for_label "$label"
done < <(nros_runner_labels_split "$labels")

# `just setup base` first, always (unless refused): it builds the in-tree `nros`
# CLI, and every platform verb below shells `nros setup …`. Ordering is not
# cosmetic — a platform recipe that runs before the CLI exists fails with an
# error naming neither.
base_plan=()
if [ "$DO_BASE" -eq 1 ]; then
    base_plan=("just setup base")
fi

# --- report ------------------------------------------------------------------

echo "runner-provision: labels: $labels"
echo "runner-provision: checkout: $repo_root"
echo ""
echo "Plan:"
if [ "${#base_plan[@]}" -gt 0 ]; then
    for cmd in "${base_plan[@]}"; do echo "    $cmd"; done
fi
if [ "${#PLAN[@]}" -gt 0 ]; then
    for cmd in "${PLAN[@]}"; do echo "    $cmd"; done
else
    echo "    (nothing to install for these labels)"
fi
# `&& …` with no `|| true` would be a non-zero final status under `set -e` and
# would end the script here whenever the label is absent. Written as an `if`
# throughout for that reason — the one-liner form has bitten this repo before.
if [ "$wants_ros2" -eq 1 ]; then
    echo "    (nros-ros2: operator action — see below)"
fi
echo ""

if [ "$CHECK" -eq 1 ]; then
    echo "runner-provision: --check — nothing above was run."
    echo ""
    echo "Current state:"
    # Read-only. Its exit code becomes ours, so `--check` answers the useful
    # question ("is provisioning needed?") rather than merely echoing a plan.
    if nros_runner_doctor "$labels"; then
        echo "runner-provision: --check — every label already holds."
        if [ "${#base_plan[@]}" -gt 0 ]; then
            echo "  (a real run would still do \`just setup base\`; it is idempotent and is"
            echo "   what keeps the host toolchains and the in-tree nros CLI current.)"
        fi
        exit 0
    fi
    if [ "$wants_ros2" -eq 1 ]; then
        _provision_ros2_notice
    fi
    echo ""
    echo "runner-provision: --check — provisioning IS needed (see the MISSING lines above)." >&2
    exit 1
fi

# --- run ---------------------------------------------------------------------

if ! command -v just >/dev/null 2>&1; then
    echo "runner-provision: \`just\` is not on PATH, and every step below is a just recipe." >&2
    echo "  A fresh host bootstraps it without one:" >&2
    echo "      $repo_root/scripts/bootstrap.sh" >&2
    exit 1
fi

# The sweep contract (CLAUDE.md): every `just <plat>` invocation wants
# `source ./activate.sh` first — it is the env/PATH SSoT and wires `nros` and
# `play_launch_parser`. It installs nothing and never errors by design, but it
# is sourced with the strict flags off anyway: an `unbound variable` inside
# somebody else's file must not abort provisioning.
if [ -r "$repo_root/activate.sh" ]; then
    echo "runner-provision: sourcing ./activate.sh (env/PATH SSoT)"
    set +eu
    # shellcheck source=/dev/null
    . "$repo_root/activate.sh"
    set -eu
fi

_run() {
    echo ""
    echo "runner-provision: + $*"
    # Not `set -e`-fatal on its own: the failure is reported with the label
    # context, which a bare non-zero exit from a nested `just` does not carry.
    if ! eval "$*"; then
        echo "runner-provision: FAILED: $*" >&2
        return 1
    fi
}

failed=()
if [ "${#base_plan[@]}" -gt 0 ]; then
    for cmd in "${base_plan[@]}"; do
        _run "$cmd" || failed+=("$cmd")
    done
fi
if [ "${#PLAN[@]}" -gt 0 ]; then
    for cmd in "${PLAN[@]}"; do
        _run "$cmd" || failed+=("$cmd")
    done
fi

if [ "$wants_ros2" -eq 1 ]; then
    _provision_ros2_notice
fi

echo ""
if [ "${#failed[@]}" -gt 0 ]; then
    echo "runner-provision: ${#failed[@]} step(s) failed:" >&2
    for cmd in "${failed[@]}"; do echo "    $cmd" >&2; done
    echo "  Re-run them individually — each is an ordinary contributor verb." >&2
    exit 1
fi

# --- verify ------------------------------------------------------------------
#
# Provisioning ends by asserting the labels, so "I ran the installer" and "the
# host has it" cannot be different answers. Half this repo's gate history is
# that gap: an installer that succeeded while the thing it installed was absent,
# unregistered, or in a second location nobody checked.
if [ "$DO_VERIFY" -eq 1 ]; then
    echo ""
    echo "runner-provision: verifying the labels it just provisioned..."
    if ! nros_runner_doctor "$labels"; then
        echo "" >&2
        echo "runner-provision: the provisioning steps SUCCEEDED but the labels do not hold." >&2
        echo "  That gap is the bug this closes — do not register this runner." >&2
        exit 1
    fi
fi

echo "runner-provision: OK — $labels"
