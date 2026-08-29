#!/usr/bin/env python3
"""Query the issue ledger — the files ARE the database.

issue 0884 / RFC-free by design. There is no service to ask and no index to
trust: every issue file carries `id`, `title`, `status`, `type` and `area` in
its frontmatter, so a query is a directory read. Measured on this repo, listing
every open issue takes ~3 ms — which is the whole argument for keeping issues in
the repo rather than in a tracker.

    scripts/issues.py                  # open issues
    scripts/issues.py --all            # open + archived
    scripts/issues.py --status resolved
    scripts/issues.py --area cmake
    scripts/issues.py zenoh router     # free-text over title AND body
    scripts/issues.py --id 870         # one issue, by number

Grep still works and is not discouraged — this exists because `status` and
`area` live in frontmatter, so filtering on them with grep alone is awkward.
"""

import argparse
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DIRS = [os.path.join(ROOT, "docs", "issues"),
        os.path.join(ROOT, "docs", "issues", "archived")]


def field(text, name):
    m = re.search(rf"^{name}:\s*(.+?)\s*$", text, re.M)
    if not m:
        return ""
    v = m.group(1).strip().strip('"').strip("'")
    # A folded YAML title continues on the following indented lines.
    if name == "title":
        rest = text[m.end():]
        for line in rest.split("\n"):
            if re.match(r"^\s+\S", line) and not re.match(r"^\s*\w+:", line):
                v += " " + line.strip().strip('"')
            else:
                break
    return v


def load(include_archived):
    out = []
    for d in (DIRS if include_archived else DIRS[:1]):
        if not os.path.isdir(d):
            continue
        for fn in sorted(os.listdir(d)):
            if not re.match(r"^\d{4}-.*\.md$", fn):
                continue
            path = os.path.join(d, fn)
            with open(path, encoding="utf8") as fh:
                text = fh.read()
            out.append({
                "file": os.path.relpath(path, ROOT),
                "id": fn[:4],
                "title": field(text, "title"),
                "status": field(text, "status"),
                "type": field(text, "type"),
                "area": field(text, "area"),
                "body": text,
            })
    return out


def main():
    ap = argparse.ArgumentParser(description="Query the issue ledger.")
    ap.add_argument("terms", nargs="*", help="free text over title and body")
    ap.add_argument("--all", action="store_true", help="include archived")
    ap.add_argument("--status", help="open | resolved | wontfix")
    ap.add_argument("--area", help="substring match on area")
    ap.add_argument("--id", help="a single issue number")
    ap.add_argument("--files", action="store_true", help="print paths only")
    a = ap.parse_args()

    rows = load(a.all or bool(a.id) or (a.status and a.status != "open"))
    if a.id:
        want = a.id.zfill(4)
        rows = [r for r in rows if r["id"] == want]
    if a.status:
        rows = [r for r in rows if r["status"] == a.status]
    elif not a.all and not a.id:
        rows = [r for r in rows if r["status"] == "open"]
    if a.area:
        rows = [r for r in rows if a.area.lower() in r["area"].lower()]
    for t in a.terms:
        rows = [r for r in rows
                if t.lower() in r["title"].lower() or t.lower() in r["body"].lower()]

    if a.files:
        for r in rows:
            print(r["file"])
        return 0
    for r in rows:
        mark = {"open": "OPEN", "resolved": "done", "wontfix": "wont"}.get(r["status"], r["status"][:4])
        print(f"#{r['id']}  {mark:<4}  {r['area'][:22]:<22}  {r['title'][:96]}")
    print(f"\n{len(rows)} issue(s).", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
