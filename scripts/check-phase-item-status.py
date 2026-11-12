#!/usr/bin/env python3
"""Every work item in a phase doc states its status on one line.

User ruling, 2026-11-12: "Ensure the implementation work for each design is
always be tracked by phase docs to maintain the progress and acceptance
status". `check-rfc-phase-tracking` ties each RFC to its phases. This gate
makes a phase answer "what is done?" item by item, without reading prose.

A work item is a `### W<n>` heading (`### W3`, `### W3a`). Its section runs to
the next `###` or `##` heading. The section carries exactly one line:

    - **Status:** OPEN
    - **Status:** DONE (<when, and the PR or commit>)
    - **Status:** BLOCKED (<what it waits on>)
    - **Status:** DROPPED (<why>)

DONE, BLOCKED and DROPPED need a non-empty parenthesis; OPEN may carry one.

Rules:

1. A section with two status lines, or a status word outside the four, fails.
2. An item in an ARCHIVED phase (`docs/roadmap/archived/`) may not be OPEN or
   BLOCKED. Archiving is the claim that the phase is finished.
3. A missing status line is a RATCHET. `.config/phase-item-status-baseline.txt`
   counts the unmarked items per phase doc, keyed by file name so that
   archiving a doc keeps its count. A doc's count may only fall. A doc not in
   the baseline (every new phase) must mark every item. A count that falls
   below the baseline fails until the baseline is lowered (`--update`), so
   the debt can only shrink.

A self-test runs on every invocation.

    check-phase-item-status.py            # the gate
    check-phase-item-status.py --update   # lower the baseline (never raise it)
    check-phase-item-status.py --report   # unmarked items, per doc
"""

import glob
import os
import re
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BASELINE = os.path.join(".config", "phase-item-status-baseline.txt")

ITEM_RE = re.compile(r"^### (W\d+[a-z]?)\b.*$", re.M)
NEXT_RE = re.compile(r"^#{2,3} ", re.M)
STATUS_RE = re.compile(r"^- \*\*Status:\*\* ([A-Z]+)\b(.*)$", re.M)
WORDS = {"OPEN", "DONE", "BLOCKED", "DROPPED"}
NEEDS_REASON = {"DONE", "BLOCKED", "DROPPED"}


def items(text):
    """[(item id, section text)] in document order."""
    out = []
    for m in ITEM_RE.finditer(text):
        start = m.end()
        nxt = NEXT_RE.search(text, start)
        out.append((m.group(1), text[start: nxt.start() if nxt else len(text)]))
    return out


def judge_doc(text, archived):
    """(unmarked item ids, hard errors)."""
    unmarked, errors = [], []
    for wid, body in items(text):
        lines = STATUS_RE.findall(body)
        if not lines:
            unmarked.append(wid)
            continue
        if len(lines) > 1:
            errors.append(f"{wid}: {len(lines)} status lines, want one")
            continue
        word, rest = lines[0]
        rest = rest.strip()
        if word not in WORDS:
            errors.append(f"{wid}: status `{word}` is not one of {', '.join(sorted(WORDS))}")
        elif word in NEEDS_REASON and not re.match(r"^\(\s*\S.*\)", rest):
            errors.append(f"{wid}: `{word}` needs a parenthesis saying when or why")
        elif archived and word in ("OPEN", "BLOCKED"):
            errors.append(f"{wid}: `{word}` in an archived phase (archiving claims it is finished)")
    return unmarked, errors


def phase_docs(root):
    out = []
    for archived, pat in ((False, "docs/roadmap/phase-*.md"), (True, "docs/roadmap/archived/phase-*.md")):
        for p in sorted(glob.glob(os.path.join(root, pat))):
            with open(p, encoding="utf8", errors="replace") as fh:
                out.append((os.path.basename(p), os.path.relpath(p, root), archived, fh.read()))
    return out


def read_baseline(root):
    out = {}
    path = os.path.join(root, BASELINE)
    if os.path.isfile(path):
        with open(path, encoding="utf8") as fh:
            for line in fh:
                line = line.split("#", 1)[0].strip()
                if line:
                    name, count = line.split()
                    out[name] = int(count)
    return out


def measure(root):
    """name -> (rel, unmarked ids, errors)."""
    return {name: (rel, *judge_doc(text, archived)) for name, rel, archived, text in phase_docs(root)}


