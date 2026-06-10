#!/usr/bin/env bash
# Phase 196.2 — guard the canonical `nros codegen` invocation shape.
#
# Phase 195 made `nros codegen --args-file …` the canonical low-level codegen
# entrypoint (the old top-level `nros --args-file …` was dropped). 195.D switched
# the in-tree consumers — but missed `zephyr/cmake/nros_generate_interfaces.cmake`
# (Phase 196.1), which silently broke every Zephyr interface build until a live
# CI run surfaced `error: unexpected argument '--args-file'`. This static check
# makes that exact regression un-mergeable.
#
# The signature is precise: any line that drives the codegen tool with
# `--args-file` MUST carry the `codegen` subcommand token before it. The
# user-facing verbs (`nros generate-rust`, `nros generate cpp`) are untouched —
# they don't use `--args-file`, so they never trip this.
#
# Scope = superproject-owned build glue. `third-party/` (vendored) and the
# codegen CLI's own source — historically `packages/codegen/` (retired
# submodule), now the in-tree sub-workspace `packages/cli/` — are excluded:
# the CLI *implements* the `--args-file` contract (doc comments, clap flag
# parsing, error strings), so its mentions are the definition, not a legacy
# invocation to guard. Exits non-zero listing every offending line.
set -uo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.." # repo root

# Build-glue file shapes that can invoke the codegen tool.
mapfile -t files < <(
    git ls-files \
        '*.cmake' '*CMakeLists.txt' '*.just' 'justfile' '*.sh' '*.rs' \
    | grep -vE '^(third-party/|packages/codegen/|packages/cli/)' \
    | grep -vxF 'scripts/ci/codegen-invocation-check.sh' # self (docs mention --args-file)
)

bad=()
for f in "${files[@]}"; do
    [ -f "$f" ] || continue
    while IFS= read -r line; do
        lineno="${line%%:*}"
        text="${line#*:}"
        # Canonical shape carries ` codegen ` before `--args-file`. Anything else
        # invoking `--args-file` is the legacy top-level form (the 196.1 bug).
        if [[ "$text" == *codegen*--args-file* ]]; then
            continue
        fi
        bad+=("$f:$lineno:$text")
    done < <(grep -nE -- '--args-file' "$f" 2>/dev/null)
done

if [ "${#bad[@]}" -ne 0 ]; then
    echo "ERROR: legacy codegen invocation(s) found — use 'nros codegen --args-file …'" >&2
    echo "       (Phase 195 dropped the top-level '--args-file'; see Phase 196.2):" >&2
    printf '  %s\n' "${bad[@]}" >&2
    exit 1
fi

echo "codegen-invocation-check: OK (all --args-file callers use the 'codegen' subcommand)"
