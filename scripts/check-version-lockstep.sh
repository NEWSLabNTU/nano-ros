#!/usr/bin/env bash
# Phase 218.J — JetPack-style bundle versioning lockstep check.
#
# The runtime workspace at `Cargo.toml` and the CLI sub-workspace at
# `packages/cli/Cargo.toml` MUST carry the same `[workspace.package].
# version`. The Phase 218.E ABI guard relies on this equality for its
# strict CLI-binary ↔ consumer-Cargo.lock check, and the JetPack
# bundle release model treats the two workspaces as a single product
# (one `git tag nros-v<X.Y.Z>` → one release with CLI binaries + the
# runtime checkout at that tag).
#
# Exits 0 if the versions match. Exits 1 with both versions printed
# if they diverge. Wired into `.github/workflows/lint.yml`; also runs
# locally on `just release-bump <X.Y.Z>` for fail-fast confirmation.
#
# Use `just release-bump <X.Y.Z>` to bump both files atomically.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Extract `[workspace.package].version`. The regex anchors on the
# `[workspace.package]` section header so a stray `version = "..."`
# under `[package]` or `[dependencies]` doesn't trip the match.
extract_version() {
    local toml="$1"
    awk '
        /^\[workspace\.package\]/ { in_section = 1; next }
        /^\[/                     { in_section = 0 }
        in_section && /^version[ \t]*=[ \t]*"/ {
            match($0, /"[^"]*"/)
            print substr($0, RSTART + 1, RLENGTH - 2)
            exit
        }
    ' "$toml"
}

root_ver="$(extract_version "$root/Cargo.toml")"
cli_ver="$(extract_version "$root/packages/cli/Cargo.toml")"

if [ -z "$root_ver" ]; then
    echo "check-version-lockstep: failed to read [workspace.package].version from Cargo.toml" >&2
    exit 1
fi
if [ -z "$cli_ver" ]; then
    echo "check-version-lockstep: failed to read [workspace.package].version from packages/cli/Cargo.toml" >&2
    exit 1
fi

if [ "$root_ver" != "$cli_ver" ]; then
    cat >&2 <<EOF
check-version-lockstep: bundle version mismatch (Phase 218.J).
  Cargo.toml                       [workspace.package].version = "$root_ver"
  packages/cli/Cargo.toml          [workspace.package].version = "$cli_ver"

Both files MUST carry the same bundle version. Use:
  just release-bump <X.Y.Z>
to bump both atomically. See docs/development/versioning.md.
EOF
    exit 1
fi

echo "check-version-lockstep: nano-ros bundle version $root_ver (root + cli in lockstep)"
