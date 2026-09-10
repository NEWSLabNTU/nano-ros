#!/usr/bin/env bash
#
# Run the nano-ros self-hosted runner INSIDE AN UNPRIVILEGED CONTAINER.
# Design: docs/development/multi-agent-ci-workflow.md ("Security").
#
# WHY A CONTAINER IS ENOUGH *HERE*, WHEN THE GENERAL ADVICE SAYS IT IS NOT
#
# The standard warning is real: a container does not isolate an untrusted job,
# because the escapes people add to make CI work ARE the vulnerability. Mounting
# the host Docker socket, or `--privileged` / docker-in-docker, both hand a job
# root on the host. Guidance that says "containers are insufficient" is talking
# about jobs that build images or start containers.
#
# nano-ros' self-hosted jobs do neither. MEASURED across all four
# (`build-wide`, `run-matrix`, `queue` L3, `nightly` matrix-nightly): zero
# references to docker, zero to KVM, zero device mounts. The work is cargo,
# cmake and QEMU in PURE EMULATION — `-icount shift=auto`, which is
# deterministic and incompatible with KVM, so not even `/dev/kvm` is wanted.
#
# So the escapes are not needed, and this script REFUSES to add them. That is
# the whole security argument, and it stops being true the moment a job needs to
# build an image — at which point the honest answer is a microVM, not a flag.
#
# WHAT IS AND IS NOT PROTECTED
#
# Fork pull requests already cannot reach these runners: every self-hosted job
# triggers only on `push`, `merge_group`, `schedule` or `workflow_dispatch`, and
# `check-workflow-runner-isolation` keeps it that way. The container bounds what
# someone WITH WRITE ACCESS can reach — it does not eliminate it. If the host
# carries unrelated work, that boundary is the point.
#
# Usage:
#   scripts/ci/runner-container.sh <labels> [--build] [--run] [--check]
#
#   <labels>   comma- or space-separated, e.g. nros-qemu,nros-sdk-zephyr,nros-big
#   --build    build the image only
#   --run      run the container only (image must exist)
#   --attach   run in the FOREGROUND with --rm, and exit with the container's
#              status. This is what a supervision loop needs: a detached
#              container's exit code has to be fished back out with `inspect`
#              after the fact, and the entrypoint's exit 78 ("these labels can
#              never be true here") is precisely the code that must not be lost.
#   --check    print what would happen, touch nothing
#
# With neither --build nor --run, does both.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
IMAGE="${NROS_RUNNER_IMAGE:-nano-ros-runner:local}"
NAME="${NROS_RUNNER_NAME:-nano-ros-runner}"
ENGINE="${NROS_CONTAINER_ENGINE:-docker}"
DO_BUILD=0 DO_RUN=0 CHECK=0 ATTACH=0
LABELS=""

while [ $# -gt 0 ]; do
    case "$1" in
        --build) DO_BUILD=1 ;;
        --run)   DO_RUN=1 ;;
        --attach) DO_RUN=1; ATTACH=1 ;;
        --check|--dry-run) CHECK=1 ;;
        # Refused ON PURPOSE. These are the two flags that turn a contained
        # runner back into an uncontained one, and a script that accepts them
        # "just this once" is how the property is lost without anyone deciding
        # to lose it.
        --privileged|-v*/var/run/docker.sock*|--docker-socket)
            echo "runner-container: refusing '$1'." >&2
            echo "  Mounting the docker socket or running privileged gives any" >&2
            echo "  job root on the HOST, which removes the only reason to use a" >&2
            echo "  container here. nano-ros' self-hosted jobs need neither —" >&2
            echo "  measured: no docker, no KVM, no device access in any of the" >&2
            echo "  four. If a job now genuinely needs to build an image, the" >&2
            echo "  answer is a microVM, not this flag." >&2
            exit 2 ;;
        -h|--help) sed -n '2,45p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        -*) echo "runner-container: unknown option '$1'" >&2; exit 2 ;;
        *)  LABELS="$1" ;;
    esac
    shift
done

