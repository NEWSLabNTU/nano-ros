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
# A compiler the kernel KILLED prints no diagnostic at all, so the first
# error line is a consequence: `could not compile `core``, or a size probe
# panicking about a missing rlib three crates later. Issue 1899 measured that
# across five families at once. So a kill is counted and named FIRST, before
# the error lines it caused.
#
# Usage: log-first-errors.sh <log> [max-lines]
#        log-first-errors.sh --selftest
# Always exits 0 on a log: it is a diagnostic inside an already-failing path,
# and a missing log must not replace the real exit status.
set -uo pipefail

# What a compile killed by a signal leaves behind, per tool. A bare `Killed`
# is not here: too many things print it.
#   sccache  `sccache: Compiler killed by signal 9`
#   cargo    `(signal: 9, SIGKILL: kill)`
#   gcc      `Killed signal terminated program cc1plus`
kill_pattern='Compiler killed by signal [0-9]+|\(signal: 9, SIGKILL|Killed signal terminated program'

first_errors() {
    local log="$1" max="$2" kills matches
    [ -n "$log" ] && [ -f "$log" ] || return 0
    kills="$(grep -c -E "$kill_pattern" "$log" 2>/dev/null || true)"
    if [ "${kills:-0}" -gt 0 ]; then
        printf '%s compile(s) in %s were KILLED by a signal and printed no diagnostic;\n' "$kills" "$log"
        printf '  the error lines below are what the kills caused. Likely memory (issue 1899):\n'
        printf '  lower the build budget, or read the host kernel log for oom-kill.\n'
    fi
    # just's own `error: recipe `X` failed` is a CONSEQUENCE: every recipe the
    # failure unwinds through prints one, so in a stage log it is the line that
    # matches first and says least (issue 2193: the job log quoted
    # `295:error: recipe `build-fixtures` failed` and nothing about the leaf).
    # It is printed only when nothing else matched.
    matches="$(grep -n -E "$pattern" "$log" 2>/dev/null | grep -v -E "$consequence" | head -n "$max" || true)"
    if [ -z "$matches" ]; then
        matches="$(grep -n -m "$max" -E "$pattern" "$log" 2>/dev/null || true)"
    fi
    if [ -n "$matches" ]; then
        printf 'first error line(s) in %s:\n' "$log"
        printf '%s\n' "$matches" | cut -c1-400 | sed 's/^/  /'
    else
        printf 'no error line matched in %s — read the tail below\n' "$log"
    fi
}

# What counts as an error line, across the toolchains a fixture log carries:
#   gcc/clang   `file:1:2: error: ...`, `fatal error: ...`
#   rustc/cargo `error: ...`, `error[E0425]: ...`
#   cmake       `CMake Error at ...`
#   west        `FATAL ERROR: ...`
#   ld          `undefined reference to ...`
#   our gates   `[FAIL] ...`
#   nros/eyre   `Error: ...` -- a Rust `main` returning `eyre::Result` reports
#               with a capital E. `nros build`'s preflight refusal is one, and
#               it matched nothing until issue 2193 (tier-2 nightly 41985062052:
#               "no error line matched" over a log whose line 1 was
#               `Error: missing prerequisites for this build:`).
#   drivers     `zephyr-fixture-make-driver: <leaf> failed`,
#               `zephyr-fixture-run-one: ... failed for <image> (...)` -- the
#               line that NAMES the failing leaf, so a stage summary says which
#               one without opening a log.
# `[-Werror=...]` inside a warning does not match: the pattern needs `error`
# followed by `:` or `[`. A class name like `ValueError:` does not match
# either: the character before `Error` must not be a letter.
pattern='(^|[^A-Za-z_-])(fatal )?[eE]rror(\[E[0-9]+\])?:|CMake Error|FATAL ERROR|undefined reference to|\[FAIL\]|fixture-(make-driver|run-one): .*failed'
consequence='error: recipe `[^`]*` failed'


