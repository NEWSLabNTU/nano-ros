#!/usr/bin/env bash
#
# Register this machine as a GitHub Actions self-hosted runner for nano-ros.
# phase-395 W6; design: docs/development/multi-agent-ci-workflow.md
# ("Setting it up on GitHub" -> "2. Register a runner (works behind NAT)").
#
# Registration should be ONE COMMAND, not a wiki page. A procedure that lives in
# prose is a procedure that is performed differently every time, and the
# difference shows up months later as a runner whose toolchain nobody can
# account for.
#
# THE ORDER MATTERS, AND IT IS THE POINT OF THIS SCRIPT.
#
#   1. runner-doctor.sh  — refuse to register a runner that LIES about its
#                          labels. GitHub routes jobs by label and verifies
#                          nothing; a machine labelled `nros-sdk-zephyr` with no
#                          SDK wins the job and fails inside the build, and that
#                          red lands on the PR author's change looking like a
#                          code failure. Registering first and discovering the
#                          gap later means every job in between is poisoned.
#   2. registration token — short-lived (~1 hour) and NOT a secret to store.
#   3. ./config.sh --ephemeral --unattended --replace
#   4. the service.
#
# BEHIND NAT IS FINE, AND IS THE NORMAL DEPLOYMENT.
#
# The runner LONG-POLLS GitHub over outbound HTTPS; nothing connects to it. No
# inbound ports, no forwarding, no static address — only outbound 443 to
# github.com, api.github.com, *.actions.githubusercontent.com and the
# artifact/cache hosts. Two consequences are planned for elsewhere rather than
# here: a NAT'd runner cannot be reached for debugging (so `queue.yml` uploads
# its logs unconditionally), and it may be on a slow uplink (so label it for
# jobs whose cost is CPU rather than download).
#
# THIS IS A PUBLIC REPO, so two flags are not optional:
#
#   --ephemeral   the runner takes ONE job and de-registers. That is what stops
#                 one job leaving state for the next. Read the note printed
#                 after registration: an ephemeral runner needs re-registering,
#                 which a plain systemd service does NOT do for you.
#   (unprivileged) this script refuses to run as root. Self-hosted jobs must
#                 trigger only on `merge_group` and `push` to main, never on a
#                 fork's `pull_request`; the account running them should hold no
#                 other work.
#
# usage:
#   scripts/ci/runner-register.sh <labels> [options]
#
#   <labels>          comma- or space-separated capability labels, e.g.
#                     nros-qemu,nros-sdk-zephyr,nros-big
#                     `self-hosted,linux` are prepended for you.
#
#   --check           resolve and report everything, contact nothing that
#                     changes state, register nothing. Exit 0 = ready to
#                     register, 1 = something would stop it.
#   --repo OWNER/REPO default NEWSLabNTU/nano-ros (or $NROS_RUNNER_REPO)
#   --dir PATH        runner install dir; default $HOME/actions-runner
#   --name NAME       runner name; default <host>-<labels-digest>
#   --work PATH       runner work dir; default <dir>/_work
#   --user USER       service account; default the invoking user
#   --with-service    ALSO install + start the systemd service (needs sudo)
#   --no-service      accepted for symmetry; this is already the default
#   --skip-doctor     NOT PROVIDED ON PURPOSE. See the header. If a label is
#                     wrong, fix the host or drop the label.
#
# env:
#   NROS_RUNNER_REPO, NROS_RUNNER_DIR, NROS_RUNNER_NAME, NROS_RUNNER_USER,
#   NROS_RUNNER_WORK      same as the flags above
#   NROS_RUNNER_VERSION   pin the actions/runner release (default: latest)
#   NROS_RUNNER_SHA256    expected sha256 of the runner tarball; when set the
#                         download is verified and a mismatch is fatal
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

CHECK=0
# Default 1 — the service install is the only sudo in these four scripts, so it
# is OPT-IN. This repo's standing rule is that nothing here sudos: an agent must
# tell the operator instead. Registering a runner is a human act, and making
# `--with-service` explicit is what keeps a plain `just runner-register` safe
# for an agent to run.
NO_SERVICE=1
labels=""
gh_repo="${NROS_RUNNER_REPO:-NEWSLabNTU/nano-ros}"
runner_dir="${NROS_RUNNER_DIR:-$HOME/actions-runner}"
runner_name="${NROS_RUNNER_NAME:-}"
runner_work="${NROS_RUNNER_WORK:-}"
runner_user="${NROS_RUNNER_USER:-$(id -un)}"

