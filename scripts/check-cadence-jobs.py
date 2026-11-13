#!/usr/bin/env python3
"""Every board cadence names a job that keeps it — phase-535 W8, RFC-0122 D8.

`packages/boards/board-support.toml` promises a run per board: `cadence =
"merge"` (every merge), `"daily"`, `"weekly"` or `"manual"` (a dated run by
hand). RFC-0122 D8 binds each automated value to ONE workflow job, and D9 says
a cadence no job keeps is lowered, not kept. This gate reads the registry and
the workflows and refuses a row whose cadence no job serves.

The binding (D8), and what "serves" means here — all static, no fixtures:

  merge   `merge-runtime.yml` job `merge-runtime`. The workflow triggers on
          `merge_group` with no `paths:` filter, the job runs `just build merge`
          and `just ci merge`, and its matrix lists the row's module. While the
          workflow triggers on `merge_group`, the reverse holds too: every shard
          names a merge row's module. Since the 2026-11-12 ruling no row is
          `merge` and the workflow is dispatch-only (a re-measurement tool), so
          its shards promise nothing and are not held to a row; a `merge` row
          added back is refused until the workflow triggers on `merge_group`
          again.
  daily   `nightly.yml` on `schedule`: the `platform` job, whose matrix is
          the `runnable="…"` list and whose run step is `just test <platform>`.
          OR, for a host witness row (`runs = "host"`, `breadth = "witness"`:
          tier 1's pool), `run-matrix.yml`'s scheduled `tier1` job running
          `just ci tier1`. A row neither reaches has no daily RUN job.
  weekly  `probe-platforms.yml` on `schedule`, job `chapter`, and every track
          the row's `probe` names is a weekly track (scripts/probe/
          probe-tracks.json).
  manual  no job: the row's `last_run` date is the evidence (check-board-tiers).

A row's MODULE (its `just` scope) is `linux` for the Linux host and its
`nightly_token` otherwise, the same derivation `check-board-tiers` makes.

An UNSERVED row fails unless `.config/cadence-job-baseline.toml` lists it with
a reason and an issue id. A listed row that IS served fails too (the baseline
only shrinks). There is no allowlist without an issue.

NOT checked here, deliberately: whether the job RAN in the last window, and
whether it was green. That is a runtime question (`just nightly-triage`,
`scripts/ci/merge-runtime-p95.py`), and a static gate that answered it would
be a probe of the network.

Usage::

    check-cadence-jobs.py [--selftest]
"""

import re
import sys
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import board_registry  # noqa: E402

try:
    import tomllib
except ImportError:  # python < 3.11
    import tomli as tomllib

REGISTRY = ROOT / "packages" / "boards" / "board-support.toml"
WORKFLOWS = ROOT / ".github" / "workflows"
BASELINE = ROOT / ".config" / "cadence-job-baseline.toml"
PROBE_TRACKS = ROOT / "scripts" / "probe" / "probe-tracks.json"

CADENCES = ("merge", "daily", "weekly", "manual")
MERGE_WF, MERGE_JOB = "merge-runtime.yml", "merge-runtime"
DAILY_WF, DAILY_JOB = "nightly.yml", "platform"
TIER1_WF, TIER1_JOB = "run-matrix.yml", "tier1"
WEEKLY_WF, WEEKLY_JOB = "probe-platforms.yml", "chapter"


def row_key(row):
    return row.get("matrix_platform") or row.get("crate") or row.get("external") or "?"


def row_module(row):
    return "linux" if row.get("matrix_platform") == "Linux" else row.get("nightly_token")


def _on(doc):
    # PyYAML reads the bare key `on` as boolean True.
    return doc.get("on", doc.get(True)) or {}


def _job(doc, name):
    return (doc.get("jobs") or {}).get(name)


def _run_text(job):
    return "\n".join(str(s.get("run", "")) for s in (job.get("steps") or []))


