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
#   PROBE_TRACK      quickstart (default) | zenoh — which documented flow runs.
#                    `zenoh` is the ROS-interop story (phase-368 follow-up):
#                    image defaults to ros:humble (the interop page's own
#                    prerequisite), setup provisions `--rmw zenoh`,
#                    first-node-rust.md joins the chapter list (its
#                    zenoh-default build needs the zenoh-pico source that only
#                    `--rmw zenoh` provisions), and the verifier replays the
#                    interop page's three terminals: the documented router
#                    invocation, the nano-ros talker, and `ros2 topic echo`
#                    proving cross-stack delivery.
#                    `installed` is the USER's path (phase-447 A3, RFC-0099
#                    D1): install a release, provision, then first-project.md's
#                    scaffold -> build -> run, on a host with NO checkout. See
#                    "THE INSTALLED TRACK" below.
#   PROBE_ASSET=<dir>  installed track: install the `nros-linux-x86_64.tar.zst`
#                    (+ `.sha256`) in <dir> instead of building one — e.g. an
#                    artifact from a `release nros` workflow run
#                    (`gh run download <id> -n nros-linux-x86_64 -D <dir>`),
#                    which is the asset a runner actually produced
#   PROBE_RELEASE_CACHE  installed track: 1 (default) keeps the asset builder's
#                    rustup, cargo registry and two target dirs in named docker
#                    volumes `nros-probe-release-*`; 0 builds cold, as a
#                    release runner does
#   PROBE_EXTRACT_ONLY=<path>  extract the probe script to <path> and exit
#                    (drift check — no docker, no execution)
#
# THE INSTALLED TRACK (phase-447 A3)
#
# The released `nros` could not build anything, and it survived because the
# only people who could notice were the only people it could not affect: every
# contributor has a checkout the SDK-root ladder reaches first, and this probe
# stopped at the bootstrap — first-project.md, the scaffold -> build -> run
# page, carried no `probe=` block. This track runs that page on a machine where
# no checkout exists, and verify-installed-first-project.sh ASSERTS that rather
# than trusting the docker invocation.
#
# No release has been cut, so the probe cannot `curl` one. It builds one — and
# what it builds is decided by `release-nros.yml`, not by this script:
# extract-workflow-steps.py pulls the workflow's own `run:` steps (build, record,
# stage) out of the COMMIT under test and runs them in the image the workflow's
# `runs-on` names. A probe with its own staging list would be a second
# definition of "what a release contains", and would stay green on the day the
# workflow dropped a line. Reading the workflow is also what makes the probe a
# gate: revert the release's SDK-root staging and the probe builds exactly the
# asset that dead-ends, then fails where a user would.
#
# The user's container is a SECOND, pristine one: it gets the asset and the
# branch's `install.sh`, read-only, and nothing else — the book's curl line is
# substituted to read them from that mount. It defaults to the release's own
# base (`ubuntu:22.04` today), because the asset's ABI floor is that base's:
# `nros-launch-resolve` links `libpython3.10.so.1.0`, which 24.04 does not ship.
# Declaring and probing that floor is phase-447 D1/D2; until then a 24.04 user
# container fails on the loader before reaching the question this track asks.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

if [[ -z "${PROBE_EXTRACT_ONLY:-}" ]]; then
    command -v docker >/dev/null || { echo "probe: docker required"; exit 1; }
fi

PROBE_TRACK="${PROBE_TRACK:-quickstart}"
case "$PROBE_TRACK" in
    quickstart) default_image="ubuntu:24.04" ;;
    zenoh)      default_image="ros:humble" ;;
    # Resolved from the release workflow's `runs-on` below, once it is read.
    installed)  default_image="" ;;
    *) echo "probe: unknown PROBE_TRACK '$PROBE_TRACK' (want quickstart|zenoh|installed)" >&2; exit 2 ;;