[ -n "$LABELS" ] || { echo "runner-container: need <labels>, e.g. nros-qemu,nros-sdk-zephyr,nros-big" >&2; exit 2; }
LABELS="${LABELS// /,}"
if [ "$DO_BUILD" -eq 0 ] && [ "$DO_RUN" -eq 0 ]; then DO_BUILD=1; DO_RUN=1; fi

command -v "$ENGINE" >/dev/null 2>&1 || {
    echo "runner-container: '$ENGINE' not found (set NROS_CONTAINER_ENGINE)" >&2; exit 2; }

CONTEXT="$REPO_ROOT/ci/docker/runner"
mkdir -p "$CONTEXT"

# --- the system-package layer ------------------------------------------------
#
# THIS is what the image is for, and it is the one thing a volume cannot hold.
# Everything else the runner needs — rustup, the SDKs, the nano-ros checkout —
# installs as the `runner` user and therefore persists in a volume. System
# packages need root, and the container runs `--cap-drop ALL
# --security-opt no-new-privileges` with a non-root user, so `sudo apt` inside a
# running container cannot work even where the package list is right.
#
# So the split is a property of WHO CAN INSTALL IT, not a preference: root-only
# things are baked, user-writable things persist. That also gives the image the
# stability the persist design wants — it changes when the prereq list changes,
# not when nano-ros code or an SDK version does.
#
# The LIST is resolved from `nros-sdk-index.toml` at generation time, through
# the same `prereq-packages.py` the workflows use (`check-workflow-indexed-apt`
# requires it there). A hand-written second list beside the index is the drift
# this repo has already paid for three times — issues 0833, 0500, 0610 — and
# `runner-provision.sh` refuses to be one for exactly that reason.
# `make` and `ninja` are deliberately NOT here, and the check-one-producer gate
# is what says so: the index declares both as TOOLS (`nros setup --tool make`),
# so apt-installing them as well is two producers for one prefix — issue 0500,
# where the store accumulates, prefixes resolve newest-first, both paths print
# success, and the stale one shadows the pin that was just installed. They come
# from the store with everything else the provisioning step fetches.
# The list is what `nros setup` REPORTED MISSING on a real bootstrap of this
# image, not a guess about what a build needs. Its `[MISSING]` lines are
# non-fatal and scroll past, which is precisely how a runner ends up one apt
# package short of a label it claims.
PREREQ_KEYS=(cmake unzip curl zstd python3-dev python3-venv
             python3-pip clang libclang-dev libglib2-dev libpixman-dev
             libgcrypt-dev socat genromfs kconfig-frontends libmbedtls
             aria2 doxygen graphviz libz3 gnu-parallel wget)
if ! PREREQ_PACKAGES="$(python3 "$REPO_ROOT/scripts/sdk/prereq-packages.py" \
        --manager apt "${PREREQ_KEYS[@]}")"; then
    echo "runner-container: could not resolve the prereq packages from the index." >&2
    echo "  The Dockerfile is GENERATED from nros-sdk-index.toml — writing it" >&2
    echo "  with a guessed list would create the second source of truth this" >&2
    echo "  script exists to avoid. Fix the index, or the key names above." >&2
    exit 1
fi

# The image provisions THROUGH `runner-provision.sh`, not through a second list
# of apt packages. A runner and a contributor must provision the same way or the
# runner's toolchain becomes a thing nobody can account for — the reason that
# script exists at all.
cat > "$CONTEXT/Dockerfile" <<'DOCKEREOF'
# Generated by scripts/ci/runner-container.sh — edit that, not this.
FROM ubuntu:22.04

# The runner refuses to run as root, and running as root would also defeat the
# isolation this image exists for.
ARG RUNNER_UID=1001
ARG RUNNER_VERSION=2.329.0

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl git sudo jq python3 build-essential pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Resolved from nros-sdk-index.toml by scripts/sdk/prereq-packages.py --manager apt
# Keys: @PREREQ_KEYS@
RUN apt-get update && apt-get install -y --no-install-recommends \
        @PREREQ_PACKAGES@ \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -m -u ${RUNNER_UID} -s /bin/bash runner
