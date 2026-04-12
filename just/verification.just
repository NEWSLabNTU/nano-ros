set working-directory := '..'

# Verification module — install and run formal verification tools (Kani + Verus).

default:
    @just --list --justfile {{source_file()}}

# Install Kani + Verus.
setup:
    @just verification kani
    @just verification verus

# Diagnose verification tool install status (read-only).
doctor:
    #!/usr/bin/env bash
    set +e
    fail=0
    if command -v cargo-kani &>/dev/null && [ -d "$HOME/.kani" ]; then
        kani_ver=$(basename "$(ls -d "$HOME"/.kani/kani-* 2>/dev/null | grep -v '\.tar' | head -1)" 2>/dev/null || true)
        echo "  [OK] kani-verifier: ${kani_ver:-installed}"
    else
        echo "  [MISSING] kani-verifier (run: just verification kani)"
        fail=1
    fi
    if [ -x tools/verus/verus ] || command -v verus &>/dev/null; then
        echo "  [OK] verus"
    else
        echo "  [MISSING] verus (run: just verification verus)"
        fail=1
    fi
    exit $fail

# Install Kani bounded model checker (skipped if already installed).
kani:
    #!/usr/bin/env bash
    set -e
    if command -v cargo-kani &>/dev/null && [ -d "$HOME/.kani" ]; then
        kani_ver=$(basename "$(ls -d "$HOME"/.kani/kani-* 2>/dev/null | grep -v '\.tar' | head -1)" 2>/dev/null || true)
        echo "kani-verifier already installed ($kani_ver)"
    else
        cargo install --locked kani-verifier && cargo kani setup
    fi

# Download Verus binary from GitHub releases to tools/verus.
verus:
    ./scripts/setup-verus.sh
