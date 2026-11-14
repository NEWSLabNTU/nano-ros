#!/usr/bin/env python3
"""check-diagnostics — the diagnostic codes' views are fresh and no code moves.

RFC-0129 D6. The crate `packages/tooling/nros-diagnostic-codes` is the one
list of diagnostic codes (`NROS-NNNN`); every other view is generated from it.
This gate asks four things:

1. every generated view is fresh: `codes.lock` and the book reference under
   `book/src/reference/diagnostics/` (`nros-diagnostics-gen --check`, which
   also refuses a book page no live code generates);
2. `codes.lock` keeps every code and name it had at the baseline: a row may
   be added or retired, never removed, renamed or renumbered (D3). The
   baseline is `check-submodule-pins`' (`$NROS_SUBMODULE_PIN_BASELINE`, else
   `scripts/ci/pin-baseline.sh resolve`: locally the merge-base with
   origin/main), read through the bounded history helper;
3. no explanation file under `explain/` lacks a live row (the reverse, a row
   with no explanation, does not compile);
4. every live row names at least one surface (the macro refuses an empty list
   at compile time; the generator reads it again).

Checks 1, 3 and 4 are the generator's `--check`; check 2 is here, because it
reads git. With no baseline to compare against, check 2 is NOT VERIFIED
(recorded in the skip ledger; a failure in CI, where the baseline is always
provided), never a quiet pass. A baseline with no lock (the change that adds
the crate) has nothing to keep.

Self-test (`--self-test`, and on every run): one planted defect per check, each
of which must turn the gate red, in a scratch root outside the checkout.

Run:  python3 scripts/check/check-diagnostics.py [--self-test]
Gate: just check diagnostics
"""

from __future__ import annotations

import os
import pathlib
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import check_skip  # noqa: E402
from git_history import HISTORY_TIMEOUT_S, HistoryUnknown, history_git  # noqa: E402

NAME = "check-diagnostics"
CRATE = "packages/tooling/nros-diagnostic-codes"
LOCK = f"{CRATE}/codes.lock"
EXPLAIN = f"{CRATE}/explain"
BOOK = "book/src/reference/diagnostics"


def parse_lock(text: str) -> dict[str, str]:
    """`codes.lock` as {code: name}. Comment and blank lines are skipped."""
    rows: dict[str, str] = {}
    for n, line in enumerate(text.splitlines(), 1):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        f = line.split()
        ok = (len(f) == 3 and f[2] == "live") or (len(f) == 4 and f[2] == "retired")
        if not ok or not f[0].startswith("NROS-"):
            raise ValueError(f"codes.lock:{n}: cannot read `{line}`")
        rows[f[0]] = f[1]
    return rows


def lock_kept(base: str, head: str) -> list[str]:
    """Each base row still present in head with the same number and name."""
    old, new = parse_lock(base), parse_lock(head)
    out = []
    for code, name in sorted(old.items()):
        if code not in new:
            out.append(f"{code} {name} was removed from codes.lock; retire it instead "
                       f"(`NNNN Retired({name}) => successor,`)")
        elif new[code] != name:
            out.append(f"{code} was `{name}` at the baseline and is `{new[code]}` here; a code "
                       "never changes its name or number")
    by_name = {v: k for k, v in new.items()}
    for code, name in sorted(old.items()):
        if code not in new and name in by_name:
            out.append(f"{name} moved from {code} to {by_name[name]}; numbers are never reused "
                       "or renumbered")
    return out


def gen_check(root: pathlib.Path) -> tuple[int, str]:
    r = subprocess.run(
        ["cargo", "run", "-q", "-p", "nros-diagnostic-codes", "--features", "std",
         "--bin", "nros-diagnostics-gen", "--", "--check", "--root", str(root)],
        cwd=ROOT, capture_output=True, text=True,
    )
    return r.returncode, (r.stdout + r.stderr).strip()


def resolve_baseline() -> str | None:
    """The baseline commit `check-submodule-pins` uses, by the same channel:
    `$NROS_SUBMODULE_PIN_BASELINE` (CI: `scripts/ci/pin-baseline.sh provide`),
    else `pin-baseline.sh resolve` (locally, the merge-base with origin/main)."""
    env = os.environ.get("NROS_SUBMODULE_PIN_BASELINE", "").strip()
    if env:
        return env
    try:
        r = subprocess.run(["bash", str(ROOT / "scripts/ci/pin-baseline.sh"), "resolve", "HEAD"],
                           cwd=ROOT, capture_output=True, text=True,
                           stdin=subprocess.DEVNULL, timeout=HISTORY_TIMEOUT_S)
    except subprocess.TimeoutExpired:
        return None
    out = r.stdout.strip()
    return out if r.returncode == 0 and out else None


