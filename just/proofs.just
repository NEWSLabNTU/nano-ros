set working-directory := '..'

# Phase 211 — proof-carrying-code (wcr + Creusot + Why3) workspace recipes.
# Wraps `wcr` (installed via `nros setup --tool wcr-stack` from the standalone
# `github.com/NEWSLabNTU/wcr` repo). All recipes assume `~/.nros/bin/wcr` is on
# PATH (the shim from scripts/install-nros.sh handles version resolution).
#
# Recipes are informational-only in PoC (do not gate `test-all`). Promotion to
# a gating tier lands at Phase 211.6+ once Verus retirement closes.

default:
    @just --list --justfile {{source_file()}}

# Verify proofs across the whole workspace (extract + replay cached, prove
# missing). Cold ~ 3 min, warm-cache ~ 30 s targets per Phase 211.4 acceptance.
[group("verify")]
verify-proofs:
    wcr verify

# Verify proofs for a single crate. CRATE is the workspace member name.
[group("verify")]
verify-proofs-crate CRATE:
    wcr verify -p {{CRATE}}

# Re-discharge cached sessions only — no fresh proof search. Use after a pull
# that bumps a dep spec hash but you trust the prior author's discharge.
[group("verify")]
verify-proofs-replay:
    wcr replay

# Workspace summary report — per-crate VC counts, dep graph, trusted-stub
# inventory, prover stats. Output goes to stdout (machine-readable JSON via
# `wcr report --json`).
[group("verify")]
proofs-report:
    wcr report

# DOT-format spec dep graph (per C2 cycle-detection mechanism) for inspection.
# Pipe through `dot -Tsvg` to render.
[group("verify")]
proofs-deps CRATE:
    wcr deps -p {{CRATE}}

# Produce .wcr.tar.zst bundles for every proof-enabled workspace crate. Output
# lands under target/proofs/bundles/. Bundle format frozen at Phase 211.4 per
# docs/proofs/schemas/bundle-layout-v0.1.md.
[group("release")]
proofs-bundle:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p target/proofs/bundles
    for c in $(wcr list-crates); do
        wcr bundle -p "$c" -o target/proofs/bundles/
    done

# Bundle a single crate.
[group("release")]
proofs-bundle-crate CRATE:
    wcr bundle -p {{CRATE}} -o target/proofs/bundles/

# Clear the per-VC discharge cache. Useful when a Why3 / prover bump
# invalidates results and you want a clean re-discharge.
[group("verify")]
clean-proofs:
    rm -rf target/proofs/cache

# Doctor probe — verifies the wcr toolchain is provisioned and that
# pinned versions match nros-sdk-index.toml.
[group("setup")]
doctor:
    @sh -c 'command -v wcr >/dev/null 2>&1 && echo "[OK] wcr: $(command -v wcr)" || echo "[MISSING] wcr — run: nros setup --tool wcr-stack"'
    @sh -c 'command -v creusot >/dev/null 2>&1 && echo "[OK] creusot: $(command -v creusot)" || echo "[MISSING] creusot — bundled in wcr-stack"'
    @sh -c 'command -v why3 >/dev/null 2>&1 && echo "[OK] why3: $(command -v why3)" || echo "[MISSING] why3 — bundled in wcr-stack"'
