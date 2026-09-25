#!/usr/bin/env bash
# Every workspace crate documents, not just the six the book deploys.
#
# Issue 1116 / phase-452 W4. `just check rustdoc-links` runs on a pull-request
# lane over the DEPLOYED crate set, deliberately: that set is what `just book`
# publishes, so a red there is a broken deploy. Everything outside it kept the
# property that lane was written to remove — a doc link could rot with nothing
# anywhere to say so.
#
# Measured 2026-09-25 on the workspace with the deployed feature set: 179
# diagnostics and TWELVE crates that could not document at all. The issue had
# recorded "~70, five" three weeks earlier, which is the drift rate of an
# unchecked surface. All 179 are fixed, so this gate holds a ZERO rather than a
# shrinking ratchet — a ratchet was the phase's fallback for a remainder that
# no longer exists, and a count that may only shrink is a weaker statement than
# a count that is already nothing.
#
# WHY IT IS NOT A WIDER `rustdoc-links`
#
# Same reason `NROS_RUSTDOC_CRATES` stays narrow: the pull-request lane exists
# to keep the docs deploy green, and a crate the book does not publish must
# not be able to make it red. Two scopes, two gates, one feature set and one
# source table (`scripts/build/rustdoc-set.sh` splices the deployed rows into
# the workspace rows, so they cannot disagree).
#
# WHY `RUSTDOCFLAGS=-D warnings`
#
# Not decoration, and not the same thing the workspace already has. Deny comes
# from `[workspace.lints.rust] warnings = "deny"`, which reaches only the
# crates that write `[lints] workspace = true`. The others get WARNINGS, and
# `cargo doc` exits 0 over a warning — so without this flag the gate would have
# returned OK over the 17 diagnostics that the board crates were carrying, and
# over `nros-cargo-profile`'s two ambiguous `[`env`]` links, which is how those
# two were found. The flag is what makes the answer uniform across a workspace
# whose crates opt into lints unevenly.
#
# THREE OUTCOMES, per issue 1043 / 1138:
#
#   FAIL          rustdoc RAN and something is wrong.
#   NOT VERIFIED  a vendored source this pass needs is absent here, so rustdoc
#                 never ran and nothing was measured either way.
#   OK            every workspace crate documents cleanly.
#
# `NROS_RUSTDOC_WORKSPACE_STRICT=1` turns the skip back into a red, and belongs
# ONLY on a lane that really provisions every source in the table — gate.yml's
# `compile-smoke` job, which runs `nros setup --source …` for all of them.
# Setting it on a lane that provisions a subset re-creates issue 1043.
set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

usage() {
    cat >&2 <<'USAGE'
usage: check-rustdoc-workspace.sh [--self-test]

  (no args)    run the gate. The negative control runs first, quietly.
  --self-test  run ONLY the negative control, verbosely.
USAGE
}

# Chatty in `--self-test`, silent on the normal path unless something is wrong.
SELF_TEST_VERBOSE=0
note() { [ "$SELF_TEST_VERBOSE" = "1" ] && echo "$@"; return 0; }

control_refused() {
    echo "rustdoc-workspace: FAILED — its own negative control did not pass, so a green" >&2
    echo "  from the real pass would have meant nothing. Fix the control first:" >&2
    echo "  scripts/check-rustdoc-workspace.sh --self-test" >&2
    exit 1
}

