#!/usr/bin/env python3
"""Candidates for a roadmap verification pass — a REPORT, never a gate.

phase-419 W3.

W2's `check-roadmap-claims` takes the mechanical half: a phase that contradicts
its own body. This takes the other half, and it cannot be a gate for the reason
the phase records — supersession, a reversed premise and a claim of absence all
need judgment, and a gate that guessed at them would either flag honest
documents or teach people to ignore it.

So this prints CANDIDATES with the evidence beside them, and a person or an
agent decides. Exit status is 0 unless the script itself broke; a finding is not
a failure. `pr-verdicts.yml` documents the same shape and why it stays advisory:
a check that produces no verdict for a pull request blocks it forever (#0975).

WHAT IT LOOKS FOR, and why each is here

  dead-path    A phase cites a repo path that does not exist. This is R2 from
               the phase doc, and W1 measured that it is only PARTLY covered:
               `check-doc-refs` validates the numbered doc series, and
               `check-ci-doc-workflow-refs` validates workflow names but only
               inside three hand-listed `docs/development/` files. Roadmap docs
               are covered by neither -- 20 dead workflow references across 8
               phases on 2026-09-04, of which phase-196 alone held 11 and one of
               them was its single open acceptance criterion.

               Extending that gate needs a baseline it does not have, so it is
               reported here rather than enforced. Not every dead path is a
               defect: a phase legitimately describes what a workflow USED to be
               called, which is why `check-ci-doc-workflow-refs` has a
               "Historical workflow names" exemption and why this cannot be
               mechanical.

  stale-issue  A phase links an issue that is now resolved or archived. R3 in
               W2 asserts this only for the `**Owns:**` line, where the house
               convention already annotates it. Everywhere else a link to a
               resolved issue is often correct -- a phase cites the issue that
               MOTIVATED it -- so it is a candidate, not a rule.

  cold         The status line's date is older than the threshold and the file
               has not been touched since. Not evidence of anything on its own;
               it is how you choose which phases to read when you cannot read
               all of them. Eight of the thirteen oldest were stale on
               2026-09-04, which is the ratio that motivates the pass.

The three judgment classes W2 cannot reach at all -- supersession, a reversed
premise, a claim of absence -- have no heuristic here on purpose. The phase doc
carries them as worked examples instead, because each was found by a different
move and the move is the transferable part.
"""

import argparse
import datetime
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ROADMAP = os.path.join(ROOT, "docs", "roadmap")

STATUS = re.compile(r"^\s*(?:\*\*|##\s*)?Status\b", re.I)
DATE = re.compile(r"(20\d{2})-(\d{2})-(\d{2})")
WORKFLOW_REF = re.compile(r"`([a-z0-9][a-z0-9._-]*\.ya?ml)`")
# A backticked path with a directory separator, which is what a citation of
# repo CODE looks like. A bare word in backticks is prose far too often.
PATH_REF = re.compile(r"`((?:[a-zA-Z0-9_.-]+/){1,}[a-zA-Z0-9_.-]+\.(?:rs|py|sh|toml|cmake|c|h|hpp|cpp|just|md))`")
ISSUE_LINK = re.compile(r"\]\(([^)]*?/(\d{4})-[a-z0-9-]+\.md)\)")

# Names that are a FORMAT or a third party's file, not one of ours. Mirrors the
# same list in `check-ci-doc-workflow-refs.py` rather than inferring one: an
# inferred exemption is one nobody can audit.
NOT_OURS = {
    "action.yml", "docker-compose.yml", "west.yml", "package.yml", "config.yml",
    # DATA files a phase names, not workflows. phase-330 cites both while
    # describing the SystemModel artifact; a `.yaml` suffix is not evidence of
    # a workflow. Explicit rather than inferred, for the reason the sibling
    # gate gives: an inferred exemption is one nobody can audit.
    "sim_model.yaml", "system_model.yaml", "launch.yaml", "system.yaml",
}

# A cited path is only OURS if it is rooted in one of our top-level directories.
# Without this the report flagged 56 of 63 phases -- 89% is not a filter. The
# noise was book-relative fragments (`concepts/architecture.md`), Zephyr's own
# tree (`soc/xlnx/.../soc.c`), and paths into the consumer repo
# (`autoware-safety-island/...`). None of those are this repo's to resolve, and
# a report nobody can read is the same as no report.
# `check-doc-refs` already owns the numbered doc series, INCLUDING its
# archived/ fallback. Reporting those here would double-report a covered class
# and make this report disagree with a gate -- the failure mode W1 exists to
# avoid.
SERIES_DOC = re.compile(r"^docs/(design|issues|roadmap)/(phase-)?\d{3,4}-")

# `tests` and `config` are DELIBERATELY absent: both are common RELATIVE
# fragments inside a package ("tests/parity.rs" means that crate's own tests
# dir), not repo-rooted paths, and including them put unresolvable relative
# references into the report as if they were dead files.
OUR_ROOTS = {
    "packages", "scripts", "just", "docs", "cmake", "examples",
    ".github", "book", "zephyr", "ci",
}