while [ "$#" -gt 0 ]; do
    case "$1" in
        --check|--dry-run) CHECK=1 ;;
        --no-service)      NO_SERVICE=1 ;;
        --with-service)    NO_SERVICE=0 ;;
        --repo)  gh_repo="${2:?--repo needs OWNER/REPO}";  shift ;;
        --dir)   runner_dir="${2:?--dir needs a path}";    shift ;;
        --name)  runner_name="${2:?--name needs a name}";  shift ;;
        --work)  runner_work="${2:?--work needs a path}";  shift ;;
        --user)  runner_user="${2:?--user needs a user}";  shift ;;
        -h|--help)
            sed -n '2,72p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        -*)
            echo "runner-register: unknown option '$1'" >&2
            exit 2
            ;;
        *)
            if [ -n "$labels" ]; then labels="$labels,$1"; else labels="$1"; fi
            ;;
    esac
    shift
done

if [ -z "$labels" ]; then
    echo "runner-register: usage: scripts/ci/runner-register.sh <labels> [--check]" >&2
    echo "  e.g. scripts/ci/runner-register.sh nros-qemu,nros-sdk-zephyr,nros-big" >&2
    exit 2
fi

[ -n "$runner_work" ] || runner_work="$runner_dir/_work"

# A default name that is stable per (host, label set) and says what the machine
# is FOR. Not the bare hostname: two runners with different label sets on one
# box are a normal shape, and GitHub keys runners by name.
if [ -z "$runner_name" ]; then
    _host="$(hostname -s 2>/dev/null || hostname 2>/dev/null || echo runner)"
    _digest="$(printf '%s' "$labels" | tr ',' '-' | tr -cd 'a-z0-9-' | cut -c1-40)"
    runner_name="${_host}-${_digest}"
fi

# GitHub applies these itself, but `config.sh --labels` replaces the set rather
# than adding to it, so they are spelled here. This is the one place the full
# label string is composed — runner-doctor and runner-provision take the
# capability labels only.
full_labels="self-hosted,linux,$labels"

echo "runner-register: repo      $gh_repo"
echo "runner-register: dir       $runner_dir"
echo "runner-register: work      $runner_work"
echo "runner-register: name      $runner_name"
echo "runner-register: labels    $full_labels"
echo "runner-register: user      $runner_user"
echo "runner-register: service   $([ "$NO_SERVICE" -eq 1 ] && echo 'no (default; --with-service to install)' || echo 'install + start (--with-service)')"
echo ""

# --- 0. refuse to run as root ------------------------------------------------
#
# `config.sh` refuses too, but failing here says WHY. Rule 3 of the design's
# security section: a dedicated unprivileged account, never the one holding
# other research work — these machines carry it.
if [ "$(id -u)" -eq 0 ]; then
    echo "runner-register: refusing to run as root." >&2
    echo "  A public repo's fork PR can execute arbitrary code on a self-hosted" >&2
    echo "  runner once a maintainer enqueues it. Run as a dedicated unprivileged" >&2
    echo "  user that owns nothing else:" >&2
    echo "      sudo -u <runner-user> $0 $*" >&2
    exit 1
fi

# --- 1. doctor, FIRST --------------------------------------------------------
#
# Standalone invocation rather than sourcing, so its exit code is the only
# contract between the two scripts and there is no way to accidentally skip it.
echo "runner-register: verifying the labels before registering anything..."
if ! "$repo_root/scripts/ci/runner-doctor.sh" "$labels"; then
    echo "" >&2
    echo "runner-register: REFUSING to register — this host does not have what its" >&2
    echo "  labels claim. Registering anyway means every job routed here fails" >&2
    echo "  inside the build, and the red lands on the PR author's change." >&2
    echo "" >&2
    echo "  Fix the host:   scripts/ci/runner-provision.sh $labels" >&2
    echo "  Or register with fewer labels." >&2
    exit 1
