#!/usr/bin/env bash
# Clean-system bootstrap probe (issue #204).
#
# Executes the book's documented setup steps VERBATIM on a pristine
# `ubuntu:24.04` container (nothing preinstalled beyond what the book's own
# "host prerequisites" block installs), then asserts the first-node
# chapter's documented outcome. Steps are extracted from the book by
# extract-book-steps.py — the book is the single source of truth, so the
# probe cannot drift from what users actually read.
#
# Substitutions (both fail loudly if the book text drifts):
#   - the pinned release tag in the clone line -> the branch/URL under test
#   - the `nros setup <board>` placeholder     -> `native --rmw zenoh`
#
# Env knobs:
#   PROBE_CLONE_URL  clone source inside the container
#                    (default: the local checkout, mounted read-only)
#   PROBE_BRANCH     branch to clone (default: current branch)
#   PROBE_IMAGE      container image (default: ubuntu:24.04)
#   PROBE_KEEP       set to 1 to keep the container on failure (debug)
#   PROBE_EXTRACT_ONLY=<path>  extract the probe script to <path> and exit
#                    (drift check — no docker, no execution)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

if [[ -z "${PROBE_EXTRACT_ONLY:-}" ]]; then
    command -v docker >/dev/null || { echo "probe: docker required"; exit 1; }
fi

PROBE_IMAGE="${PROBE_IMAGE:-ubuntu:24.04}"
# issue 0373 — the book's install path was only ever exercised on ubuntu+bash,
# which is why three Arch-only defects (an apt-only prereq block, a `just`
# contradiction, an unactionable ROS warning) and a zsh-fatal glob in
# activate.sh all survived. These two knobs make the OTHER host shapes
# runnable with the same probe:
#   PROBE_DISTRO=arch PROBE_IMAGE=archlinux:base-devel just probe bootstrap
#   PROBE_SHELL=zsh just probe bootstrap
# PROBE_DISTRO selects which `probe=NN distro=…` book blocks are extracted;
# PROBE_SHELL is the shell the extracted steps run under.
PROBE_DISTRO="${PROBE_DISTRO:-debian}"
PROBE_SHELL="${PROBE_SHELL:-bash}"
PROBE_CLONE_URL="${PROBE_CLONE_URL:-/nano-ros-src}"
if [[ -z "${PROBE_BRANCH:-}" ]]; then
    PROBE_BRANCH="$(git -C "$REPO_ROOT" symbolic-ref --short -q HEAD || true)"
    if [[ -z "$PROBE_BRANCH" && -z "${PROBE_EXTRACT_ONLY:-}" ]]; then
        echo "probe: detached HEAD — set PROBE_BRANCH to a branch/tag to clone" >&2
        exit 1
    fi
fi

# The chapters carrying probe=NN tagged blocks, in reading order (order of
# execution comes from the NN numbers, not this list).
CHAPTERS=(
    book/src/getting-started/installation.md
    book/src/getting-started/first-node-rust.md
)

workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

python3 "$SCRIPT_DIR/extract-book-steps.py" \
    --out "$workdir/probe.sh" \
    --distro "$PROBE_DISTRO" \
    --subst 'git clone --branch nros-v0.5.0 https://github.com/NEWSLabNTU/nano-ros.git:::git clone --branch "$PROBE_BRANCH" "$PROBE_CLONE_URL" nano-ros' \
    --subst 'nros setup <board> --rmw <zenoh|xrce|cyclonedds>:::nros setup native --rmw zenoh' \
    "${CHAPTERS[@]/#/$REPO_ROOT/}"

# Probe-owned runtime verification (the book's Run section is interactive).
cat "$SCRIPT_DIR/verify-first-node.sh" >>"$workdir/probe.sh"

if [[ -n "${PROBE_EXTRACT_ONLY:-}" ]]; then
    cp "$workdir/probe.sh" "$PROBE_EXTRACT_ONLY"
    echo "probe: extract-only -> $PROBE_EXTRACT_ONLY"
    exit 0
fi

rm_flag=(--rm)
[[ "${PROBE_KEEP:-0}" = 1 ]] && rm_flag=()

echo "probe: image=$PROBE_IMAGE distro=$PROBE_DISTRO shell=$PROBE_SHELL branch=$PROBE_BRANCH url=$PROBE_CLONE_URL"

# Two host-configuration shims, not book prerequisites: `sudo` (the book's
# prereq block uses it; real user machines have it, the root container doesn't)
# and a `safe.directory` gitconfig entry (the mounted checkout is owned by
# the host uid — an artifact of cloning from a bind mount, not of the
# documented GitHub clone; git ignores safe.directory from env config).
#
# The shim is package-manager specific, so it follows PROBE_DISTRO. It installs
# ONLY sudo (+ the probe shell when it is not the image default) — everything
# the book tells the reader to install stays in the book's own step 10, which
# is the whole point of the probe.
case "$PROBE_DISTRO" in
    debian) install_shim="apt-get update -qq && apt-get install -y -qq sudo SHELLPKG >/dev/null" ;;
    fedora) install_shim="dnf install -y -q sudo SHELLPKG" ;;
    arch)   install_shim="pacman -Sy --noconfirm --needed sudo SHELLPKG >/dev/null" ;;
    *)      echo "probe: unknown PROBE_DISTRO '$PROBE_DISTRO' (want debian|fedora|arch)" >&2
            exit 2 ;;
esac
# bash is assumed present (the generated probe script has a bash shebang and
# the runner invokes $PROBE_SHELL explicitly); any other shell is installed by
# name, which happens to match the package name on all three distros.
# NB: an `[[ … ]] && x=y` one-liner would exit the script under `set -e` on the
# common path (PROBE_SHELL=bash makes the test false, so the whole statement
# returns 1). Keep the `if`.
shell_pkg=""
if [[ "$PROBE_SHELL" != "bash" ]]; then
    shell_pkg="$PROBE_SHELL"
fi
install_shim="${install_shim/SHELLPKG/$shell_pkg}"

docker run "${rm_flag[@]}" \
    --name "nros-bootstrap-probe-$$" \
    -v "$REPO_ROOT:/nano-ros-src:ro" \
    -v "$workdir/probe.sh:/probe.sh:ro" \
    -e PROBE_BRANCH="$PROBE_BRANCH" \
    -e PROBE_CLONE_URL="$PROBE_CLONE_URL" \
    -e PROBE_SHELL="$PROBE_SHELL" \
    -w /root \
    "$PROBE_IMAGE" \
    sh -c "$install_shim \
        && printf '[safe]\n\tdirectory = *\n' >/root/.gitconfig \
        && \"\$PROBE_SHELL\" /probe.sh"
