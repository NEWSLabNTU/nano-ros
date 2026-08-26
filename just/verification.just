set working-directory := '..'

# Verification module — install and run formal verification tools (Kani + Verus).

# The Kani version this repo's proofs are verified with. PINNED, and low on
# purpose: 0.62.0's `kani-driver` needs only GLIBC_2.34, so it runs on Ubuntu
# 22.04 (glibc 2.35) and 24.04 alike. 0.67.0's needs GLIBC_2.39 — 24.04 only —
# and it is the ONLY binary in that bundle over the line, so the failure looks
# like a broken toolchain rather than one over-linked entry point (issue 0802).
#
# `cargo install --locked` does NOT pin this: `--locked` pins the crate's own
# dependency graph, not which version you get. Without the `--version` below you
# get whatever crates.io published today, which makes "the harnesses verify" a
# claim about an unrecorded prover.
#
# Raising it is a decision (drops 22.04), not a default. Re-check with:
#   objdump -T ~/.kani/kani-<ver>/bin/kani-driver | grep -o 'GLIBC_2\.[0-9]*' | sort -V | tail -1
KANI_VERSION := "0.62.0"

default:
    @just --list --justfile {{source_file()}}

# Install Kani + Verus.
[group("setup")]
setup:
    @just verification kani
    @just verification verus

# Diagnose verification tool install status (read-only).
[group("setup")]
doctor:
    #!/usr/bin/env bash
    set +e
    fail=0
    # Ask the driver, not the filesystem — a ~/.kani directory can be stale or
    # unrunnable and still look installed (issue 0802).
    kani_ver=$(cargo kani --version 2>/dev/null | awk '{print $NF}' || true)
    if [ -z "$kani_ver" ]; then
        if [ -d "$HOME/.kani" ]; then
            echo "  [BROKEN] kani-verifier is installed but will not run (glibc? see issue 0802)"
        else
            echo "  [MISSING] kani-verifier (run: just verification kani)"
        fi
        fail=1
    elif [ "$kani_ver" != "{{KANI_VERSION}}" ]; then
        echo "  [WRONG] kani-verifier $kani_ver, this repo pins {{KANI_VERSION}} (run: just verification kani)"
        fail=1
    else
        echo "  [OK] kani-verifier: $kani_ver (pinned)"
    fi
    if [ -x tools/verus ] || command -v verus &>/dev/null; then
        ver=$(tools/verus --version 2>/dev/null | grep -i Version | head -1 | tr -d ' ' || echo "")
        echo "  [OK] verus${ver:+ ($ver)}"
    else
        echo "  [MISSING] verus (run: just verification verus)"
        fail=1
    fi
    exit $fail

# Install Kani bounded model checker (skipped if already installed).
[group("verification")]
kani:
    #!/usr/bin/env bash
    set -e
    want="{{KANI_VERSION}}"
    # A directory under ~/.kani is NOT evidence the right prover is installed:
    # the store ACCUMULATES, so a stale bundle shadows the pin exactly as issue
    # 0500's Corrosion prefixes do. Ask the driver what it is, and make it prove
    # it can run at all — 0802 was a bundle that installed fine and could not
    # execute.
    have=$(cargo kani --version 2>/dev/null | awk '{print $NF}' || true)
    if [ "$have" = "$want" ]; then
        echo "kani-verifier $have already installed (pinned)"
    else
        if [ -n "$have" ]; then
            echo "kani-verifier $have installed, but this repo pins $want — replacing."
        fi
        cargo install --locked --version "$want" kani-verifier
        cargo kani setup
        got=$(cargo kani --version 2>/dev/null | awk '{print $NF}' || true)
        if [ "$got" != "$want" ]; then
            echo "ERROR: installed kani-verifier reports '${got:-nothing}', wanted $want." >&2
            echo "       If it reports nothing, it installed but cannot RUN — check glibc:" >&2
            echo "       objdump -T ~/.kani/kani-$want/bin/kani-driver | grep -o 'GLIBC_2\.[0-9]*' | sort -V | tail -1" >&2
            exit 1
        fi
    fi

# Download Verus binary from GitHub releases to tools/verus.
[group("verification")]
verus:
    ./scripts/setup-verus.sh
