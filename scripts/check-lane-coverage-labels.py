#!/usr/bin/env python3
"""issue 1514 — a lane's LABEL must not claim an event its step does not run on.

`gate.yml`'s `just check build` step — the FULL compile tier — carries
`if: contains(fromJSON('["schedule","workflow_dispatch"]'), github.event_name)`,
so it never runs on a pull request. The job it lives in was NAMED
`check (fast on push; full on PR/nightly)` and the file header said *"the compile
tier runs on PR + nightly only"*. Both asserted PR coverage the `if:` denies.

That is "a gate that WORKS is not a gate that RUNS" (issue 1226) in its
REPORTING form, and it has a measured cost: two agent sessions in one day chased
`check::build` reds that the PR lane would never have shown them, and neither
could tell from the workflow whether the lane was supposed to cover their branch.
A label is how a contributor decides whether a red is theirs.

WHAT IS CHECKED, and the scope is deliberately narrow: the EVENTS are MEASURED
from the step's own `if:`, and the CLAIM is read from the file rather than
authored here — so there is no table to drift. If `just check build` does not run
on `pull_request`, no phrase in the enclosing job's `name:` or in the workflow
header may claim that it does.

NOT checked: whether every step's conditions are individually described. A
workflow that documented each `if:` in prose would drift worse than one that
documents none. This catches the specific lie that cost the sessions: the lane
advertising compile-tier coverage on an event where only a SMOKE runs.

Self-test drives both verdicts on the normal path (issue 1167).
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
GATE = REPO / ".github/workflows/gate.yml"

# A phrase claiming the FULL tier on a pull request. Matched case-insensitively
# against the job name and the header. Deliberately about the tier, not about
# the smoke: `check compile-smoke` really is PR-only and saying so is correct.
PR_TIER_CLAIMS = (
    r"full\s+on\s+pr",
    r"compile\s+tier\s+runs\s+on\s+pr",
    r"compile\s+tier[^.]{0,40}\bon\s+a?\s*pull\s+request",
)


# issue 1616 (W7): the claim is a SHAPE, not a list of phrases — a clause that
# names the compile tier (or "full") AND a pull request. The authored phrase
# list missed "full compile tier for every PR".
_TIER = re.compile(r"\b(full|compile[\s-]+tier)\b", re.I)
_PR = re.compile(r"\b(PRs?|pull[\s-]+requests?)\b", re.I)
_NEG = re.compile(r"\b(not|never|no|except|only\s+on\s+(?:nightly|schedule|push|manual))\b", re.I)


def pr_tier_claims(blob: str) -> list[str]:
    """Clauses of `blob` that claim the compile tier runs on a pull request."""
    out = []
    for clause in re.split(r"[;.\n]|\s+-{1,2}\s+|—", blob):
        c = clause.strip(" #()")
        if _TIER.search(c) and _PR.search(c) and not _NEG.search(c):
            out.append(c)
    for pat in PR_TIER_CLAIMS:
        m = re.search(pat, blob, re.I)
        if m and m.group(0) not in " ".join(out):
            out.append(m.group(0))
    return out


def build_step_events(text: str) -> set[str] | None:
    """Events the `just check build` step runs on, measured from its `if:`."""
    m = re.search(
        r"^\s*-\s*name:[^\n]*just check build[^\n]*\n\s*if:\s*(?P<cond>[^\n]*)",
        text,
        re.M,
    )
    if not m:
        return None
    cond = m.group("cond")
    if "github.event_name" not in cond:
        return None
    return set(re.findall(r'"([a-z_]+)"', cond))


def check_job_name(text: str) -> str | None:
    m = re.search(r"^  check:\n(?:\s+#[^\n]*\n)*\s+name:\s*(?P<n>[^\n]*)", text, re.M)
    return m.group("n").strip() if m else None


def header(text: str) -> str:
    return "\n".join(l for l in text.splitlines()[:60] if l.startswith("#"))


def scan(text: str) -> list[str]:
    events = build_step_events(text)
    if events is None:
        return [
            "gate.yml: could not find the `just check build` step's event `if:` — "
            "this gate cannot verify issue 1514's invariant, which is a failure, "
            "not a pass"
        ]
    if "pull_request" in events:
        return []  # It DOES run on a PR; claiming so is then correct.

    out: list[str] = []
    name = check_job_name(text)
    for where, blob in (("the `check` job's `name:`", name or ""), ("the workflow header", header(text))):
        for claim in pr_tier_claims(blob):
            if claim:
                out.append(
                    f"gate.yml: {where} claims {claim!r}, but `just check "
                    f"build` runs only on {sorted(events)} — never on a pull "
                    f"request (issue 1514).\n"
                    f"    A PR gets `check compile-smoke`, not the tier. Say that, "
                    f"or the next contributor cannot tell whether a "
                    f"`check::build` red is theirs."
                )
    return out


def self_test() -> None:
    good = GATE.read_text()
    assert not scan(good), "the live workflow fails this gate; fix the finding it reports"

    bad_name = good.replace(
        check_job_name(good) or "", "check (fast on push; full on PR/nightly)", 1
    )
    assert scan(bad_name), (
        "NEGATIVE CONTROL FAILED: a job name claiming 'full on PR' was accepted "
        "while `just check build` excludes pull_request — that is issue 1514"
    )

    assert scan(good.replace(check_job_name(good) or "", "check (fast + full compile tier for every PR)", 1)), \
        "NEGATIVE CONTROL FAILED: 'full compile tier for every PR' was accepted (issue 1616)"
    assert not pr_tier_claims("check (fast + PR source gates; full compile tier on nightly/manual)")

    bad_hdr = good.replace(
        "# nano-ros", "# the compile tier runs on PR + nightly only\n# nano-ros", 1
    )
    if bad_hdr != good:
        assert scan(bad_hdr), (
            "NEGATIVE CONTROL FAILED: a header claiming PR compile-tier coverage "
            "was accepted"
        )

    # And the gate must not fire when the claim is TRUE.
    widened = re.sub(
        r'(just check build[^\n]*\n\s*if:\s*[^\n]*)"schedule"',
        r'\1"pull_request", "schedule"',
        good,
        count=1,
    )
    if widened != good:
        assert not scan(widened.replace(
            check_job_name(good) or "", "check (full on PR/nightly)", 1
        )), "a claim of PR coverage must be ACCEPTED when the step really runs on a PR"


def main() -> int:
    self_test()
    findings = scan(GATE.read_text())
    if findings:
        print("check-lane-coverage-labels: FAIL\n")
        for f in findings:
            print(f"  {f}\n")
        return 1
    ev = sorted(build_step_events(GATE.read_text()) or [])
    print(
        "check-lane-coverage-labels: OK (`just check build` runs on "
        f"{ev}; no label claims a pull request)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
