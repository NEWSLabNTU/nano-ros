#!/usr/bin/env bash
#
# The ONE answer to "which commit does `check-submodule-pins` compare against?"
#
# Issue 1912. Every job in gate.yml (and host-tests.yml) used to spell
#
#     base="${GITHUB_BASE_REF:-main}"
#     git fetch --depth=1 origin "+refs/heads/$base:refs/remotes/origin/$base"
#     echo "NROS_SUBMODULE_PIN_BASELINE=origin/$base" >> "$GITHUB_ENV"
#
# nine times over. On a pull request that is the branch the change goes into,
# which is the right question. On a PUSH to main it is main's tip at the moment
# the job RUNS — and push gates queue, so by then main has often moved past the
# commit under test. A pin the LATER commit moved forward then compares as
# `new = this commit's pin` against `old = a newer main's pin`: a REWIND that
# nobody made (four sightings in two days, on three submodules), or, when the
# submodule clone is shallow, a NOT VERIFIED that the skip allowlist turns red.
# The local default had the same shape: `origin/main` for a branch that main has
# since moved past reads every later pin bump as a rewind on this branch.
#
# The rule: the baseline is a property of the COMMIT UNDER TEST, never of a
# moving tip.
#
#   push           `github.event.before` — the commit the push replaced. For a
#                  push of several commits that is the whole range, which is
#                  what the push published. (All-zero = a new ref: fall back
#                  to the tested commit's first parent.)
#   merge_group    `merge_group.base_sha` — what the queue entry builds on.
#   pull_request   `pull_request.base.sha` — the base the event was raised
#                  against, fixed in the payload.
#   anything else  (schedule, workflow_dispatch) the tested commit's FIRST
#                  PARENT: "did this commit move a pin backward?"
#   local          `git merge-base HEAD origin/main` — what THIS branch changed,
#                  never what main did after the branch point.
#
# An OLDER baseline can never manufacture a rewind (a forward move it has not
# seen yet reads as a forward move); only a NEWER one can. So a stale event
# payload is safe in the direction that matters, and the live tip is the one
# answer that is not.
#
# The pre-push hook already does the push arm right: git hands it the remote
# ref's sha, which IS `before`, and it passes that explicitly.
#
# Usage:
#   pin-baseline.sh resolve [<ref>]  print the baseline (a sha, or a ref), rc 1 if
#                                none can be named. Network-free.
#   pin-baseline.sh provide      CI: resolve, fetch that commit (depth 1 — the
#                                gate reads its TREE), export
#                                NROS_SUBMODULE_PIN_BASELINE to $GITHUB_ENV.
#                                Also fetches the base BRANCH ref as before,
#                                because other fast gates read `origin/main`.
#   pin-baseline.sh init-moved   CI: initialise (commits only) every
#                                uninitialised submodule whose pin differs
#                                between the baseline and HEAD.
#   pin-baseline.sh deepen       CI: give each initialised submodule the
#                                baseline pin's commit, unshallowing for real
#                                connectivity (issue 1476).
#
# `resolve` is also what `submodule-pins-check.sh` asks when it is given no
# baseline, so the workflow and the gate cannot disagree about the rule.

set -uo pipefail

_nros_pin_zero() { case "$1" in *[!0]*) return 1 ;; *) return 0 ;; esac; }

# The tested commit's first parent, read from the RAW object: in a depth-1
# clone the parent is grafted away, so `HEAD^1` does not resolve, but the
# commit object still names it — and that sha is all `provide` needs to fetch.
_nros_pin_first_parent() {
    git cat-file -p "${1:-HEAD}" 2>/dev/null | awk '/^parent / { print $2; exit } /^$/ { exit }'
}

# One field out of the event payload. python3, not jq: every runner image and
# every dev host here has python3; jq is not provisioned anywhere.
_nros_pin_event_field() {
    python3 - "$GITHUB_EVENT_PATH" "$1" <<'PY' 2>/dev/null
import json, sys
try:
    node = json.load(open(sys.argv[1]))
    for key in sys.argv[2].split("."):
        node = node[key]
except Exception:
    sys.exit(1)
if not isinstance(node, str) or not node:
    sys.exit(1)
print(node)
PY
}

# $1 = the commit under test (default HEAD).
nros_pin_baseline_resolve() {
    local sha="" tested="${1:-HEAD}"
    if [ -n "${GITHUB_EVENT_NAME:-}" ] && [ -n "${GITHUB_EVENT_PATH:-}" ] \
        && [ -f "${GITHUB_EVENT_PATH}" ]; then
        case "$GITHUB_EVENT_NAME" in
            push)
                sha="$(_nros_pin_event_field before)" || sha=""
                if [ -z "$sha" ] || _nros_pin_zero "$sha"; then
                    sha="$(_nros_pin_first_parent "$tested")"
                fi
                ;;
            merge_group)
                sha="$(_nros_pin_event_field merge_group.base_sha)" || sha=""
                ;;
            pull_request | pull_request_target)
                sha="$(_nros_pin_event_field pull_request.base.sha)" || sha=""
                ;;
            *)
                sha="$(_nros_pin_first_parent "$tested")"
                ;;
        esac
        [ -n "$sha" ] || return 1
        printf '%s\n' "$sha"
        return 0
    fi

    # Local. The branch point, never the tip: a branch that main has moved
    # past must not inherit main's later pin bumps as rewinds of its own.
    local upstream="${NROS_SUBMODULE_PIN_UPSTREAM:-origin/main}"
    git rev-parse --verify --quiet "${upstream}^{commit}" >/dev/null || return 1
    sha="$(git merge-base "$tested" "$upstream" 2>/dev/null)" || return 1
    [ -n "$sha" ] || return 1
    printf '%s\n' "$sha"
}

