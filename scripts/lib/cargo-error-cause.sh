# shellcheck shell=bash
# The first cargo error on stdin, with its root cause appended (issue 1898).
#
# cargo prints `error: <what failed>` and then a `Caused by:` chain whose LAST
# block says why: a network timeout, an unknown tag, an `--offline` refusal. A
# bare `grep -m1 '^error'` keeps the first line and drops the reason, so a
# network flake and a real resolution defect read the same.
#
# Shell twin of `nros_cargo_profile::diagnostic::first_error_with_cause` with
# `pick = starts_with("error")`; `diagnostic::tests::the_shell_twin_agrees`
# holds the two to the same answers. Prints nothing when there is no error.
#
#   reason="$(printf '%s\n' "$out" | nros_cargo_first_error)"
nros_cargo_first_error() {
    awk '
        function trim(s) { sub(/^[ \t\r]+/, "", s); sub(/[ \t\r]+$/, "", s); return s }
        found == 0 {
            t = trim($0)
            if (t ~ /^error/) { found = 1; line = t; done = (index(t, " (root cause: ") > 0) }
            next
        }
        done { next }
        /^(error|warning)/ { done = 1; next }
        want_next {
            t = trim($0)
            if (t != "") { cause = t; want_next = 0 }
            next
        }
        {
            t = trim($0)
            if (substr(t, 1, 10) == "Caused by:") {
                rest = trim(substr(t, 11))
                if (rest != "") { cause = rest } else { want_next = 1 }
            }
        }
        END {
            if (!found) exit 0
            if (cause != "") print line " (root cause: " cause ")"; else print line
        }
    '
}