fi
echo ""

# --- 2. prerequisites --------------------------------------------------------

if ! command -v gh >/dev/null 2>&1; then
    echo "runner-register: \`gh\` is not on PATH." >&2
    echo "  The registration token comes from the GitHub API, and \`gh\` carries the" >&2
    echo "  credential. Install it (https://cli.github.com) and \`gh auth login\`." >&2
    exit 1
fi
if ! gh auth status >/dev/null 2>&1; then
    echo "runner-register: \`gh\` is installed but not authenticated." >&2
    echo "  Run: gh auth login" >&2
    echo "  The account needs admin rights on $gh_repo to mint a registration token." >&2
    exit 1
fi
echo "runner-register: gh authenticated."

# --- 3. the runner binaries --------------------------------------------------
#
# Downloaded rather than assumed present, so a fresh machine is one command.
# The tarball is NOT verified unless NROS_RUNNER_SHA256 is set — say so out
# loud rather than implying a check that is not happening. The digest of what
# was actually fetched is printed so an operator can compare it against the
# release page, and pin it for the next machine.
_runner_arch() {
    case "$(uname -m)" in
        x86_64|amd64)  printf 'x64' ;;
        aarch64|arm64) printf 'arm64' ;;
        *) echo "runner-register: unsupported architecture $(uname -m)" >&2; return 1 ;;
    esac
}

_download_runner() {
    local ver arch url tarball got
    ver="${NROS_RUNNER_VERSION:-}"
    if [ -z "$ver" ]; then
        ver="$(gh api repos/actions/runner/releases/latest --jq .tag_name 2>/dev/null || true)"
        ver="${ver#v}"
    fi
    if [ -z "$ver" ]; then
        echo "runner-register: cannot resolve the actions/runner version." >&2
        echo "  Pin one explicitly:  NROS_RUNNER_VERSION=2.x.y $0 $labels" >&2
        return 1
    fi
    arch="$(_runner_arch)" || return 1
    url="https://github.com/actions/runner/releases/download/v${ver}/actions-runner-linux-${arch}-${ver}.tar.gz"
    tarball="$runner_dir/actions-runner-linux-${arch}-${ver}.tar.gz"

    mkdir -p "$runner_dir"
    echo "runner-register: downloading actions/runner v$ver ($arch)"
    echo "                 $url"
    curl -fSL -o "$tarball" "$url"

    got="$(sha256sum "$tarball" | awk '{print $1}')"
    if [ -n "${NROS_RUNNER_SHA256:-}" ]; then
        if [ "$got" != "$NROS_RUNNER_SHA256" ]; then
            echo "runner-register: sha256 MISMATCH for $tarball" >&2
            echo "  expected $NROS_RUNNER_SHA256" >&2
            echo "  got      $got" >&2
            rm -f "$tarball"
            return 1
        fi
        echo "runner-register: sha256 verified ($got)"
    else
        echo "runner-register: sha256 of the download: $got"
        echo "                 NOT verified — no NROS_RUNNER_SHA256 was given."
        echo "                 Compare it with the actions/runner release page, then set"
        echo "                 NROS_RUNNER_SHA256 for the next machine."
    fi
    tar xzf "$tarball" -C "$runner_dir"
    rm -f "$tarball"
}

