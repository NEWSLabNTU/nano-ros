#!/usr/bin/env bash
#
# Populate the contained runner's persistent stores — ONCE, before any runner
# registers. The runner itself never does this.
#
# WHY IT IS SEPARATE FROM THE RUNNER.
#
# The runner container is `--ephemeral`: one job, then the registration is spent
# and the container exits. That is deliberate — it is what stops the orphan and
# disk rot a long-lived runner accumulates (71 orphaned processes, oldest ten
# days, on the persistent one).
#
# Provisioning inside that lifecycle would therefore run PER JOB. Twenty queued
# jobs would pay for twenty fetches, twenty CLI builds and twenty SDK checks.
# But provisioning is not per-job state — it is a durable input, which is why it
# lives in a volume. So it belongs outside the ephemeral cycle: bootstrap once,
# then run as many one-job runners as you like against a store that is already
# true.
#
# WHY NOT BAKE IT INTO THE IMAGE.
#
# The SDK versions live in `nros-sdk-index.toml`. A baked image is correct until
# that file moves and then it is stale with nothing to say so — the same shape
# as a gate whose scope is narrower than its rule. A store is refreshed by the
# same `nros setup` a contributor runs, which is what `runner-provision.sh`
# exists to preserve. And nano-ros code never enters the image at all, so a code
# change never rebuilds it.
#
# WHAT IT DOES
#
#   1. ensure the stores (dirs, ACLs, volumes)
#   2. clone-or-FETCH nano-ros into the `src` volume
#   3. `runner-provision.sh <labels>` -> the `.nros` store
#   4. `runner-doctor <labels>` -> the labels are TRUE, or this exits non-zero
#
# Step 4 is the point. A bootstrap that "succeeded" without it is a claim; the
# entrypoint runs the same check again before registering, so a store that rots
# later still cannot produce a lying runner.
#
# Usage:
#   scripts/ci/runner-bootstrap.sh <labels> [--ref <git-ref>] [--check]
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
IMAGE="${NROS_RUNNER_IMAGE:-nano-ros-runner:local}"
REMOTE="${NROS_RUNNER_REMOTE:-https://github.com/NEWSLabNTU/nano-ros.git}"
REF="${NROS_RUNNER_REF:-main}"
CHECK=0
LABELS=""

while [ $# -gt 0 ]; do
    case "$1" in
        --check|--dry-run) CHECK=1 ;;
        --ref) REF="${2:?--ref needs a git ref}"; shift ;;
        -*) echo "runner-bootstrap: unknown flag '$1'" >&2; exit 2 ;;
        *) LABELS="$1" ;;
    esac
    shift
done
[ -n "$LABELS" ] || { echo "runner-bootstrap: need <labels>, e.g. nros-qemu,nros-sdk-zephyr,nros-big" >&2; exit 2; }

say() { echo "runner-bootstrap: $*"; }

# One `docker run` for the whole bootstrap, sharing the same volumes the runner
# will get. Same UID, same ACLs, same paths — a bootstrap that provisions
# somewhere the runner cannot read is the failure this design exists to avoid.
BOOTSTRAP_SH='
set -euo pipefail
# Inside the volume, never AT it: the mountpoint is a host directory owned by
# another UID, and git refuses a repo it does not own ("detected dubious
# ownership") however the ACLs read. A directory this container creates is
# owned by this container.
SRC=/home/runner/src/nano-ros
mkdir -p "$(dirname "$SRC")"
if [ -d "$SRC/.git" ]; then
    echo "  fetching $REF into an existing checkout"
    git -C "$SRC" fetch --depth 1 origin "$REF"
    git -C "$SRC" checkout -q FETCH_HEAD
else
    echo "  cloning (shallow) — this is the one expensive start"
    git clone --depth 1 --branch "$REF" "$REMOTE" "$SRC"
fi
cd "$SRC"

# `runner-provision.sh` is a thin caller over `just` recipes and says so when
# `just` is absent — it points at scripts/bootstrap.sh, which is the front door
# that needs no just. A fresh image has neither, so run it here rather than
# leave the operator to read an error and type the next command themselves.
#
# `.cargo` and `.rustup` are VOLUMES, so this is a first-run cost, not a
# per-bootstrap one: a later refresh finds the toolchain already there.
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
if ! command -v just >/dev/null 2>&1; then
    echo "  no just in this image — running the front door (installs rustup + just)"
    ./scripts/bootstrap.sh --no-prompt base
    export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
fi

