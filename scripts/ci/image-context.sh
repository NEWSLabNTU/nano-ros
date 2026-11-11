#!/usr/bin/env bash
# scripts/ci/image-context.sh <runner> <dir> -- write an image's build context
# to <dir> and nothing else: no engine, no labels, no runner name. The CLI face
# of scripts/ci/lib/image-context.sh (RFC-0127 D1, phase-551 W1), for the
# golden gate and for an operator who wants to read what an image would be
# built from. `runner-container.sh` writes the runner's context the same way,
# through the same functions.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
case "${1:-}" in
    runner) ;;
    *) echo "usage: image-context.sh runner <dir>" >&2; exit 2 ;;
esac
[ -n "${2:-}" ] || { echo "usage: image-context.sh runner <dir>" >&2; exit 2; }
# shellcheck source=scripts/ci/lib/image-context.sh
. "$REPO_ROOT/scripts/ci/lib/image-context.sh"
nros_image_resolve
nros_runner_context "$2"
