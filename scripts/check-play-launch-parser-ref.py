#!/usr/bin/env python3
"""`[tool.play_launch_parser].source.ref` vs the `play_launch` gitlink — issue 1413.

WHY THIS GATE EXISTS

One component, two pins. The index installs the BINARY from
`[tool.play_launch_parser].source.ref`; `nros-launch-resolve` path-deps the
LIBRARY out of `packages/cli/third-party/play_launch`, whose gitlink is a
different value. Three documents asserted the two "cannot disagree" — the index
beside the entry, `nano-ros-sdk`'s `build-play_launch_parser.sh`, and
`just/workspace.just`'s header — and nothing measured any of them, so they
drifted ~40 commits apart and stayed there. That is the shape of issues 0609
(zenoh router) and 0507 (cyclonedds): prose guarding an unchecked invariant.

WHAT IT ASSERTS, AND WHY NOT EQUALITY

Equality is the invariant the prose claimed, and it is NOT what this checks.
The two are legitimately unequal today: issue 0897 W2b/W3 moved pyo3 out of the
`play_launch_parser` crate, so at the gitlink the standalone CLI has no Python
backend at all — `.launch.py` hard-errors and, worse, `$(eval …)` exits 0 with
the substitution UNEXPANDED. An equality gate would have been red the day it
landed and would have to be disabled by the first person to bump either side,
which is the one thing a gate must never be.

So it asserts the three things that ARE true and useful:

  1. `upstream` and `source.ref` name the SAME commit. They are two spellings of
     one fact (the build workflow reads `upstream`, `nros setup` reads
     `source.ref`) and nothing else kept them together.
  2. `source.ref` is an ANCESTOR-OR-EQUAL of the recorded gitlink. The index may
     LAG the submodule; it may never LEAD it, and it may never sit on a commit
     that is not on the submodule's line at all.
  3. While they differ, the index must carry a `# ref-lag:` line with a reason.
     When they become equal that line must be DELETED. Two-way, so the
     declaration cannot rot green in either direction.

THREE OUTCOMES, NOT TWO (issue 1043's rule)

Ancestry needs the submodule's object store, and no lane checks out every
submodule. Absent it the verdict is NOT VERIFIED — reported, never a silent
pass and never a false failure. `NROS_PLAY_LAUNCH_REF_STRICT=1` makes that a
failure for a lane that really does provide the submodule.

Usage::

    check-play-launch-parser-ref.py              # the gate
    check-play-launch-parser-ref.py --selftest   # negative controls alone
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib.git_history import interpret_ancestry  # noqa: E402,F401

try:
    import tomllib  # Python 3.11+
except ModuleNotFoundError:  # 3.10 backport, same spelling as the sibling gates
    import tomli as tomllib

ROOT = Path(__file__).resolve().parents[1]
INDEX = ROOT / "nros-sdk-index.toml"
SUBMODULE = "packages/cli/third-party/play_launch"
TOOL = "play_launch_parser"

# The declaration a lagging `ref` owes its reader. Anchored to a comment line so
# it cannot be satisfied by the word appearing inside prose.
REF_LAG = re.compile(r"^#\s*ref-lag:\s*(\S.*)$", re.MULTILINE)


def read_index(text: str) -> tuple[str | None, str | None]:
    """(`upstream`, `source.ref`) for the tool, or (None, None)."""
    data = tomllib.loads(text)
    tool = data.get("tool", {}).get(TOOL)
    if tool is None:
        return None, None
    return tool.get("upstream"), (tool.get("source") or {}).get("ref")


def ref_lag_reason(text: str) -> str | None:
    """The `# ref-lag:` reason, or None. Empty/whitespace does not count."""
    m = REF_LAG.search(text)
    if m is None:
        return None
    reason = m.group(1).strip()
    return reason or None


def verdict(upstream, ref, gitlink, reason, ancestry):
    """The whole decision, as data. `ancestry` is True / False / None (unknown).

    Returns a list of problem strings — empty means OK. Pure, so the selftest
    exercises the RULE rather than a git fixture.
    """
    problems = []
    if not ref:
        return [f"[tool.{TOOL}].source.ref is missing — nothing to compare."]
    if not upstream:
        problems.append(f"[tool.{TOOL}].upstream is missing; source.ref = {ref}.")
    elif upstream != ref:
        problems.append(
            f"[tool.{TOOL}] names two commits: upstream = {upstream}, "
            f"source.ref = {ref}. The build workflow reads `upstream` and "
            f"`nros setup` reads `source.ref` — they are one fact."
        )
    if not gitlink:
        return problems

    equal = ref == gitlink
    if equal:
        if reason is not None:
            problems.append(
                f"source.ref now EQUALS the {SUBMODULE} gitlink ({gitlink[:8]}), "
                f"so the `# ref-lag:` line beside [tool.{TOOL}] is stale — delete "
                f"it. (It said: {reason})"
            )
    else:
        if ancestry is False:
            problems.append(
                f"source.ref {ref[:8]} is NOT an ancestor of the {SUBMODULE} "
                f"gitlink {gitlink[:8]}. The index may LAG the submodule; it may "
                f"never lead it, and it may never sit off the submodule's line. "
                f"Move source.ref onto that line, or move the gitlink forward."
            )
        if reason is None:
            problems.append(
                f"source.ref {ref[:8]} differs from the {SUBMODULE} gitlink "
                f"{gitlink[:8]} and nothing says why. Add a `# ref-lag: <reason>` "
                f"comment line beside [tool.{TOOL}] in nros-sdk-index.toml — a "
                f"lag nobody declared is how issue 1413 happened."
            )
    return problems


def _git(args, cwd=None):
    try:
        out = subprocess.run(
            ["git", *args],
            cwd=cwd or ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
    except OSError:
        return None
    return out.stdout.strip() if out.returncode == 0 else None


def recorded_gitlink() -> str | None:
    """The COMMIT the superproject records for the submodule — not the checkout."""
    line = _git(["ls-tree", "HEAD", SUBMODULE])
    if not line:
        return None
    parts = line.split()
    # `<mode> commit <sha>\t<path>`; anything else is not a gitlink.
    return parts[2] if len(parts) >= 3 and parts[1] == "commit" else None


def _stores():
    """Places the submodule's objects might live, in order of preference.

    NEVER models git's layout (issue 1336): the module store is asked for by
    name, not spelled as `.git/modules/<x>`.
    """
    yield ROOT / SUBMODULE
    name = _git(["rev-parse", "--path-format=absolute", "--git-path", f"modules/{SUBMODULE}"])
    if name:
        yield Path(name)


# `interpret_ancestry` is imported from `scripts/lib/git_history.py`, which is
# where this rule now lives for the whole repository.
#
# It was authored HERE, and the reasoning is worth keeping beside its
# measurement: a SHALLOW clone's graft cuts history, so `merge-base
# --is-ancestor` can report a pair as unrelated when the full history relates
# them — the `--depth 1`-initialised submodule said `838ce948` is not an
# ancestor of `07f0461e`, and the full clone says it is. Truncation can only
# manufacture a FALSE negative, never a false positive, so a `True` from a
# shallow store still counts and a `False` becomes "cannot tell".
#
# It moved because a second gate wrote the rule again and got half of it
# (issue 1476): `check-roadmap-commit-refs` guarded the absent-object negative
# and not the cut-history one, and stopped tier 2 with three verdicts that
# were the opposite of the truth. One spelling, one place.


def _has(store: Path, commit: str) -> bool:
    return (
        subprocess.run(
            ["git", "cat-file", "-e", f"{commit}^{{commit}}"],
            cwd=store,
            capture_output=True,
        ).returncode
        == 0
    )


def ancestry_of(ref: str, gitlink: str):
    """True / False / None — None means no store here could answer."""
    for store in _stores():
        if not store.exists():
            continue
        if not (_has(store, ref) and _has(store, gitlink)):
            continue
        rc = subprocess.run(
            ["git", "merge-base", "--is-ancestor", ref, gitlink],
            cwd=store,
            capture_output=True,
        ).returncode
        if rc not in (0, 1):
            continue
        shallow = _git(["rev-parse", "--is-shallow-repository"], cwd=store) == "true"
        answer = interpret_ancestry(rc == 0, shallow)
        if answer is not None:
            return answer
    return None


CASES = [
    # (name, upstream, ref, gitlink, reason, ancestry, expect_problem_substring)
    ("equal and undeclared is OK", "a" * 40, "a" * 40, "a" * 40, None, True, None),
    ("equal but still declared", "a" * 40, "a" * 40, "a" * 40, "why", True, "stale"),
    ("lag, declared, on the line", "a" * 40, "a" * 40, "b" * 40, "why", True, None),
    ("lag, undeclared", "a" * 40, "a" * 40, "b" * 40, None, True, "nothing says why"),
    ("off the line", "a" * 40, "a" * 40, "b" * 40, "why", False, "NOT an ancestor"),
    ("two spellings disagree", "c" * 40, "a" * 40, "a" * 40, None, True, "two commits"),
    ("unknown ancestry never fails", "a" * 40, "a" * 40, "b" * 40, "why", None, None),
    ("empty reason is no reason", "a" * 40, "a" * 40, "b" * 40, None, True, "nothing says why"),
]


def selftest() -> int:
    """Negative controls on the RULE. No `git init` anywhere — this gate builds
    no repository, so it cannot have the issue-0986 side-effect class."""
    bad = 0
    for name, up, ref, link, reason, anc, want in CASES:
        got = verdict(up, ref, link, reason, anc)
        joined = " | ".join(got)
        ok = (want is None and not got) or (want is not None and want in joined)
        if not ok:
            print(f"check-play-launch-parser-ref selftest: FAIL {name} -> {joined!r}", file=sys.stderr)
            bad += 1
    # The `# ref-lag:` parser, on the two shapes that matter.
    if ref_lag_reason("# ref-lag: issue 1413 — because\n") != "issue 1413 — because":
        print("check-play-launch-parser-ref selftest: FAIL ref-lag parse", file=sys.stderr)
        bad += 1
    if ref_lag_reason("# ref-lag:   \n") is not None:
        print("check-play-launch-parser-ref selftest: FAIL blank reason accepted", file=sys.stderr)
        bad += 1
    if ref_lag_reason("a line mentioning ref-lag: in prose\n") is not None:
        print("check-play-launch-parser-ref selftest: FAIL prose matched", file=sys.stderr)
        bad += 1
    # The shallow-store asymmetry. A truncated history can only manufacture a
    # false NEGATIVE, so only the negative is downgraded.
    for is_anc, shallow, want in (
        (True, False, True),
        (True, True, True),
        (False, False, False),
        (False, True, None),
    ):
        if interpret_ancestry(is_anc, shallow) is not want:
            print(
                f"check-play-launch-parser-ref selftest: FAIL shallow rule "
                f"({is_anc}, {shallow})",
                file=sys.stderr,
            )
            bad += 1
    if bad:
        return 1
    print(f"check-play-launch-parser-ref selftest: OK ({len(CASES) + 7} cases)")
    return 0


def main() -> int:
    if "--selftest" in sys.argv:
        return selftest()
    if selftest():
        return 1
    if not INDEX.is_file():
        sys.exit(f"check-play-launch-parser-ref: index missing at {INDEX}")
    text = INDEX.read_text(encoding="utf-8")
    try:
        upstream, ref = read_index(text)
    except Exception as e:  # noqa: BLE001 — report, never conclude
        sys.exit(f"check-play-launch-parser-ref: {INDEX} did not parse: {e}")
    if ref is None and upstream is None:
        sys.exit(
            f"check-play-launch-parser-ref: [tool.{TOOL}] is not in the index. "
            "If the tool was retired, retire this gate with it."
        )
    gitlink = recorded_gitlink()
    if gitlink is None:
        sys.exit(
            f"check-play-launch-parser-ref: no gitlink recorded for {SUBMODULE} "
            "at HEAD — that is a repository-shape bug, not an empty result."
        )
    ancestry = ancestry_of(ref, gitlink) if ref and ref != gitlink else True
    problems = verdict(upstream, ref, gitlink, ref_lag_reason(text), ancestry)

    if problems:
        print(f"check-play-launch-parser-ref: {len(problems)} problem(s):\n")
        for p in problems:
            print(f"  - {p}\n")
        print("  Background: docs/issues/1413-play-launch-parser-ref-lags-submodule-pin.md")
        return 1

    if ancestry is None:
        strict = os.environ.get("NROS_PLAY_LAUNCH_REF_STRICT") == "1"
        msg = (
            f"check-play-launch-parser-ref: NOT VERIFIED — no object store here could "
            f"relate {ref[:8]} and {gitlink[:8]}. Either {SUBMODULE} is not checked out, "
            f"or its clone is SHALLOW, whose graft reports an unrelated pair for a "
            f"related one (measured: a `--depth 1` init said exactly that here). "
            f"`git -C {SUBMODULE} fetch --unshallow origin` to measure it. The two "
            f"spellings agree and the lag is declared; only ancestry is unmeasured."
        )
        if strict:
            print(msg.replace("NOT VERIFIED", "FAILED (strict)"), file=sys.stderr)
            return 1
        print(msg)
        return 0

    state = "equal" if ref == gitlink else f"lags by a declared reason"
    print(
        f"check-play-launch-parser-ref OK — [tool.{TOOL}] source.ref {ref[:8]} "
        f"{state} vs the {SUBMODULE} gitlink {gitlink[:8]}; both spellings agree."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
