# nros_grep_q — `grep -q` that cannot report a tool failure as a finding.
#
# Issue 0726. `grep` exits 1 for "no match" and >=2 for an ERROR, and the
# natural spellings cannot tell them apart:
#
#     if ! printf '%s' "$text" | grep -q "$pat"; then   # error => false finding
#     if   printf '%s' "$text" | grep -q "$pat"; then   # error => check skipped
#
# Both are wrong and they fail in OPPOSITE directions. The first was a real
# defect in `check-rmw-force-link-anchor`: under a 32-way gate fan-out a forked
# grep failed to start, and the gate reported a missing force-link anchor for an
# example that has one — a confident, specific, false claim about the source
# tree. It only ever failed green->red under load, which is the direction that
# teaches people to re-run a gate rather than believe it.
#
# Usage mirrors `grep -q`, and the caller branches on the STATUS:
#
#     nros_grep_q "$pattern" "$file"          # or: … <<<"$text"
#     case $? in
#         0) : ;;                             # matched
#         1) report_the_finding ;;            # genuinely absent
#     esac                                    # >=2 never returns — it exits 2
#
# A tool failure exits the whole script with 2 rather than returning, because
# every caller of this helper is a checker: continuing past a grep that did not
# run means producing a verdict from missing evidence, and there is no useful
# way for a caller to "handle" that.
nros_grep_q() {
    local pat="${1:?nros_grep_q: pattern}"
    shift
    local rc
    if [ "$#" -gt 0 ]; then
        grep -q -- "$pat" "$@"
        rc=$?
    else
        grep -q -- "$pat"
        rc=$?
    fi
    if [ "$rc" -ge 2 ]; then
        echo "FATAL: grep failed (rc=$rc) searching for: $pat" >&2
        echo "       This is a TOOL failure, not a finding. Refusing to draw a" >&2
        echo "       conclusion from a grep that did not run (issue 0726)." >&2
        exit 2
    fi
    return "$rc"
}
