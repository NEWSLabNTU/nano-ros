#!/usr/bin/env python3
"""Dispatch the scheduled lane a merged fix names, on `main` (phase-591 W2).

A commit that fixes a red scheduled lane says so with a trailer:

    Fixes-Lane: nightly
    Fixes-Lane: tier 2, ros2-interop

When it lands on `main`, `automation.yml`'s `lane-fix` job runs this script,
which dispatches each named lane's workflow on `main`
(`gh workflow run <file> --ref main`). A green dispatch on `main` at the
release commit counts as release evidence (`release-lanes-green.py`), so the
fix is proven within hours instead of at the next night's cron.

NAMES  a lane name is one `release-lanes-green.py` LANES names (`nightly`,
       `tier 1`, `tier 2`, `host-suite`, `ros2-interop`), or its workflow file's
       stem (`platform-matrix`). Case, spaces, `-` and `_` do not matter (`tier-2`,
       `Tier2`). Several lanes of one workflow dispatch it once. Any other name
       is REFUSED: the check fails, naming it and the known names, and nothing
       is dispatched for it.
QUIET  the lanes' own crons own the self-hosted runners. The quiet window runs
       from the earliest lane cron's hour to one hour after the latest's
       (read from the lane workflows, never written here: 03:00-07:00 UTC
       today). A merge inside it is not dispatched on push; `automation.yml`'s
       deferred cron, at the window's end, dispatches every lane named by a
       commit that landed in today's window.

  lane-fix-dispatch.py --check-pr N --repo OWNER/NAME   refuse an unknown name in PR N's commits
  lane-fix-dispatch.py --push BEFORE AFTER --repo R     dispatch the lanes the pushed commits name
  lane-fix-dispatch.py --deferred --repo R              dispatch the lanes today's quiet-window merges name
  lane-fix-dispatch.py --list                           the lane names and their workflows
  lane-fix-dispatch.py --gate                           (`just check lane-fix-dispatch`)
  lane-fix-dispatch.py --self-test                      the cases alone; every mode runs them first

Every mode prints what it did, and writes it to `$GITHUB_STEP_SUMMARY` when set.
"""

from __future__ import annotations

import datetime as dt
import importlib.util
import json
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
WORKFLOWS = ROOT / ".github" / "workflows"
AUTOMATION = WORKFLOWS / "automation.yml"
TRAILER = "Fixes-Lane"
# The marker on automation.yml's deferred cron line, so the self-test can find
# it and hold it to the quiet window's end.
DEFERRED_MARK = "# lane-fix deferred"

_TRAILER_RE = re.compile(rf"^\s*{TRAILER}\s*:\s*(.*?)\s*$", re.IGNORECASE | re.MULTILINE)
_CRON_RE = re.compile(r"""^\s*-\s*cron:\s*["']([^"']+)["']""", re.MULTILINE)


def _load_release_lanes():
    path = ROOT / "scripts" / "release" / "release-lanes-green.py"
    spec = importlib.util.spec_from_file_location("release_lanes_green", path)
    if spec is None or spec.loader is None:
        raise ImportError(f"cannot load {path}")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod.LANES


def key(name: str) -> str:
    """A lane name as compared: lower case, letters and digits only."""
    return re.sub(r"[^a-z0-9]", "", name.lower())


def lane_map(lanes=None) -> dict[str, str]:
    """{key(name): workflow file} for every LANES name and lane workflow stem."""
    lanes = _load_release_lanes() if lanes is None else lanes
    out: dict[str, str] = {}
    for name, wf, _ in lanes:
        out[key(name)] = wf
        out[key(Path(wf).stem)] = wf
    return out


def known_names(lanes=None) -> list[str]:
    lanes = _load_release_lanes() if lanes is None else lanes
    names = {name for name, _, _ in lanes} | {Path(wf).stem for _, wf, _ in lanes}
    return sorted(names)


def trailer_values(messages: list[str]) -> list[str]:
    """Every lane name the `Fixes-Lane:` lines of `messages` state, in order."""
    vals: list[str] = []
    for msg in messages:
        for m in _TRAILER_RE.finditer(msg):
            vals.extend(v.strip() for v in m.group(1).split(","))
    return vals


