#!/usr/bin/env python3
"""An open issue and an active phase that name each other do so both ways.

WHY THIS EXISTS. On 2026-10-12, with 167 open issues on main, 23 links ran
issue -> phase only (an issue's `related:` named an active phase that never
mentioned it) and 47 ran phase -> issue only (an active phase cited the issue
and the issue's `related:` did not carry that phase). A one-way link is a
claim one side does not know about: the phase's reader never learns the issue
counts on it, or the issue's reader never learns a live plan cites it.

WHAT IT DOES NOT REQUIRE. An open issue may name no active phase at all.
Phases are archived when their work lands, and an issue left over stands on
its own (#1957 archived 36 phases that way). This gate checks the links that
exist, never demands one.

THE RULE. For each open issue (`docs/issues/NNNN-*.md`, not `archived/`) and
each ACTIVE phase doc (`docs/roadmap/phase-N-*.md`, not `archived/`):

  L1  the issue's `related:` names `phase-N`  =>  some active doc of phase N
      links back: `issue NNNN`, `issues NNNN`, `#NNNN`, a `../issues/NNNN-`
      link, or `[NNNN](`;
  L2  an active doc of phase N cites the issue in a STRICT form (`issue NNNN`,
      `issues/NNNN-`, or a markdown link to `../issues/NNNN-`)  =>  the
      issue's `related:` names `phase-N`.

L2 does not count a bare `#NNNN`: in a phase doc that is as often a pull
request, and treating it as an issue citation would demand nonsense links.
A phase number may carry several docs (26 numbers do), so any active doc with
that number counts. No allowlist: each finding is one line to add.
"""
from __future__ import annotations

import re
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CLOSED = {"resolved", "wontfix", "duplicate", "superseded"}
_REL = re.compile(r"^related:\s*\[(.*?)\]", re.M | re.S)
_STATUS = re.compile(r"^status:\s*(\S+)", re.M)
_PHASE = re.compile(r"\bphase-(\d+)\b")


def _backref(n: str) -> re.Pattern[str]:
    n = re.escape(n)
    return re.compile(
        rf"(?:\bissues?[ -]#?{n}\b|#{n}\b|issues/(?:archived/)?{n}-|\[{n}\]\()")


def _citation(n: str) -> re.Pattern[str]:
    n = re.escape(n)
    return re.compile(rf"(?:\bissues?[ -]{n}\b|issues/{n}-)")


def frontmatter(text: str) -> str:
    if not text.startswith("---"):
        return ""
    end = text.find("\n---", 3)
    return text[3:end] if end > 0 else ""


def check(root: Path) -> list[str]:
    active: dict[str, list[Path]] = {}
    for d in sorted((root / "docs" / "roadmap").glob("phase-*.md")):
        m = re.match(r"phase-(\d+)", d.name)
        if m:
            active.setdefault(m.group(1), []).append(d)
    texts = {d: d.read_text(encoding="utf-8") for ds in active.values() for d in ds}

    problems = []
    for f in sorted((root / "docs" / "issues").glob("[0-9][0-9][0-9][0-9]-*.md")):
        fm = frontmatter(f.read_text(encoding="utf-8"))
        st = _STATUS.search(fm)
        if st and st.group(1).strip("\"'") in CLOSED:
            continue
        n = f.name[:4]
        rel = _REL.search(fm)
        named = list(dict.fromkeys(_PHASE.findall(rel.group(1)) if rel else []))
        back = _backref(n)
        for p in named:
            if p in active and not any(back.search(texts[d]) for d in active[p]):
                docs = ", ".join(d.name for d in active[p])
                problems.append(
                    f"L1 {f.name}: names phase-{p}, which never mentions it ({docs}) "
                    f"-- list `issue {n}` there, or drop phase-{p} from `related:`")
        cite = _citation(n)
        for p, ds in active.items():
            if p in named:
                continue
            hit = next((d for d in ds if cite.search(texts[d])), None)
            if hit:
                problems.append(
                    f"L2 {f.name}: {hit.name} cites it, and its `related:` does not "
                    f"name phase-{p} -- add phase-{p} there")
    return problems


def self_test() -> None:
    def tree(issue_fm: str, phase_body: str, archived: bool = False) -> Path:
        t = Path(tempfile.mkdtemp(prefix="issue-phase-links-"))
        (t / "docs" / "issues").mkdir(parents=True)
        (t / "docs" / "roadmap" / "archived").mkdir(parents=True)
        (t / "docs" / "issues" / "0001-x.md").write_text(
            f"---\nid: 0001\n{issue_fm}\n---\nbody\n")
        where = t / "docs" / "roadmap" / ("archived" if archived else "")
        (where / "phase-7-x.md").write_text(f"# P\n\n**Status.** open.\n\n{phase_body}\n")
        return t

    named = "status: open\nrelated: [rfc-1, phase-7]"
    alone = "status: open\nrelated: [rfc-1]"
    assert not check(tree(named, "- [issue 0001](../issues/0001-x.md)")), "both ways"
    assert not check(tree(named, "fixes #0001")), "#NNNN counts as a back-link"
    assert not check(tree("status: open\nrelated: [phase-9,\n  phase-7]", "issue 0001")), \
        "multi-line related"
    assert not check(tree(alone, "nothing about it")), "an issue may stand alone"
    assert not check(tree(named, "", archived=True)), "an archived phase owes nothing"
    assert not check(tree(alone, "merged in #0001")), "a bare #NNNN is not a citation"
    assert check(tree(named, "nothing here, not 10001 nor 00012"))[0].startswith("L1"), \
        "named, phase silent; a number INSIDE another must not count"
    assert check(tree(alone, "see issue 0001"))[0].startswith("L2"), \
        "cited, issue silent"
    assert check(tree(alone, "[x](../issues/0001-x.md)"))[0].startswith("L2"), \
        "a link is a citation"
    assert not check(tree("status: resolved\nrelated: [phase-7]", "issue 0002")), \
        "closed issues are exempt"


def main() -> int:
    self_test()
    problems = check(REPO)
    if problems:
        print("check-issue-phase-links: one-way issue/phase link(s):", file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        return 1
    print("check-issue-phase-links OK -- every open issue and active phase that "
          "name each other do so both ways.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
