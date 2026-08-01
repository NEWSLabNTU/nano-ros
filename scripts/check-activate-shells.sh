#!/usr/bin/env bash
# Gate: the activation files must run to COMPLETION in every shell they claim
# to support, on a host whose SDK store is empty AND on one whose store holds
# only the versioned layout.
#
# Why this exists (issue 0372): `activate.sh` advertises "POSIX shell (bash /
# zsh)" and "the script never errors", but two unmatched SDK-store globs made
# zsh abort it mid-file — at line 92 before `nros setup`, at line 115 after it
# — silently dropping every export below the failure (the SDK PATH loop that
# wires zenohd, pinned ninja/make, .env, sdk-env.sh). No lane sourced these
# files under anything but bash, so it went unnoticed. Anything that reaches
# the shell's word expansion as a glob can regress this; the check is cheap,
# so it runs in check-fast.
#
# zsh / fish are OPTIONAL: absent interpreters are skipped loudly, never
# treated as passes for a shell that was never run.

set -uo pipefail

REPO_ROOT="$(cd -P "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
FAILURES=0
RAN=0

fail() {
    echo "FAIL: $*" >&2
    FAILURES=$((FAILURES + 1))
}

# A completed activation is one that (a) exits 0, (b) leaves NROS_REPO_DIR set
# to the checkout, and (c) reached the LAST statement of the file — proven by
# `_nros_root` being unset again, which only happens on the final lines.
# Sentinel output is a single line so any shell can emit it identically.
readonly PROBE_SH='
printf "PROBE repo=%s root_unset=%s\n" "${NROS_REPO_DIR:-}" "${_nros_root:+set}"
'
readonly PROBE_FISH='
printf "PROBE repo=%s root_unset=%s\n" "$NROS_REPO_DIR" (set -q _nros_root; and echo set; or echo "")
'

# $1 shell name, $2 store dir, $3 label
run_case() {
    local shell="$1" store="$2" label="$3" out rc
    if ! command -v "$shell" >/dev/null 2>&1; then
        echo "SKIP: $shell not installed — $label not covered on this host" >&2
        return 0
    fi
    RAN=$((RAN + 1))

    case "$shell" in
        fish)
            out="$(NROS_HOME="$store" NROS_QUIET_ACTIVATE=1 "$shell" -c \
                "source '$REPO_ROOT/activate.fish'; $PROBE_FISH" 2>&1)"
            ;;
        *)
            out="$(NROS_HOME="$store" NROS_QUIET_ACTIVATE=1 "$shell" -c \
                ". '$REPO_ROOT/activate.sh'; $PROBE_SH" 2>&1)"
            ;;
    esac
    rc=$?

    local probe
    probe="$(printf '%s\n' "$out" | grep '^PROBE ' || true)"

    if [ "$rc" -ne 0 ]; then
        fail "$shell/$label: activation exited $rc"
        printf '%s\n' "$out" | sed 's/^/    /' >&2
        return 0
    fi
    if [ -z "$probe" ]; then
        fail "$shell/$label: activation never reached the end of the file"
        printf '%s\n' "$out" | sed 's/^/    /' >&2
        return 0
    fi
    if ! printf '%s' "$probe" | grep -q "repo=$REPO_ROOT "; then
        fail "$shell/$label: NROS_REPO_DIR wrong or unset ($probe)"
        return 0
    fi
    if ! printf '%s' "$probe" | grep -q 'root_unset=$'; then
        # `_nros_root` is unset by the file's last lines, so a shell that still
        # has it never got there — the file aborted partway (issue 0372's exact
        # symptom) or grew an early return. Note a zsh `nomatch` abort ends the
        # SOURCED file only: the outer shell keeps going and still exits 0, so
        # this sentinel — not the exit status — is what catches the regression.
        fail "$shell/$label: activation stopped before its final lines ($probe)"
        printf '%s\n' "$out" | sed 's/^/    /' >&2
        return 0
    fi
    # An unmatched-glob abort in a shell that reports it non-fatally would still
    # show up here, so treat the message itself as a failure regardless of rc.
    if printf '%s\n' "$out" | grep -qi 'no matches\|bad pattern'; then
        fail "$shell/$label: glob reached word expansion"
        printf '%s\n' "$out" | sed 's/^/    /' >&2
        return 0
    fi
    echo "ok: $shell/$label"
}

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# Case 1 — empty store: the fresh-machine state, before any `nros setup`.
empty_store="$tmp/empty"
mkdir -p "$empty_store"

# Case 2 — versioned store only: the state `nros setup` actually leaves behind
# (sdk/<tool>/<version>/bin), which is what made the second glob site fatal.
versioned_store="$tmp/versioned"
mkdir -p "$versioned_store/sdk/zenohd/1.7.2-nros2/bin"
: >"$versioned_store/sdk/zenohd/1.7.2-nros2/bin/zenohd"
chmod +x "$versioned_store/sdk/zenohd/1.7.2-nros2/bin/zenohd"
mkdir -p "$versioned_store/sdk/play_launch_parser/0.5.0/bin"
: >"$versioned_store/sdk/play_launch_parser/0.5.0/bin/play_launch_parser"
chmod +x "$versioned_store/sdk/play_launch_parser/0.5.0/bin/play_launch_parser"

for sh in bash zsh fish; do
    run_case "$sh" "$empty_store" "empty-store"
    run_case "$sh" "$versioned_store" "versioned-store"
done

# The versioned store must actually put its bin dirs on PATH — a fix that
# silences the abort by skipping the lookup would otherwise pass the checks
# above. bash is mandatory, so assert the wiring there.
path_out="$(NROS_HOME="$versioned_store" NROS_QUIET_ACTIVATE=1 bash -c \
    ". '$REPO_ROOT/activate.sh'; printf '%s' \"\$PATH\"" 2>/dev/null)"
for tool in zenohd play_launch_parser; do
    if ! printf '%s' "$path_out" | tr ':' '\n' | grep -q "^$versioned_store/sdk/$tool/"; then
        fail "activate.sh: versioned $tool bin dir never reached PATH"
    fi
done

if [ "$RAN" -eq 0 ]; then
    fail "no shell was exercised"
fi

if [ "$FAILURES" -ne 0 ]; then
    echo "check-activate-shells: $FAILURES failure(s)" >&2
    exit 1
fi
echo "Activation files run to completion in every available shell."
