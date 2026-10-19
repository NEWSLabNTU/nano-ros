#!/usr/bin/env python3
"""Is the merge queue's check timeout still wide enough for the gate lane?

Issue 1629. The `main-rules` ruleset's `merge_queue` rule carries
`check_response_timeout_minutes`: a queued entry whose required check has not
reported within that many minutes is ejected `checks_timed_out`, even when the
check goes on to PASS. phase-396 W3 set it to 60 with a written rule — "the usual
rule is ~2x observed duration" — for a ~30-minute lane. The lane then ran 32 to
73 minutes on the same event, an entry was ejected ten minutes before its own
gate reported green, and `main` stood still for 2 h 43 m with nothing wrong
anywhere. The policy was right; its INPUT moved, and nothing watched the input.

This watches it. Three facts, and the two relations between them:

  declared   the value `scripts/ci/enable-merge-queue.sh` writes when it is run
             with `--with-queue` — the one committed spelling of the setting,
             so a re-run of that script cannot silently undo a raise
  live       the value the ruleset holds now (`gh api .../rulesets/<id>`)
  lane       recent COMPLETED `merge_group` runs of `gate.yml`, measured from
             run creation (when the queue starts waiting) to completion

  FAIL  live != declared      someone changed one and not the other
  FAIL  declared < RATIO x p90 the lane outgrew the window (RATIO default 2.0)
  FAIL  live < slowest run     an entry of that duration WOULD have been ejected

Every FAIL prints the exact command that brings the ruleset to the declared
value. It never runs it: a ruleset edit binds every contributor and is an
administrator's decision, not a gate's.

NOT VERIFIED, never OK, when `gh` is absent, unauthenticated or offline — the
ruleset and the run history are only on GitHub, and a gate that cannot read them
has measured nothing (issue 1043's middle outcome, through the `nros_check_skip`
ledger). `--selftest` exercises the verdict logic and the declared-value parse
offline, and is what the fast line runs.

Usage::

    queue-timeout-agreement.py                 # live check (needs gh)
    queue-timeout-agreement.py --runs 60 --ratio 2.0
    queue-timeout-agreement.py --selftest
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import math
import os
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import check_skip  # noqa: E402

GATE = "queue-timeout-agreement"
REPO = os.environ.get("NROS_QUEUE_REPO", "NEWSLabNTU/nano-ros")
RULESET = os.environ.get("NROS_QUEUE_RULESET", "main-rules")
DECLARING_SCRIPT = ROOT / "scripts" / "ci" / "enable-merge-queue.sh"
KEY = "check_response_timeout_minutes"


def declared_timeout(text: str) -> int:
    """The timeout `enable-merge-queue.sh` writes. It spells it twice — the
    JSON it PUTs and the plan it prints — and both must agree, or the dry run
    describes a different setting from the one `--apply` writes."""
    written = re.findall(rf'"{KEY}":\s*(\d+)', text)
    planned = re.findall(r"^\s*check_response_timeout\s+(\d+)\s*min", text, re.M)
    if len(written) != 1 or len(planned) != 1:
        raise ValueError(
            f"expected one `\"{KEY}\": N` and one `check_response_timeout N min` "
            f"line in {DECLARING_SCRIPT.name}, found {len(written)} and {len(planned)}")
    if written[0] != planned[0]:
        raise ValueError(
            f"{DECLARING_SCRIPT.name} writes {KEY}={written[0]} but its plan prints "
            f"{planned[0]} min")
    return int(written[0])


def p90(values: list[float]) -> float:
    """Nearest-rank 90th percentile."""
    v = sorted(values)
    return v[max(0, math.ceil(0.9 * len(v)) - 1)]


def durations(runs: list[dict]) -> list[float]:
    """Minutes from creation to completion, for runs that reached a verdict.
    Cancelled runs are left out: an ejected or superseded batch is cancelled at
    an arbitrary point and says nothing about how long the lane takes."""
    def ts(s: str) -> dt.datetime:
        return dt.datetime.fromisoformat(s.replace("Z", "+00:00"))
    out = []
    for r in runs:
        if r.get("status") != "completed" or r.get("conclusion") not in ("success", "failure"):
            continue
        out.append((ts(r["updatedAt"]) - ts(r["createdAt"])).total_seconds() / 60)
    return out


def recommend(lane_p90: float, ratio: float) -> int:
    """The smallest multiple of 10 minutes that is RATIO x p90."""
    return int(math.ceil(ratio * lane_p90 / 10.0) * 10)


def verdict(declared: int, live: int, lane: list[float], ratio: float) -> list[str]:
    errs = []
    if live != declared:
        errs.append(f"the ruleset holds {KEY}={live}, but {DECLARING_SCRIPT.name} "
                    f"declares {declared}")
    if lane:
        q = p90(lane)
        if declared < ratio * q:
            errs.append(f"declared {declared} min < {ratio:g} x p90 ({q:.0f} min) of "
                        f"{len(lane)} merge_group gate runs — raise the declared value "
                        f"to {recommend(q, ratio)}")
        worst = max(lane)
        if live < worst:
            errs.append(f"live {live} min < the slowest observed run ({worst:.0f} min): "
                        f"an entry that slow is ejected `checks_timed_out` while its gate "
                        f"is still running")
    return errs


def update_command(ruleset_id: int, value: int) -> str:
    # A ruleset PUT replaces the WHOLE rules array, so the command edits the
    # current one in place rather than sending one rule.
    return (
        f"gh api repos/{REPO}/rulesets/{ruleset_id} \\\n"
        f"  | jq '{{rules: (.rules | map(if .type == \"merge_queue\" then "
        f".parameters.{KEY} = {value} else . end))}}' \\\n"
        f"  | gh api -X PUT repos/{REPO}/rulesets/{ruleset_id} --input -")


def gh_json(args: list[str]):
    try:
        out = subprocess.run(["gh", *args], capture_output=True, text=True, timeout=120)
    except (OSError, subprocess.TimeoutExpired) as exc:
        return None, str(exc)
    if out.returncode != 0:
        return None, f"gh exited {out.returncode}: {out.stderr.strip()[:200]}"
    try:
        return json.loads(out.stdout), ""
    except json.JSONDecodeError as exc:
        return None, f"unparseable gh output: {exc}"


def live_timeout() -> tuple[int | None, int | None, str]:
    rulesets, err = gh_json(["api", f"repos/{REPO}/rulesets"])
    if rulesets is None:
        return None, None, err
    rid = next((r["id"] for r in rulesets if r.get("name") == RULESET), None)
    if rid is None:
        return None, None, f"no ruleset named {RULESET!r} on {REPO}"
    rs, err = gh_json(["api", f"repos/{REPO}/rulesets/{rid}"])
    if rs is None:
        return None, rid, err
    for rule in rs.get("rules", []):
        if rule.get("type") == "merge_queue":
            return int(rule["parameters"][KEY]), rid, ""
    return None, rid, f"ruleset {RULESET!r} has no merge_queue rule"


def selftest() -> int:
    text = DECLARING_SCRIPT.read_text()
    declared = declared_timeout(text)
    # The two spellings must be caught disagreeing, and a missing one refused.
    bad = text.replace(f'"{KEY}": {declared}', f'"{KEY}": {declared + 1}')
    for broken in (bad, text.replace(f'"{KEY}"', '"x"')):
        try:
            declared_timeout(broken)
        except ValueError:
            pass
        else:
            raise SystemExit("selftest FAILED: a broken declaration parsed")
    assert p90([float(i) for i in range(1, 11)]) == 9.0
    assert p90([5.0]) == 5.0
    assert recommend(31.0, 2.0) == 70
    # 2026-10-02, the measured ejection: 60 against a 73-minute run.
    oct02 = [41, 35, 64, 32, 40, 73]
    errs = verdict(60, 60, oct02, 2.0)
    assert len(errs) == 2 and "raise the declared value to 150" in errs[0], errs
    assert verdict(150, 150, oct02, 2.0) == []
    assert len(verdict(90, 60, [20.0, 25.0], 2.0)) == 1  # drift alone
    assert verdict(90, 90, [], 2.0) == []                 # no runs: no lane claim
    runs = [
        {"status": "completed", "conclusion": "success",
         "createdAt": "2026-10-18T18:00:00Z", "updatedAt": "2026-10-18T18:30:00Z"},
        {"status": "completed", "conclusion": "cancelled",
         "createdAt": "2026-10-18T18:00:00Z", "updatedAt": "2026-10-18T18:01:00Z"},
        {"status": "in_progress", "conclusion": None,
         "createdAt": "2026-10-18T18:00:00Z", "updatedAt": "2026-10-18T18:05:00Z"},
    ]
    assert durations(runs) == [30.0]
    assert "rulesets/7 --input -" in update_command(7, 90)
    print(f"{GATE} selftest: OK (declared {KEY}={declared})")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--runs", type=int, default=50)
    ap.add_argument("--ratio", type=float, default=2.0)
    ap.add_argument("--selftest", action="store_true")
    a = ap.parse_args()
    if a.selftest:
        return selftest()

    declared = declared_timeout(DECLARING_SCRIPT.read_text())
    live, rid, err = live_timeout()
    if live is None:
        return check_skip.unverified(GATE, f"cannot read the {RULESET} ruleset ({err})")
    runs, err = gh_json(["run", "list", "--repo", REPO, "--workflow", "gate.yml",
                         "--event", "merge_group", "--limit", str(a.runs),
                         "--json", "status,conclusion,createdAt,updatedAt"])
    if runs is None:
        return check_skip.unverified(GATE, f"cannot list merge_group gate runs ({err})")
    lane = durations(runs)
    if lane:
        print(f"{GATE}: declared {declared}, live {live}; {len(lane)} merge_group gate "
              f"runs p50 {sorted(lane)[len(lane) // 2]:.0f} / p90 {p90(lane):.0f} / "
              f"max {max(lane):.0f} min")
    else:
        print(f"{GATE}: declared {declared}, live {live}; no completed merge_group runs")
    errs = verdict(declared, live, lane, a.ratio)
    if not errs:
        print(f"{GATE}: OK")
        return 0
    for e in errs:
        print(f"[FAIL] {e}")
    target = declared
    if lane and declared < a.ratio * p90(lane):
        target = recommend(p90(lane), a.ratio)
        print(f"\nFirst raise the declared value in {DECLARING_SCRIPT.relative_to(ROOT)} "
              f"to {target} (both spellings), then:")
    print("\nAn administrator applies it (this gate never does):\n")
    print(update_command(rid, target))
    return 1


if __name__ == "__main__":
    sys.exit(main())