esac
PROBE_IMAGE="${PROBE_IMAGE:-$default_image}"
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
# phase-368 W4 — the probe follows the QUICK START (cyclonedds, no router).
# `first-node-rust.md` left this list when the probe's rmw moved to
# cyclonedds: its zenoh-default `cargo build` needs `nros setup --source
# zenoh-pico` (the cargo path does NOT self-provision submodules, unlike the
# cmake path, which bootstraps them at configure) — that page's flow belongs
# to a future zenoh-track probe run under `--rmw zenoh`. Rust coverage lives
# in verify-first-node.sh's scaffolded-workspace run instead.
CLONE_SUBST=(--subst 'git clone --branch nros-v0.5.0 https://github.com/NEWSLabNTU/nano-ros.git:::git clone --branch "$PROBE_BRANCH" "$PROBE_CLONE_URL" nano-ros')
C_CD_SUBST=()
# Which `track=` blocks of the book this run reads — how the reader got `nros`.
BOOK_TRACK=checkout
# The installed track's fixed names. The asset name is the one
# `release-nros.yml` stages and `install.sh` derives from the host key.
RELEASE_MOUNT=/probe-release
ASSET_NAME=nros-linux-x86_64.tar.zst
if [[ "$PROBE_TRACK" = "installed" ]]; then
    CHAPTERS=(
        book/src/getting-started/installation.md
        book/src/getting-started/first-project.md
    )
    PROBE_RMW="cyclonedds"
    VERIFIER="verify-installed-first-project.sh"
    BOOK_TRACK=installed
    # No clone on this track — that is the point. The book's curl line fetches
    # `install.sh` from GitHub and the installer fetches the newest release;
    # both are redirected to the mount, and nothing else about the line
    # changes (NROS_INSTALL_URL is install.sh's own documented knob, and the
    # checksum it requires is still checked).
    CLONE_SUBST=(--subst "curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install.sh | sh:::curl -fsSL file://$RELEASE_MOUNT/install.sh | NROS_INSTALL_URL=file://$RELEASE_MOUNT/$ASSET_NAME sh")
    # The no-checkout + shipped-SDK-root check runs right after the install
    # step, not at the end — see the header of check-installed-sdk-root.sh
    # for the measurement that moved it. Assembled below, once $workdir exists.
    INSTALLED_CHECK=1
elif [[ "$PROBE_TRACK" = "zenoh" ]]; then
    # first-node-rust.md is IN this track: its zenoh-default `nros sync &&
    # cargo build` is exactly what a reader on the interop path runs, and it
    # needs the zenoh-pico source that only `--rmw zenoh` provisions.
    CHAPTERS=(
        book/src/getting-started/installation.md
        book/src/getting-started/first-node-rust.md
    )
    PROBE_RMW="zenoh"
    VERIFIER="verify-zenoh-interop.sh"
else
    CHAPTERS=(
        book/src/getting-started/installation.md
        book/src/getting-started/first-node-c.md
    )
    PROBE_RMW="cyclonedds"
    VERIFIER="verify-first-node.sh"
    # The C-chapter `cd` subst rides only this track (each --subst must match
    # EXACTLY ONCE, and the zenoh track does not extract that chapter).
    C_CD_SUBST=(--subst 'cd examples/native/c/talker:::cd "$(git rev-parse --show-toplevel)/examples/native/c/talker"')
fi

workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

# The installed track's post-install check, with scripts/lib/grep-q.sh in front
# of it. The container has no checkout to source a library from — that is the
# point of the track — so the helper travels inside the script, and every check
# after it (the final verifier too, same shell) greps through `nros_grep_q`.
if [[ "${INSTALLED_CHECK:-0}" = 1 ]]; then
    cat "$REPO_ROOT/scripts/lib/grep-q.sh" "$SCRIPT_DIR/check-installed-sdk-root.sh" \
        >"$workdir/check-installed-sdk-root.sh"
    C_CD_SUBST=(--after-step "20=$workdir/check-installed-sdk-root.sh")
fi

python3 "$SCRIPT_DIR/extract-book-steps.py" \
    --out "$workdir/probe.sh" \
    --distro "$PROBE_DISTRO" \
    --track "$BOOK_TRACK" \
    "${CLONE_SUBST[@]}" \
    --subst "nros setup <board> --rmw <zenoh|xrce|cyclonedds>:::nros setup native --rmw $PROBE_RMW" \
    ${C_CD_SUBST[@]+"${C_CD_SUBST[@]}"} \
    "${CHAPTERS[@]/#/$REPO_ROOT/}"

