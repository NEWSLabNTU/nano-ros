"""Resolve an issue id to the `status:` its file declares.

Issue 1092: two RMW gates accepted "names an issue" as a deferral, and checked
the NAME only — a regex for four digits, or truthiness of an `issue =` field.
an id with no file (9999), `issue 0776` (resolved and archived) and
`issue = "banana"` were all accepted, so the exception mechanism that lets a
real gap sit on the fast line could hold a gap nobody was tracking. A
deferral is only a deferral while the issue it names is OPEN; this is the one
place that question is answered.
"""

import os
import re

# A deferral written in prose: `issue NNNN` / `issue-NNNN`, case-insensitive.
# Deliberately the same narrow shape `check-prose-issue-refs` uses — a bare
# four-digit number is a port, a size or a year far more often than an issue.
ISSUE_REF = re.compile(r"\bissue[ -]?(\d{4})\b", re.I)

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
_DIRS = (
    os.path.join("docs", "issues"),
    os.path.join("docs", "issues", "archived"),
)
_FM = re.compile(r"^---\n(.*?)\n---", re.S)
_STATUS = re.compile(r"^status:\s*[\"']?([A-Za-z_-]+)", re.M)


def issue_status(num, root=ROOT):
    """`"open"` / `"resolved"` / … for issue `num`, or None when no issue file
    carries that id (or `num` is not an issue id at all).

    Accepts `776`, `"0776"` and `"issue 0776"`-free digits alike; anything that
    is not digits resolves to None rather than raising, because the callers
    hold AUTHORED values and "banana" must read as "no such issue".
    """
    s = str(num).strip()
    if not s.isdigit():
        return None
    prefix = f"{int(s):04d}-"
    for rel in _DIRS:
        d = os.path.join(root, rel)
        try:
            names = os.listdir(d)
        except FileNotFoundError:
            continue
        for fn in sorted(names):
            if fn.startswith(prefix) and fn.endswith(".md"):
                with open(os.path.join(d, fn), encoding="utf-8") as fh:
                    m = _FM.match(fh.read())
                st = _STATUS.search(m.group(1)) if m else None
                return st.group(1) if st else "unknown"
    return None


def why_not_open(num, status_of=None):
    """`None` when `num` names an OPEN issue, else the phrase saying why not.

    THE one place a deferral is judged (phase-428 W11). Three RMW gates and
    several authored tables ask this question; before this they each answered
    it themselves, and the versions disagreed — one matched four digits and
    stopped, one tested truthiness. A rule with several implementations is a
    rule with several reaches, which is how issue 1092's `9999` / `0776` /
    `"banana"` all read as tracked.
    """
    status_of = issue_status if status_of is None else status_of
    st = status_of(num)
    if st == "open":
        return None
    return (
        f"is `{st}`" if st else "is not a file under docs/issues/"
    ) + " — a deferral to an issue nobody holds open is an exemption"


def deferral(why, status_of=None):
    """`(issue id, None)` when `why` defers to an OPEN issue, else `(None, reason)`.

    For a PROSE deferral, where naming the issue is the whole mechanism — a
    `gap` reason in the parity map. The first id in the text is the deferral.
    """
    m = ISSUE_REF.search(why)
    if not m:
        return None, "names no issue"
    num = m.group(1)
    bad = why_not_open(num, status_of)
    return (None, f"names issue {num}, which {bad}") if bad else (num, None)


def refused_deferrals(value, status_of=None):
    """`[(id, why not)]` for every id in a STRUCTURED deferral field.

    `value` may be a single id or an iterable of them. Structured rather than
    prose because a REASON legitimately cites resolved issues — history is
    what a reason is made of — while a `defer =` field asserts the work is
    still tracked, and only the second claim is checkable. `None` defers to
    nothing and is always fine.
    """
    if value is None:
        return []
    ids = value if isinstance(value, (list, tuple, set)) else [value]
    out = []
    for num in ids:
        bad = why_not_open(num, status_of)
        if bad:
            out.append((num, bad))
    return out
