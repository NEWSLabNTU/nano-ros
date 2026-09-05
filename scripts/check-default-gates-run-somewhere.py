#!/usr/bin/env python3
"""A gate CI never runs, and a gate CI runs but cannot hear — issue 1040.

Two rules over one scan of `.github/workflows/`. They are the two halves of one
question, "does this gate report", and both halves have now failed in
production:

  R1 PLACEMENT   Every gate `just check` runs -- the derived fast lane, the
                 `build-serial:` registry, and the names on `default:` -- must
                 be reached by SOME workflow event. A gate no event runs is
                 invisible between local full-tier runs, so its reds accumulate
                 until whoever next runs the tier finds five at once with five
                 unrelated owners. `check-api-parity` was exactly this: `grep
                 -rl api-parity .github/workflows/` returned nothing, and three
                 unclassified ledger rows landed on main on 2026-09-04 alone.

  R2 VERDICT     A placement must be able to REPORT. A GitHub `run:` block is
                 `bash -e`, so a `just check <gate>` sequenced after another
                 `just check` in the same block does not execute when the
                 earlier one is red -- and a gate that did not execute is
                 indistinguishable from one that passed.

R2 is not hypothetical and it is why R1 alone is not enough. Measured
2026-09-05: gate.yml ran `check build`, `check no-std` and `check api-parity`
from a single `run:` block. `check build` had been red on that lane since
2026-09-01 (`workspace-all`, `workspace-features`), so on all five scheduled
runs since api-parity was wired there it NEVER EXECUTED -- while `grep -rl`
answered yes and the fix for 1040 read as landed. One red was reported for
three gates and two of them had no verdict at all. That is issue 0952's
withdrawal class (`ci::gate` prints what it withdrew for the same reason) one
level down, in YAML, where no recipe can print anything.

WHAT THIS DOES NOT REQUIRE

Merge-gating. `check-build` is deliberately `schedule`/`workflow_dispatch` only:
it resolves artifacts no PR job builds, and making it required once turned every
pull request red for a day (`check-lane-contracts` now forbids that shape). A
DAILY or PER-COMMIT signal is the bar here, not a blocking one -- so both rules
ask only that a verdict EXISTS somewhere, never that it gates.

MEMBERSHIP IS READ, NEVER RE-DERIVED

`check-gate-lists.py --list <lane>` is the one place lane membership lives
(issue 1072 deleted the authored `fast-serial:` registry; a fast gate is now any
recipe in `just/check.just` that is not in `build-serial:`, not exempt in
`.config/gate-lane-exempt.txt`, and takes no parameters). A second derivation
here would be the two-spellings bug this repo has paid for repeatedly -- and it
would answer from a stale set the day the derivation moves again.

Usage::

    check-default-gates-run-somewhere.py            # the gate
    check-default-gates-run-somewhere.py --survey   # gate -> events, never fails
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CHECK_JUST = ROOT / "just/check.just"
CI_JUST = ROOT / "just/ci.just"
WORKFLOWS = ROOT / ".github/workflows"
GATE_LISTS = ROOT / "scripts/check/check-gate-lists.py"

EVENTS = ("pull_request", "merge_group", "push", "schedule", "workflow_dispatch",
          "workflow_run")

# ---------------------------------------------------------------------------
# Gates that need no workflow placement, each with the reason it needs none.
#
# EMPTY BY MEASUREMENT, not by weakening. Every gate in scope reaches at least
# one event today, so there is nothing to excuse and inventing entries to be
# safe would be the six-entry false baseline issue 1030 rejected for the sibling
# rule. The shape stays because the honest exemption does exist in principle --
# a gate that can only run on hardware CI does not have, say -- and when one
# turns up it belongs here with its reason, not in a bare list and not by
# quietly narrowing the scope above.
#
# NOT the place for "it is not really a gate": that classification already has a
# home in `.config/gate-lane-exempt.txt`, which decides lane MEMBERSHIP. This
# file only ever asks whether something already in a lane is heard.
# ---------------------------------------------------------------------------
NO_PLACEMENT_NEEDED: "dict[str, str]" = {}

# Gates allowed to sit behind another gate in one `run:` block. Same reasoning
# as above, and the same emptiness: after the 2026-09-05 split every placement
# in the tree is first in its own block, and a shared block has no upside worth
# an exemption -- splitting a step costs three lines of YAML.
SHADOWED_PLACEMENT_OK: "dict[str, str]" = {}


def lane_members(lane):
    """Gate names in `lane`, from the ONE derivation that owns them."""
    out = subprocess.run(
        [sys.executable, str(GATE_LISTS), "--list", lane],
        capture_output=True, text=True, cwd=str(ROOT),
    )
    if out.returncode != 0:
        raise SystemExit(
            f"check-default-gates-run-somewhere: `check-gate-lists.py --list {lane}` "
            f"failed — membership has exactly one home and it did not answer:\n"
            f"{out.stderr.strip()}"
        )
    return [n.strip() for n in out.stdout.split() if n.strip()]


def default_list():
    """The recipes `just check` runs with no argument."""
    for line in CHECK_JUST.read_text().splitlines():
        m = re.match(r"^default:\s*(.+?)\s*$", line)
        if m:
            return m.group(1).split()
    return []


def _events_of(guard, wf_events):
    """Which of the workflow's events this `if:` guard admits.

    Over-approximates, like `check-lane-contracts._events_of` and for the same
    reason: this decides whether a gate is HEARD, and crediting an event too
    many costs a missed finding only if the guard was already narrower than it
    looks, while crediting too few invents findings nobody can act on.
    """
    if not guard:
        return set(wf_events)
    in_list = re.search(r"fromJSON\(\s*'\[([^\]]*)\]'\s*\)\s*,\s*github\.event_name", guard)
    if in_list:
        named = set(re.findall(r"['\"](" + "|".join(EVENTS) + r")['\"]", in_list.group(1)))
        if guard[: in_list.start()].rstrip().endswith("!"):
            return set(wf_events) - named
        return (named & set(wf_events)) or named
    eq = set(re.findall(r"github\.event_name\s*==\s*['\"](" + "|".join(EVENTS) + r")['\"]", guard))
    if eq:
        return (eq & set(wf_events)) or eq
    ne = set(re.findall(r"github\.event_name\s*!=\s*['\"](" + "|".join(EVENTS) + r")['\"]", guard))
    if ne and "||" not in guard:
        return set(wf_events) - ne
    return set(wf_events)


def run_blocks():
    """[(workflow, events, [command lines])] — one entry per `run:` block.

    Text-scanned, not YAML-parsed. The unit that matters is the `run:` BLOCK,
    because that is the unit `bash -e` aborts: two `just check` lines in one
    block share a fate, and two in adjacent steps do not. A YAML load would give
    the same blocks and would also have to re-implement the `if:` handling, so
    it buys nothing here.

    A step's own `if:` wins over the workflow's event list, including the folded
    form whose events live on continuation lines — reading only the `if:` line
    credits every step with every event the workflow declares, which is the
    scanner bug issue 1030 had to fix in the sibling gate before its rule could
    fire at all.
    """
    out = []
    if not WORKFLOWS.is_dir():
        return out
    for path in sorted(WORKFLOWS.glob("*.yml")) + sorted(WORKFLOWS.glob("*.yaml")):
        lines = path.read_text().split("\n")
        wf_events = set(re.findall(r"^\s{2}(" + "|".join(EVENTS) + r")\s*:", "\n".join(lines), re.M))
        guard, guard_indent = "", None
        block, block_indent = None, None
        for line in lines:
            stripped = line.strip()
            indent = len(line) - len(line.lstrip())
            # Inside a `run:` block: everything more-indented than the key.
            if block is not None:
                if not stripped or indent > block_indent:
                    block.append(stripped)
                    continue
                out.append((path.name, _events_of(guard, wf_events), block))
                block, block_indent = None, None
            if re.match(r"^\s*- (name|uses|run):", line):
                if re.match(r"^\s*- (name|uses):", line):
                    guard, guard_indent = "", None
            m_if = re.match(r"^(\s+)if:", line)
            if m_if:
                guard, guard_indent = line, len(m_if.group(1))
                continue
            if guard and guard_indent is not None:
                if (stripped and indent > guard_indent
                        and not re.match(r"^(name|uses|run|with|env|shell|id|"
                                         r"continue-on-error|timeout-minutes):", stripped)):
                    guard += "\n" + line
                    continue
                guard_indent = None
            m_run = re.match(r"^(\s*)-?\s*run:\s*(\S.*)?$", line)
            if m_run:
                inline = (m_run.group(2) or "").strip()
                if inline and inline not in ("|", ">", "|-", ">-", "|+", ">+"):
                    out.append((path.name, _events_of(guard, wf_events), [inline]))
                else:
                    block, block_indent = [], indent
        if block is not None:
            out.append((path.name, _events_of(guard, wf_events), block))
    return out


def lane_gates(lane, fast, build, default):
    """`just check <lane>` -> the gate names it runs."""
    if lane in ("fast", "fast-serial", "fast-parallel"):
        return set(fast)
    if lane in ("build", "build-serial"):
        return set(build)
    if lane == "default":
        out = set()
        for name in default:
            out |= lane_gates(name, fast, build, default)
        return out
    return {lane}


def _ci_lane_body(lane):
    """The body lines of `just ci <lane>`, continuation-joined."""
    text = CI_JUST.read_text() if CI_JUST.exists() else ""
    m = re.search(r"^%s(?:\s+\w+=\S*)*:.*$" % re.escape(lane), text, re.M)
    if not m:
        return []
    body, started = [], False
    for line in text[m.end():].split("\n"):
        if not started:
            started = True
        if line and not line[:1].isspace():
            break
        body.append(line)
    return body


def ci_lane_gates(lane, depth, fast, build, default):
    """`just ci <lane> [depth]` -> the gate names it reaches.

    Bounded at one hop on purpose: a tier reaches gates by naming `just check`
    (bare, or with an argument) or a `check::<gate>` step, and both are visible
    in the tier's own body. A full recipe-graph walk would over-attribute --
    `ci::matrix`'s `case` arms are two branches of which a given invocation runs
    exactly one, and crediting both makes `check::default` look merge-gating
    when only `_matrix-build` runs in the queue.
    """
    if lane == "matrix" and depth in ("run", "build"):
        lane = "_matrix-" + depth
    out = set()
    for line in _ci_lane_body(lane):
        if line.lstrip().startswith("#"):
            continue
        for m in re.finditer(r"check::([a-z0-9-]+)", line):
            out |= lane_gates(m.group(1), fast, build, default)
        for m in re.finditer(r"\bjust\s+check\b([^\n|&;]*)", line):
            args = [w for w in m.group(1).split() if re.fullmatch(r"[a-z0-9-]+", w)]
            out |= lane_gates(args[0] if args else "default", fast, build, default)
        for m in re.finditer(r"\bjust\s+ci::(_?[a-z0-9-]+)", line):
            if m.group(1) != lane:
                out |= ci_lane_gates(m.group(1), None, fast, build, default)
    return out


def placements(fast, build, default):
    """{gate: [(workflow, events, shadowed_by)]} over every workflow `run:` block."""
    found = {}
    for wf, events, cmds in run_blocks():
        seen_gate = None
        for cmd in cmds:
            if cmd.startswith("#"):
                continue
            hits = set()
            m = re.search(r"\bjust\s+check\b([^\n|&;#]*)", cmd)
            if m:
                args = [w for w in m.group(1).split() if re.fullmatch(r"[a-z0-9-]+", w)]
                hits |= lane_gates(args[0] if args else "default", fast, build, default)
            m = re.search(r"\bjust\s+ci\s+([a-z0-9-]+)(?:\s+([a-z0-9-]+))?", cmd)
            if m:
                hits |= ci_lane_gates(m.group(1), m.group(2), fast, build, default)
            if not hits:
                continue
            named = sorted(hits)[0] if len(hits) == 1 else None
            for gate in hits:
                found.setdefault(gate, []).append((wf, frozenset(events), seen_gate))
            seen_gate = seen_gate or named or "a lane"
    return found


def self_test():
    """Prove both rules can fail. Runs on the NORMAL path, every invocation —
    a negative control nobody runs decays into a comment."""
    ok = True

    def chk(desc, cond):
        nonlocal ok
        if not cond:
            print(f"self-test FAILED: {desc}", file=sys.stderr)
            ok = False

    # The parse must find the real lists, not empty ones that pass vacuously.
    fast, build, default = lane_members("fast-serial"), lane_members("build-serial"), default_list()
    chk("the fast lane is empty", len(fast) > 50)
    chk("the build lane is empty", len(build) > 5)
    chk("`default:` did not parse", bool(default))
    chk("a lane name looks wrong",
        all(re.fullmatch(r"[a-z0-9-]+", n) for n in fast + build + default))

    # R1's expansion: `just check fast` IS every fast gate, not the literal name.
    chk("`fast` does not expand to the lane",
        lane_gates("fast", fast, build, default) == set(fast))
    chk("a bare `just check` does not reach the build tier",
        set(build) <= lane_gates("default", fast, build, default))

    # R2's shadow detection, on the exact shape measured on 2026-09-05.
    blocks = run_blocks()
    chk("no `run:` block was scanned at all", len(blocks) > 10)
    probe = ["source ./activate.sh", "just check build", "just check api-parity"]
    seen, shadow = None, {}
    for cmd in probe:
        hits = set()
        m = re.search(r"\bjust\s+check\b([^\n|&;#]*)", cmd)
        if m:
            args = [w for w in m.group(1).split() if re.fullmatch(r"[a-z0-9-]+", w)]
            if args:
                hits = {args[0]}
        for g in hits:
            shadow[g] = seen
        seen = seen or (sorted(hits)[0] if len(hits) == 1 else None)
    chk("a leading `just check` is not credited as unshadowed", shadow.get("build") is None)
    chk("a trailing `just check` is not seen as shadowed", shadow.get("api-parity") == "build")

    # The event reader decides which lane a placement counts for.
    allev = {"pull_request", "merge_group", "push", "schedule"}
    chk("no `if:` must mean every event the workflow declares",
        _events_of("", allev) == allev)
    chk("a fromJSON list must narrow to exactly those events",
        _events_of("""if: ${{ contains(fromJSON('["schedule","workflow_dispatch"]'),"""
                   """ github.event_name) }}""", allev) == {"schedule"})
    chk("a folded guard must keep the events on its continuation lines",
        _events_of("        if: >-\n"
                   "          ${{ contains(fromJSON('[\"pull_request\"]'), github.event_name)\n"
                   "              && !cancelled() }}", allev) == {"pull_request"})
    chk("an exclusion OR-ed with a non-event condition must not exclude",
        _events_of("if: ${{ always() && (github.event_name != 'pull_request'"
                   " || needs.changes.outputs.code == 'true') }}", allev) == allev)

    if ok:
        print(
            f"check-default-gates-run-somewhere self-test: OK "
            f"({len(fast)} fast + {len(build)} build gate(s), "
            f"{len(blocks)} workflow `run:` block(s))"
        )
    return ok


def survey(fast, build, default, place):
    """gate -> the events that run it. Never fails; this is the measurement."""
    scope = sorted(set(fast) | set(build) | set(default) | set(place))
    rows = []
    for gate in scope:
        ev = set()
        for _wf, events, _sh in place.get(gate, []):
            ev |= set(events)
        rows.append((gate, ev))
    for gate, ev in rows:
        lane = "fast" if gate in fast else "build" if gate in build else "other"
        print(f"  {lane:6s} {gate:44s} {','.join(sorted(ev)) or 'NO EVENT'}")
    # Three buckets, because two would hide the interesting one. "not
    # merge-gating" lumps a post-submit gate (a verdict per landed commit)
    # together with a nightly-only one (a verdict per day, attributed to a
    # batch), and the whole of issue 1040 is about that difference.
    gating = [g for g, e in rows if e & {"pull_request", "merge_group"}]
    post = [g for g, e in rows if not (e & {"pull_request", "merge_group"}) and "push" in e]
    sched = [g for g, e in rows
             if e and not (e & {"pull_request", "merge_group", "push"})]
    none = [g for g, e in rows if not e]
    print(f"\n  {len(rows)} gate(s): {len(gating)} merge-gating, {len(post)} "
          f"post-submit (push) but not gating, {len(sched)} schedule/dispatch "
          f"only, {len(none)} on NO event")
    if post:
        print("  post-submit only: " + " ".join(sorted(post)))
    if sched:
        print("  schedule/dispatch only: " + " ".join(sorted(sched)))
    if none:
        print("  NO event: " + " ".join(sorted(none)))


def main() -> int:
    if not self_test():
        return 1
    fast, build, default = lane_members("fast-serial"), lane_members("build-serial"), default_list()
    default = sorted(lane_gates("default", fast, build, default))
    place = placements(fast, build, default)

    if "--survey" in sys.argv:
        survey(fast, build, default, place)
        return 0

    scope = sorted(set(fast) | set(build) | set(default))
    errs = []

    # R1 — a gate no workflow event runs.
    for gate in scope:
        if place.get(gate) or gate in NO_PLACEMENT_NEEDED:
            continue
        errs.append(
            f"`just check {gate}` runs in NO workflow.\n"
            f"      No event reaches it, so it is invisible between local\n"
            f"      full-tier runs and its reds accumulate until somebody finds\n"
            f"      several at once (issue 1040). It need not gate a merge — a\n"
            f"      nightly or post-submit signal is the bar. Add it to gate.yml's\n"
            f"      `schedule`/`workflow_dispatch` steps, or to post-submit.yml if\n"
            f"      per-commit attribution is worth its cost."
        )

    # R2 — a placement that cannot produce a verdict.
    #
    # PER PLACEMENT, not "every placement of this gate": a gate that is heard
    # elsewhere is still silent HERE, and the whole cost of the 2026-09-05
    # defect was a lane reporting one red for three gates. Weakening this to
    # "all placements shadowed" lets exactly that stand — `api-parity` also
    # reaches an unshadowed `just ci tier1` in host-tests.yml, so the weaker
    # rule found only `no-std` when run against the unfixed gate.yml.
    for gate, where in sorted(place.items()):
        if gate in SHADOWED_PLACEMENT_OK:
            continue
        shadowed = sorted({f"{wf} (behind `{sh}`)" for wf, _ev, sh in where if sh})
        if not shadowed:
            continue
        errs.append(
            f"`just check {gate}` is SEQUENCED BEHIND another\n"
            f"      gate in the same `run:` block: {', '.join(shadowed)}.\n"
            f"      A `run:` block is `bash -e`, so the earlier gate going red\n"
            f"      means this one never executes — and a gate that did not run\n"
            f"      is indistinguishable from one that passed. gate.yml reported\n"
            f"      one red for three gates for five consecutive nights that way\n"
            f"      (issue 1040; issue 0952's withdrawal class, in YAML).\n"
            f"      Give it its own step with the same `if:` — `!cancelled()` is\n"
            f"      what lets a later step run after an earlier failure."
        )

    if errs:
        print(f"check-default-gates-run-somewhere: {len(errs)} gate(s) CI cannot hear\n",
              file=sys.stderr)
        for e in errs:
            print(f"  - {e}\n", file=sys.stderr)
        return 1

    heard = sum(1 for g in scope if place.get(g))
    gating = sum(1 for g in scope
                 for ev in [set().union(*(set(e) for _w, e, _s in place[g]))]
                 if place.get(g) and ev & {"pull_request", "merge_group"})
    print(
        f"check-default-gates-run-somewhere: OK — {len(scope)} gate(s) in "
        f"`just check`'s lanes, all reached by some workflow event ({gating} on a "
        f"merge-gating one, {heard - gating} report-only); no placement anywhere "
        f"is sequenced behind another gate in its `run:` block. "
        f"`--survey` prints gate -> events."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
