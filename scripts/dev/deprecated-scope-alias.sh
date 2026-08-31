#!/usr/bin/env bash
# The `just <platform> <verb>` deprecation notice — phase-407 W3.
#
# The module recipes STAY: they are the implementation `just <verb> <scope>`
# dispatches to, and deleting them would delete the work. What is deprecated is
# the module-first SPELLING as a user surface, for one release, so there is one
# shape to learn instead of two.
#
# # Why this is one script and not a line in each recipe
#
# The notice must fire when a PERSON types `just zephyr test`, and must NOT
# fire when the tree calls the same recipe as implementation — from `just test
# zephyr`, from `build-test-fixtures-leaves`' fan-out, from `_orchestrate`, or
# as an intra-module dependency (`test: build-fixtures`). Those are the same
# recipe reached four ways, and a naive `echo` in the body prints for all of
# them: a deprecation notice attached to the tree's own internals is noise that
# teaches people to ignore deprecation notices.
#
# Nothing `just` exports distinguishes the four (verified on just 1.58: a
# recipe's environment carries no `JUST_*` at all). What DOES distinguish them
# is the process tree, and it answers exactly the right question:
#
#   * more than one `just` among our ancestors  => the tree called us, not a
#     person. Silent.
#   * the verb is absent from the invoking `just`'s own argv  => we are a
#     DEPENDENCY of the recipe that was named, not the recipe. Silent.
#   * otherwise                                  => somebody typed it. Notice.
#
# The second test is what keeps `just native test` from printing a second,
# confusing notice for the `build-fixtures` dependency it pulls in.
#
# Never fails, never changes an exit status: a deprecation that can break a
# build is a worse defect than the one it announces. Silence it wholesale with
# NROS_NO_DEPRECATION=1.
#
# Usage:  bash scripts/dev/deprecated-scope-alias.sh <module> <module-verb> <new spelling…>

set -u

module="${1:-}"
verb="${2:-}"
shift 2 2>/dev/null || true
replacement="$*"

[ -n "$module" ] && [ -n "$verb" ] || exit 0
[ "${NROS_NO_DEPRECATION:-0}" = "0" ] || exit 0

# Walk the ancestor chain, collecting the pids whose comm is `just`. /proc is
# the only reader here; a host without it (macOS) gets silence rather than a
# wrong answer, which is the safe direction for a cosmetic notice.
[ -r /proc/self/stat ] || exit 0

just_pids=""
pid="$PPID"
hops=0
while [ -n "$pid" ] && [ "$pid" -gt 1 ] && [ "$hops" -lt 32 ]; do
    hops=$((hops + 1))
    comm_file="/proc/$pid/comm"
    stat_file="/proc/$pid/stat"
    [ -r "$comm_file" ] && [ -r "$stat_file" ] || break
    if [ "$(cat "$comm_file" 2>/dev/null)" = "just" ]; then
        just_pids="$just_pids $pid"
    fi
    # field 4 of /proc/<pid>/stat is the ppid; comm (field 2) can contain
    # spaces and parentheses, so cut at the LAST ')' before splitting.
    stat_line="$(cat "$stat_file" 2>/dev/null)" || break
    pid="$(printf '%s' "${stat_line##*) }" | cut -d' ' -f2)"
    case "$pid" in ''|*[!0-9]*) break ;; esac
done

set -- $just_pids
[ "$#" -eq 1 ] || exit 0

# The one `just` above us: was this recipe NAMED, or pulled in as a dependency?
argv="$(tr '\0' ' ' < "/proc/$1/cmdline" 2>/dev/null)" || exit 0
case " $argv " in
    *" $verb "*) ;;
    *) exit 0 ;;
esac

echo "note: \`just $module $verb\` is deprecated — use \`$replacement\` (phase-407; the module recipe stays as the implementation)." >&2
exit 0
