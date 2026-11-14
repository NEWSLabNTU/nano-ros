#!/usr/bin/env bash
#
# Run the nano-ros self-hosted runner INSIDE AN UNPRIVILEGED CONTAINER.
# Design: docs/development/multi-agent-ci-workflow.md ("Security", and
# "A self-hosted runner is a container" for where a missing dependency is fixed).
#
# A SELF-HOSTED RUNNER IS A CONTAINER, SO A MISSING DEPENDENCY IS AN IMAGE FIX.
#
# When a self-hosted job fails for want of something, the fix belongs HERE — in
# the layers this script generates from `nros-sdk-index.toml` — and never in an
# `apt install` somebody typed on the workstation. Two reasons, both structural:
# the container drops every capability and runs as a non-root user, so a job
# cannot install a system package even where the name is right; and a package
# installed on the host is invisible to the image, so a fresh container does not
# reproduce it and nobody can say what the runner actually has. That is the
# state issue 0833 describes one level down. The rebuild is
# `runner-container.sh <labels> --build`, the restart is `--run`, and
# `runner-doctor.sh <labels>` says which labels hold and why.
#
# WHY A CONTAINER IS ENOUGH *HERE*, WHEN THE GENERAL ADVICE SAYS IT IS NOT
#
# The standard warning is real: a container does not isolate an untrusted job,
# because the escapes people add to make CI work ARE the vulnerability. Mounting
# the host Docker socket, or `--privileged` / docker-in-docker, both hand a job
# root on the host. Guidance that says "containers are insufficient" is talking
# about jobs that build images or start containers.
#
# nano-ros' self-hosted jobs do neither. MEASURED across all of them
# (`run-matrix` incl. matrix-nightly, `queue` L3; also the since-deleted
# `build-wide`): zero
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
#   NROS_RUNNER_NAME=<unique> scripts/ci/runner-container.sh <labels> [--build] [--run] [--sweep] [--check]
#   (the name is REQUIRED — it is the GitHub registration; scripts/lib/runner-name.sh)
#
#   <labels>   comma- or space-separated, e.g. nros-qemu,nros-sdk-zephyr,nros-big
#   --build    build the image only
#   --run      run the container only (image must exist)
#   --attach   run in the FOREGROUND with --rm, and exit with the container's
#              status. This is what a supervision loop needs: a detached
#              container's exit code has to be fished back out with `inspect`
#              after the fact, and the entrypoint's exit 78 ("these labels can
#              never be true here") is precisely the code that must not be lost.
#   --sweep    sweep the runner's VOLUMES in a throwaway container from the
#              image (issue 2191), then exit. The fixture tree lives in
#              `nros-runner-work`, which only a container mounts, so a sweep on
#              the host — what `runner-loop-container.sh` ran between jobs —
#              swept the operator's own checkout instead and never reached it.
#   --check    print what would happen, touch nothing
#
# With neither --build nor --run, does both.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
IMAGE="${NROS_RUNNER_IMAGE:-nano-ros-runner:local}"
ENGINE="${NROS_CONTAINER_ENGINE:-docker}"
DO_BUILD=0 DO_RUN=0 DO_SWEEP=0 CHECK=0 ATTACH=0
LABELS=""

while [ $# -gt 0 ]; do
    case "$1" in
        --build) DO_BUILD=1 ;;
        --run)   DO_RUN=1 ;;
        --attach) DO_RUN=1; ATTACH=1 ;;
        --sweep) DO_SWEEP=1 ;;
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
. "$REPO_ROOT/scripts/lib/runner-name.sh"
. "$REPO_ROOT/scripts/lib/runner-tmpfs.sh"
. "$REPO_ROOT/scripts/lib/runner-work-volume.sh"
NAME="$(nros_require_runner_name runner-container)"

[ -n "$LABELS" ] || { echo "runner-container: need <labels>, e.g. nros-qemu,nros-sdk-zephyr,nros-big" >&2; exit 2; }
LABELS="${LABELS// /,}"
if [ "$DO_BUILD" -eq 0 ] && [ "$DO_RUN" -eq 0 ] && [ "$DO_SWEEP" -eq 0 ]; then DO_BUILD=1; DO_RUN=1; fi

# The runner's volumes, ONE list for the job container and the sweep container:
# a sweep that mounted a different set would measure a different disk.
#
# `_work` is mounted `nocopy`. Without it Docker copies the image's (empty)
# `/home/runner/_work` into an EMPTY volume on first mount and hands the bind
# directory to the image's uid, which is how host 241's `work-queue` changed
# owner on 2026-11-14 (see runner-store.sh `ensure`). The toolchain volumes
# below keep the copy: on a fresh host it is what puts the image's cargo and
# rustup into them.
VOLUME_ARGS=(
    -v "$(nros_runner_work_volume "$LABELS"):/home/runner/_work:nocopy"
    -v "nros-runner-cargo:/home/runner/.cargo"
    -v "nros-runner-sccache:/home/runner/.cache/sccache"
    -v "nros-runner-nros:/home/runner/.nros"
    -v "nros-runner-rustup:/home/runner/.rustup"
    -v "nros-runner-local:/home/runner/.local"
    -v "nros-runner-cmake:/home/runner/.cmake"
    -v "nros-runner-src:/home/runner/src"
)
# The job container's confinement, shared for the same reason. Each flag is
# explained where the job container starts, below.
# Resolved once, up front: a bad size must stop here, not reach `docker run`.
TMPFS_ARG="$(nros_runner_tmpfs_arg)"
CONFINE_ARGS=(
    --cap-drop ALL
    --security-opt no-new-privileges
    --user runner
    --pids-limit 4096
    --tmpfs "$TMPFS_ARG"
)

