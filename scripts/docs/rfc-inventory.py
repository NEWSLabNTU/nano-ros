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

A pair a review has already answered "no link applies" (two subjects, or
complementary) is recorded in `docs/design/reviewed-pairs.toml`, with the
ruling that answered it, and is no longer listed. Without that record a pair
could only stop being a candidate by gaining a link it should not have
(phase-555 W3's acceptance). An entry is refused if it names an RFC that
does not exist, or a pair that IS linked, since the link already answers it.

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

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11
    import tomli as tomllib

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
# A citing sentence with one of these words claims the newer RFC changes the
# older one: the pairs a reviewer should read first after the overlap pairs.
CHANGE_WORDS = re.compile(
    r"\b(replac\w*|supersed\w*|instead of|retir\w*|amend\w*|contradict\w*|"
    r"no longer|obsolete|reverses?|overturn\w*|was wrong|deprecat\w*)\b", re.I)
SHARED_PHASE_FLOOR = 0.08
REVIEWED = os.path.join(ROOT, "docs", "design", "reviewed-pairs.toml")


def load_reviewed(docs, path=REVIEWED):
    """{frozenset(num, num)} from reviewed-pairs.toml, refusing a stale entry."""
    if not os.path.exists(path):
        return set()
    with open(path, "rb") as fh:
        entries = tomllib.load(fh).get("pair", [])
    out, errors = set(), []
    for e in entries:
        nums = [str(n).zfill(4) for n in e.get("rfcs", [])]
        where = f"{os.path.relpath(path, ROOT)}: {nums}"
        if len(nums) != 2 or nums[0] == nums[1]:
            errors.append(f"{where}: `rfcs` must name two different RFCs")
        elif not e.get("ruling"):
            errors.append(f"{where}: no `ruling` says who answered it")
        elif any(n not in docs for n in nums):
            errors.append(f"{where}: names an RFC that does not exist")
        elif linked(docs[nums[0]], docs[nums[1]]):
            errors.append(f"{where}: the pair is linked, so the link answers it; remove the entry")
        else:
            out.add(frozenset(nums))
    if errors:
        sys.exit("rfc-inventory: stale reviewed-pairs entries:\n  " + "\n  ".join(errors))
    return out


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
            body_text = body
            docs[num] = dict(
                num=num, path=os.path.relpath(p, ROOT), archived=archived, title=title,
                status=status, area=field(fm, "area") or "-", reviewed=field(fm, "last-reviewed") or "-",
                since=field(fm, "since") or "-",
                phases=sorted(set(re.findall(r"phase-(\d+)", field(fm, "implements-tracked-by"))), key=int),
                sup=sup, supby=supby, amends=amends, cites=cites,
                title_words=words(title), paras=paras, body=body_text,
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


def candidates(docs, reviewed=frozenset()):
    out = []
    live = [d for d in docs.values() if not d["archived"] and not d["status"].lower().startswith("superseded")]
    for a, b in combinations(sorted(live, key=lambda d: d["num"]), 2):
        if linked(a, b) or frozenset((a["num"], b["num"])) in reviewed:
            continue
        union = a["words"] | b["words"]
        sim = len(a["words"] & b["words"]) / len(union) if union else 0.0
        shared_phases = sorted(set(a["phases"]) & set(b["phases"]), key=int)
        # A shared tracking phase alone is weak (one phase often implements
        # several complementary RFCs), so it needs some vocabulary overlap too.
        if sim >= THRESHOLD or (shared_phases and sim >= SHARED_PHASE_FLOOR):
            out.append((round(sim, 2), a, b, shared_phases))
        elif a["num"] in b["cites"]:
            # A newer RFC that discusses an older one, and neither declares it
            # amends or supersedes the other. Most such citations are plain
            # references; the ones whose citing SENTENCE says the newer RFC
            # changes the older are ranked next, with that sentence as evidence.
            out.append((0.0, a, b, shared_phases))
    out.sort(key=lambda t: (-t[0], t[1]["num"], t[2]["num"]))
    return out


def change_evidence(older, newer):
    """The first sentence in `newer` that cites `older` and says it changes it."""
    n = int(older["num"])
    cite = re.compile(r"RFC[- ]0*%d\b|design/(?:archived/)?%s-" % (n, older["num"]))
    for sent in re.split(r"(?<=[.!?])\s+|\n\s*\n", newer["body"]):
        if cite.search(sent) and CHANGE_WORDS.search(sent):
            return re.sub(r"\s+", " ", sent).strip()[:300]
    return None


def ranked(pairs):
    """(tier, sim, older, newer, phases, evidence): 1 overlap, 2 a citing
    sentence that claims a change, 3 a plain citation."""
    out = []
    for sim, a, b, ph in pairs:
        if sim > 0:
            out.append((1, sim, a, b, ph, None))
        else:
            ev = change_evidence(a, b)
            out.append((2 if ev else 3, sim, a, b, ph, ev))
    out.sort(key=lambda t: (t[0], -t[1], t[2]["num"], t[3]["num"]))
    return out


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=os.path.join(ROOT, "build", "rfc-inventory"))
    ap.add_argument("--count", action="store_true")
    args = ap.parse_args(argv)
    docs = load()
    reviewed = load_reviewed(docs)
    pairs = candidates(docs, reviewed)
    rk = ranked(pairs)
    tiers = {t: sum(1 for r in rk if r[0] == t) for t in (1, 2, 3)}
    if args.count:
        print(f"{len(pairs)} (overlap {tiers[1]}, citing a change {tiers[2]}, plain citation {tiers[3]})")
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
        fh.write("Tier 1: subject overlap. Tier 2: the newer RFC cites the older in a sentence that "
                 "says it changes it (the sentence is the evidence). Tier 3: a plain citation, "
                 "listed for completeness and not dispatched for review.\n\n")
        fh.write("| tier | similarity | older | newer | shared phases | evidence |\n")
        fh.write("| --- | --- | --- | --- | --- | --- |\n")
        for tier, sim, a, b, ph, ev in rk:
            ev = (ev or "-").replace("|", "\\|")
            fh.write(
                f"| {tier} | {sim} | {a['num']} {a['title']} ({a['status']}) | {b['num']} {b['title']} ({b['status']}) | "
                f"{', '.join(ph) or '-'} | {ev} |\n"
            )
    # One prioritized list per area (W2): a pair goes to the slice of each RFC
    # in it, so a cross-area pair reaches both reviewers.
    slices = {}
    for r in rk:
        if r[0] == 3:
            continue
        for area in {r[2]["area"], r[3]["area"]}:
            slices.setdefault(area, []).append(r)
    os.makedirs(os.path.join(args.out, "slices"), exist_ok=True)
    for area, rows in sorted(slices.items()):
        with open(os.path.join(args.out, "slices", f"{area}.md"), "w", encoding="utf8") as fh:
            mine = sorted(d["num"] for d in docs.values() if d["area"] == area and not d["archived"])
            fh.write(f"# W2 slice: {area} (generated)\n\n")
            fh.write(f"RFCs in this area ({len(mine)}): {', '.join(mine)}\n\n")
            fh.write("Pairs to review, tier 1 (subject overlap) then tier 2 (a citation that claims a change):\n\n")
            for tier, sim, a, b, ph, ev in rows:
                fh.write(f"- [tier {tier}] RFC-{a['num']} ({a['area']}, {a['status']}) vs RFC-{b['num']} "
                         f"({b['area']}, {b['status']})" + (f", similarity {sim}" if tier == 1 else "") + "\n")
                fh.write(f"  - {a['title']}\n  - {b['title']}\n")
                if ev:
                    fh.write(f"  - evidence: {ev}\n")
    print(f"rfc-inventory: {len(docs)} RFC(s), {len(pairs)} candidate pair(s): overlap {tiers[1]}, "
          f"citing a change {tiers[2]}, plain citation {tiers[3]} ({len(reviewed)} reviewed pair(s) "
          f"excluded) -> {os.path.relpath(args.out, ROOT)}/")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