def merge_errors(rows, wf_text):
    """(errors, unserved) for the merge cadence."""
    errs, unserved = [], {}
    merge_rows = [r for r in rows if r.get("cadence") == "merge"]
    if wf_text is None:
        for r in merge_rows:
            unserved[row_key(r)] = f"no {MERGE_WF}"
        return errs, unserved
    doc = yaml.safe_load(wf_text) or {}
    on = _on(doc)
    why = []
    if "merge_group" not in on:
        why.append(f"{MERGE_WF} does not trigger on merge_group")
    elif isinstance(on.get("merge_group"), dict) and (
            "paths" in on["merge_group"] or "paths-ignore" in on["merge_group"]):
        why.append(f"{MERGE_WF} path-filters merge_group (a filtered run never "
                   "starts, so the promise is kept only for some merges)")
    job = _job(doc, MERGE_JOB)
    shards = set()
    if job is None:
        why.append(f"{MERGE_WF} has no job `{MERGE_JOB}`")
    else:
        run = _run_text(job)
        for verb in ("just build merge", "just ci merge"):
            if verb not in run:
                why.append(f"job `{MERGE_JOB}` does not run `{verb}`")
        matrix = ((job.get("strategy") or {}).get("matrix") or {})
        shards = {str(b) for b in (matrix.get("board") or [])}
        shards |= {str(i.get("board")) for i in (matrix.get("include") or [])
                   if isinstance(i, dict) and i.get("board")}
    for r in merge_rows:
        mod = row_module(r)
        if why:
            unserved[row_key(r)] = "; ".join(why)
        elif not mod:
            unserved[row_key(r)] = "the row has no module (`nightly_token`) to shard on"
        elif mod not in shards:
            unserved[row_key(r)] = (f"job `{MERGE_JOB}` has no `{mod}` shard "
                                    f"(shards: {sorted(shards)})")
    wanted = {row_module(r) for r in merge_rows}
    # A dispatch-only measurement tool promises nothing, so its shards need
    # no row; the reverse rule binds only a lane that runs per merge.
    reverse = "merge_group" in on
    for s in sorted(shards - wanted) if reverse else []:
        errs.append(f"{MERGE_WF}: shard `{s}` is the module of no `cadence = \"merge\"` "
                    "row; a lowered cadence takes its shard with it")
    return errs, unserved


def nightly_runnable(text):
    m = re.search(r'^\s*runnable="([^"]+)"', text, re.M)
    return set(m.group(1).split()) if m else set()


def is_host_witness(row):
    return row.get("runs") == "host" and row.get("breadth") == "witness"


def tier1_serves(wf_text):
    """'' when run-matrix.yml's scheduled `tier1` job runs `just ci tier1`,
    else why not."""
    if wf_text is None:
        return f"no {TIER1_WF}"
    doc = yaml.safe_load(wf_text) or {}
    if "schedule" not in _on(doc):
        return f"{TIER1_WF} has no schedule"
    job = _job(doc, TIER1_JOB)
    if job is None:
        return f"{TIER1_WF} has no job `{TIER1_JOB}`"
    if "just ci tier1" not in _run_text(job):
        return f"job `{TIER1_JOB}` does not run `just ci tier1`"
    return ""


def daily_job_per_board(doc, job):
    """Why the daily job is NOT one job per board, or [] (phase-558).

    User ruling 2026-11-13: "Have results per board instead of per platform
    family". So the job's matrix is the registry's daily boards
    (`scripts/ci/nightly-boards.py`, run by the `changes` job), each job is
    named after its board, and it runs its module's tests.
    """
    why = []
    if str(job.get("name", "")) != "${{ matrix.board }}":
        why.append(f"job `{DAILY_JOB}` is not named `${{{{ matrix.board }}}}`, so a "
                   "verdict does not name one board")
    matrix = ((job.get("strategy") or {}).get("matrix") or {})
    if "outputs.boards" not in str(matrix.get("include", "")):
        why.append(f"job `{DAILY_JOB}`'s matrix is not `include:` over the "
                   "`changes` job's `boards` output")
    if "nightly-boards.py" not in _run_text(_job(doc, "changes") or {}):
        why.append("the `changes` job does not derive `boards` with "
                   "scripts/ci/nightly-boards.py")
    if "just test ${{ matrix.module }}" not in _run_text(job):
        why.append(f"job `{DAILY_JOB}` does not run `just test <module>`")
    return why