def resolve(values: list[str], lanes=None) -> tuple[list[str], list[str]]:
    """(workflow files to dispatch, sorted and unique; names refused)."""
    table = lane_map(lanes)
    wfs: set[str] = set()
    bad: list[str] = []
    for v in values:
        wf = table.get(key(v)) if v else None
        if wf is None:
            bad.append(v)
        else:
            wfs.add(wf)
    return sorted(wfs), bad


# User ruling 2026-11-24 -- three lanes were renamed, with no alias: an old
# name is REFUSED like any unknown one, and the refusal names its new spelling,
# because a trailer written before the rename is the likeliest place to meet it.
RENAMED = {
    "host-tests": "host-suite",
    "live-peer": "ros2-interop",
    "run-matrix": "platform-matrix",
}


def refusal(bad: list[str], lanes=None) -> str:
    shown = ", ".join(f"`{b}`" if b else "an empty name" for b in bad)
    renamed = {key(old): new for old, new in RENAMED.items()}
    hints = [f"`{b}` was renamed `{renamed[key(b)]}`" for b in bad if b and key(b) in renamed]
    return (
        f"{TRAILER}: {shown} is not a scheduled lane"
        + (f" ({'; '.join(hints)})" if hints else "")
        + ". Name one of: "
        + ", ".join(known_names(lanes))
        + " (release-lanes-green.py LANES, or a lane workflow's file stem)."
    )


def quiet_window(lanes=None, workflows: Path = WORKFLOWS) -> tuple[int, int]:
    """(start hour, end hour), UTC, end exclusive: from the earliest lane cron's
    hour to one past the latest's. A cron whose hour is not a plain number is
    an error, never guessed."""
    lanes = _load_release_lanes() if lanes is None else lanes
    hours: list[int] = []
    for wf in sorted({wf for _, wf, _ in lanes}):
        text = (workflows / wf).read_text(encoding="utf-8")
        for cron in _CRON_RE.findall(text):
            hour = cron.split()[1]
            if not hour.isdigit():
                raise ValueError(f"{wf}: cron `{cron}` has no plain hour")
            hours.append(int(hour))
    if not hours:
        raise ValueError("no lane workflow has a cron")
    start, end = min(hours), max(hours) + 1
    if end > 24:
        raise ValueError(f"the lane crons wrap past midnight ({start}..{end})")
    return start, end


def in_window(now: dt.datetime, window: tuple[int, int]) -> bool:
    return window[0] <= now.hour < window[1]


def deferred_cron_hour(automation: Path = AUTOMATION) -> int:
    """The hour of automation.yml's deferred cron (the line marked DEFERRED_MARK)."""
    for line in automation.read_text(encoding="utf-8").splitlines():
        if DEFERRED_MARK in line:
            m = _CRON_RE.match(line)
            if m:
                return int(m.group(1).split()[1])
    raise ValueError(f"{automation.name}: no cron line marked `{DEFERRED_MARK}`")


# --- I/O --------------------------------------------------------------------


def gh_json(args: list[str]) -> Any:
    out = subprocess.run(["gh", *args], check=True, capture_output=True, text=True).stdout
    return json.loads(out) if out.strip() else []


def pr_messages(repo: str, pr: int) -> list[str]:
    pages = gh_json(["api", "--paginate", "--slurp", f"repos/{repo}/pulls/{pr}/commits"])
    return [c["commit"]["message"] for page in pages for c in page]


def push_messages(repo: str, before: str, after: str) -> list[str]:
    if not before or set(before) == {"0"}:
        data = gh_json(["api", f"repos/{repo}/commits/{after}"])
        return [data["commit"]["message"]]
    data = gh_json(["api", f"repos/{repo}/compare/{before}...{after}"])
    return [c["commit"]["message"] for c in data.get("commits", [])]


def window_messages(repo: str, now: dt.datetime, window: tuple[int, int]) -> list[str]:
    day = now.astimezone(dt.timezone.utc).date()
    since = dt.datetime(day.year, day.month, day.day, window[0], tzinfo=dt.timezone.utc)
    until = dt.datetime(day.year, day.month, day.day, window[1], tzinfo=dt.timezone.utc)
    iso = lambda t: t.strftime("%Y-%m-%dT%H:%M:%SZ")  # noqa: E731
    pages = gh_json([
        "api", "--paginate", "--slurp",
        f"repos/{repo}/commits?sha=main&since={iso(since)}&until={iso(until)}&per_page=100",
    ])
    return [c["commit"]["message"] for page in pages for c in page]