command -v "$ENGINE" >/dev/null 2>&1 || {
    echo "runner-container: '$ENGINE' not found (set NROS_CONTAINER_ENGINE)" >&2; exit 2; }

CONTEXT="$REPO_ROOT/ci/docker/runner"
mkdir -p "$CONTEXT"

# The build context is GENERATED by the one image generator (RFC-0127 D1,
# phase-551 W1): scripts/ci/lib/image-context.sh holds what this script used
# to write inline, byte for byte (check-runner-context-golden).
# shellcheck source=scripts/ci/lib/image-context.sh
. "$REPO_ROOT/scripts/ci/lib/image-context.sh"
nros_image_resolve
nros_runner_context "$CONTEXT"

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
    #                        (size: scripts/lib/runner-tmpfs.sh)
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
        "${CONFINE_ARGS[@]}" \
        "${VOLUME_ARGS[@]}" \
        -e NROS_RUNNER_LABELS="$LABELS" \
        -e NROS_RUNNER_MIN_FREE_GIB="${NROS_RUNNER_MIN_FREE_GIB:-}" \
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

if [ "$DO_SWEEP" -eq 1 ]; then
    # What runs inside: the persistent fixture tree is the checkout to sweep
    # (its host target dirs are cargo-sweep's subject), and `_work` is the
    # runner work dir (`_temp`, `_actions`). The script is the TREE's own copy
    # when there is one — the tree is at a recent job's commit, while the
    # bootstrap checkout in `src` is refreshed only by a re-bootstrap and may
    # predate `--provision`, which an older script refuses as an unknown flag.
    # Never fails the loop: a sweep that cannot run is reported, and the next
    # job's own sweep (`fixture-tree.sh sync`) gets another chance.
    # shellcheck disable=SC2016  # expanded inside the container, not here
    SWEEP_SH='
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
src="${NROS_SRC:-/home/runner/src/nano-ros}"
work="${NROS_RUNNER_WORK_DIR:-/home/runner/_work}"
tree="$work/${GH_REPO##*/}/nros-fixture-tree"
root="$src"
[ -e "$tree/.git" ] && root="$tree"
sweep="$root/scripts/ci/runner-sweep.sh"
if [ ! -x "$sweep" ]; then
    echo "runner sweep: no runner-sweep.sh under $root (bootstrap first) -- nothing swept"
    exit 0
fi
flags=(--disk)
# An older tree'"'"'s sweep has no --provision; ask it rather than assume
# (issue 0726: a grep that fails to run is not "no match").
. "$root/scripts/lib/grep-q.sh"
rc=0; (nros_grep_q -- "--provision)" "$sweep") || rc=$?
case $rc in
    0) flags+=(--provision) ;;
    1) ;;
    *) echo "runner sweep: could not read $sweep -- sweeping without --provision" ;;
esac
echo "runner sweep: $root (${flags[*]})"
df -h "$work" | tail -1
NROS_RUNNER_REPO_ROOT="$root" NROS_SWEEP_WORK="$work" "$sweep" "${flags[@]}" \
    || echo "runner sweep: the sweep reported a problem -- continuing"
df -h "$work" | tail -1
'
    # The sweep's budgets (`NROS_SWEEP_*`, documented in runner-sweep.sh) are
    # the OPERATOR's knobs — host disk is the operator's to size — so each one
    # set here is forwarded. Without this the sweep inside the container saw
    # none of them and always ran on its defaults: on 2026-11-07 runner 133's
    # fixture tree reached 184 GiB under the default 280 GiB budget on a volume
    # it shares with other work, the volume hit 0 bytes free, and the loop
    # stopped after three failed starts — with NROS_SWEEP_BUDGET_FIXTURES_GIB
    # exported and silently ignored.
    SWEEP_ENV=()
    while IFS= read -r var; do
        SWEEP_ENV+=(-e "$var")
    done < <(compgen -e | grep '^NROS_SWEEP_' || true)
    echo "runner-container: sweeping the runner volumes in a throwaway container"
    [ "${#SWEEP_ENV[@]}" -gt 0 ] && echo "  forwarding: $(compgen -e | grep '^NROS_SWEEP_' | tr '\n' ' ')"
    run_or_echo "$ENGINE" run --rm --name "$NAME-sweep" \
        "${CONFINE_ARGS[@]}" \
        "${VOLUME_ARGS[@]}" \
        "${SWEEP_ENV[@]}" \
        -e GH_REPO="${GH_REPO:-NEWSLabNTU/nano-ros}" \
        --entrypoint bash \
        "$IMAGE" -c "$SWEEP_SH"
fi