WORKDIR /home/runner

# v2.329.0 or later is REQUIRED to configure or re-register since GitHub's
# 2026-06-12 change, and job execution needs staying inside a moving 30-day
# window. Pinned so the image is reproducible; bump it deliberately.
RUN curl -fsSL -o actions-runner.tar.gz \
      "https://github.com/actions/runner/releases/download/v${RUNNER_VERSION}/actions-runner-linux-x64-${RUNNER_VERSION}.tar.gz" \
    && tar xzf actions-runner.tar.gz && rm actions-runner.tar.gz \
    && ./bin/installdependencies.sh \
    && chown -R runner:runner /home/runner

COPY --chown=runner:runner entrypoint.sh /home/runner/entrypoint.sh
RUN chmod +x /home/runner/entrypoint.sh
USER runner
ENTRYPOINT ["/home/runner/entrypoint.sh"]
DOCKEREOF

# The Dockerfile heredoc is QUOTED so its own $VAR references survive verbatim;
# the two generated values are substituted here instead.
python3 - "$CONTEXT/Dockerfile" "$PREREQ_PACKAGES" "${PREREQ_KEYS[*]}" <<'SUBEOF'
import sys
path, packages, keys = sys.argv[1], sys.argv[2], sys.argv[3]
text = open(path).read()
text = text.replace("@PREREQ_PACKAGES@", " \\\n        ".join(packages.split()))
text = text.replace("@PREREQ_KEYS@", keys)
open(path, "w").write(text)
SUBEOF

cat > "$CONTEXT/entrypoint.sh" <<'ENTRYEOF'
#!/usr/bin/env bash
# Generated by scripts/ci/runner-container.sh.
set -euo pipefail
: "${NROS_RUNNER_LABELS:?labels required}"
: "${GH_REPO:?GH_REPO required, e.g. NEWSLabNTU/nano-ros}"
: "${RUNNER_TOKEN:?RUNNER_TOKEN required — a SHORT-LIVED registration token}"
# The checkout the label gate reads. A VOLUME, not a layer: the image carries no
# nano-ros source, so code changes never rebuild it, and `runner-bootstrap`
# refreshes this with a fetch rather than a 2.8 GB clone per start.
# A SUBDIRECTORY of the volume, not the volume's own mountpoint. The store is
# bind-backed by a host directory owned by the human who created it, and an ACL
# grants this UID access — but git's `safe.directory` check is about OWNERSHIP,
# not access, so a repo AT the mountpoint is refused with "detected dubious
# ownership" and no amount of ACL fixes it. A directory the container creates
# inside the mount is owned by the container's UID, so the clone below it is
# ordinary. The alternatives are worse: chown needs root on the host, and
# `safe.directory` in ~/.gitconfig dies with the writable layer every job.
NROS_SRC="${NROS_SRC:-/home/runner/src/nano-ros}"

# --ephemeral: one job, then the registration is spent. A job cannot leave state
# for the next one, which is also what stops the orphan/disk rot a long-lived
# runner accumulates (the design doc records 71 orphaned processes, oldest 10
# days, on a persistent runner).
#
# THE LABEL GATE, and it runs BEFORE config.sh on purpose. A runner registers
# its labels and then wins jobs that match them; if the toolchain behind a label
# is absent, it wins a job it cannot do and the failure surfaces as that job's
# error, not as this container's. Measured: a first container with an empty SDK
# store connected and took `L3 (cross build + link)` three seconds later.
#
# So the order is: prove the labels, THEN become visible. An unprovisioned
# runner is better absent than lying.
#
# Exit 78 (EX_CONFIG) rather than 1, because the supervision loop must tell
# "this container finished its job" from "this container can never work". The
# first is a restart; the second is a stop. Without the distinction one bad SDK
# fetch becomes a restart storm against the download it is failing on.
if [ "${NROS_RUNNER_SKIP_DOCTOR:-0}" != "1" ]; then
    if [ ! -x "$NROS_SRC/scripts/ci/runner-doctor.sh" ]; then
        echo "entrypoint: no checkout at $NROS_SRC — run the one-shot bootstrap first:" >&2
        echo "    just runner-bootstrap ${NROS_RUNNER_LABELS}" >&2
        exit 78
    fi
    echo "entrypoint: verifying labels before registering"
    if ! ( cd "$NROS_SRC" && ./scripts/ci/runner-doctor.sh "${NROS_RUNNER_LABELS}" ); then
        echo "entrypoint: labels are NOT true — refusing to register." >&2
        echo "  Provision the store, then start again:" >&2
        echo "    just runner-bootstrap ${NROS_RUNNER_LABELS}" >&2
        exit 78
    fi
