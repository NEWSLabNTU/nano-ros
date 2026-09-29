# An empty population is not a pass — phase-472 W4. Shell spelling of
# `scripts/lib/population.py`'s `require_population`; read that docstring for
# the ten gates that were OFF while printing OK, and why.
#
# Usage (in a gate, after counting what it examined):
#   # shellcheck source=scripts/lib/population.sh
#   source "$ROOT/scripts/lib/population.sh"
#   nros_require_population "$n" "open row(s)" check-issue-index || exit 1
#   nros_require_population "$n" "things" my-gate "none exist yet, because …" || exit 1
#
# Prints the count. Returns 1 on an undeclared zero (the caller fails), 0
# otherwise. A declared zero prints NOTHING TO CHECK — a stated fact, never OK.

nros_require_population() {
    local n="${1:?nros_require_population: count}"
    local what="${2:?nros_require_population: what}"
    local gate="${3:?nros_require_population: gate}"
    local declared="${4:-}"
    if [ "$n" -gt 0 ] 2>/dev/null; then
        echo "${gate}: examined ${n} ${what}"
        return 0
    fi
    if [ -n "$declared" ]; then
        echo "${gate}: NOTHING TO CHECK — 0 ${what}. Declared: ${declared}"
        echo "  That is a measured fact about this tree, not a pass."
        return 0
    fi
    {
        echo "${gate}: FAILED — examined 0 ${what}."
        echo "  An empty population is not a pass: the gate is OFF, not green"
        echo "  (phase-472 W4). Either the population moved — its spelling, its"
        echo "  location, or the tool that reads it — and the gate must be"
        echo "  re-pointed at where it went, or the tree genuinely has none and the"
        echo "  call site must DECLARE that, with a reason."
    } >&2
    return 1
}

# The helper's own negative control. Gates call it on their normal path.
nros_require_population_self_test() {
    if nros_require_population 0 "things" probe >/dev/null 2>&1; then
        echo "nros_require_population self-test: an undeclared zero PASSED" >&2
        return 1
    fi
    if ! nros_require_population 2 "things" probe >/dev/null 2>&1; then
        echo "nros_require_population self-test: a non-zero count FAILED" >&2
        return 1
    fi
    local said
    said="$(nros_require_population 0 "things" probe "declared" 2>/dev/null)" || {
        echo "nros_require_population self-test: a declared zero FAILED" >&2
        return 1
    }
    case "$said" in
        *"NOTHING TO CHECK"*) ;;
        *)
            echo "nros_require_population self-test: a declared zero was not STATED" >&2
            return 1
            ;;
    esac
    return 0
}