# ---------------------------------------------------------------------------
# The negative control.
#
# A gate that cannot fail is this repository's recurring defect, so the two
# things that could silently stop working are probed directly:
#
#   ARM 1  the LINT POSTURE. A crate that does NOT opt into the workspace
#          lints table, carrying one broken intra-doc link, must come back
#          non-zero under the flags this gate uses — and the SAME crate with
#          the link repaired must come back zero, so the arm is measuring the
#          link and not the harness. Synthetic and out-of-tree, because a gate
#          may only assert on something it BUILT (issue 1465) and because
#          editing a tracked file to test a gate re-stales every fixture.
#
#   ARM 2  the REACH. The scope really is wider than `NROS_RUSTDOC_CRATES`:
#          every deployed crate is in it, and it holds crates that are not.
#          Without this, arm 1 would still pass over a gate that had quietly
#          narrowed to the six the other gate already covers (issue 0196's
#          shape, which this phase family keeps meeting).
self_test() {
    # NOT `local`: the EXIT trap runs after this frame has popped, and a
    # `local tmp` leaves it reading an unbound name under `set -u`.
    local rc out
    tmp="$(mktemp -d)"
    trap 'rm -rf "${tmp:-}"' EXIT

    mkdir -p "$tmp/probe/src"
    cat > "$tmp/probe/Cargo.toml" <<'TOML'
[package]
name = "nros-rustdoc-probe"
version = "0.0.0"
edition = "2021"

[lib]
path = "src/lib.rs"
TOML
    # `--locked` is injected project-wide by the `scripts/bin/cargo` PATH shim,
    # so a lockless probe would fail for a reason that is not the probe's.
    cat > "$tmp/probe/Cargo.lock" <<'LOCK'
version = 3

[[package]]
name = "nros-rustdoc-probe"
version = "0.0.0"
LOCK

    _probe() {
        # $1 = doc line, prints rc
        printf '//! %s\npub fn thing() {}\n' "$1" > "$tmp/probe/src/lib.rs"
        rc=0
        out="$(RUSTDOCFLAGS="-D warnings" cargo doc --quiet --no-deps \
                   --manifest-path "$tmp/probe/Cargo.toml" \
                   --target-dir "$tmp/target" 2>&1)" || rc=$?
        printf '%s' "$out" > "$tmp/last-out"
        echo "$rc"
    }

    note "self-test arm 1a: a broken intra-doc link in a crate with NO lints table"
    rc="$(_probe 'See [`no_such_item_anywhere`] for the rule.')"
    if [ "$rc" = "0" ]; then
        echo "  FAIL — rustdoc exited 0 over a broken link. The deny posture is not" >&2
        echo "  reaching a crate that does not opt into the workspace lints table," >&2
        echo "  which is exactly what this gate exists to cover." >&2
        cat "$tmp/last-out" >&2
        return 1
    fi
    note "  ok — rc=$rc"

    note "self-test arm 1b: the same crate with the link repaired"
    rc="$(_probe 'See `no_such_item_anywhere` for the rule.')"
    if [ "$rc" != "0" ]; then
        echo "  FAIL — rc=$rc on a probe with nothing wrong with it, so arm 1a" >&2
        echo "  proved nothing about links. Harness output:" >&2
        cat "$tmp/last-out" >&2
        return 1
    fi
    note "  ok — rc=0"

    note "self-test arm 2: the scope is WIDER than the deployed set"
    # shellcheck source=scripts/build/rustdoc-set.sh
    source scripts/build/rustdoc-set.sh
    local members missing_deployed=() extra=0 name
    if ! members="$(cargo metadata --no-deps --format-version 1 \
                    | python3 -c 'import json,sys; print("\n".join(p["name"] for p in json.load(sys.stdin)["packages"]))')"; then
        echo "  FAIL — cannot read workspace members from cargo metadata" >&2
        return 1
    fi
    # Set membership in bash, not `grep -q`: issue 0726's rule is that a
    # conditional must tell a NON-MATCH from a tool that failed to start, and
    # the cheapest way to obey it is to fork nothing.
    local -A is_member=() is_deployed=()
    while IFS= read -r name; do
        [ -n "$name" ] && is_member["$name"]=1
    done <<< "$members"
    for name in "${NROS_RUSTDOC_CRATES[@]}"; do
        is_deployed["$name"]=1
        [ -n "${is_member[$name]:-}" ] || missing_deployed+=("$name")
    done
    if [ "${#missing_deployed[@]}" -ne 0 ]; then
        echo "  FAIL — deployed crate(s) absent from the workspace: ${missing_deployed[*]}" >&2
        return 1
    fi
    for name in "${!is_member[@]}"; do
        [ -n "${is_deployed[$name]:-}" ] || extra=$((extra + 1))
    done
    if [ "$extra" -lt 1 ]; then
        echo "  FAIL — the workspace scope holds no crate outside the deployed set," >&2
        echo "  so this gate measures nothing the deployed lane does not already." >&2
        return 1
    fi
    note "  ok — ${#NROS_RUSTDOC_CRATES[@]} deployed crates, $extra more workspace member(s) beyond them"

    note "self-test: PASS"
}

