#!/usr/bin/env bash
#
# A `ps` scan that enumerates PROCESS-GROUP MEMBERSHIP must exclude zombies.
# Issue 0853 — three sites had this idiom and all three were wrong.
#
# WHY THIS CLASS IS INVISIBLE UNTIL IT IS EXPENSIVE
#
# A zombie keeps its pid and its pgid and stays in `ps` output until its parent
# calls wait(). When the parent is a launcher we just killed, the corpses
# reparent to PID 1 — and whether they are ever reaped depends entirely on what
# PID 1 IS. Under systemd or an interactive bash they vanish immediately, so a
# state-blind predicate is correct on every developer machine. In a GitHub
# Actions `container:` job PID 1 is `tail -f /dev/null`, which never reaps, so
# the zombies are permanent and a fully-drained group reads as alive forever.
#
# That is why this cost a whole issue: the code was right everywhere anyone
# could run it, and wrong in the one environment nobody could get into.
#
# THE RULE
#
# A `ps -eo …pgid…` scan enumerates a group. Enumerating for liveness without
# `stat=` cannot distinguish "running" from "already dead", so the columns must
# include `stat=` — and once they do, the caller is forced to decide about Z
# rather than not know the question exists.
#
# A single-pid lookup (`ps -o pgid= -p "$pid"`) is deliberately NOT covered: it
# asks which group a KNOWN process is in, which is a different question, and a
# zombie's answer to it is still correct.

set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

if [ "${1:-}" = "--selftest" ]; then
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    fails=0
    printf 'ps -eo pid=,pgid= | awk "..."\n' > "$tmp/bad.sh"
    printf 'ps -eo pid=,pgid=,stat= | awk "$3 !~ /^Z/"\n' > "$tmp/good.sh"
    printf 'ps -o pgid= -p "$pid"\n' > "$tmp/lookup.sh"
    scan_file() {
        awk '/ps +-eo/ && /pgid=/ && !/stat=/ { found = 1 } END { exit found ? 1 : 0 }' "$1"
    }
    if scan_file "$tmp/bad.sh"; then
        echo "  FAIL  a zombie-blind group scan was NOT detected"; fails=$((fails + 1))
    else
        echo "  ok    a zombie-blind group scan is detected"
    fi
    if scan_file "$tmp/good.sh"; then
        echo "  ok    a scan carrying stat= passes"
    else
        echo "  FAIL  a scan carrying stat= was flagged"; fails=$((fails + 1))
    fi
    if scan_file "$tmp/lookup.sh"; then
        echo "  ok    a single-pid lookup is not covered"
    else
        echo "  FAIL  a single-pid lookup was flagged"; fails=$((fails + 1))
    fi
    [ "$fails" -eq 0 ] || { echo "selftest FAILED"; exit 1; }
    echo "check-ps-zombie-blind selftest: OK"
    exit 0
fi

# Always, not only behind --selftest: a negative control nobody runs decays into
# a comment, and this gate's whole job is to fire.
"${BASH_SOURCE[0]}" --selftest >/dev/null || {
    echo "check-ps-zombie-blind: its own selftest FAILED — the gate is not trustworthy" >&2
    exit 1
}

bad=0
scanned=0
while IFS= read -r f; do
    case "$f" in
        scripts/check-ps-zombie-blind.sh) continue ;;
    esac
    scanned=$((scanned + 1))
    if awk '/ps +-eo/ && /pgid=/ && !/stat=/ { print FILENAME ":" FNR ": " $0; found = 1 }
            END { exit found ? 1 : 0 }' "$f"; then
        :
    else
        bad=1
    fi
done < <(git ls-files '*.sh' '*.rs' '*.py' 'justfile' 'just/*.just')

if [ "$bad" -ne 0 ]; then
    cat >&2 <<'MSG'

check-ps-zombie-blind: a process-GROUP scan above omits `stat=`, so it cannot
tell a running member from a zombie.

  A zombie keeps its pgid and stays in `ps` until its parent waits for it. When
  the parent is a launcher that was just killed, the corpses reparent to PID 1 —
  and a GitHub Actions container job has `tail -f /dev/null` as PID 1, which
  never reaps. There the corpses are permanent and a group that has fully exited
  reads as alive forever.

  Fix:  ps -eo pid=,pgid=,stat= | awk -v g="$pgid" '$2 == g && $3 !~ /^Z/ { print $1 }'

  A single-pid lookup (`ps -o pgid= -p "$pid"`) is a different question and is
  not covered by this rule.  -> issue 0853
MSG
    exit 1
fi

echo "check-ps-zombie-blind OK — $scanned tracked file(s), no zombie-blind process-group scan."