# --- check mode --------------------------------------------------------------
#
# Everything above this point is read-only (doctor, `gh auth status`), so
# `--check` gets a genuine answer rather than a description of one. What it
# deliberately does NOT do is mint a token: that is a POST, it consumes an API
# call, and a token minted by a dry run is a live credential nobody asked for.
if [ "$CHECK" -eq 1 ]; then
    echo ""
    echo "runner-register: --check — nothing was registered."
    if [ -x "$runner_dir/config.sh" ]; then
        echo "  runner binaries: present ($runner_dir/config.sh)"
    else
        echo "  runner binaries: ABSENT — a real run would download actions/runner"
        echo "                   ${NROS_RUNNER_VERSION:+(pinned v$NROS_RUNNER_VERSION) }into $runner_dir"
    fi
    if [ -f "$runner_dir/.runner" ]; then
        echo "  existing config: $runner_dir/.runner — a real run passes --replace,"
        echo "                   which re-registers this NAME and drops the old entry."
    fi
    echo ""
    echo "  A real run would then:"
    echo "    gh api -X POST repos/$gh_repo/actions/runners/registration-token --jq .token"
    echo "    $runner_dir/config.sh --url https://github.com/$gh_repo --token <token> \\"
    echo "        --name $runner_name --work $runner_work \\"
    echo "        --labels \"$full_labels\" --ephemeral --unattended --replace"
    if [ "$NO_SERVICE" -eq 0 ]; then
        echo "    sudo $runner_dir/svc.sh install $runner_user && sudo $runner_dir/svc.sh start"
    fi
    echo ""
    echo "runner-register: --check — ready to register."
    exit 0
fi

# --- 4. register -------------------------------------------------------------

if [ ! -x "$runner_dir/config.sh" ]; then
    _download_runner
fi

# The token is short-lived (~1 hour) and single-use. It is deliberately NOT
# written to a file: nothing needs it after `config.sh`, and a token on disk is
# a credential somebody eventually copies.
echo "runner-register: minting a registration token for $gh_repo..."
token="$(gh api -X POST "repos/$gh_repo/actions/runners/registration-token" --jq .token)"
if [ -z "$token" ]; then
    echo "runner-register: the API returned no token." >&2
    echo "  The authenticated account needs ADMIN on $gh_repo to mint one." >&2
    exit 1
fi

echo "runner-register: configuring..."
(
    cd "$runner_dir"
    # --unattended: no prompts (this is a script).
    # --replace:    re-registering the same NAME replaces the old entry instead
    #               of failing, which is what makes re-running this idempotent.
    # --ephemeral:  one job, then de-register. Not optional on a public repo.
    ./config.sh \
        --url "https://github.com/$gh_repo" \
        --token "$token" \
        --name "$runner_name" \
        --work "$runner_work" \
        --labels "$full_labels" \
        --ephemeral \
        --unattended \
        --replace
)
unset token

# --- 5. the service ----------------------------------------------------------
#
# `svc.sh install` writes a systemd unit and needs root for that one step; the
# unit then RUNS as $runner_user. This is the only sudo in the four scripts and
# it is OPT-IN (`--with-service`): the default path below prints the commands
# for a human to run and executes none of them.
if [ "$NO_SERVICE" -eq 1 ]; then
    echo ""
    echo "runner-register: registered but NOT started — no sudo was run."
    echo "  Foreground (one job, then it de-registers):  cd $runner_dir && ./run.sh"
    echo "  Service later:  sudo $runner_dir/svc.sh install $runner_user && sudo $runner_dir/svc.sh start"
else
    echo ""
    echo "runner-register: installing the service (this is the one step needing root):"
    echo "    sudo $runner_dir/svc.sh install $runner_user"
    echo "    sudo $runner_dir/svc.sh start"
    (
        cd "$runner_dir"
        sudo ./svc.sh install "$runner_user"
        sudo ./svc.sh start
    )
fi

cat <<EOF

runner-register: registered $runner_name -> $gh_repo
                 labels: $full_labels

READ THIS — --ephemeral and a service are not a complete pairing.

An ephemeral runner takes ONE job, de-registers itself and exits. The systemd
unit will then restart a runner whose registration no longer exists, and the
machine stops taking jobs. That is the correct security trade (rule 4: one job
cannot leave state for the next) but it means SOMETHING has to re-register.

Options, in order of preference:
  * a supervisor loop that re-runs this script after each job;
  * a scheduled re-run (systemd timer / cron) — re-running is idempotent
    thanks to --replace;
  * drop --ephemeral. Do NOT do this on a public repo.

Between jobs, run the sweep — a leaked peer becomes every later job's flake,
and a full disk fails in ways that look like code failures:

    $repo_root/scripts/ci/runner-sweep.sh

Self-hosted jobs must trigger ONLY on merge_group and push-to-main. A fork's
pull_request must never reach this machine; enqueueing a fork PR is the trust
decision, not merging it.
EOF
