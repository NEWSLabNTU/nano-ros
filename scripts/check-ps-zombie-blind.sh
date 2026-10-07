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
#
# TWO SPELLINGS (issue 1544). The shell form is `ps -eo pid=,pgid=`. A Rust or
# Python caller spells the same scan as an ARGV — `.args(["-eo", "pid,pgid",
# "--no-headers"])` / `["ps", "-eo", "pid,pgid"]` — with no `ps -eo` substring
# and no `pgid=` either, so the first version of this gate read `*.rs` and
# `*.py` and could match neither. `nros-tests/src/process.rs` had one, feeding an
# assertion that a group was still ALIVE. The argv form is matched on a quoted
# `"-eo"` whose column list (same line or the next two) names `pgid` without
# `stat`.

set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

# ONE matcher, used by the selftest AND the scan — a selftest that exercises a
# copy is not a control on the thing that runs (phase-472 W9).
#   shell spelling: a `ps -eo` line naming `pgid=` and not `stat=`.
#   argv spelling:  a quoted "-eo"; the column list is the rest of that line
#                   plus the next two, and must name pgid without stat.
SCAN_AWK='
function report(file, line, text) { print file ":" line ": " text; found = 1 }
function flush() {
    if (pending > 0 && win ~ /pgid/ && win !~ /stat/) report(sfile, start, stext)
    pending = 0
}
FNR == 1 { flush() }
/ps +-eo/ && /pgid=/ && !/stat=/ { report(FILENAME, FNR, $0) }
pending > 0 {
    win = win " " $0
    if (--pending == 0 && win ~ /pgid/ && win !~ /stat/) report(sfile, start, stext)
}
/"-eo"/ { flush(); win = $0; start = FNR; stext = $0; sfile = FILENAME; pending = 2 }
END { flush(); exit found ? 1 : 0 }'