def gh_dispatch(repo: str, wf: str) -> None:
    subprocess.run(["gh", "workflow", "run", wf, "--ref", "main", "--repo", repo], check=True)


def report(lines: list[str]) -> None:
    text = "\n".join(lines)
    print(text)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as f:
            f.write(text + "\n")


# --- modes ------------------------------------------------------------------


def run_check(messages: list[str], lanes=None) -> tuple[int, list[str]]:
    values = trailer_values(messages)
    wfs, bad = resolve(values, lanes)
    if bad:
        return 1, [refusal(bad, lanes)]
    if not values:
        return 0, [f"No `{TRAILER}:` line; nothing to dispatch after merge."]
    return 0, [f"After merge this dispatches on main: {', '.join(wfs)}."]


def run_dispatch(messages, now, window, dispatch, deferred: bool, lanes=None):
    """Returns (exit code, report lines). `dispatch(wf)` starts one run."""
    values = trailer_values(messages)
    wfs, bad = resolve(values, lanes)
    lines: list[str] = []
    rc = 0
    if bad:
        lines.append(refusal(bad, lanes))
        rc = 1
    if not wfs:
        lines.append(f"No lane to dispatch ({len(messages)} commit(s) read).")
        return rc, lines
    if not deferred and in_window(now, window):
        lines.append(
            f"Not dispatched now: {now:%H:%M}Z is inside the lane crons' window "
            f"({window[0]:02d}:00-{window[1]:02d}:00Z). The deferred run at "
            f"{window[1]:02d}:15Z dispatches: {', '.join(wfs)}."
        )
        return rc, lines
    for wf in wfs:
        dispatch(wf)
        lines.append(f"Dispatched {wf} on main.")
    return rc, lines


# --- self-test --------------------------------------------------------------


def self_test(quiet: bool = False) -> int:
    fails: list[str] = []

    def chk(name, cond):
        if not cond:
            fails.append(name)

    lanes = _load_release_lanes()
    chk("trailer read", trailer_values(["fix: x\n\nFixes-Lane: nightly\n"]) == ["nightly"])
    # A pre-rename name is refused, never mapped, and the refusal names the new one.
    old_wfs, old_bad = resolve(["live-peer"])
    chk("an old lane name is refused, not mapped", old_wfs == [] and old_bad == ["live-peer"])
    chk("the refusal names the new spelling",
        "`live-peer` was renamed `ros2-interop`" in refusal(old_bad))
    chk("trailer key is case-free, values split on commas",
        trailer_values(["x\n\nfixes-lane:  tier 2, ros2-interop\n"]) == ["tier 2", "ros2-interop"])
    chk("a line that only mentions the key is not a trailer",
        trailer_values(["see Fixes-Lane: below\n"]) == [])
    chk("no trailer", trailer_values(["fix: x\n"]) == [])

    wfs, bad = resolve(["nightly"], lanes)
    chk("nightly -> nightly.yml", (wfs, bad) == (["nightly.yml"], []))
    for spelling in ("tier 2", "tier-2", "Tier2", "tier_1", "platform-matrix"):
        chk(f"{spelling} -> platform-matrix.yml", resolve([spelling], lanes) == (["platform-matrix.yml"], []))
    chk("tier 1 and tier 2 dispatch platform-matrix once",
        resolve(["tier 1", "tier 2"], lanes)[0] == ["platform-matrix.yml"])
    chk("every LANES name resolves",
        all(resolve([n], lanes)[1] == [] for n, _, _ in lanes))
    # Negative control: a misspelled name is refused and dispatches nothing.
    wfs, bad = resolve(["nighlty"], lanes)
    chk("a misspelled lane is refused", wfs == [] and bad == ["nighlty"])
    chk("an empty name is refused", resolve([""], lanes)[1] == [""])
    chk("a non-lane workflow is refused (automation)", resolve(["automation"], lanes)[1] == ["automation"])
    rc, lines = run_check(["x\n\nFixes-Lane: nighlty\n"], lanes)
    chk("check refuses an unknown name", rc == 1 and "nighlty" in lines[0] and "nightly" in lines[0])
    rc, _ = run_check(["x\n\nFixes-Lane: ros2-interop\n"], lanes)
    chk("check passes a known name", rc == 0)

    window = quiet_window(lanes)
    chk(f"the quiet window spans every lane cron ({window})", window[0] < window[1])
    chk("the deferred cron runs at the window's end", deferred_cron_hour() == window[1])
    utc = dt.timezone.utc
    at = lambda h, m=0: dt.datetime(2026, 11, 24, h, m, tzinfo=utc)  # noqa: E731
    chk("window start is inside", in_window(at(window[0]), window))
    chk("one minute before the end is inside", in_window(at(window[1] - 1, 59), window))
    chk("the end is outside", not in_window(at(window[1]), window))
    chk("one minute before the start is outside", not in_window(at(window[0] - 1, 59), window))

    sent: list[str] = []
    msgs = ["a\n\nFixes-Lane: tier 2\n", "b\n\nFixes-Lane: tier 1, nightly\n", "c\n"]
    rc, _ = run_dispatch(msgs, at(12), window, sent.append, deferred=False, lanes=lanes)
    chk("a push outside the window dispatches each workflow once",
        rc == 0 and sent == ["nightly.yml", "platform-matrix.yml"])
    sent.clear()
    rc, lines = run_dispatch(msgs, at(window[0]), window, sent.append, deferred=False, lanes=lanes)
    chk("a push inside the window dispatches nothing and says when it will",
        rc == 0 and sent == [] and "deferred" in lines[0])
    sent.clear()
    rc, _ = run_dispatch(msgs, at(window[1]), window, sent.append, deferred=True, lanes=lanes)
    chk("the deferred run dispatches", sent == ["nightly.yml", "platform-matrix.yml"])
    sent.clear()
    rc, lines = run_dispatch(["x\n\nFixes-Lane: nighlty, ros2-interop\n"], at(12), window,
                             sent.append, deferred=False, lanes=lanes)
    chk("a push with one bad name fails and still dispatches the good one",
        rc == 1 and sent == ["ros2-interop.yml"] and "nighlty" in lines[0])
    sent.clear()
    rc, _ = run_dispatch(["x\n"], at(12), window, sent.append, deferred=False, lanes=lanes)
    chk("a push naming no lane dispatches nothing", rc == 0 and sent == [])

    if fails:
        for f in fails:
            print(f"lane-fix-dispatch self-test FAIL: {f}", file=sys.stderr)
        return 1
    if not quiet:
        print("lane-fix-dispatch self-test OK")
    return 0


