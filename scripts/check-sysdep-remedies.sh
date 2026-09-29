#!/usr/bin/env bash
# phase-327 W3 (RFC-0062, issue 0368) — remedy text derives from the index.
#
# The dependency SSoT is nros-sdk-index.toml: `nros setup --system` composes
# the native install command for the HOST's package manager, and doctors
# print entry-derived remedies. A hand-written `sudo apt …` line in a just
# recipe re-creates the drift class 0368 measured (three remedies pointed at
# apt/sudo where an index prebuilt existed) and is Debian-only besides.
#
# Scope: every justfile `just` loads — the root `justfile`, its `mod`s and
# every `import` (phase-472 W2: the shell glob `just/*.just` never read the 13
# files of `mod check` under `just/check/`). Shell scripts under scripts/ may
# keep a distro-labelled fallback line for the no-CLI bootstrap path.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib/population.sh
source "$ROOT/scripts/lib/population.sh"

# graph <root> — every justfile in the graph, repo-relative, one per line.
graph() {
    python3 "$ROOT/scripts/lib/check_just_sources.py" --list --root "$1"
}

# hits <root> <files> — `file:line:text` for every hand-written `sudo apt`.
hits() {
    (cd "$1" && printf '%s\n' "$2" | xargs grep -n 'sudo apt' -- 2>/dev/null) || true
}

# Negative control on the normal path (phase-472 W9): a remedy in a file behind
# `mod check` + `import` is found, and a clean graph is clean.
self_test() {
    local t files out
    t="$(mktemp -d)"
    mkdir -p "$t/just/check"
    printf '%s\n' "mod check 'just/check.just'" > "$t/justfile"
    printf '%s\n' "import 'check/a.just'" > "$t/just/check.just"
    printf '%s\n' "gate:" "    echo 'run: sudo apt install foo'" > "$t/just/check/a.just"
    files="$(graph "$t")"
    out="$(hits "$t" "$files")"
    case "$out" in
        *just/check/a.just:2:*) ;;
        *) echo "check-sysdep-remedies SELFTEST FAILED: a remedy in just/check/*.just was not read" >&2
           rm -rf "$t"; exit 1 ;;
    esac
    printf '%s\n' "gate:" "    nros setup --system" > "$t/just/check/a.just"
    out="$(hits "$t" "$files")"
    rm -rf "$t"
    if [ -n "$out" ]; then
        echo "check-sysdep-remedies SELFTEST FAILED: a clean graph reported: $out" >&2
        exit 1
    fi
    nros_require_population_self_test
}
self_test

rc=0; files="$(graph "$ROOT")" || rc=$?
if [ "$rc" -ne 0 ]; then
    echo "check-sysdep-remedies: could not read the justfile graph" >&2
    exit 1
fi
n="$(printf '%s\n' "$files" | grep -c .)" || true
nros_require_population "$n" "justfile(s)" check-sysdep-remedies || exit 1
out="$(hits "$ROOT" "$files")"
if [ -n "$out" ]; then
    echo "ERROR: hand-written 'sudo apt' remedy in a just recipe — declare the"
    echo "package in nros-sdk-index.toml [system.*] and point the remedy at"
    echo "'nros setup --system' instead (phase-327 W3 / issue 0368):"
    echo "$out" | sed 's/^/  /'
    exit 1
fi
echo "sysdep remedies OK (no hand-written 'sudo apt' in $n justfile(s))"