# --- Python packages ---------------------------------------------------------
#
# nano-ros deliberately does NOT install these: choosing between a distro
# package, `pip --user` and a venv is a decision about YOUR interpreter, and on
# a PEP 668 host pip refuses `--user` outright. It reports them and stops.
#
# A runner container has no "your interpreter" to respect — the image owns it,
# it is the python3.10 Ubuntu 22.04 ships, and it is not PEP 668. The decision
# the repo correctly leaves to an operator is made HERE, once, where it is a
# property of the image rather than of one contributor laptop. `pip --user` puts it in
# ~/.local, which is a volume, so it persists exactly like rustup and the SDKs.
#
# `tomli` is not optional and does not announce itself. python3.10 has no
# `tomllib`, so `fixtures-manifest.py` cannot parse `examples/fixtures.toml` —
# and `nros_platform_rmws` runs it with stderr redirected to /dev/null, so the
# import error is reported as: no fixture rows for platform threadx-riscv64.
# That failed `just setup threadx_riscv64` with a message about a platform name
# that is in fact present, 42 times, in the manifest it could not read.
echo "  installing the Python packages the image owes its interpreter"
python3 -m pip install --user --quiet --disable-pip-version-check \
    tomli PyYAML jsonschema packaging pyelftools pykwalify west

# PROVISIONING AND LABEL TRUTH ARE DIFFERENT QUESTIONS, and the second one is
# the contract. A provisioning step can fail for a reason that has nothing to do
# with the labels this runner claims — issue 1276 is exactly that: a panic in
# `nros setup rv-virt-threadx --rmw cyclonedds` on the SEVENTH package, after
# the toolchain and QEMU the `nros-qemu` label actually names were already
# installed and reported `present (skip)`.
#
# So the failure is reported LOUDLY and then the doctor is asked anyway. What it
# must never do is launder the failure: an unread non-zero here is the bug this
# repo keeps paying for, so the status is captured explicitly (issue 1249 — a
# status meant for inspection dies at the assignment under `set -e`) and printed
# whatever the doctor says.
provision_rc=0
./scripts/ci/runner-provision.sh "$LABELS" || provision_rc=$?
if [ "$provision_rc" -ne 0 ]; then
    echo ""
    echo "runner-bootstrap: PROVISIONING REPORTED FAILURES (exit $provision_rc)." >&2
    echo "  Asking runner-doctor anyway — the labels are the contract, and a" >&2
    echo "  step can fail for a reason none of them depend on. If the doctor" >&2
    echo "  passes, this runner is honest about what it can build; the failure" >&2
    echo "  above is still real and still wants fixing." >&2
    echo ""
fi

# The verification is the point of the whole script. `runner-provision` ends
# with the same check, which is deliberate duplication of the CHEAP half: this
# one is what the exit code of `just runner-bootstrap` means, and it is the same
# check the entrypoint runs before registering, so "provisioned" and "actually
# has it" cannot be different answers at any of the three points.
./scripts/ci/runner-doctor.sh "$LABELS"

if [ "$provision_rc" -ne 0 ]; then
    echo ""
    echo "runner-bootstrap: the labels hold, but provisioning exited $provision_rc."
    echo "  Read the failure above — it is not about these labels, and it is not fixed."
fi
'

if [ "$CHECK" -eq 1 ]; then
    say "would ensure stores, then run in $IMAGE:"
    echo "    clone-or-fetch $REMOTE@$REF -> /home/runner/src"
    echo "    runner-provision.sh $LABELS"
    echo "    runner-doctor.sh $LABELS   (labels TRUE, or non-zero)"
    exit 0
fi

"$REPO_ROOT/scripts/ci/runner-store.sh" --ensure

say "provisioning $LABELS in $IMAGE (first run downloads the SDKs; later runs are a fetch)"
docker run --rm \
    --user runner \
    --cap-drop ALL --security-opt no-new-privileges \
    --tmpfs /tmp:rw,exec,nosuid,size=8g \
    -v "nros-runner-work:/home/runner/_work" \
    -v "nros-runner-cargo:/home/runner/.cargo" \
    -v "nros-runner-rustup:/home/runner/.rustup" \
    -v "nros-runner-local:/home/runner/.local" \
    -v "nros-runner-sccache:/home/runner/.cache/sccache" \
    -v "nros-runner-nros:/home/runner/.nros" \
    -v "nros-runner-src:/home/runner/src" \
    -e REMOTE="$REMOTE" -e REF="$REF" -e LABELS="$LABELS" \
    --entrypoint bash "$IMAGE" -c "$BOOTSTRAP_SH"

say "store is provisioned and the labels verified — start runners with:"
echo "    just runner-loop-container $LABELS"
