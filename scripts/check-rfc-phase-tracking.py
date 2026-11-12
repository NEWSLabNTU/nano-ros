#!/usr/bin/env python3
"""Every live RFC names the phase doc that tracks its implementation.

User ruling, 2026-11-12: "Ensure the implementation work for each design is
always be tracked by phase docs to maintain the progress and acceptance
status".

An RFC records a decision; a phase doc records the work that carries it out
and whether that work is done. RFC frontmatter has had an
`implements-tracked-by:` field for that link since the series began, but
nothing read it: 35 RFCs left it `[]`, 19 had no frontmatter at all, and six
named a phase that does not name the RFC back. So "is this design built yet?"
had no answer short of reading every phase doc.

Each RFC in `docs/design/` whose status is not Superseded must:

1. carry frontmatter;
2. either name at least one phase in `implements-tracked-by:` (`phase-NNN` or
   a phase slug), where every named phase exists, active or archived, and
   names the RFC back (`RFC-NNNN`, `RFC NNNN`, or a link to
   `design/NNNN-...`); OR state `implementation: none -- <reason>` for an
   RFC with nothing to build (a reference or an analysis), with a non-empty
   reason.

A RATCHET, so the gate lands green on today's tree: the RFCs that fail today
are listed in `.config/rfc-phase-tracking-baseline.txt` with the reason they
fail. A new RFC is never baselined. A baselined RFC that now passes fails the
gate until its line is removed (`--update` rewrites the file), so the list can
only shrink.

A self-test runs on every invocation, over a synthetic tree.

    check-rfc-phase-tracking.py              # the gate
    check-rfc-phase-tracking.py --update     # rewrite the baseline (shrink only)
    check-rfc-phase-tracking.py --report     # every RFC and its phases
"""

import glob
import os
import re
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BASELINE = os.path.join(".config", "rfc-phase-tracking-baseline.txt")

FM_RE = re.compile(r"\A---\n(.*?)\n---\n", re.S)
PHASE_REF_RE = re.compile(r"phase-(\d+)")
NONE_RE = re.compile(r"^implementation:\s*none\b\s*(?:--|—|-|:)?\s*(.*)$", re.M)


def field(fm, name):
    m = re.search(r"^%s:\s*(.*)$" % re.escape(name), fm, re.M)
    return m.group(1).strip() if m else None


def names_rfc(text, num):
    n = int(num)
    return bool(
        re.search(r"RFC[- ]0*%d\b" % n, text)
        or re.search(r"design/(?:archived/)?%s-" % num, text)
        or re.search(r"(?<![\w/])%s-[a-z][\w-]*\.md" % num, text)
    )


def phase_index(root):
    """phase number -> list of (relative path, text), active and archived."""
    idx = {}
    for pat in ("docs/roadmap/phase-*.md", "docs/roadmap/archived/phase-*.md"):
        for p in glob.glob(os.path.join(root, pat)):
            m = re.match(r"phase-(\d+)", os.path.basename(p))
            if not m:
                continue
            with open(p, encoding="utf8", errors="replace") as fh:
                idx.setdefault(str(int(m.group(1))), []).append(
                    (os.path.relpath(p, root), fh.read())
                )
    return idx


def verdict(num, text, phases):
    """None when the RFC is tracked (or superseded), else the reason it is not."""
    if num == "0000":
        return None  # the template, not an RFC
    m = FM_RE.match(text)
    if not m:
        if re.search(r"\*\*Status:\*\*\s*Superseded", text):
            return None
        return "no frontmatter"
    fm = m.group(1)
    status = (field(fm, "status") or "").strip().strip('"')
    if status.lower().startswith("superseded"):
        return None
    none = NONE_RE.search(fm)
    if none:
        if not none.group(1).strip():
            return "`implementation: none` with no reason"
        return None
    tracked = field(fm, "implements-tracked-by")
    if tracked is None:
        return "no `implements-tracked-by:` field"
    refs = sorted(set(PHASE_REF_RE.findall(tracked)), key=int)
    if not refs:
        return "`implements-tracked-by:` names no phase"
    for ref in refs:
        docs = phases.get(str(int(ref)))
        if not docs:
            return f"phase-{ref} does not exist"
        if not any(names_rfc(t, num) for _, t in docs):
            return f"phase-{ref} does not name RFC-{num} back"
    return None


def rfcs(root):
    out = []
    for p in sorted(glob.glob(os.path.join(root, "docs/design/[0-9][0-9][0-9][0-9]-*.md"))):
        with open(p, encoding="utf8", errors="replace") as fh:
            out.append((os.path.basename(p)[:4], os.path.relpath(p, root), fh.read()))
    return out


def problems(root):
    phases = phase_index(root)
    found = {}
    for num, _rel, text in rfcs(root):
        why = verdict(num, text, phases)
        if why:
            found[num] = why
    return found


def read_baseline(root):
    path = os.path.join(root, BASELINE)
    out = {}
    if not os.path.isfile(path):
        return out
    with open(path, encoding="utf8") as fh:
        for line in fh:
            line = line.split("#", 1)[0].strip()
            if line:
                num, _, why = line.partition(" ")
                out[num] = why.strip()
    return out


def write_baseline(root, found):
    with open(os.path.join(root, BASELINE), "w", encoding="utf8") as fh:
        fh.write(
            "# RFCs not yet tracked by a phase doc (scripts/check-rfc-phase-tracking.py).\n"
            "# May only SHRINK. Fix an RFC, then remove its line (or run the\n"
            "# script with --update). A new RFC is never added here.\n"
        )
        for num in sorted(found):
            fh.write(f"{num} {found[num]}\n")


