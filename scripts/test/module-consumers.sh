#!/usr/bin/env bash
# Issue 2997 -- the nros-tests binaries a `just <module> test` run must select:
# every consumer (`matrix::consumer_of`) of a Runtime cell of the module's
# platforms, read from the committed packages/testing/nros-tests/
# module-consumers.txt (GENERATED from `matrix::module_consumers`, held current
# by its unit test). A recipe passes these to `_nextest-platform`, adding only
# side targets of its own, so no lane can run a hand-picked subset of its cells
# again (`check platform-lane-cells` refuses a literal list).
#
#   module-consumers.sh <module>      space-separated binaries, or exit 1
#   module-consumers.sh --self-test
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
file="$root/packages/testing/nros-tests/module-consumers.txt"

consumers() { # <module> <file>
    awk -v m="$1" '!/^#/ && $1 == m { printf "%s%s", sep, $2; sep = " " } END { print "" }' "$2"
}

if [ "${1:-}" = "--self-test" ]; then
    t="$(mktemp)"
    trap 'rm -f "$t"' EXIT
    printf '# header\nqemu emulator\nqemu logging_smoke\nnuttx rtos_e2e\n' > "$t"
    [ "$(consumers qemu "$t")" = "emulator logging_smoke" ] || { echo "self-test: qemu" >&2; exit 1; }
    [ -z "$(consumers px4 "$t")" ] || { echo "self-test: unknown module" >&2; exit 1; }
    echo "module-consumers self-test: OK"
    exit 0
fi
module="${1:?usage: module-consumers.sh <module> | --self-test}"
[ -f "$file" ] || { echo "module-consumers: $file is missing" >&2; exit 1; }
out="$(consumers "$module" "$file")"
if [ -z "$out" ]; then
    echo "module-consumers: module \`$module\` owns no Runtime cell in $file" >&2
    exit 1
fi
printf '%s\n' "$out"
