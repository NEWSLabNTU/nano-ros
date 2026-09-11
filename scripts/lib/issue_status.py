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