fi

./config.sh \
    --url "https://github.com/${GH_REPO}" \
    --token "${RUNNER_TOKEN}" \
    --labels "${NROS_RUNNER_LABELS}" \
    --name "${RUNNER_NAME:-$(hostname)}" \
    --work /home/runner/_work \
    --ephemeral --unattended --replace
exec ./run.sh
ENTRYEOF
chmod +x "$CONTEXT/entrypoint.sh"

run_or_echo() {
    if [ "$CHECK" -eq 1 ]; then
        # REDACT the token. A --check is the command an operator runs first,
        # often piping it somewhere or pasting it into an issue, and echoing a
        # live registration token there is a leak the dry-run itself caused.
        # Short-lived (~1h) is not harmless: it is enough to attach a runner.
        local shown=()
        local arg
        for arg in "$@"; do
            case "$arg" in
                RUNNER_TOKEN=*) shown+=("RUNNER_TOKEN=<redacted>") ;;
                *)              shown+=("$arg") ;;
            esac
        done
        printf '  would run: %s\n' "${shown[*]}"
    else
        "$@"
    fi
}

if [ "$DO_BUILD" -eq 1 ]; then
    echo "runner-container: building $IMAGE (labels: $LABELS)"
    run_or_echo "$ENGINE" build -t "$IMAGE" "$CONTEXT"
    echo "  NOTE: the image carries the RUNNER, not the toolchain and not this"
    echo "  checkout. Both live in PERSISTENT VOLUMES, populated once by:"
    echo "      just runner-bootstrap $LABELS"
    echo "  Baking them in instead would go stale the moment nros-sdk-index.toml"
    echo "  moves, with nothing to say so, and would rebuild the image on every"
    echo "  nano-ros code change. A volume is refreshed by the same \`nros setup\`"
    echo "  a contributor runs, which is what runner-provision.sh exists to keep."
fi