selftest() {
    local d fail=0 out
    d="$(mktemp -d)"
    printf 'warning: x\nsrc/a.c:1:2: error: boom\n' > "$d/plain.log"
    printf 'sccache: Compiler killed by signal 9\nerror: could not compile core (lib)\n' > "$d/kill.log"
    printf 'process didn'"'"'t exit successfully: rustc (signal: 9, SIGKILL: kill)\n' > "$d/cargo.log"
    printf 'note: Killed\nwarning: y\n' > "$d/bare.log"
    # Issue 2193, verbatim from tier-2 nightly 41985062052's leaf log.
    printf 'Error: missing prerequisites for this build:\n  - Rust target `x86_64-unknown-none` (a Rust image on board `zephyr`)\n      run: rustup target add x86_64-unknown-none\nzephyr-fixture-run-one: nros build --dry-run failed for demo_bringup:zephyr (zephyr/native_sim/native/64/workspace-entry)\n' > "$d/refusal.log"
    # A stage log: the driver's lines first, just's unwinding last.
    printf 'zephyr-fixture-make-driver: build-ws-rs-entry-zenoh failed\n  1:Error: missing prerequisites for this build:\nerror: recipe `build-fixtures` failed with exit code 2\n' > "$d/stage.log"
    printf 'error: recipe `build-fixtures` failed with exit code 2\n' > "$d/recipe-only.log"
    printf 'ValueError: not ours\nwarning: y\n' > "$d/classname.log"
    out="$(first_errors "$d/plain.log" 12)"
    case "$out" in *KILLED*) echo "selftest: a plain error read as a kill" >&2; fail=1 ;; esac
    case "$out" in *"error: boom"*) ;; *) echo "selftest: the plain error line is missing" >&2; fail=1 ;; esac
    out="$(first_errors "$d/kill.log" 12)"
    case "$out" in "1 compile(s)"*KILLED*"could not compile"*) ;;
        *) echo "selftest: an sccache kill is not named before its consequence: $out" >&2; fail=1 ;; esac
    out="$(first_errors "$d/cargo.log" 12)"
    case "$out" in *KILLED*) ;; *) echo "selftest: cargo's SIGKILL is not named" >&2; fail=1 ;; esac
    out="$(first_errors "$d/bare.log" 12)"
    case "$out" in *KILLED*) echo "selftest: a bare Killed read as a compile kill" >&2; fail=1 ;; esac
    out="$(first_errors "$d/refusal.log" 12)"
    case "$out" in *"Error: missing prerequisites"*"failed for demo_bringup:zephyr"*) ;;
        *) echo "selftest: an eyre refusal and its leaf line are not quoted: $out" >&2; fail=1 ;; esac
    out="$(first_errors "$d/stage.log" 12)"
    case "$out" in *"build-ws-rs-entry-zenoh failed"*"Error: missing prerequisites"*) ;;
        *) echo "selftest: the stage summary does not name the leaf and its error: $out" >&2; fail=1 ;; esac
    case "$out" in *"error: recipe"*) echo "selftest: just's recipe line outranked the real error: $out" >&2; fail=1 ;; esac
    out="$(first_errors "$d/recipe-only.log" 12)"
    case "$out" in *"error: recipe"*) ;;
        *) echo "selftest: with nothing else, the recipe line must still print: $out" >&2; fail=1 ;; esac
    out="$(first_errors "$d/classname.log" 12)"
    case "$out" in *ValueError*) echo "selftest: a class name read as an error line" >&2; fail=1 ;; esac
    out="$(first_errors "$d/absent.log" 12)"
    [ -z "$out" ] || { echo "selftest: a missing log printed something" >&2; fail=1; }
    rm -f "$d"/*.log && rmdir "$d"
    [ "$fail" -eq 0 ] || return 1
    echo "log-first-errors --selftest: OK (10 cases)"
}

if [ "${1:-}" = "--selftest" ]; then
    selftest
    exit $?
fi
first_errors "${1:-}" "${2:-${NROS_FIXTURE_FIRST_ERRORS:-12}}"
exit 0