if [ "${1:-}" = "--selftest" ]; then
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    fails=0
    printf 'ps -eo pid=,pgid= | awk "..."\n' > "$tmp/bad.sh"
    printf 'ps -eo pid=,pgid=,stat= | awk "$3 !~ /^Z/"\n' > "$tmp/good.sh"
    printf 'ps -o pgid= -p "$pid"\n' > "$tmp/lookup.sh"
    printf 'Command::new("ps")\n    .args(["-eo", "pid,pgid", "--no-headers"])\n' > "$tmp/bad.rs"
    printf 'Command::new("ps")\n    .args([\n        "-eo",\n        "pid,pgid",\n    ])\n' > "$tmp/bad-split.rs"
    printf 'Command::new("ps")\n    .args(["-eo", "pid=,pgid=,stat="])\n' > "$tmp/good.rs"
    printf 'subprocess.run(["ps", "-eo", "pid,comm"])\n' > "$tmp/nopgid.py"
    scan_file() {
        awk "$SCAN_AWK" "$1" >/dev/null
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
    if scan_file "$tmp/bad.rs"; then
        echo "  FAIL  a zombie-blind ARGV group scan (Rust) was NOT detected"; fails=$((fails + 1))
    else
        echo "  ok    a zombie-blind argv group scan is detected"
    fi
    if scan_file "$tmp/bad-split.rs"; then
        echo "  FAIL  an argv scan split across lines was NOT detected"; fails=$((fails + 1))
    else
        echo "  ok    an argv scan split across lines is detected"
    fi
    if scan_file "$tmp/good.rs"; then
        echo "  ok    an argv scan carrying stat= passes"
    else
        echo "  FAIL  an argv scan carrying stat= was flagged"; fails=$((fails + 1))
    fi
    if scan_file "$tmp/nopgid.py"; then
        echo "  ok    an argv scan that enumerates no group is not covered"
    else
        echo "  FAIL  an argv scan with no pgid column was flagged"; fails=$((fails + 1))
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

# ONE awk over the whole population: a process per file cost minutes over the
# ~2100 files the widened scan reads.
bad=0
mapfile -d '' files < <(git ls-files -z '*.sh' '*.rs' '*.py' 'justfile' 'just/*.just' \
    ':!scripts/check-ps-zombie-blind.sh')
scanned=${#files[@]}
if [ "$scanned" -eq 0 ]; then
    echo "check-ps-zombie-blind: the scan found NO files — an empty scan reports OK" >&2
    exit 1
fi
awk "$SCAN_AWK" "${files[@]}" || bad=1

# issue 1737 — the rule has TWO halves and the awk above checks one: that the
# state column is REQUESTED. `subtree-guard.sh` kept `stat=` and dropped
# `$3 !~ /^Z/`, and passed. So every group scan must also EXCLUDE Z on the
# rows it reads: a Z predicate in the consumer that follows the site (a pipe's
# next stages, a Rust `.filter`), or — for `done < <(ps …)` — in the loop body
# the rows feed. A capability probe whose stdout goes to /dev/null reads no
# rows and needs none.
zfilter_py='
import re, sys
sys.path.insert(0, "scripts/lib")
import comments

SITE_SH = re.compile(r"\bps\s+-eo\b[^\n]*pgid=")
SITE_ARGV = re.compile(r"\"-eo\"")
ZPRED = re.compile(
    r"!~\s*/\^Z/|\bZ\*\)|starts_?with\(\s*[\x27\"]Z[\x27\"]|!=\s*[\x27\"]Z[\x27\"]"
    r"|\[\[\s*\$\w+\s*!=\s*Z\*")
PROBE = re.compile(r"(?:^|[^0-9])>\s*/dev/null")  # stdout gone; 2>/dev/null is not
FWD = 12

def lang(rel):
    return comments.lang_for(rel) or "sh"

def sites(text, rel):
    """[(lineno, line)] group scans that never exclude a zombie row."""
    lines = comments.strip_comments(text, lang(rel)).split("\n")
    out = []
    for i, ln in enumerate(lines):
        argv = SITE_ARGV.search(ln) and "pgid" in " ".join(lines[i:i + 3])
        if not (SITE_SH.search(ln) or argv):
            continue
        if PROBE.search(ln) and "<(" not in ln:
            continue
        lo = i
        if re.search(r"\bdone\s*<\s*<\(", ln):
            while lo > 0 and not re.search(r"\bwhile\b", lines[lo]):
                lo -= 1
        window = "\n".join(lines[lo:i + FWD + 1])
        if not ZPRED.search(window):
            out.append((i + 1, ln.strip()))
    return out

def selftest():
    bad = "ps -eo pid=,pgid=,stat= 2>/dev/null |\n    awk -v g=1 \x27$2 == g { print $1 }\x27\n"
    good = "ps -eo pid=,pgid=,stat= 2>/dev/null |\n    awk -v g=1 \x27$2 == g && $3 !~ /^Z/ { print $1 }\x27\n"
    loop = ("while read -r pid pgid state; do\n    case \"$state\" in Z*) continue ;; esac\n"
            + "\n" * 30 + "done < <(ps -eo pid=,pgid=,stat= 2>/dev/null)\n")
    loop_bad = "while read -r pid pgid state; do\n    echo $pid\ndone < <(ps -eo pid=,pgid=,stat=)\n"
    probe = "if ! ps -eo pid=,pgid=,stat= >/dev/null 2>&1; then exit 0; fi\n"
    rs_good = ("Command::new(\"ps\").args([\"-eo\", \"pid=,pgid=,stat=\"]);\n"
               "let z = !stat.starts_with(\x27Z\x27);\n")
    rs_bad = "Command::new(\"ps\").args([\"-eo\", \"pid=,pgid=,stat=\"]);\nlet z = 1;\n"
    commented = good.replace("&& $3 !~ /^Z/", "") + "# $3 !~ /^Z/\n"
    for name, txt, rel, want in (("bad", bad, "a.sh", 1), ("good", good, "a.sh", 0),
                                 ("loop", loop, "a.sh", 0), ("loop_bad", loop_bad, "a.sh", 1),
                                 ("probe", probe, "a.sh", 0), ("rs_good", rs_good, "a.rs", 0),
                                 ("rs_bad", rs_bad, "a.rs", 1), ("commented", commented, "a.sh", 1)):
        got = len(sites(txt, rel))
        if got != want:
            sys.exit(f"check-ps-zombie-blind: Z-filter selftest FAILED on {name}: {got} != {want}")

selftest()
bad = 0
for rel in sys.argv[1:]:
    try:
        text = open(rel, errors="replace").read()
    except OSError:
        continue
    for n, ln in sites(text, rel):
        print(f"{rel}:{n}: requests stat= but never drops a Z row: {ln}")
        bad = 1
sys.exit(bad)
'
python3 -c "$zfilter_py" "${files[@]}" || bad=1

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
        Rust: .args(["-eo", "pid=,pgid=,stat="]) and drop rows whose stat starts with Z

  A single-pid lookup (`ps -o pgid= -p "$pid"`) is a different question and is
  not covered by this rule.  -> issue 0853
MSG
    exit 1
fi

echo "check-ps-zombie-blind OK — $scanned tracked file(s), no zombie-blind process-group scan."
