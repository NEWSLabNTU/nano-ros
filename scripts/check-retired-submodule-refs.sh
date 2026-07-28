#!/usr/bin/env bash
#
# Issue 0336 — drift gate for retired submodule paths.
#
# RFC-0060 collapsed the CLI's two vendored launch submodules into ONE pin,
# `packages/cli/third-party/ros-launch-resolve` (which nests ros-launch-manifest
# + parser). The tree was swept; nine documented copies of the old bootstrap
# command, scripts/bootstrap.sh itself, and eight .github workflow references
# were NOT — so `scripts/bootstrap.sh` silently did nothing and a fresh clone
# could not build the CLI (0336), while remote pr-checks stayed red for 60+ runs
# (0337).
#
# One grep would have caught all of it at review time. That grep is this file.
#
# Extend RETIRED[] whenever a submodule or vendored path is retired — the point
# is that a path can only be retired ONCE, so the cost of a permanent entry is
# a few microseconds and the benefit is that the next sweep cannot be partial.

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

# path-regex : what replaced it
RETIRED=(
    "packages/cli/third-party/ros-launch-manifest|packages/cli/third-party/ros-launch-resolve (which nests it — a scoped init must be --recursive)"
    "packages/cli/third-party/play_launch_parser|the ros-launch-resolve pin (RFC-0060); play_launch_parser is a TEST-tier tool from ~/.nros/sdk"
)

# Paths that legitimately still mention a retired path:
#   docs/issues/**            — the issues that RECORD the drift
#   docs/development/audit-*  — audit findings quoting the broken command
#   docs/roadmap/**           — phase history is a record, not an instruction
#   docs/design/**            — RFCs describe the before/after
#   packages/cli/third-party/ — the real nested path lives under here
#   */Cargo.toml              — path deps THROUGH the resolver are correct
EXCLUDES=(
    ':!docs/issues'
    ':!docs/issues/**'
    ':!docs/development/audit-findings-*'
    ':!docs/roadmap/**'
    ':!docs/design/**'
    ':!packages/cli/third-party/**'
    ':!scripts/check-retired-submodule-refs.sh'
)

fail=0
for entry in "${RETIRED[@]}"; do
    path="${entry%%|*}"
    replacement="${entry#*|}"
    # A reference is only a violation when it points AT the retired path, not
    # when it passes THROUGH the live one (…/ros-launch-resolve/third-party/…).
    if hits=$(git grep -n -- "$path" "${EXCLUDES[@]}" 2>/dev/null | grep -v "ros-launch-resolve/third-party" || true); [ -n "$hits" ]; then
        echo "RETIRED PATH still referenced: $path"
        echo "  replaced by: $replacement"
        echo "$hits" | sed 's/^/    /'
        echo
        fail=1
    fi
done

if [ "$fail" -ne 0 ]; then
    cat <<'EOF'
A retired submodule path is still referenced outside the docs that record it.
Every reference is a place a user or a CI job will follow into a path that no
longer exists — that is issue 0336 (fresh clone cannot build the CLI) and the
.github half of 0337 (pr-checks red for 60+ runs).
EOF
    exit 1
fi

echo "retired-submodule refs OK — ${#RETIRED[@]} retired path(s), no live references"
