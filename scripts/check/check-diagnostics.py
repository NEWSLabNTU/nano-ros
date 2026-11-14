#!/usr/bin/env python3
"""check-diagnostics — the diagnostic codes' views are fresh and no code moves.

RFC-0129 D6. The crate `packages/tooling/nros-diagnostic-codes` is the one
list of diagnostic codes (`NROS-NNNN`); every other view is generated from it.
This gate asks four things:

1. every generated view is fresh: `codes.lock` and the book reference under
   `book/src/reference/diagnostics/` (`nros-diagnostics-gen --check`, which
   also refuses a book page no live code generates);
2. `codes.lock` keeps every code and name it had on `origin/main`: a row may
   be added or retired, never removed, renamed or renumbered (D3);
3. no explanation file under `explain/` lacks a live row (the reverse, a row
   with no explanation, does not compile);
4. every live row names at least one surface (the macro refuses an empty list
   at compile time; the generator reads it again).

Checks 1, 3 and 4 are the generator's `--check`; check 2 is here, because it
reads git. With no `origin/main` to compare against, check 2 is NOT VERIFIED
(recorded in the skip ledger), never a quiet pass. A lock absent from
`origin/main` (the change that adds the crate) has nothing to keep.

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

NAME = "check-diagnostics"
CRATE = "packages/tooling/nros-diagnostic-codes"
LOCK = f"{CRATE}/codes.lock"
EXPLAIN = f"{CRATE}/explain"
BOOK = "book/src/reference/diagnostics"
BASE = "origin/main"


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
            out.append(f"{code} was `{name}` on {BASE} and is `{new[code]}` here; a code "
                       "never changes its name or number")
    by_name = {v: k for k, v in new.items()}
    for code, name in sorted(old.items()):
        if code not in new and name in by_name:
            out.append(f"{name} moved from {code} to {by_name[name]}; numbers are never reused "
                       "or renumbered")
    return out


def gen_check(root: pathlib.Path) -> tuple[int, str]:
    r = subprocess.run(
        ["cargo", "run", "-q", "-p", "nros-diagnostic-codes", "--features", "gen",
         "--bin", "nros-diagnostics-gen", "--", "--check", "--root", str(root)],
        cwd=ROOT, capture_output=True, text=True,
    )
    return r.returncode, (r.stdout + r.stderr).strip()


def base_lock() -> str | None | bool:
    """origin/main's codes.lock text; None when origin/main has no lock yet;
    False when there is no origin/main to read."""
    if subprocess.run(["git", "rev-parse", "--verify", "-q", f"{BASE}^{{commit}}"],
                      cwd=ROOT, capture_output=True).returncode != 0:
        return False
    r = subprocess.run(["git", "show", f"{BASE}:{LOCK}"], cwd=ROOT, capture_output=True, text=True)
    return r.stdout if r.returncode == 0 else None


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
    base = base_lock()
    head = (ROOT / LOCK).read_text()
    if base is False:
        if problems:
            print(f"{NAME}: FAIL\n" + "\n".join(problems), file=sys.stderr)
            return 1
        return check_skip.unverified(NAME, f"no {BASE} to compare codes.lock with; "
                                           "the views are fresh, the lock's history is unchecked")
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
    kept = "no lock on origin/main yet" if base is None else f"keeps {BASE}'s {len(parse_lock(base))}"
    print(f"{NAME}: OK ({rows} code(s), views fresh, {kept})")
    return 0


if __name__ == "__main__":
    os.environ.setdefault("CARGO_TERM_QUIET", "true")
    sys.exit(main())
