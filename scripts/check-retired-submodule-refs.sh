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
    "packages/cli/third-party/ros-launch-manifest|a git-tag cargo dep on ros-launch-manifest (phase-332 W2; it no longer nests anywhere)"
    "packages/cli/third-party/play_launch_parser|the play_launch pin (RFC-0060/phase-332); play_launch_parser is a TEST-tier tool from ~/.nros/sdk"
    # phase-332 W1 — the launch pin was repointed from the ros-launch-resolve
    # repo to play_launch (layer 2 is regular files at src/ros-launch-resolve
    # inside it; init NON-recursively — no --recursive, no nested rlm). The
    # rename shipped with this gate still one generation behind, so bootstrap,
    # activate hints, README, nine book pages, ci-conventions, and nine
    # workflow init lines all kept the retired path — 0336's exact shape again.
    "packages/cli/third-party/ros-launch-resolve|packages/cli/third-party/play_launch, init NON-recursively: git submodule update --init packages/cli/third-party/play_launch (phase-332 W1)"
    # phase-321 W2.d — the RMW backends were regrouped under packages/rmw/.
    # These are here because the cyclonedds move SHIPPED with two live stale
    # refs (`root.join("packages/dds")` in nros-tests/src/zephyr.rs): the
    # rewrite matched `packages/dds/` WITH a trailing slash, and `just check`
    # plus 817 unit tests stayed green because that resolver path is only
    # exercised by a Zephyr fixture build. A path can only be retired once, so
    # a permanent entry costs nothing and closes the class.
    "packages/zpico|packages/rmw/zenoh"
    "packages/dds|packages/rmw/cyclonedds"
    "packages/px4|packages/rmw/uorb"
    "packages/bridge|packages/rmw/bridge"
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
    ':!docs/superpowers/**'
    ':!packages/cli/third-party/**'
    ':!scripts/check-retired-submodule-refs.sh'
)

# Retired COMMAND SPELLINGS — same drift class as retired paths, but a verb
# rather than a directory. Issue 0367: phase-265 renamed `nros ws sync` to
# `nros sync` yet the old spelling kept re-entering new prose because greppable
# precedent is how text gets written here. The whole class is `\bws sync\b`
# (issue-0196 rule: gate the class, not the exact string swept today).
#   regex : what replaced it
RETIRED_SPELLINGS=(
    "\\bws sync\\b|nros sync (phase-265 renamed the verb; the \`nros ws sync\` alias is retired — issue 0367)"
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

for entry in "${RETIRED_SPELLINGS[@]}"; do
    regex="${entry%%|*}"
    replacement="${entry#*|}"
    if hits=$(git grep -nE -- "$regex" "${EXCLUDES[@]}" 2>/dev/null || true); [ -n "$hits" ]; then
        echo "RETIRED COMMAND SPELLING still used: $regex"
        echo "  use instead: $replacement"
        echo "$hits" | sed 's/^/    /'
        echo
        fail=1
    fi
done

if [ "$fail" -ne 0 ]; then
    cat <<'EOF'
A retired submodule path or command spelling is still referenced outside the docs
that record it. Every reference is a place a user or a CI job will follow into a
path/verb that no longer exists — that is issue 0336 (fresh clone cannot build the
CLI), the .github half of 0337 (pr-checks red for 60+ runs), and issue 0367 (the
`nros ws sync` ghost re-entering new prose).
EOF
    exit 1
fi

echo "retired refs OK — ${#RETIRED[@]} retired path(s) + ${#RETIRED_SPELLINGS[@]} retired spelling(s), no live references"