def base_lock(base: str) -> tuple[str | None, str | None]:
    """(the lock at `base` or None when it has none yet, None), or
    (None, why it cannot be read here)."""
    # One path at one commit: two or three trees and a blob, so a LAZY fetch
    # is allowed and still bounded (the history helper, issue 2442).
    try:
        if history_git(ROOT, "rev-parse", "--verify", "--quiet", f"{base}^{{commit}}",
                       check=False).returncode != 0:
            return None, (f"the baseline {base} is not in this clone "
                          "(CI: `scripts/ci/pin-baseline.sh provide` fetches it)")
        r = history_git(ROOT, "rev-parse", "--verify", "--quiet", f"{base}:{LOCK}",
                        check=False, fetch=True)
        if r.returncode != 0:
            return None, None  # the baseline predates the crate: nothing to keep
        return history_git(ROOT, "show", f"{base}:{LOCK}", fetch=True).stdout, None
    except HistoryUnknown as e:
        return None, f"reading {LOCK} at {base}: {e}"


def self_test() -> None:
    lock = "# c\nNROS-0001 A live\nNROS-0002 B retired NROS-0001\n"
    assert lock_kept(lock, lock) == [], "an unchanged lock passes"
    assert lock_kept(lock, lock + "NROS-0003 C live\n") == [], "an added row passes"
    retired = lock.replace("NROS-0001 A live", "NROS-0001 A retired NROS-0003") + "NROS-0003 C live\n"
    assert lock_kept(lock, retired) == [], "a retired row passes"
    got = lock_kept(lock, "NROS-0002 B retired NROS-0001\n")
    assert len(got) == 1 and "removed" in got[0], f"self-test: a removed row: {got}"
    got = lock_kept(lock, "NROS-0001 Z live\nNROS-0002 B retired NROS-0001\n")
    assert len(got) == 1 and "never changes" in got[0], f"self-test: a renamed row: {got}"
    got = lock_kept(lock, "NROS-0002 B retired NROS-0001\nNROS-0005 A live\n")
    assert any("moved from NROS-0001 to NROS-0005" in g for g in got), f"self-test: renumbered: {got}"
    try:
        parse_lock("NROS-0001 A\n")
        raise AssertionError("self-test: a malformed row parses")
    except ValueError:
        pass

    # The generator's checks, each against one planted defect in a scratch
    # root: the real tree's views, copied out, then broken one way at a time.
    with tempfile.TemporaryDirectory(prefix="check-diagnostics-") as tmp:
        base = pathlib.Path(tmp) / "clean"
        for rel in (LOCK, EXPLAIN, BOOK):
            src, dst = ROOT / rel, base / rel
            dst.parent.mkdir(parents=True, exist_ok=True)
            (shutil.copytree if src.is_dir() else shutil.copy2)(src, dst)
        rc, out = gen_check(base)
        assert rc == 0, f"self-test: the copied tree must be clean: {out}"

        def planted(name: str, plant) -> None:
            root = pathlib.Path(tmp) / name
            shutil.copytree(base, root)
            plant(root)
            rc, out = gen_check(root)
            assert rc != 0, f"self-test: planted `{name}` passed: {out}"

        planted("stale-view", lambda r: (r / BOOK / "NROS-0001.md").write_text("edited by hand\n"))
        planted("missing-view", lambda r: (r / LOCK).unlink())
        planted("orphan-page", lambda r: (r / BOOK / "NROS-9999.md").write_text("# stray\n"))
        planted("orphan-explanation", lambda r: (r / EXPLAIN / "NROS-9999.md").write_text("stray\n"))
    # Check 4 cannot be planted without compiling a crate: a live row with
    # `surfaces: []` is refused by `diagnostics!` itself (its `$(..),+`).


def main() -> int:
    self_test()
    if "--self-test" in sys.argv:
        print(f"{NAME}: self-test OK")
        return 0
    problems: list[str] = []
    rc, out = gen_check(ROOT)
    if rc != 0:
        problems.append(out)
    head = (ROOT / LOCK).read_text()
    commit = resolve_baseline()
    if commit is None:
        base, unverified = None, "no baseline commit resolves (pin-baseline.sh resolve)"
    else:
        base, unverified = base_lock(commit)
    if unverified:
        if problems:
            print(f"{NAME}: FAIL\n" + "\n".join(problems), file=sys.stderr)
            return 1
        msg = (f"lock history NOT VERIFIED: {unverified}; the views are fresh, and a code "
               "removed or renumbered against the baseline cannot be seen")
        if os.environ.get("GITHUB_ACTIONS") == "true":
            # Every CI job that runs a gate lane provides the baseline first.
            print(f"{NAME}: FAIL: {msg}. In CI the baseline is provided by "
                  "`bash scripts/ci/pin-baseline.sh provide`.", file=sys.stderr)
            return 1
        return check_skip.unverified(NAME, msg)
    if base is not None:
        try:
            problems += lock_kept(base, head)
        except ValueError as e:
            problems.append(str(e))
    if problems:
        print(f"{NAME}: FAIL", file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        return 1
    rows = len(parse_lock(head))
    kept = ("the baseline has no lock yet" if base is None
            else f"keeps the baseline's {len(parse_lock(base))}")
    print(f"{NAME}: OK ({rows} code(s), views fresh, {kept})")
    return 0


if __name__ == "__main__":
    os.environ.setdefault("CARGO_TERM_QUIET", "true")
    sys.exit(main())