# `$path\t$sha` for every gitlink at <ref>.
_nros_pin_gitlinks() {
    git ls-tree -r "$1" 2>/dev/null | awk '$2 == "commit" { print $4 "\t" $3 }'
}

_nros_pin_require_env_baseline() {
    if [ -z "${NROS_SUBMODULE_PIN_BASELINE:-}" ]; then
        echo "pin-baseline: NROS_SUBMODULE_PIN_BASELINE is unset — run \`$0 provide\` first" >&2
        exit 1
    fi
}

cmd_provide() {
    local base sha
    if ! sha="$(nros_pin_baseline_resolve)"; then
        echo "pin-baseline: no baseline can be named for event '${GITHUB_EVENT_NAME:-local}'" >&2
        exit 1
    fi
    # Other fast-line gates (doc citations, issue index) read `origin/<base>`,
    # and this step used to be what provided it. Keep providing it; it is just
    # no longer what the PIN gate compares against.
    base="${GITHUB_BASE_REF:-main}"
    git fetch --no-tags --quiet --depth=1 origin \
        "+refs/heads/$base:refs/remotes/origin/$base" || true
    if ! git cat-file -e "${sha}^{commit}" 2>/dev/null; then
        # GitHub serves a reachable commit by sha. depth=1 is enough: the gate
        # reads the superproject's TREE at the baseline, never its history.
        if ! git fetch --no-tags --quiet --depth=1 origin "$sha"; then
            echo "pin-baseline: could not fetch the baseline ${sha} (${GITHUB_EVENT_NAME:-local})" >&2
            exit 1
        fi
    fi
    echo "pin-baseline: ${GITHUB_EVENT_NAME:-local} -> ${sha}"
    if [ -n "${GITHUB_ENV:-}" ]; then
        echo "NROS_SUBMODULE_PIN_BASELINE=$sha" >> "$GITHUB_ENV"
    fi
}

# An UNINITIALISED submodule whose pin moved reaches the gate with no object
# store to read (issue 1034). `--filter=tree:0`: the gate needs COMMITS only.
# Not `--depth`: a shallow commit is grafted as a root and destroys the
# connectivity `merge-base --is-ancestor` needs.
cmd_init_moved() {
    _nros_pin_require_env_baseline
    local path base_sha head_sha
    _nros_pin_gitlinks "$NROS_SUBMODULE_PIN_BASELINE" \
        | while IFS=$'\t' read -r path base_sha; do
            [ -n "$path" ] || continue
            head_sha="$(git ls-tree -r HEAD -- "$path" | awk '$2 == "commit" { print $3 }')"
            [ -n "$head_sha" ] || continue
            [ "$head_sha" != "$base_sha" ] || continue
            [ ! -e "$path/.git" ] || continue
            echo "  pin moved in $path (${base_sha:0:12} -> ${head_sha:0:12}); initialising"
            git submodule update --init --filter=tree:0 "$path" \
                || git submodule update --init "$path" \
                || true
        done
}

# The baseline pin is by definition the commit BEFORE the submodule's tip,
# which a shallow submodule clone never has. UNSHALLOW, not `--depth=1 <sha>`:
# a shallow fetch grafts the sha as a root and `merge-base` then finds no path
# for a clean fast-forward (issue 1476).
cmd_deepen() {
    _nros_pin_require_env_baseline
    local path sha
    _nros_pin_gitlinks "$NROS_SUBMODULE_PIN_BASELINE" \
        | while IFS=$'\t' read -r path sha; do
            [ -n "$path" ] || continue
            [ -e "$path/.git" ] || continue
            git -C "$path" cat-file -e "${sha}^{commit}" 2>/dev/null && continue
            echo "  deepening $path for $sha"
            git -C "$path" fetch --no-tags --quiet --unshallow 2>/dev/null \
                || git -C "$path" fetch --no-tags --quiet origin "$sha" 2>/dev/null \
                || true
        done
}

# Sourced (by submodule-pins-check.sh) for the function; executed for a verb.
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
    case "${1:-}" in
        resolve) nros_pin_baseline_resolve "${2:-HEAD}" ;;
        provide) cmd_provide ;;
        init-moved) cmd_init_moved ;;
        deepen) cmd_deepen ;;
        *)
            echo "usage: $0 resolve|provide|init-moved|deepen" >&2
            exit 2
            ;;
    esac
fi