# The installed track's release steps, extracted here so a drift check
# (PROBE_EXTRACT_ONLY) also catches a renamed or conditional workflow step.
# Read from the COMMIT under test — the same commit the builder checks out —
# never from this worktree, so the workflow and the tree it builds agree.
if [[ "$PROBE_TRACK" = "installed" ]]; then
    under_test="${PROBE_BRANCH:-HEAD}"
    rc=0
    git -C "$REPO_ROOT" show "$under_test:.github/workflows/release-nros.yml" \
        >"$workdir/release-nros.yml" || rc=$?
    if [[ "$rc" -ne 0 ]]; then
        echo "probe: $under_test has no .github/workflows/release-nros.yml — the workflow the installed track builds its asset from" >&2
        exit 1
    fi
    python3 "$SCRIPT_DIR/extract-workflow-steps.py" \
        --workflow "$workdir/release-nros.yml" --job build \
        --out "$workdir/release-steps.sh" \
        --runner-image-out "$workdir/runner-image" \
        --step "Build the CLI from source" \
        --step "Record what this release is made of" \
        --step "Stage the prefix" \
        --expr "inputs.version=${PROBE_RELEASE_VERSION:-0.0.0-probe}"
    RELEASE_BUILDER_IMAGE="$(cat "$workdir/runner-image")"
    PROBE_IMAGE="${PROBE_IMAGE:-$RELEASE_BUILDER_IMAGE}"
fi

# (The C-chapter cd subst is declared with its track above; it resolves the
# repo root through `git rev-parse` rather than a literal so it does not
# assume where the clone landed — the same move the verifiers make.)

# Probe-owned runtime verification (the book's Run sections are interactive).
cat "$SCRIPT_DIR/$VERIFIER" >>"$workdir/probe.sh"

if [[ -n "${PROBE_EXTRACT_ONLY:-}" ]]; then
    cp "$workdir/probe.sh" "$PROBE_EXTRACT_ONLY"
    echo "probe: extract-only -> $PROBE_EXTRACT_ONLY"
    exit 0
fi

rm_flag=(--rm)
[[ "${PROBE_KEEP:-0}" = 1 ]] && rm_flag=()

echo "probe: track=$PROBE_TRACK image=$PROBE_IMAGE distro=$PROBE_DISTRO shell=$PROBE_SHELL branch=$PROBE_BRANCH url=$PROBE_CLONE_URL"

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
# `tzdata` on debian is the same kind of shim, and measured, not assumed: on a
# pristine `ubuntu:22.04` the book's own prereq block pulls it in, and its
# debconf prompt ("Geographic area:") then waits on a stdin the container has
# no terminal for — the installed track hung there for ten minutes. A real
# Ubuntu host has tzdata installed and configured, so preinstalling it
# non-interactively reproduces a real machine rather than doing a book step.
# (`sudo` resets the environment, so a bare DEBIAN_FRONTEND would not reach the
# book's `sudo apt-get` anyway.)
case "$PROBE_DISTRO" in
    debian) install_shim="apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq sudo tzdata SHELLPKG >/dev/null" ;;
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

# The repository's GIT DIR, not its worktree. In a linked worktree (every agent
# session here) `$REPO_ROOT/.git` is a FILE naming a host path the container
# cannot see, so mounting the worktree made the clone fail before any book step
# ran. The common dir holds every branch and object, and cloning from it is an
# ordinary local clone.
git_dir="$(git -C "$REPO_ROOT" rev-parse --path-format=absolute --git-common-dir)"

