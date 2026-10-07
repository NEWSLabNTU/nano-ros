"""One LINE of shell is not one statement — issue 1738.

Two gates read shell a line at a time and treated the line as the statement:
`check-lane-skip-protocol` (is an `echo …skip…` followed by `exit 0`?) and
`check-pipefail-sigpipe-assertions` (is the LAST pipeline stage an early-exit
matcher whose status is read?). A one-line compound defeats both:

    if [ -z "$X" ]; then echo "skip: no X"; exit 0; fi
    _f() { if ! printf '%s' "$1" | grep -c x; then echo n; fi; }

The first's `echo` follows `then`, not a line start, and `; fi` sits after the
`exit 0`; the second's "last stage" ran to end of line (`grep -c x; then …; }`).

So there is ONE splitter, here. `statements(line)` cuts a line of shell into
simple statements at every UNQUOTED, depth-0 `;`, `;;`, `&&`, `||` and `&`,
and peels the compound-command keywords that can open a statement (`then`,
`do`, `else`, `{`, `(`, `}`, `fi`, `done`, `esac`, and a `name() {` function
header) off its front, keeping them in `lead`. Quotes and comments come from
`comments.py`'s shell stripper, so a `;` or `|` inside a string or after a
word-start `#` is never a separator.

Each `Stmt` carries the separator before and after it, which is what a
"is the status READ?" question needs (`a | grep -c x && b`, `… || continue`).

Not a parser: a `case` pattern's `)` and a multi-line construct are read as
text, and a `$(…)` that spans lines is not followed. Both gates that use it
keep their next-line arms for the multi-line forms.

`python3 scripts/lib/shell_statements.py --self-test` runs the controls.
"""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import comments  # noqa: E402 — the one shell quote/comment model

# Compound-command words that may OPEN a statement without being its command.
_LEAD = re.compile(
    r"\s*(?:(?:then|do|else|fi|done|esac)(?=\s|$)"
    r"|[{}()](?=\s|$)"
    r"|(?:function\s+)?[A-Za-z_][\w:.-]*\s*\(\s*\)\s*\{?(?=\s|$))"
)


@dataclass(frozen=True)
class Stmt:
    text: str        # the statement, compound keywords peeled off its front
    lead: str        # the peeled keywords, space-joined ("then", "{", …)
    sep_before: str  # "" at line start, else ";", ";;", "&&", "||" or "&"
    sep_after: str   # "" at line end, else the same set


def _peel(seg: str) -> tuple[int, str]:
    """(offset where the statement proper starts, the peeled keywords)."""
    lead: list[str] = []
    off = 0
    while True:
        m = _LEAD.match(seg, off)
        if not m or not m.group(0).strip():
            break
        lead.append(m.group(0).strip())
        off = m.end()
    return off, " ".join(lead)


def statements(line: str) -> list[Stmt]:
    """The simple statements of ONE line of shell, in order, empties dropped."""
    masked = comments.strip_comments(line, "sh", strings=True)
    cuts: list[tuple[int, int, str]] = []  # (start, end, sep)
    depth = 0
    i, n = 0, len(masked)
    while i < n:
        c = masked[i]
        if c == "(":
            depth += 1
        elif c == ")":
            depth = max(0, depth - 1)
        elif depth == 0:
            two = masked[i:i + 2]
            if two in (";;", "&&", "||"):
                cuts.append((i, i + 2, two))
                i += 2
                continue
            if c == ";":
                cuts.append((i, i + 1, ";"))
            elif c == "&" and not (
                (i > 0 and masked[i - 1] in "<>") or masked[i + 1:i + 2] == ">"
            ):
                cuts.append((i, i + 1, "&"))
        i += 1
    out: list[Stmt] = []
    start, before = 0, ""
    for a, b, sep in cuts + [(n, n, "")]:
        off, lead = _peel(masked[start:a])
        # Cut the ORIGINAL at the masked offsets; take only the code part, so a
        # trailing comment (blanked in `masked`) is not part of the statement.
        lo = start + off
        body = masked[lo:a]
        lo += len(body) - len(body.lstrip())
        hi = lo + len(body.strip())
        text = line[lo:hi]
        if text or lead:
            out.append(Stmt(text, lead, before, sep))
        start, before = b, sep
    return out


def self_test() -> list[str]:
    bad: list[str] = []

    def texts(line):
        return [s.text for s in statements(line) if s.text]

    cases = [
        ('if [ -z "$X" ]; then echo "skip: a; b"; exit 0; fi',
         ['if [ -z "$X" ]', 'echo "skip: a; b"', "exit 0"]),
        ("_f() { if ! printf '%s' \"$1\" | grep -c x; then echo n; fi; }",
         ["if ! printf '%s' \"$1\" | grep -c x", "echo n"]),
        ('command -v gcc || { echo "skip: no gcc"; exit 0; }',
         ["command -v gcc", 'echo "skip: no gcc"', "exit 0"]),
        ('a | grep -c x && b  # trailing; comment && here',
         ["a | grep -c x", "b"]),
        ('x="$(a; b)"; y=1', ['x="$(a; b)"', "y=1"]),
        ("cmd >&2 2>&1; done", ["cmd >&2 2>&1"]),
        ("docker run x; do_thing", ["docker run x", "do_thing"]),
    ]
    for line, want in cases:
        got = texts(line)
        if got != want:
            bad.append(f"statements({line!r}) = {got}, want {want}")
    s = statements("if a | grep -c x; then echo n; fi")
    if [x.lead for x in s] != ["", "then", "fi"] or s[0].sep_after != ";":
        bad.append(f"lead/sep bookkeeping broke: {s}")
    return bad


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        problems = self_test()
        for p in problems:
            print(f"shell_statements: SELF-TEST FAILED — {p}", file=sys.stderr)
        sys.exit(1 if problems else 0)
    for arg in sys.argv[1:]:
        for st in statements(arg):
            print(st)