def daily_unserved(rows, wf_text, tier1_text):
    unserved = {}
    daily = [r for r in rows if r.get("cadence") == "daily"]
    why = []
    runnable = set()
    if wf_text is None:
        why.append(f"no {DAILY_WF}")
    else:
        doc = yaml.safe_load(wf_text) or {}
        if "schedule" not in _on(doc):
            why.append(f"{DAILY_WF} has no schedule")
        job = _job(doc, DAILY_JOB)
        if job is None:
            why.append(f"{DAILY_WF} has no job `{DAILY_JOB}`")
        else:
            why += daily_job_per_board(doc, job)
        runnable = nightly_runnable(wf_text)
    tier1_why = tier1_serves(tier1_text)
    for r in daily:
        tok = row_module(r)
        if is_host_witness(r) and not tier1_why:
            continue
        # phase-558 — the job is the ROW's board, so a row with no board names
        # no job, whatever module it shares with one that does.
        if not why and tok in runnable and r.get("matrix_platform"):
            continue
        parts = why or ([f"module {tok!r} is not in the `{DAILY_JOB}` job's runnable "
                         f"set {sorted(runnable)}"] if tok not in runnable else
                        ["the row names no `matrix_platform`, so no per-board job is its"])
        if is_host_witness(r):
            parts = parts + [tier1_why]
        else:
            parts = parts + [f"it is not a host witness row, so `{TIER1_JOB}` "
                             "does not cover it"]
        unserved[row_key(r)] = "; ".join(parts)
    return unserved


def weekly_unserved(rows, wf_text, weekly_tracks):
    unserved = {}
    weekly = [r for r in rows if r.get("cadence") == "weekly"]
    if not weekly:
        return unserved
    doc = yaml.safe_load(wf_text) if wf_text else None
    for r in weekly:
        if doc is None:
            unserved[row_key(r)] = f"no {WEEKLY_WF}"
        elif "schedule" not in _on(doc) or _job(doc, WEEKLY_JOB) is None:
            unserved[row_key(r)] = f"{WEEKLY_WF} has no scheduled `{WEEKLY_JOB}` job"
        else:
            named = set(r.get("probe") or [])
            if not named or not named <= weekly_tracks:
                unserved[row_key(r)] = (f"probe tracks {sorted(named)} are not all weekly "
                                        f"tracks ({sorted(weekly_tracks)})")
    return unserved


def evaluate(rows, workflows, baseline, weekly_tracks):
    """workflows: {file name: text or None}; baseline: {key: entry}."""
    errs = []
    for r in rows:
        c = r.get("cadence")
        if c is not None and c not in CADENCES:
            errs.append(f"{row_key(r)}: cadence {c!r} is none of {CADENCES}")
    merge_errs, unserved = merge_errors(rows, workflows.get(MERGE_WF))
    errs += merge_errs
    unserved.update(daily_unserved(rows, workflows.get(DAILY_WF),
                                   workflows.get(TIER1_WF)))
    unserved.update(weekly_unserved(rows, workflows.get(WEEKLY_WF), weekly_tracks))
    cadence_of = {row_key(r): r.get("cadence") for r in rows}
    reported = []
    for key, why in sorted(unserved.items()):
        entry = baseline.get(key)
        if entry and entry.get("cadence") == cadence_of.get(key):
            # The entry's issue id stays in the baseline file: output names
            # no issue/RFC/phase ids (user ruling 2026-11-12).
            reported.append(f"{key} ({cadence_of[key]}): {why} — listed in "
                            f"{BASELINE.name}")
        else:
            errs.append(f"{key}: cadence = {cadence_of.get(key)!r} has no job that keeps "
                        f"it: {why}. Give it a job, or lower the cadence")
    for key, entry in sorted(baseline.items()):
        if key not in unserved:
            errs.append(f"{BASELINE.name}: {key} is listed, but its cadence is served "
                        "now — delete the entry (the baseline only shrinks)")
        elif entry.get("cadence") != cadence_of.get(key):
            errs.append(f"{BASELINE.name}: {key} is listed for cadence "
                        f"{entry.get('cadence')!r}, the registry says "
                        f"{cadence_of.get(key)!r}")
    return errs, reported


def load_baseline(path, errs):
    if not path.exists():
        return {}
    doc = tomllib.loads(path.read_text())
    out = {}
    for e in doc.get("unserved", []):
        key = e.get("row")
        if not key or not e.get("cadence") or not e.get("reason") or not isinstance(
                e.get("issue"), int):
            errs.append(f"{path.name}: every [[unserved]] entry needs `row`, `cadence`, "
                        f"`reason` and an integer `issue` (got {e})")
            continue
        issue_files = list((ROOT / "docs" / "issues").glob(f"{e['issue']:04d}-*.md")) + \
            list((ROOT / "docs" / "issues" / "archived").glob(f"{e['issue']:04d}-*.md"))
        if not issue_files:
            errs.append(f"{path.name}: {key}'s `issue` names no file under docs/issues/")
        out[key] = e
    return out


