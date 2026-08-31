#!/usr/bin/env bash
#
# One command to stand up a contained self-hosted runner.
#
# Resolves the two things the procedure needs — the repo and a registration
# token — from `gh` when it can, and asks for them explicitly when it cannot,
# rather than failing with whatever error the underlying API returned.
#
#   scripts/ci/runner-up.sh <labels> [--repo O/R] [--token TOK] [--check]
#
# ON PASSING A TOKEN: prefer the environment (`RUNNER_TOKEN=… just runner-up …`)
# or `--token -` to read one line from stdin. `--token <value>` works, but a
# command-line argument is visible in `ps` to every user on the machine and
# lands in shell history; the script says so once rather than silently
# accepting it. A registration token is short-lived (~1h) but it is enough to
# attach a runner to the repo, so it is worth not leaking.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LABELS="" REPO="${GH_REPO:-}" TOKEN="${RUNNER_TOKEN:-}" CHECK=0
CONTAINER_NAME="${NROS_RUNNER_NAME:-nano-ros-runner}"
ENGINE="${NROS_CONTAINER_ENGINE:-docker}"

die() { printf 'runner-up: %s\n' "$1" >&2; exit "${2:-2}"; }

while [ $# -gt 0 ]; do
    case "$1" in
        --repo)  REPO="${2:?--repo needs OWNER/REPO}"; shift ;;
        --token)
            if [ "${2:-}" = "-" ]; then
                IFS= read -r TOKEN || die "no token on stdin"
            else
                TOKEN="${2:?--token needs a value, or - to read stdin}"
                echo "runner-up: note — a token in argv is visible in \`ps\` and shell" >&2
                echo "  history. Prefer RUNNER_TOKEN=… or '--token -' next time." >&2
            fi
            shift ;;
        --check|--dry-run) CHECK=1 ;;
        -h|--help) sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        -*) die "unknown option '$1'" ;;
        *)  LABELS="$1" ;;
    esac
    shift
done

[ -n "$LABELS" ] || die "need <labels>, e.g. nros-qemu,nros-sdk-zephyr,nros-big"
LABELS="${LABELS// /,}"

have_gh() { command -v gh >/dev/null 2>&1 && gh auth status >/dev/null 2>&1; }

# --- repo -------------------------------------------------------------------
# `gh repo view` reads the ORIGIN remote, which is what an operator standing in
# this checkout means by "the repo". Falling back to a hardcoded default would
# be worse than asking: it would silently attach a runner to the wrong repo for
# anyone working from a fork.
if [ -z "$REPO" ]; then
    if have_gh; then
        REPO="$(gh repo view --json nameWithOwner -q .nameWithOwner 2>/dev/null || true)"
    fi
fi
[ -n "$REPO" ] || die "cannot determine the repo.
  Either authenticate gh (\`gh auth login\`) from this checkout, or pass it:
      just runner-up $LABELS --repo OWNER/REPO"

# --- token ------------------------------------------------------------------
# Minting a registration token needs ADMIN on the repo. A non-admin's gh is
# authenticated and still cannot do this, so the failure is reported as what it
# is rather than as "gh missing".
TOKEN_SOURCE="supplied"
if [ -z "$TOKEN" ] && [ "$CHECK" -eq 1 ]; then
    # A DRY RUN MUST NOT MINT A CREDENTIAL. Minting here was the first version's
    # bug: `--check` is the command an operator runs before anything else, and
    # it created a live registration token — then printed it. Ask the cheaper
    # question instead, which is the one --check is for: WOULD this work?
    if have_gh && [ "$(gh api "repos/${REPO}" --jq .permissions.admin 2>/dev/null)" = "true" ]; then
        TOKEN="<would-be-minted>"; TOKEN_SOURCE="mintable via gh (not minted — dry run)"
    else
        TOKEN="<would-be-minted>"; TOKEN_SOURCE="NOT mintable — needs admin on ${REPO}, or pass --token"
    fi
elif [ -z "$TOKEN" ]; then
    if have_gh; then
        if TOKEN="$(gh api -X POST "repos/${REPO}/actions/runners/registration-token" \
                      --jq .token 2>/dev/null)" && [ -n "$TOKEN" ]; then
            TOKEN_SOURCE="minted via gh"
        else
            TOKEN=""
        fi
    fi
fi
[ -n "$TOKEN" ] || die "cannot mint a registration token for ${REPO}.
  This needs ADMIN on the repo — an authenticated gh is not enough. Ask an
  admin for one (Settings > Actions > Runners > New self-hosted runner), then:
      RUNNER_TOKEN=<token> just runner-up $LABELS
  or:  just runner-up $LABELS --token -   # reads one line from stdin"

echo "runner-up: repo=${REPO}  labels=${LABELS}  token=${TOKEN_SOURCE}"

if [ "$CHECK" -eq 1 ]; then
    echo "  would: build image + start container '${CONTAINER_NAME}'"
    echo "  would: verify labels with runner-doctor INSIDE the container"
    GH_REPO="$REPO" RUNNER_TOKEN="$TOKEN" \
        "$REPO_ROOT/scripts/ci/runner-container.sh" "$LABELS" --check
    echo "  would: print the merge-queue enable command (NOT run — it changes repo settings)"
    exit 0
fi

# --- stand it up ------------------------------------------------------------
GH_REPO="$REPO" RUNNER_TOKEN="$TOKEN" \
    "$REPO_ROOT/scripts/ci/runner-container.sh" "$LABELS"

# --- prove the labels ------------------------------------------------------
# A runner labelled `nros-sdk-zephyr` without the SDK wins jobs it cannot run,
# and the red lands on some author's PR looking like a code failure. Checking
# INSIDE the container is the point: the host's toolchain is not the one jobs
# will use.
echo "runner-up: verifying labels inside the container"
if "$ENGINE" exec "$CONTAINER_NAME" test -x /home/runner/nano-ros/scripts/ci/runner-doctor.sh 2>/dev/null; then
    "$ENGINE" exec "$CONTAINER_NAME" /home/runner/nano-ros/scripts/ci/runner-doctor.sh "$LABELS" || {
        echo "runner-up: the runner is UP but does not have what its labels claim." >&2
        echo "  Provision inside it, or stop it before it wins a job it cannot run:" >&2
        echo "      $ENGINE exec $CONTAINER_NAME just runner-provision $LABELS" >&2
        echo "      $ENGINE rm -f $CONTAINER_NAME" >&2
        exit 1; }
else
    # Honest about the gap rather than reporting success: the base image carries
    # the runner, not this checkout, so there is nothing to run the doctor from
    # until the labels are provisioned into the image.
    echo "runner-up: SKIPPED the label check — the container has no nano-ros checkout."
    echo "  The base image carries the RUNNER only. Until the labels are baked in"
    echo "  (extend ci/docker/runner/Dockerfile with runner-provision.sh), this"
    echo "  runner will win jobs whose toolchain it does not have."
fi

cat <<EOF

runner-up: '${CONTAINER_NAME}' is up (ephemeral — it retires after ONE job).
  logs:  ${ENGINE} logs -f ${CONTAINER_NAME}
  stop:  ${ENGINE} rm -f ${CONTAINER_NAME}

Not done automatically, because it changes REPO-WIDE settings and wants a human:
  just merge-queue --apply --self-hosted-ready
EOF