def workflows_present():
    d = os.path.join(ROOT, ".github", "workflows")
    return set(os.listdir(d)) if os.path.isdir(d) else set()


def status_date(text):
    for line in text.split("\n"):
        if STATUS.match(line):
            m = DATE.search(line)
            if m:
                try:
                    return datetime.date(int(m.group(1)), int(m.group(2)), int(m.group(3)))
                except ValueError:
                    return None
            return None
    return None


def issue_state(path):
    """(status, exists) for a linked issue path written relative to docs/roadmap."""
    full = os.path.normpath(os.path.join(ROADMAP, path))
    if not os.path.exists(full):
        return None, False
    try:
        with open(full, encoding="utf-8") as fh:
            head = fh.read(2000)
    except OSError:
        return None, True
    m = re.search(r"^status:\s*(\S+)", head, re.M)
    return (m.group(1).strip() if m else None), True


def audit(max_age_days):
    present = workflows_present()
    today = datetime.date.today()
    rows = []
    for name in sorted(os.listdir(ROADMAP)):
        if not name.endswith(".md"):
            continue
        path = os.path.join(ROADMAP, name)
        with open(path, encoding="utf-8") as fh:
            text = fh.read()

        dead_wf = sorted(
            {
                w
                for w in WORKFLOW_REF.findall(text)
                if w not in present and w not in NOT_OURS
            }
        )
        dead_paths = sorted(
            {
                p
                for p in PATH_REF.findall(text)
                if p.split("/", 1)[0] in OUR_ROOTS
                and not SERIES_DOC.match(p)
                and not os.path.exists(os.path.join(ROOT, p))
            }
        )
        stale_issues = sorted(
            {
                num
                for p, num in ISSUE_LINK.findall(text)
                if (lambda s: s[0] in ("resolved", "wontfix"))(issue_state(p))
            }
        )
        d = status_date(text)
        age = (today - d).days if d else None
        cold = age is not None and age > max_age_days

        # `cold` alone does NOT qualify a row. Age is a prioritisation hint,
        # not evidence -- phase-162 is 'Not Started' and CORRECT, and a report
        # that flagged every old phase would be a list of the roadmap.
        if dead_wf or dead_paths or stale_issues:
            rows.append((name, dead_wf, dead_paths, stale_issues, age))
    # Strongest signal first. Measured 2026-09-04 across 63 active phases: dead
    # WORKFLOW refs and resolved-issue links flag 8 phases each and were right
    # both times they were chased (phase-196's stranded acceptance criterion,
    # phase-356's issue 0260). Dead PATHS flag 17 and are the weak column -- a
    # phase legitimately names a file that has since moved, and only phase-196's
    # eleven-at-once was a real finding. A triage reads top-down and stops when
    # the evidence thins.
    rows.sort(key=lambda r: (-len(r[1]), -len(r[3]), -len(r[2]), r[0]))
    return rows


def main(argv):
    ap = argparse.ArgumentParser(description=(__doc__ or "").split("\n")[0])
    ap.add_argument("--max-age-days", type=int, default=60)
    ap.add_argument("--markdown", action="store_true", help="emit a report body")
    args = ap.parse_args(argv[1:])

    rows = audit(args.max_age_days)
    total = len([f for f in os.listdir(ROADMAP) if f.endswith(".md")])

    if args.markdown:
        print("## Roadmap verification candidates (phase-419 W3)\n")
        print(f"{len(rows)} of {total} active phases have at least one candidate signal.")
        print("A signal is NOT a defect — see `scripts/roadmap-audit.py` for why each")
        print("needs a person. Work the evidence, not the count.\n")
        print("| phase | dead workflow refs | dead paths | resolved issues linked | status age |")
        print("| --- | --- | --- | --- | ---: |")
        for name, wf, paths, iss, age in rows:
            print(
                f"| `{name[:-3]}` | {', '.join(f'`{w}`' for w in wf) or '—'} "
                f"| {', '.join(f'`{p}`' for p in paths[:3]) or '—'} "
                f"| {', '.join(iss) or '—'} | {age if age is not None else '—'}d |"
            )
        return 0

    print(f"roadmap-audit: {len(rows)} of {total} active phase(s) carry a candidate signal.")
    print("A signal is NOT a defect — each needs a person. Evidence below.\n")
    for name, wf, paths, iss, age in rows:
        print(f"── {name[:-3]}" + (f"   (status {age}d old)" if age is not None else ""))
        if wf:
            print(f"     dead workflow refs: {', '.join(wf)}")
        if paths:
            print(f"     dead paths:         {', '.join(paths[:5])}")
        if iss:
            print(f"     resolved issues:    {', '.join(iss)}")
    print(
        "\nThe three classes this cannot reach — supersession, a reversed premise,\n"
        "a claim of absence — are worked examples in phase-419 W3, not heuristics."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