if [[ "$PROBE_TRACK" = "installed" ]]; then
    mkdir -p "$workdir/release"
    if [[ -n "${PROBE_ASSET:-}" ]]; then
        for f in "$PROBE_ASSET/$ASSET_NAME" "$PROBE_ASSET/$ASSET_NAME.sha256"; do
            if [[ ! -s "$f" ]]; then
                echo "probe: PROBE_ASSET=$PROBE_ASSET has no $(basename "$f") (the release asset and the checksum install.sh refuses to install without)" >&2
                exit 1
            fi
        done
        cp "$PROBE_ASSET/$ASSET_NAME" "$PROBE_ASSET/$ASSET_NAME.sha256" "$workdir/release/"
        echo "probe: installed track — using the asset in $PROBE_ASSET"
    else
        echo "probe: installed track — building the asset $under_test would release, on $RELEASE_BUILDER_IMAGE"
        cache_args=()
        if [[ "${PROBE_RELEASE_CACHE:-1}" = 1 ]]; then
            # Warm state only: a toolchain, a registry, two target dirs. cargo's
            # fingerprints decide what rebuilds, and the extracted workflow step
            # itself asserts `nros source-stamp` against the checked-out tree,
            # so a stale binary cannot ship out of the cache silently.
            # The two target dirs mount at /cache and build-release-asset.sh
            # links them into the clone, because a clone refuses a destination
            # that already has volumes mounted inside it.
            cache_args=(
                -v nros-probe-release-cargo:/root/.cargo
                -v nros-probe-release-rustup:/root/.rustup
                -v nros-probe-release-target-cli:/cache/target-cli
                -v nros-probe-release-target-resolve:/cache/target-resolve
            )
        fi
        docker run --rm \
            --name "nros-probe-release-builder-$$" \
            -v "$git_dir:/nano-ros-git:ro" \
            -v "$workdir/release-steps.sh:/probe/release-steps.sh:ro" \
            -v "$SCRIPT_DIR/build-release-asset.sh:/probe/build-release-asset.sh:ro" \
            -v "$workdir/release:/out" \
            ${cache_args[@]+"${cache_args[@]}"} \
            -e PROBE_BRANCH="$under_test" \
            -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
            ${CARGO_BUILD_JOBS:+-e "CARGO_BUILD_JOBS=$CARGO_BUILD_JOBS"} \
            "$RELEASE_BUILDER_IMAGE" \
            bash /probe/build-release-asset.sh
    fi
    # The installer a user curls — the branch's own, so the asset and the
    # script that unpacks it come from one commit.
    git -C "$REPO_ROOT" show "$under_test:scripts/install.sh" >"$workdir/release/install.sh"
    # The index — issue 1304. A released `nros` FETCHES its index from `main`
    # (RFC-0097 D5), and the commit under test is not `main` yet: without this,
    # the probe would provision against whatever `main` declares today and a
    # change to the index would go untested until after it merged. The
    # commit's own index stands in for "the index as of this release", through
    # the CLI's own documented knob — the same substitution the curl line
    # makes for install.sh, and nothing the user types changes.
    git -C "$REPO_ROOT" show "$under_test:nros-sdk-index.toml" >"$workdir/release/nros-sdk-index.toml"

    echo "probe: installed track — user container $PROBE_IMAGE, mounting the asset and nothing else"
    # No repo mount, no git dir, no PROBE_BRANCH: the user container sees the
    # asset directory and the probe script, and verify-installed-first-project.sh
    # asserts that nothing else resembling a nano-ros root reached it.
    docker run "${rm_flag[@]}" \
        --name "nros-bootstrap-probe-$$" \
        -v "$workdir/probe.sh:/probe.sh:ro" \
        -v "$workdir/release:$RELEASE_MOUNT:ro" \
        -e PROBE_SHELL="$PROBE_SHELL" \
        -e NROS_INDEX_URL="file://$RELEASE_MOUNT/nros-sdk-index.toml" \
        -w /root \
        "$PROBE_IMAGE" \
        sh -c "$install_shim && \"\$PROBE_SHELL\" /probe.sh"
    exit 0
fi

docker run "${rm_flag[@]}" \
    --name "nros-bootstrap-probe-$$" \
    -v "$git_dir:/nano-ros-src:ro" \
    -v "$workdir/probe.sh:/probe.sh:ro" \
    -e PROBE_BRANCH="$PROBE_BRANCH" \
    -e PROBE_CLONE_URL="$PROBE_CLONE_URL" \
    -e PROBE_SHELL="$PROBE_SHELL" \
    -w /root \
    "$PROBE_IMAGE" \
    sh -c "$install_shim \
        && printf '[safe]\n\tdirectory = *\n' >/root/.gitconfig \
        && \"\$PROBE_SHELL\" /probe.sh"
