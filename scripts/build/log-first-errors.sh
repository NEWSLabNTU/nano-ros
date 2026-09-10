#!/usr/bin/env bash
# Print the FIRST error lines of a build log, before anyone prints its tail.
#
# A failing fixture printed `tail -n 80` of its log and nothing else. Under a
# parallel ninja the failing translation unit's `error:` is printed when that
# unit fails, and every unit still in flight keeps printing warnings after it,
# so the tail is the warnings and the error is above it. Run 34319241943
# (tier 2, 2026-09-09) read exactly like that: `== zephyr == FAILED`, eighty
# lines of `-Wunused-result` notes, `ninja: build stopped`, and no error line.
# issue 1158's rule, one level down: a lane's failure text must NAME the
# failure, and "it failed, here are some lines near the end" does not.
#
# One spelling, called by BOTH printers (the fixture fan-out in `justfile` and
# the Zephyr leaf scheduler in `scripts/build/zephyr-fixture-make-driver.sh`),
# because two hand-written greps drift apart and the class is "a failure
# printer shows the tail only", not either site.
#
# Usage: log-first-errors.sh <log> [max-lines]
# Always exits 0: it is a diagnostic inside an already-failing path, and a
# missing log must not replace the real exit status.
set -uo pipefail

log="${1:-}"
max="${2:-${NROS_FIXTURE_FIRST_ERRORS:-12}}"

[ -n "$log" ] && [ -f "$log" ] || exit 0

# What counts as an error line, across the toolchains a fixture log carries:
#   gcc/clang   `file:1:2: error: ...`, `fatal error: ...`
#   rustc/cargo `error: ...`, `error[E0425]: ...`
#   cmake       `CMake Error at ...`
#   west        `FATAL ERROR: ...`
#   ld          `undefined reference to ...`
#   our gates   `[FAIL] ...`
# `[-Werror=...]` inside a warning does not match: the pattern needs `error`
# followed by `:` or `[`.
pattern='(^|[^A-Za-z_-])(fatal )?error(\[E[0-9]+\])?:|CMake Error|FATAL ERROR|undefined reference to|\[FAIL\]'

matches="$(grep -n -m "$max" -E "$pattern" "$log" 2>/dev/null || true)"
if [ -n "$matches" ]; then
    printf 'first error line(s) in %s:\n' "$log"
    printf '%s\n' "$matches" | cut -c1-400 | sed 's/^/  /'
else
    printf 'no error line matched in %s — read the tail below\n' "$log"
fi
exit 0
