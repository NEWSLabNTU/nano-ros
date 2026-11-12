#!/usr/bin/env python3
"""The design-doc inventory, generated from the RFCs themselves (phase-555 W0).

For each RFC under `docs/design/` (and `docs/design/archived/`):
- its id, title, status and last-reviewed date;
- the phases it names in `implements-tracked-by`;
- the RFCs it cites, and the RFCs that cite it;
- its declared links (`supersedes`, `superseded-by`, `amends`, from the
  frontmatter or from a `**Supersedes**` / `**Amends**` line in the text).

It then lists CANDIDATE conflict pairs: an older RFC and a newer one whose
subjects overlap (shared title and summary vocabulary, or a shared tracking
phase), with no supersede or amend link between them in either direction.
A candidate is a question for a reviewer, never a verdict: two RFCs on one
subject can be complementary.

Usage:
    rfc-inventory.py [--out DIR]     # writes inventory.md, pairs.md
    rfc-inventory.py --count         # prints the candidate-pair count only

The output is generated and goes under `build/` by default; it is never
committed. Re-run it to refresh.
"""

import argparse
import glob
import os
import re
import sys
from itertools import combinations

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

FM_RE = re.compile(r"\A---\n(.*?)\n---\n", re.S)
CITE_RE = re.compile(r"RFC[- ]0*(\d{1,4})\b|design/(?:archived/)?(\d{4})-")
STOP = set(
    """a an and the of to in on for is are be by as at it its this that with from
    not no one or every each per into than over under via vs when where which who
    what how why all any our we us nano ros nros rfc design one two do does
    image images build road roads""".split()
)
# Pairs at or above this similarity are candidates. Chosen by reading the
# top of the ranked list, not derived: lower it to see more.
THRESHOLD = 0.2
SHARED_PHASE_FLOOR = 0.08


def field(fm, name):
    m = re.search(r"^%s:\s*(.*)$" % re.escape(name), fm, re.M)
    return m.group(1).strip() if m else ""


def ids_in(value):
    return sorted({str(int(x)).zfill(4) for x in re.findall(r"(?:rfc-?|RFC-?)?0*(\d{1,4})\b", value)
                   if 0 < int(x) < 2000} - {"0000"})


def words(text):
    return {w for w in re.findall(r"[a-z][a-z0-9]{2,}", text.lower()) if w not in STOP}


def load():
    docs = {}
    for archived, pat in ((False, "docs/design/[0-9][0-9][0-9][0-9]-*.md"),
                          (True, "docs/design/archived/[0-9][0-9][0-9][0-9]-*.md")):
        for p in sorted(glob.glob(os.path.join(ROOT, pat))):
            num = os.path.basename(p)[:4]
            if num == "0000":
                continue
            text = open(p, encoding="utf8", errors="replace").read()
            m = FM_RE.match(text)
            fm = m.group(1) if m else ""
            body = text[m.end():] if m else text
            title = field(fm, "title").strip('"') or (re.search(r"^# (.*)$", body, re.M) or [None, "?"])[1]
            status = field(fm, "status").strip('"') or "(no frontmatter)"
            sup = set(ids_in(field(fm, "supersedes")))
            supby = set(ids_in(field(fm, "superseded-by")))
            amends = set(ids_in(field(fm, "amends")))
            head = "\n".join(body.split("\n")[:30])
            for line in re.findall(r"^\*\*(?:Supersedes|Amends)[^\n]*", head, re.M | re.I):
                (amends if "amend" in line.lower() else sup).update(ids_in(line))
            paras = [p.strip() for p in re.split(r"\n\s*\n", body)
                     if p.strip() and not p.lstrip().startswith(("#", "|", "```", "**Status", "> "))]
            cites = {str(int(a or b)).zfill(4) for a, b in CITE_RE.findall(body)} - {num, "0000"}
            docs[num] = dict(
                num=num, path=os.path.relpath(p, ROOT), archived=archived, title=title,
                status=status, reviewed=field(fm, "last-reviewed") or "-",
                since=field(fm, "since") or "-",
                phases=sorted(set(re.findall(r"phase-(\d+)", field(fm, "implements-tracked-by"))), key=int),
                sup=sup, supby=supby, amends=amends, cites=cites,
                title_words=words(title), paras=paras,
            )
    # The subject statement is the first paragraph that is the RFC's own. A
    # paragraph several RFCs share (a rename note stamped across the series)
    # says nothing about any one subject, and made unrelated RFCs look alike.
    seen = {}
    for d in docs.values():
        for para in d["paras"]:
            key = re.sub(r"\s+", " ", para)
            seen[key] = seen.get(key, 0) + 1
    for d in docs.values():
        own = [p for p in d["paras"] if seen[re.sub(r"\s+", " ", p)] == 1]
        d["words"] = d["title_words"] | words(own[0] if own else "")
    # Words in more than a tenth of the RFCs name the project, not a subject.
    df = {}
    for d in docs.values():
        for w in d["words"]:
            df[w] = df.get(w, 0) + 1
    common = {w for w, n in df.items() if n > len(docs) / 10}
    for d in docs.values():
        d["words"] -= common
        del d["paras"], d["title_words"]
    for d in docs.values():
        d["cited_by"] = sorted(n for n, o in docs.items() if d["num"] in o["cites"])
    return docs


