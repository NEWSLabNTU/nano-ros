#!/usr/bin/env python3
"""Every `[tool.*].dist.<host>` row declares a FLOOR, or is recorded debt.

WHY THIS EXISTS

`host_key()` is `<os>-<arch>` — no OS version, no libc — so the key offers every
Linux x86_64 host every Linux x86_64 dist, including hosts that cannot run it.
RFC-0099 D5 rejected widening the key (compatibility is a RANGE, not an
identity) and phase-447 D1 put a MEASURED floor on the dist row instead, which
`nros setup` compares the host against BEFORE downloading.

A floor only protects the rows that carry one. `smoke`'s own doc says "absent
means no opinion, not a pass", and the same is true here: a row with no floor is
offered to every host, silently. So this is a RATCHET, not an allowlist:

  * a dist row must carry `floor = { .. }` — measured with
    `scripts/sdk/measure-dist-floor.py`, or `none = "<why>"` when the
    measurement says there is none (a static binary);
  * the only escape is a line in `.config/dist-floor-baseline.txt`, and that
    file may only SHRINK: `BASELINE_CEILING` below is its size, so growing it
    is an edit to this script a reviewer sees, and a row that gains a floor
    must leave the baseline.

Another agent adding dist rows (phase-447 C2) is exactly who this catches.

WHAT IT DOES NOT CHECK

The floor's SHAPE (a `macos` field on a Linux artifact, a non-numeric version)
is refused by `SdkIndex::validate` when `nros` loads the index; this gate
repeats only the part that decides whether a row counts as covered.
Whether the NUMBER is right is the measuring script's job: re-run it when an
artifact is re-cut.

Usage:  check-dist-floors.py
"""

import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
INDEX = os.path.join(ROOT, "nros-sdk-index.toml")
BASELINE = os.path.join(ROOT, ".config", "dist-floor-baseline.txt")

# The number of rows `.config/dist-floor-baseline.txt` may hold. It may only go
# DOWN: lower it in the same change that removes a row.
BASELINE_CEILING = 0

LINUX_FIELDS = ("glibc", "glibcxx")
MAC_FIELDS = ("macos",)


def load_index(path=INDEX):
    try:
        import tomllib as toml
    except ModuleNotFoundError:
        import tomli as toml
    with open(path, "rb") as fh:
        return toml.load(fh)


def load_baseline(path=BASELINE):
    rows = []
    try:
        with open(path) as fh:
            for line in fh:
                line = line.split("#", 1)[0].strip()
                if line:
                    rows.append(tuple(line.split()[:2]))
    except FileNotFoundError:
        pass
    return rows


def covered(host, floor):
    """Does `floor` say something checkable about a `host` artifact?"""
    if not isinstance(floor, dict):
        return False
    if str(floor.get("none", "")).strip():
        return True
    fields = MAC_FIELDS if host.startswith("macos-") else LINUX_FIELDS
    return any(floor.get(f) for f in fields)


def audit(index, baseline, ceiling):
    """Problems, as printable lines. Empty means the gate holds."""
    rows = {}
    for name, tool in sorted((index.get("tool") or {}).items()):
        for host, dist in sorted((tool.get("dist") or {}).items()):
            rows[(name, host)] = covered(host, dist.get("floor"))
    problems = []
    listed = set(baseline)
    for (name, host), ok in rows.items():
        if not ok and (name, host) not in listed:
            problems.append(
                f"[tool.{name}] dist.{host} has no floor. Measure it:\n"
                f"      python3 scripts/sdk/measure-dist-floor.py {name} --host {host}\n"
                f"    and write the `floor = {{ .. }}` it prints into the dist row "
                f"(`none = \"<why>\"` if it measures none)."
            )
    for row in baseline:
        if row not in rows:
            problems.append(
                f"baseline row {' '.join(row)} names no dist row — delete it "
                f"(and lower BASELINE_CEILING)."
            )
        elif rows[row]:
            problems.append(
                f"baseline row {' '.join(row)} now HAS a floor — delete it from "
                f"the baseline and lower BASELINE_CEILING. The debt only shrinks."
            )
    if len(baseline) > ceiling:
        problems.append(
            f"the baseline holds {len(baseline)} row(s) against a ceiling of "
            f"{ceiling}. It may only SHRINK: a new dist declares its floor instead."
        )
    if len(set(baseline)) != len(baseline):
        problems.append("the baseline lists a row twice.")
    return problems, len(rows)


def self_test():
    """The gate must be able to go red — synthetic BAD input, every run."""
    good = {"glibc": "2.35"}
    idx = {
        "tool": {
            "a": {"dist": {"linux-x86_64": {"floor": good}, "macos-arm64": {"floor": {"macos": "11.0"}}}},
            "s": {"dist": {"linux-x86_64": {"floor": {"none": "static musl"}}}},
        }
    }
    assert audit(idx, [], 0)[0] == [], audit(idx, [], 0)
    # A row with no floor, not in the baseline: red.
    bad = {"tool": {"b": {"dist": {"linux-x86_64": {}}}}}
    assert any("no floor" in p for p in audit(bad, [], 0)[0])
    # ...unless it is recorded debt under the ceiling.
    assert audit(bad, [("b", "linux-x86_64")], 1)[0] == []
    # A floor that cannot apply to its host does not count as covered.
    wrong = {"tool": {"c": {"dist": {"linux-x86_64": {"floor": {"macos": "11.0"}}}}}}
    assert any("no floor" in p for p in audit(wrong, [], 0)[0])
    # An empty `none` is not a reason.
    blank = {"tool": {"c": {"dist": {"linux-x86_64": {"floor": {"none": "  "}}}}}}
    assert any("no floor" in p for p in audit(blank, [], 0)[0])
    # The baseline may only shrink: over the ceiling is red.
    assert any("ceiling" in p for p in audit(bad, [("b", "linux-x86_64")], 0)[0])
    # A baseline row whose dist gained a floor must leave.
    assert any("now HAS a floor" in p for p in audit(idx, [("a", "linux-x86_64")], 1)[0])
    # A baseline row naming nothing is stale.
    assert any("names no dist row" in p for p in audit(idx, [("zz", "linux-x86_64")], 1)[0])


def main():
    self_test()
    problems, n = audit(load_index(), load_baseline(), BASELINE_CEILING)
    if problems:
        print("check-dist-floors: %d problem(s):\n" % len(problems), file=sys.stderr)
        for p in problems:
            print("  " + p, file=sys.stderr)
        print(
            "\n  A dist with no floor is offered to every host with the same "
            "<os>-<arch>,\n  including one that cannot run it (RFC-0099 D5, "
            "phase-447 D1).",
            file=sys.stderr,
        )
        return 1
    debt = len(load_baseline())
    print(
        f"check-dist-floors OK — {n} dist row(s); every one declares a floor "
        f"({debt} recorded as debt, ceiling {BASELINE_CEILING})."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
