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
# issue 1615 (W6): per IMPORTED PATH, use-trees expanded and comments stripped
# (`scripts/lib/per_item.py` `rust_use_paths`) — a `use eyre::{` whose `Context`
# sits on the next line was invisible to a line grep.
scan_rc=0
hits="$(python3 - <<'PY'
import re, sys
sys.path.insert(0, "scripts/lib")
import comments, file_kinds, per_item

def hits_in(text):
    code = comments.strip_comments(text, "rust")
    out = [code.count("\n", 0, off) + 1 for p, off in per_item.rust_use_paths(code)
           if p == "eyre::Context"]
    out += [code.count("\n", 0, m.start()) + 1
            for m in re.finditer(r"(?<!use )\beyre::Context\b", code)
            if not code[max(0, m.start() - 4):m.start()].endswith("use ")]
    return sorted(set(out))

assert hits_in("use eyre::{\n    Context,\n};\n") == [1]
assert hits_in("// use eyre::Context;\nuse eyre::WrapErr;\n") == []
for rel in file_kinds.files_of_kind("rust"):
    for n in hits_in(open(rel, errors="replace").read()):
        print(f"{rel}:{n}")
PY
)" || scan_rc=$?
if [ "$scan_rc" -ne 0 ]; then
    echo "[FAIL] check-eyre-context-alias: the scan itself failed (rc=$scan_rc)" >&2
    exit 1
fi

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