if [ "$DO_RUN" -eq 1 ]; then
    # A dry run must not need a credential it is not going to use. Requiring one
    # here made `--check` — the command an operator runs FIRST — exit 2 on a box
    # with no token, which reads as "this is broken" rather than "here is what
    # would happen".
    [ -n "${RUNNER_TOKEN:-}" ] || [ "$CHECK" -eq 0 ] || RUNNER_TOKEN="<would-be-supplied>"
    [ -n "${RUNNER_TOKEN:-}" ] || {
        echo "runner-container: RUNNER_TOKEN is unset." >&2
        echo "  Get a SHORT-LIVED registration token (valid ~1h):" >&2
        echo "    gh api -X POST repos/${GH_REPO:-NEWSLabNTU/nano-ros}/actions/runners/registration-token --jq .token" >&2
        echo "  Never bake a token into the image or a file." >&2
        exit 2; }
    # The stores must exist and be writable by the container's UID BEFORE the
    # first start; a missing ACL is EACCES inside a job, four layers from its
    # cause. `--ensure` is idempotent, so this is a precondition made
    # unforgettable rather than a step in a runbook.
    if [ "$CHECK" -eq 1 ]; then
        echo "  would run: runner-store.sh --ensure  (dirs + ACLs + volumes)"
    else
        "$(dirname "${BASH_SOURCE[0]}")/runner-store.sh" --ensure
    fi

    echo "runner-container: starting $NAME"
    # Every flag here is a security decision, and each was TESTED, not assumed:
    #   --cap-drop ALL       no capabilities; nothing here needs one
    #   --security-opt       no privilege escalation via setuid
    #   --user runner        never root, in or out of the container
    #   --pids-limit         a fork bomb in a job cannot take the host down
    #   --tmpfs /tmp         exec-capable scratch that dies with the container
    #   named volumes        caches persist; nothing else does
    #
    # `.nros` is the FOURTH volume and it is not a cache — it is the SDK store
    # `nros setup` writes (`~/.nros/sdk`, ~9.2 GB with a Zephyr SDK in it).
    # Without it a `--ephemeral` container loses the toolchain with its writable
    # layer after ONE job, so `runner-provision.sh` would have to re-run per job
    # and a label like `nros-sdk-zephyr` would be a claim the image cannot keep.
    #
    # Persisting it rather than BAKING it into the image is deliberate: the SDK
    # versions live in `nros-sdk-index.toml`, so a baked image goes stale the
    # moment that file moves — and nothing would say so. A volume is refreshed
    # by the same `nros setup` a contributor runs, which is the property
    # `runner-provision.sh` exists to preserve ("the same script a contributor
    # uses, so the two cannot drift").
    #
    # It is the RUNNER's store, never a developer's. Point it at a directory
    # that is not `$HOME/.nros`; sharing one store between a runner and the
    # human on the same box is how a job's provisioning edits a developer's
    # toolchain (the class of issue 1166, one layer over).
    # and NOTHING mounts the host filesystem or the docker socket.
    #
    # NOT `--read-only`, deliberately, having measured what it costs here: the
    # runner resolves its `_diag` log directory from its binary's REAL path, so
    # an immutable install under /opt is still written to through the symlink
    # and the process dies (SIGSEGV, rc=139 — an unhandled IOException in
    # HostTraceListener, not a clean error). Making it work means copying the
    # whole ~200 MB install into a tmpfs on every start. It would buy little:
    # `--ephemeral` already destroys the writable layer after ONE job, which is
    # the property `--read-only` was there to approximate.
    # `-d` for an operator standing one up by hand (the logs stay after it
    # exits); `--rm` + foreground for a supervision loop, which wants the status
    # code and starts a fresh container next iteration anyway.
    MODE_FLAGS=(-d)
    [ "$ATTACH" -eq 1 ] && MODE_FLAGS=(--rm)
    run_or_echo "$ENGINE" run "${MODE_FLAGS[@]}" --name "$NAME" \
        --cap-drop ALL \
        --security-opt no-new-privileges \
        --user runner \
        --pids-limit 4096 \
        --tmpfs /tmp:rw,exec,nosuid,size=8g \
        -v "nros-runner-work:/home/runner/_work" \
        -v "nros-runner-cargo:/home/runner/.cargo" \
        -v "nros-runner-sccache:/home/runner/.cache/sccache" \
        -v "nros-runner-nros:/home/runner/.nros" \
        -v "nros-runner-rustup:/home/runner/.rustup" \
        -v "nros-runner-local:/home/runner/.local" \
        -v "nros-runner-src:/home/runner/src" \
        -e NROS_RUNNER_LABELS="$LABELS" \
        -e GH_REPO="${GH_REPO:-NEWSLabNTU/nano-ros}" \
        -e RUNNER_TOKEN="$RUNNER_TOKEN" \
        -e RUNNER_NAME="${RUNNER_NAME:-$NAME}" \
        "$IMAGE"
    if [ "$ATTACH" -eq 0 ]; then
        echo "  logs: $ENGINE logs -f $NAME"
        echo "  The entrypoint proves the labels BEFORE registering, so a runner"
        echo "  that appears on GitHub has already passed runner-doctor. If the"
        echo "  container exits 78, the store is not provisioned:"
        echo "    just runner-bootstrap $LABELS"
        echo "  NEXT, once it is up:"
        echo "    scripts/ci/enable-merge-queue.sh --apply --self-hosted-ready"
    fi
fi
