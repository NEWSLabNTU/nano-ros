#!/usr/bin/env bash
# Build-host absolute-path guard (issues 0320 / 0334).
#
# A tracked file naming `/home/<someone>` or `/Users/<someone>` only works on
# the machine that wrote it. Two ways this got into the tree:
#
#   * a hardcoded SDK path in the test harness, which pinned both the SDK
#     version and the host tuple and failed with a bare ENOENT anywhere else
#     (issue 0334);
#   * committed SystemModels whose `meta.inputs[].path` was recorded absolute
#     because the resolver inferred the bringup root instead of being told it
#     (issue 0320 — fixed by `--bringup-root`).
#
# Neither was caught at review: an absolute path looks unremarkable in a diff.
# A grep is what would have caught both, so this is that grep.
#
# SCOPE: code and configuration only — `.md` is excluded deliberately.
# Documentation legitimately shows example paths (`/home/user/project/...` in
# packages/cli/docs/CLI_REFERENCE.md), and historical roadmap notes reference a
# retired checkout. Those are prose, not something a build consumes.
set -euo pipefail

cd "$(dirname "$0")/../.."

# `git grep` = tracked files only; untracked build output is irrelevant here.
hits="$(git grep -nE '/home/|/Users/' -- examples/ packages/ | grep -vE '\.md:' || true)"

if [ -n "$hits" ]; then
    echo "[FAIL] build-host absolute paths in tracked code/config:" >&2
    echo "$hits" | sed 's/^/  /' >&2
    echo >&2
    echo "  These only resolve on the machine that wrote them." >&2
    echo "  - test harness: resolve through an env var or project_root(), and" >&2
    echo "    glob anything version- or host-specific (issue 0334)." >&2
    echo "  - SystemModels: regenerate with 'nros-launch-resolve --bringup-root" >&2
    echo "    <pkg dir>' so meta.inputs[].path stays relative (issue 0320)." >&2
    exit 1
fi

echo "[OK] no build-host absolute paths in tracked code/config"