def selftest():
    wf_merge = """
name: merge-runtime
on:
  merge_group:
jobs:
  merge-runtime:
    strategy:
      matrix:
        include:
          - board: linux
          - board: zephyr
    steps:
      - run: just build merge
      - run: just ci merge
"""
    wf_daily = """
on:
  schedule:
    - cron: "0 7 * * *"
jobs:
  changes:
    steps:
      - run: |
          runnable="qemu freertos"
          boards="$(python3 scripts/ci/nightly-boards.py --modules "$sel")"
  platform:
    name: ${{ matrix.board }}
    strategy:
      matrix:
        include: ${{ fromJSON(needs.changes.outputs.boards) }}
    steps:
      - run: just test ${{ matrix.module }}
"""
    wf_tier1 = """
on:
  schedule:
    - cron: "0 6 * * *"
jobs:
  tier1:
    steps:
      - run: just ci tier1 run
"""
    wf_dispatch = wf_merge.replace("  merge_group:\n", "  workflow_dispatch:\n")
    witness_daily = {"matrix_platform": "ThreadxLinux", "cadence": "daily",
                     "nightly_token": "threadx_linux", "runs": "host", "breadth": "witness"}
    rows = [
        {"matrix_platform": "Linux", "cadence": "merge"},
        {"matrix_platform": "ZephyrNativeSim", "cadence": "merge", "nightly_token": "zephyr"},
        {"matrix_platform": "QemuBaremetal", "cadence": "daily", "nightly_token": "qemu"},
        {"matrix_platform": "Fvp", "cadence": "manual"},
    ]
    wfs = {MERGE_WF: wf_merge, DAILY_WF: wf_daily, TIER1_WF: wf_tier1, WEEKLY_WF: None}
    cases = []

    def case(name, errs_want, rows=rows, wfs=wfs, baseline=None, tracks=frozenset()):
        errs, _ = evaluate(rows, wfs, baseline or {}, set(tracks))
        ok = bool(errs) == errs_want
        cases.append((name, ok, errs))

    case("a served registry is clean", False)
    case("a merge board with no shard is refused", True,
         rows=rows + [{"matrix_platform": "ThreadxLinux", "cadence": "merge",
                       "nightly_token": "threadx_linux"}])
    case("a shard with no merge board is refused", True, rows=rows[:1] + rows[2:])
    case("a path-filtered merge_group is refused", True,
         wfs={**wfs, MERGE_WF: wf_merge.replace("  merge_group:\n",
                                                "  merge_group:\n    paths: [x]\n")})
    case("a merge job that does not run the lane is refused", True,
         wfs={**wfs, MERGE_WF: wf_merge.replace("just ci merge", "echo")})
    case("a missing merge workflow is refused", True, wfs={**wfs, MERGE_WF: None})
    case("a per-FAMILY daily job is refused (phase-558)", True,
         wfs={**wfs, DAILY_WF: wf_daily.replace("name: ${{ matrix.board }}",
                                                "name: ${{ matrix.plat }}")})
    case("a daily job whose matrix is not the registry's boards is refused", True,
         wfs={**wfs, DAILY_WF: wf_daily.replace("outputs.boards", "outputs.platforms")})
    case("a daily row with no board is refused", True,
         rows=rows + [{"crate": "c", "cadence": "daily", "nightly_token": "qemu"}])
    case("a daily token outside the runnable set is refused", True,
         rows=rows + [{"matrix_platform": "ZephyrQemuCortexM", "cadence": "daily",
                       "nightly_token": "zephyr"}])
    case("…unless baselined with an issue", False,
         rows=rows + [{"matrix_platform": "ZephyrQemuCortexM", "cadence": "daily",
                       "nightly_token": "zephyr"}],
         baseline={"ZephyrQemuCortexM": {"cadence": "daily", "issue": 1, "reason": "r"}})
    case("a baseline entry for a served row is refused", True,
         baseline={"QemuBaremetal": {"cadence": "daily", "issue": 1, "reason": "r"}})
    case("a weekly row with no probe workflow is refused", True,
         rows=rows + [{"matrix_platform": "X", "cadence": "weekly", "probe": ["t"]}])
    wf_weekly = "on:\n  schedule:\n    - cron: '0 9 * * 1'\njobs:\n  chapter:\n    steps: []\n"
    weekly_row = {"matrix_platform": "X", "cadence": "weekly", "probe": ["t"]}
    case("a weekly row on weekly tracks is served", False, rows=rows + [weekly_row],
         wfs={**wfs, WEEKLY_WF: wf_weekly}, tracks={"t"})
    case("a weekly row naming a non-weekly track is refused", True,
         rows=rows + [weekly_row], wfs={**wfs, WEEKLY_WF: wf_weekly}, tracks={"other"})
    daily_only = [r for r in rows if r.get("cadence") != "merge"]
    case("a host witness daily row is served by tier 1", False,
         rows=daily_only + [witness_daily], wfs={**wfs, MERGE_WF: wf_dispatch})
    case("…and refused when tier 1 is not scheduled", True,
         rows=daily_only + [witness_daily],
         wfs={**wfs, MERGE_WF: wf_dispatch,
              TIER1_WF: wf_tier1.replace("  schedule:\n    - cron: \"0 6 * * *\"\n",
                                         "  workflow_dispatch:\n")})
    case("…and refused when tier 1 does not run the tier", True,
         rows=daily_only + [witness_daily],
         wfs={**wfs, MERGE_WF: wf_dispatch,
              TIER1_WF: wf_tier1.replace("just ci tier1 run", "echo")})
    case("a non-witness daily row is not served by tier 1", True,
         rows=daily_only + [{"matrix_platform": "ZephyrQemuCortexM", "cadence": "daily",
                             "nightly_token": "zephyr", "runs": "emulator",
                             "breadth": "witness"}],
         wfs={**wfs, MERGE_WF: wf_dispatch})
    case("no merge rows and a dispatch-only merge workflow is clean", False,
         rows=daily_only, wfs={**wfs, MERGE_WF: wf_dispatch})
    case("a merge row with a dispatch-only merge workflow is refused", True,
         wfs={**wfs, MERGE_WF: wf_dispatch})
    case("an unknown cadence is refused", True,
         rows=rows + [{"matrix_platform": "Y", "cadence": "hourly"}])
    bad = [c for c in cases if not c[1]]
    for name, ok, errs in cases:
        print(f"  {'ok ' if ok else 'BAD'} {name}" + ("" if ok else f": {errs}"))
    if bad:
        print(f"check-cadence-jobs: selftest FAILED ({len(bad)} case(s))", file=sys.stderr)
        return 1
    print(f"check-cadence-jobs: selftest OK ({len(cases)} cases)")
    return 0