def judge(root):
    """(exit code, message lines)."""
    found = problems(root)
    base = read_baseline(root)
    if not rfcs(root):
        return 1, ["found NO RFCs under docs/design -- the gate would pass vacuously"]
    new = {n: w for n, w in found.items() if n not in base}
    stale = sorted(n for n in base if n not in found)
    lines = []
    if new:
        lines.append(f"{len(new)} RFC(s) not tracked by a phase doc:")
        for n in sorted(new):
            lines.append(f"  RFC-{n}: {new[n]}")
        lines.append(
            "Name the phase that carries the work in `implements-tracked-by:` "
            "(and make that phase name the RFC), or state "
            "`implementation: none -- <reason>` for an RFC with nothing to build."
        )
    if stale:
        lines.append(f"{len(stale)} baseline entr(y/ies) now pass -- remove them (--update):")
        lines += [f"  RFC-{n}" for n in stale]
    if lines:
        return 1, lines
    return 0, [f"OK -- {len(rfcs(root)) - len(found)} RFC(s) pass, {len(found)} baselined."]


def self_test():
    def tree(files):
        d = tempfile.mkdtemp(prefix="rfc-phase-selftest-")
        for rel, text in files.items():
            p = os.path.join(d, rel)
            os.makedirs(os.path.dirname(p), exist_ok=True)
            with open(p, "w", encoding="utf8") as fh:
                fh.write(text)
        return d

    # Synthetic paths, built so a doc-reference scanner never reads them as
    # links into the real tree.
    RFC_PATH = os.path.join("docs", "design", "0901" + "-x.md")
    PHASE_PATH = os.path.join("docs", "roadmap", "phase-901" + "-x.md")
    ARCHIVED_PATH = os.path.join("docs", "roadmap", "archived", "phase-901" + "-x.md")

    def rfc(extra, status="Draft"):
        return f"---\nrfc: 0901\ntitle: \"t\"\nstatus: {status}\n{extra}---\n\n# body\n"

    phase_ok = "# Phase 901\n\nImplements [RFC-0901](../design/0901-x.md).\n"
    phase_silent = "# Phase 901\n\nNothing about the design.\n"
    cases = [
        ("tracked", {RFC_PATH: rfc("implements-tracked-by: [phase-901]\n"),
                     PHASE_PATH: phase_ok}, None),
        ("tracked by an archived phase", {RFC_PATH: rfc("implements-tracked-by: [phase-901]\n"),
                     ARCHIVED_PATH: phase_ok}, None),
        ("empty list", {RFC_PATH: rfc("implements-tracked-by: []\n")}, "names no phase"),
        ("no field", {RFC_PATH: rfc("")}, "no `implements-tracked-by:`"),
        ("missing phase", {RFC_PATH: rfc("implements-tracked-by: [phase-901]\n")}, "does not exist"),
        ("no back-link", {RFC_PATH: rfc("implements-tracked-by: [phase-901]\n"),
                          PHASE_PATH: phase_silent}, "does not name RFC-0901 back"),
        ("none with reason", {RFC_PATH: rfc("implements-tracked-by: []\nimplementation: none -- an analysis\n")}, None),
        ("none without reason", {RFC_PATH: rfc("implementation: none\n")}, "no reason"),
        ("superseded", {RFC_PATH: rfc("implements-tracked-by: []\n", "Superseded")}, None),
        ("no frontmatter", {RFC_PATH: "# RFC-0901\n\n**Status:** Draft\n"}, "no frontmatter"),
    ]
    bad = 0
    for name, files, want in cases:
        got = problems(tree(files)).get("0901")
        if (want is None) != (got is None) or (want and want not in got):
            print(f"  self-test FAIL: {name}: got {got!r}, want {want!r}")
            bad += 1
    # The ratchet: a baselined failure passes, a new one fails, a stale entry fails.
    failing = {RFC_PATH: rfc("implements-tracked-by: []\n")}
    d = tree(dict(failing, **{BASELINE: "0901 names no phase\n"}))
    if judge(d)[0] != 0:
        print("  self-test FAIL: a baselined RFC did not pass")
        bad += 1
    if judge(tree(failing))[0] != 1:
        print("  self-test FAIL: an unbaselined failing RFC passed")
        bad += 1
    d = tree({RFC_PATH: rfc("implementation: none -- ref\n"),
              BASELINE: "0901 names no phase\n"})
    if judge(d)[0] != 1:
        print("  self-test FAIL: a stale baseline entry passed")
        bad += 1
    total = len(cases) + 3
    if bad:
        print(f"check-rfc-phase-tracking self-test: {bad} of {total} case(s) FAILED")
        return 1
    print(f"check-rfc-phase-tracking self-test: OK ({total} cases)")
    return 0


def main(argv):
    if self_test() != 0:
        return 1
    if "--report" in argv:
        phases = phase_index(ROOT)
        for num, _rel, text in rfcs(ROOT):
            print(f"RFC-{num}: {verdict(num, text, phases) or 'ok'}")
        return 0
    if "--update" in argv:
        found = problems(ROOT)
        base = read_baseline(ROOT)
        grown = sorted(n for n in found if n not in base)
        if grown and os.path.isfile(os.path.join(ROOT, BASELINE)):
            print("refusing to GROW the baseline; fix these instead: "
                  + ", ".join(f"RFC-{n}" for n in grown), file=sys.stderr)
            return 1
        write_baseline(ROOT, found)
        print(f"check-rfc-phase-tracking: baseline written, {len(found)} RFC(s)")
        return 0
    rc, lines = judge(ROOT)
    stream = sys.stdout if rc == 0 else sys.stderr
    for line in lines:
        print(f"check-rfc-phase-tracking: {line}" if line is lines[0] else line, file=stream)
    return rc


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