def linked(a, b):
    return (b["num"] in a["sup"] | a["supby"] | a["amends"]
            or a["num"] in b["sup"] | b["supby"] | b["amends"])


def candidates(docs):
    out = []
    live = [d for d in docs.values() if not d["archived"] and not d["status"].lower().startswith("superseded")]
    for a, b in combinations(sorted(live, key=lambda d: d["num"]), 2):
        if linked(a, b):
            continue
        union = a["words"] | b["words"]
        sim = len(a["words"] & b["words"]) / len(union) if union else 0.0
        shared_phases = sorted(set(a["phases"]) & set(b["phases"]), key=int)
        # A shared tracking phase alone is weak (one phase often implements
        # several complementary RFCs), so it needs some vocabulary overlap too.
        if sim >= THRESHOLD or (shared_phases and sim >= SHARED_PHASE_FLOOR):
            out.append((round(sim, 2), a, b, shared_phases))
        elif a["num"] in b["cites"]:
            # A newer RFC that discusses an older one, and neither says it
            # amends or supersedes the other: either a missing link or a
            # complement. Ranked after the similar pairs.
            out.append((0.0, a, b, shared_phases))
    out.sort(key=lambda t: (-t[0], t[1]["num"], t[2]["num"]))
    return out


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=os.path.join(ROOT, "build", "rfc-inventory"))
    ap.add_argument("--count", action="store_true")
    args = ap.parse_args(argv)
    docs = load()
    pairs = candidates(docs)
    if args.count:
        print(len(pairs))
        return 0
    os.makedirs(args.out, exist_ok=True)
    with open(os.path.join(args.out, "inventory.md"), "w", encoding="utf8") as fh:
        fh.write("# RFC inventory (generated by scripts/docs/rfc-inventory.py; do not edit)\n\n")
        fh.write("| RFC | title | status | since | last-reviewed | phases | cites | cited by | supersedes | superseded by | amends |\n")
        fh.write("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |\n")
        for d in sorted(docs.values(), key=lambda d: d["num"]):
            j = lambda xs: ", ".join(sorted(xs)) or "-"
            fh.write(
                f"| {d['num']}{' (archived)' if d['archived'] else ''} | {d['title']} | {d['status']} | "
                f"{d['since']} | {d['reviewed']} | {', '.join('phase-' + p for p in d['phases']) or '-'} | "
                f"{j(d['cites'])} | {j(d['cited_by'])} | {j(d['sup'])} | {j(d['supby'])} | {j(d['amends'])} |\n"
            )
    with open(os.path.join(args.out, "pairs.md"), "w", encoding="utf8") as fh:
        fh.write("# Candidate conflict pairs (generated; a question, not a verdict)\n\n")
        fh.write(f"Older RFC, newer RFC, subject similarity (threshold {THRESHOLD}) or a shared tracking "
                 "phase, and no supersede or amend link either way.\n\n")
        fh.write("| similarity | older | newer | shared phases | older cites newer | newer cites older |\n")
        fh.write("| --- | --- | --- | --- | --- | --- |\n")
        for sim, a, b, ph in pairs:
            fh.write(
                f"| {sim} | {a['num']} {a['title']} ({a['status']}) | {b['num']} {b['title']} ({b['status']}) | "
                f"{', '.join(ph) or '-'} | {'yes' if b['num'] in a['cites'] else 'no'} | "
                f"{'yes' if a['num'] in b['cites'] else 'no'} |\n"
            )
    print(f"rfc-inventory: {len(docs)} RFC(s), {len(pairs)} candidate pair(s) -> {os.path.relpath(args.out, ROOT)}/")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