def main():
    if "--selftest" in sys.argv:
        return selftest()
    # Always, as a negative control nobody can skip: a gate whose rules stopped
    # firing would print OK forever.
    import contextlib
    import io
    with contextlib.redirect_stdout(io.StringIO()):
        if selftest() != 0:
            print("check-cadence-jobs: the selftest failed; run --selftest", file=sys.stderr)
            return 1
    rows = [r for r in board_registry.load(REGISTRY).get("board", []) if not r.get("infra")]
    errs = []
    baseline = load_baseline(BASELINE, errs)
    wfs = {}
    for name in (MERGE_WF, DAILY_WF, TIER1_WF, WEEKLY_WF):
        p = WORKFLOWS / name
        wfs[name] = p.read_text() if p.exists() else None
    import json
    weekly = {t["track"] for t in json.loads(PROBE_TRACKS.read_text())["tracks"]
              if t.get("weekly")}
    e, reported = evaluate(rows, wfs, baseline, weekly)
    errs += e
    for line in reported:
        print(f"check-cadence-jobs: NOTE {line}")
    if errs:
        # RFC-0122 D8/D9 is the rule; the message states it instead of citing it.
        print("check-cadence-jobs: a board cadence has no job behind it, and a "
              "cadence no job keeps must be lowered:",
              file=sys.stderr)
        for line in errs:
            print(f"  - {line}", file=sys.stderr)
        return 1
    counts = {}
    for r in rows:
        if r.get("cadence"):
            counts[r["cadence"]] = counts.get(r["cadence"], 0) + 1
    print("check-cadence-jobs: OK — "
          + ", ".join(f"{n} {c}" for c, n in sorted(counts.items()))
          + f" row(s); {len(reported)} baselined")
    return 0


if __name__ == "__main__":
    sys.exit(main())
