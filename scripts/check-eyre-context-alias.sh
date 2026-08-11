#!/usr/bin/env bash
# `eyre::Context` is VERSION-CONDITIONAL — use `WrapErr` / `wrap_err`.
#
# eyre 0.6.12 aliased `pub use WrapErr as Context;` unconditionally. 0.6.13 put
# that alias — and `Error`, `anyhow`, `DefaultContext`, `EyreContext` — behind
# `#[cfg(feature = "anyhow")]`, a compat feature nothing here enables. So a
# graph that resolves 0.6.13 stops compiling, and one that resolves 0.6.12 is
# fine:
#
#     error[E0599]: no method named `with_context` found for enum `Result`
#     error[E0432]: unresolved import `eyre::Context`
#
# Which graph you get is decided by whichever lockfile is in scope.
# `packages/cli/Cargo.lock` pins 0.6.12, so the CLI builds; the mixed
# workspace's runtime crate resolves fresh, got 0.6.13, and broke. That is why
# this was invisible for as long as it was: the failing path only runs on a COLD
# build, and `scripts/dev/measure-fixture-build.sh` wiping the workspace trees
# is what surfaced it (phase-340 W7, 2026-08-12).
#
# Adding `features = ["anyhow"]` is NOT the fix and does not even resolve: in
# 0.6.12 `anyhow` is an optional DEPENDENCY name, not a feature, so declaring it
# fails on the locked version. `WrapErr` is unconditional in both, so converting
# the call sites is the version-independent answer — and it is already the
# dominant idiom in this tree.
set -uo pipefail
cd "$(dirname "$0")/.."

# `git grep`, not a filesystem walk (check-no-tracked-file-find), and tracked
# files only — which is the right boundary for a pre-push gate.
hits="$(git grep -n -E 'use eyre::(\{[^}]*\b)?Context\b|eyre::Context' -- '*.rs' 2>/dev/null || true)"

if [ -n "$hits" ]; then
    echo "[FAIL] eyre's anyhow-compat \`Context\` is used somewhere:" >&2
    printf '  %s\n' "$hits" >&2
    cat >&2 <<'EOF'

It is `#[cfg(feature = "anyhow")]` in eyre >= 0.6.13, so this compiles only
against a graph that happens to resolve 0.6.12.

  use eyre::WrapErr;      not  use eyre::Context;
  .wrap_err("…")          not  .context("…")
  .wrap_err_with(|| …)    not  .with_context(|| …)

`WrapErr` is unconditional in every 0.6.x.
EOF
    exit 1
fi

echo "check-eyre-context-alias: OK (no use of the version-conditional \`eyre::Context\` alias)"