def judge(root):
    m = measure(root)
    if not m:
        return 1, ["found NO phase docs -- the gate would pass vacuously"]
    base = read_baseline(root)
    lines = []
    for name in sorted(m):
        rel, unmarked, errors = m[name]
        for e in errors:
            lines.append(f"  {rel}: {e}")
        allowed = base.get(name, 0)
        if len(unmarked) > allowed:
            lines.append(
                f"  {rel}: {len(unmarked)} item(s) with no `- **Status:**` line "
                f"(baseline {allowed}): {', '.join(unmarked)}"
            )
        elif len(unmarked) < allowed:
            lines.append(f"  {rel}: baseline {allowed} but {len(unmarked)} unmarked -- lower it (--update)")
    stale = sorted(n for n in base if n not in m)
    for n in stale:
        lines.append(f"  {n}: in the baseline but no such phase doc -- remove it (--update)")
    if lines:
        return 1, ["phase work items without a valid status line:"] + lines + [
            "Each `### W<n>` item carries one line: `- **Status:** OPEN`, "
            "`DONE (<when, PR>)`, `BLOCKED (<what>)` or `DROPPED (<why>)`. "
            "Mark DONE only what is done."
        ]
    total = sum(len(v[1]) for v in m.values())
    return 0, [f"OK -- {len(m)} phase doc(s); {total} unmarked item(s) baselined."]


def self_test():
    def tree(files):
        d = tempfile.mkdtemp(prefix="phase-status-selftest-")
        for rel, text in files.items():
            p = os.path.join(d, rel)
            os.makedirs(os.path.dirname(p), exist_ok=True)
            with open(p, "w", encoding="utf8") as fh:
                fh.write(text)
        return d

    doc = "# Phase 901\n\n### W1 -- a\n\n{a}\n### W2 -- b\n\n{b}\n## Order\n\n- **Status:** OPEN\n"
    active = "docs/roadmap/phase-901-x.md"
    archived = "docs/roadmap/archived/phase-901-x.md"
    ok = "- **Status:** DONE (2026-11-12, #1)"
    cases = [
        ("both marked", {active: doc.format(a=ok, b="- **Status:** OPEN")}, 0),
        ("one unmarked, no baseline", {active: doc.format(a=ok, b="text")}, 1),
        ("one unmarked, baselined", {active: doc.format(a=ok, b="text"),
                                     BASELINE: "phase-901-x.md 1\n"}, 0),
        ("baseline above the count", {active: doc.format(a=ok, b=ok),
                                      BASELINE: "phase-901-x.md 1\n"}, 1),
        ("DONE with no parenthesis", {active: doc.format(a="- **Status:** DONE", b=ok)}, 1),
        ("unknown word", {active: doc.format(a="- **Status:** LANDED (x)", b=ok)}, 1),
        ("two status lines", {active: doc.format(a=ok + "\n" + ok, b=ok)}, 1),
        ("archived and OPEN", {archived: doc.format(a=ok, b="- **Status:** OPEN")}, 1),
        ("archived, all done", {archived: doc.format(a=ok, b="- **Status:** DROPPED (moved)")}, 0),
        ("archived keeps its baseline", {archived: doc.format(a=ok, b="text"),
                                         BASELINE: "phase-901-x.md 1\n"}, 0),
        ("stale baseline entry", {active: doc.format(a=ok, b=ok),
                                  BASELINE: "phase-999-gone.md 2\n"}, 1),
        # A status line after `## Order` belongs to no item.
        ("line outside any item", {active: doc.format(a=ok, b="text")}, 1),
    ]
    bad = 0
    for name, files, want in cases:
        got = judge(tree(files))[0]
        if got != want:
            print(f"  self-test FAIL: {name}: exit {got}, want {want}")
            bad += 1
    if bad:
        print(f"check-phase-item-status self-test: {bad} of {len(cases)} case(s) FAILED")
        return 1
    print(f"check-phase-item-status self-test: OK ({len(cases)} cases)")
    return 0


def main(argv):
    if self_test() != 0:
        return 1
    if "--report" in argv:
        for name, (rel, unmarked, errors) in sorted(measure(ROOT).items()):
            if unmarked or errors:
                print(f"{rel}: unmarked {', '.join(unmarked) or '-'}; errors {errors or '-'}")
        return 0
    if "--update" in argv:
        m = measure(ROOT)
        base = read_baseline(ROOT)
        exists = os.path.isfile(os.path.join(ROOT, BASELINE))
        grown = [n for n, v in m.items() if len(v[1]) > base.get(n, 0)]
        if exists and grown:
            print("refusing to RAISE the baseline; mark these instead: " + ", ".join(sorted(grown)),
                  file=sys.stderr)
            return 1
        with open(os.path.join(ROOT, BASELINE), "w", encoding="utf8") as fh:
            fh.write(
                "# Phase docs with work items that carry no `- **Status:**` line\n"
                "# (scripts/check-phase-item-status.py), by file name, so an archived doc\n"
                "# keeps its count. May only FALL. A new phase is never added here.\n"
            )
            for n in sorted(m):
                if m[n][1]:
                    fh.write(f"{n} {len(m[n][1])}\n")
        print("check-phase-item-status: baseline written")
        return 0
    rc, lines = judge(ROOT)
    stream = sys.stdout if rc == 0 else sys.stderr
    for i, line in enumerate(lines):
        print(f"check-phase-item-status: {line}" if i == 0 else line, file=stream)
    return rc


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