run_gate() {
    # shellcheck source=scripts/build/rustdoc-set.sh
    source scripts/build/rustdoc-set.sh

    local missing=() names=() row path name why list
    mapfile -t missing < <(nros_rustdoc_missing_workspace_sources)
    if [ "${#missing[@]}" -ne 0 ]; then
        for row in "${missing[@]}"; do
            IFS='|' read -r path name why <<< "$row"
            names+=("$name")
            echo "rustdoc-workspace: NOT VERIFIED — vendored source '$name' is not provisioned here." >&2
            echo "    expected at: $path" >&2
            echo "    needed by:   $why" >&2
        done
        list="$(IFS=','; printf '%s' "${names[*]}")"
        if [ "${NROS_RUSTDOC_WORKSPACE_STRICT:-0}" = "1" ]; then
            echo "rustdoc-workspace: FAILED — NROS_RUSTDOC_WORKSPACE_STRICT=1 and the source(s)" >&2
            echo "  above are absent. This lane DECLARES that it provisions them, so their" >&2
            echo "  absence is the lane's regression and not the author's: see gate.yml's" >&2
            echo "  \"Provision compile-tier sources\" step, which runs before this gate." >&2
            echo "  Nothing the author pushes can fix it." >&2
            return 1
        fi
        if [ -n "${GITHUB_ACTIONS:-}${CI:-}" ]; then
            echo "  In CI: THIS JOB did not provision it, so the workspace doc links were not" >&2
            echo "  checked. A lane that does provision them sets" >&2
            echo "  NROS_RUSTDOC_WORKSPACE_STRICT=1, which turns this skip back into a red." >&2
        else
            echo "  Locally: 'nros setup --source ${names[0]}' (or the matching" >&2
            echo "  'git submodule update --init') turns this skip into a verdict. This is" >&2
            echo "  not a finding about your documentation." >&2
        fi
        # shellcheck source=scripts/build/check-skip.sh
        source scripts/build/check-skip.sh
        nros_check_skip rustdoc-workspace \
            "vendored source(s) not provisioned: ${list} — rustdoc never ran, so the workspace's doc links were NOT checked"
        return 0
    fi

    # issue 1249 — a status this code means to INSPECT is captured, never read
    # back from a bare call. `mapfile < <(…)` would discard the helper's exit
    # status entirely, so the derivation runs into a variable first.
    local scope_text scope=() rc=0
    scope_text="$(nros_rustdoc_workspace_scope_args)" || rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "rustdoc-workspace: FAILED — cannot derive the workspace scope (rc=$rc)." >&2
        return 1
    fi
    mapfile -t scope <<< "$scope_text"
    if [ "${#scope[@]}" -lt 2 ]; then
        echo "rustdoc-workspace: FAILED — the derived scope is '${scope[*]}', which excludes" >&2
        echo "  nothing. embedded-only-members.sh is supposed to refuse an empty list; an" >&2
        echo "  empty one here would document crates that cannot build for the host and" >&2
        echo "  fail naming a crate nobody touched." >&2
        return 1
    fi

    # `-D warnings` for the reason in the header: the workspace lints table
    # reaches only the crates that opt in, and the ones that do not would
    # otherwise warn their way to a green.
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --quiet \
        --features "$NROS_RUSTDOC_FEATURES" \
        "${scope[@]}" || return 1

    local count
    count="$(printf '%s\n' "${scope[@]}" | grep -c -- '--exclude' || true)"
    echo "rustdoc-workspace OK — every workspace crate documents cleanly (${count} embedded-only crate(s) excluded)."
}

case "${1:-}" in
    --self-test)
        SELF_TEST_VERBOSE=1
        self_test
        ;;
    "")
        # The control FIRST, as a bare call at statement position — the one
        # spelling `check-gate-selftests` can read in shell, and it can read
        # only that one because every other spelling was indistinguishable
        # from a control hidden behind a flag. A gate whose green looks the
        # same as a gate that cannot go red is the recurring defect here, so
        # the proof that it CAN go red is paid on every run (~2 s of 8.6 s).
        self_test || control_refused
        run_gate
        ;;
    -h|--help) usage; exit 0 ;;
    *) usage; exit 2 ;;
esac
