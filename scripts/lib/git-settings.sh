#!/usr/bin/env bash
# Reader for config/git-settings.txt — the ONE list of repo-local git settings.
#
# Issue 0840. Sourced by `just setup-hooks` (applies them) and `just doctor`
# (verifies them), so the two cannot disagree the way the rust-target lists did
# in issue 0833.

# nros_git_settings [severity] — print `<key>\t<value>\t<severity>` per line.
#   severity: "all" (default), "required", or "advisory"
nros_git_settings() {
    local want="${1:-all}"
    local root
    root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
    local list="$root/config/git-settings.txt"
    [ -r "$list" ] || {
        echo "nros_git_settings: missing $list" >&2
        return 2
    }
    awk -v want="$want" '
        /^[[:space:]]*#/ || /^[[:space:]]*$/ { next }
        (want == "all" || $3 == want) { printf "%s\t%s\t%s\n", $1, $2, $3 }
    ' "$list"
}