def main(argv: list[str]) -> int:
    import argparse

    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    g = p.add_mutually_exclusive_group(required=True)
    g.add_argument("--self-test", action="store_true")
    g.add_argument("--gate", action="store_true")
    g.add_argument("--list", action="store_true")
    g.add_argument("--check-pr", type=int, metavar="N")
    g.add_argument("--push", nargs=2, metavar=("BEFORE", "AFTER"))
    g.add_argument("--deferred", action="store_true")
    p.add_argument("--repo")
    a = p.parse_args(argv)

    if a.self_test:
        return self_test()
    # Always, not only behind the flag: a negative control nobody runs decays
    # into a comment, and a dispatcher whose name table or window is wrong
    # must not dispatch.
    if self_test(quiet=True):
        return 1
    if a.gate:
        start, end = quiet_window()
        print(
            f"lane-fix-dispatch OK — {len(known_names())} lane names, quiet window "
            f"{start:02d}:00-{end:02d}:00Z, deferred cron at {deferred_cron_hour():02d}:15Z"
        )
        return 0
    if a.list:
        table = lane_map()
        for n in known_names():
            print(f"{n:12} {table[key(n)]}")
        return 0
    if not a.repo:
        p.error("--repo is required")
    if a.check_pr is not None:
        rc, lines = run_check(pr_messages(a.repo, a.check_pr))
        report(lines)
        return rc
    now = dt.datetime.now(dt.timezone.utc)
    window = quiet_window()
    if a.push:
        msgs = push_messages(a.repo, a.push[0], a.push[1])
        rc, lines = run_dispatch(msgs, now, window, lambda wf: gh_dispatch(a.repo, wf), deferred=False)
    else:
        msgs = window_messages(a.repo, now, window)
        rc, lines = run_dispatch(msgs, now, window, lambda wf: gh_dispatch(a.repo, wf), deferred=True)
    report(lines)
    return rc


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
