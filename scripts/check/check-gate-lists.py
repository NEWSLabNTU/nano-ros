#!/usr/bin/env python3
"""A gate registry holds ONE name per line, sorted.

`just/check.just` names every gate twice: once as a recipe, and once as a
dependency of the lane that runs it. That dependency list is the shape issues
0883/0884 are about — a file EVERY pull request appends to — and it had the
worse variant of it: up to fifteen names packed onto a line, with new gates
appended at the tail. Two agents adding unrelated gates edited the same
physical line, so git conflicted on changes that do not overlap in meaning, and
the merge queue ejected them.

0884 fixed the same shape for `docs/issues/open.md` with `merge=union`, and that
IS working (measured: it conflicts in none of the open PRs). Union is not
available here — `.gitattributes` says so in as many words, "NEVER apply this to
an authored file" — because a union of two recipe bodies is not a recipe, it is
both of them.

One name per line, sorted, is the fix that does not need a merge driver: two
gates with different names land at different, usually non-adjacent, lines, and
git merges them without help. It costs nothing, because ORDER IN THIS LIST HAS
NEVER BEEN LOAD-BEARING: `fast` runs the same gates concurrently, and
`run-gates-parallel.sh` `sort -u`s the list it derives from this very block. A
list whose consumer sorts it cannot have been ordered on purpose.

AND THE REGISTRY MAY NOT SHRINK (issue 1071)
--------------------------------------------
Sorted-and-one-per-line are properties a DELETION satisfies perfectly. PR #431
added one gate and removed four unrelated ones -- recipes and registry entries
both, against a stale copy of this file -- and every gate stayed green, this one
included: it printed `232 gate(s)` where main had 236, and compared that number
to nothing. One of the four was the guard for a defect that had survived from
2026-06-13 to 2026-09-03.

So the registry now carries a BASELINE name set that may only grow. A name in
the baseline and not in the registries is a failure; new names are free.

On the NAME SET and not the count, deliberately: #431 was one addition against
four removals, so a count ratchet would have had to notice a net of -3, and a
delete-plus-add that nets to zero would pass it outright. The set catches both,
and costs a longer file.

A FLAT set across registries rather than per-registry, so moving a gate between
`fast` and `build` -- a real and legitimate change -- is not a deletion. What it
asserts is only that a gate that once existed still exists somewhere.

Retiring a gate is rare and always deliberate:

    python3 scripts/check/check-gate-lists.py --write-baseline

and say in the commit message which gate went and why. Re-stating the number is
the right price for a deletion nobody meant to make.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
JUSTFILE = REPO / "just" / "check.just"
# Beside `.config/gate-selftest-baseline.txt`, the same shape one question over:
# that one ratchets how many gates TEST THEMSELVES, this one how many EXIST.
# Deleting a gate that has a selftest makes that ratchet EASIER to satisfy,
# which is why it could not have caught #431.
BASELINE = REPO / ".config" / "gate-registry-baseline.txt"

# The lane recipes whose dependencies are a gate REGISTRY. A lane with a
# handful of names on one line (`default: cli-fresh fast build api-parity`) is
# not one: nobody appends to it, so it is not a conflict site.
# `build` gained a parallel runner (issue 0993), so its LIST moved to
# `build-serial` exactly as `fast`'s did — the registry is the dependency line,
# not the verb.
REGISTRIES = ("fast-serial", "build-serial")


def dep_block(lines: list[str], name: str) -> tuple[int, int]:
    """(start, end) of `name:`'s dependency lines, end EXCLUSIVE.

    A recipe's dependencies run to the first line that does not end in a
    backslash — that last line is a dependency too, not the body.
    """
    for i, line in enumerate(lines):
        if line.startswith(f"{name}:"):
            j = i
            while lines[j].rstrip().endswith("\\"):
                j += 1
                if j >= len(lines):
                    raise SystemExit(f"{name}: unterminated continuation")
            return i, j + 1
    raise SystemExit(f"check-gate-lists: no recipe `{name}` in {JUSTFILE}")


def names_per_line(lines: list[str], s: int, e: int) -> list[list[str]]:
    """The dependency names, grouped by the line they sit on."""
    out = []
    for k in range(s, e):
        text = lines[k].strip().rstrip("\\").strip()
        if k == s:
            text = text.split(":", 1)[1]
        out.append([n for n in text.split() if n])
    return out


# The no-op every registry ends on, so the LAST real gate carries a trailing
# backslash like every other. Without it the final entry is the one line with
# different syntax, and a gate that sorts last must edit someone else's line to
# add the backslash — two such PRs conflict with CERTAINTY rather than by bad
# luck. The "trailing comma" fix, and the one conflict class the
# one-name-per-line format could not reach.
SENTINEL = "_gate-list-end"


def check(lines: list[str], name: str) -> list[str]:
    s, e = dep_block(lines, name)
    per_line = names_per_line(lines, s, e)
    flat = [n for group in per_line for n in group]
    problems = []

    # The sentinel is REQUIRED and must be last. Checked before it is dropped:
    # if it drifts into the middle it stops terminating anything, and the last
    # real gate silently becomes the odd line again.
    if flat and flat[-1] == SENTINEL:
        flat = flat[:-1]
        per_line = per_line[:-1]
    elif SENTINEL in flat:
        problems.append(
            f"  {name}: `{SENTINEL}` is present but not LAST.\n"
            f"      It exists to give the final real gate a trailing backslash;\n"
            f"      anywhere else it terminates nothing."
        )
        flat = [n for n in flat if n != SENTINEL]
        per_line = [[n for n in g if n != SENTINEL] for g in per_line]
        per_line = [g for g in per_line if g]
    else:
        problems.append(
            f"  {name}: missing the `{SENTINEL}` terminator.\n"
            f"      End the list with it so every real gate's line looks the\n"
            f"      same; otherwise a gate that sorts last has to edit the\n"
            f"      previous line too, and two such PRs always conflict."
        )

    crowded = sum(1 for group in per_line if len(group) > 1)
    if crowded:
        problems.append(
            f"  {name}: {crowded} line(s) carry more than one gate.\n"
            f"      Two agents adding unrelated gates then edit the SAME line and\n"
            f"      git conflicts on changes that do not overlap in meaning."
        )

    if flat != sorted(flat):
        first = next(
            (a for a, b in zip(flat, sorted(flat)) if a != b), "?"
        )
        problems.append(
            f"  {name}: not sorted (first out of order: `{first}`).\n"
            f"      Sorted order is what makes a new gate's line position\n"
            f"      predictable, and so unlikely to abut someone else's."
        )

    dupes = sorted({n for n in flat if flat.count(n) > 1})
    if dupes:
        problems.append(f"  {name}: listed twice: {', '.join(dupes)}")

    return problems


def registry_names(lines: list[str]) -> set[str]:
    """Every gate named by any registry, sentinel excluded."""
    out: set[str] = set()
    for name in REGISTRIES:
        s, e = dep_block(lines, name)
        for group in names_per_line(lines, s, e):
            out.update(n for n in group if n != SENTINEL)
    return out


def load_baseline() -> set[str] | None:
    if not BASELINE.exists():
        return None
    return {
        line.strip()
        for line in BASELINE.read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.startswith("#")
    }


def write_baseline(names: set[str]) -> None:
    BASELINE.parent.mkdir(parents=True, exist_ok=True)
    BASELINE.write_text(
        "# The gate names `just/check.just`'s registries have carried.\n"
        "# May only GROW. A name here and not in a registry fails\n"
        "# `check-gate-lists` (issue 1071 -- a PR deleted four gates and every\n"
        "# gate stayed green, because sorted-and-one-per-line is a property a\n"
        "# deletion satisfies).\n"
        "#\n"
        "# Regenerate ONLY for a deliberate retirement, and say which gate went:\n"
        "#   python3 scripts/check/check-gate-lists.py --write-baseline\n"
        + "".join(f"{n}\n" for n in sorted(names)),
        encoding="utf-8",
    )


def ratchet(present: set[str], baseline: set[str] | None) -> list[str]:
    """Names the baseline knows and the registries no longer name."""
    if baseline is None:
        return [
            f"  the baseline is missing at {BASELINE.relative_to(REPO)}.\n"
            f"      Without it a deleted gate is invisible again. Create it with\n"
            f"      `python3 scripts/check/check-gate-lists.py --write-baseline`."
        ]
    gone = sorted(baseline - present)
    if not gone:
        return []
    return [
        "  {} gate(s) in the baseline are no longer in any registry:\n{}".format(
            len(gone), "".join(f"      {n}\n" for n in gone)
        ).rstrip()
    ]


def self_test() -> None:
    """Both directions, on synthetic input.

    `check-gate-selftests` requires this on the normal path: a control nobody
    runs decays into a comment.
    """
    good = ["build: \\", "    alpha \\", "    beta \\", "    gamma \\",
            f"    {SENTINEL}", "    @echo hi"]
    assert check(good, "build") == [], "selftest: rejected a well-formed list"

    crowded = ["build: \\", "    alpha beta \\", "    gamma \\",
               f"    {SENTINEL}", "    @echo hi"]
    assert any("more than one gate" in p for p in check(crowded, "build")), (
        "selftest: missed two names sharing a line"
    )

    unsorted = ["build: \\", "    gamma \\", "    alpha \\",
                f"    {SENTINEL}", "    @echo hi"]
    assert any("not sorted" in p for p in check(unsorted, "build")), (
        "selftest: missed an out-of-order list"
    )

    dup = ["build: \\", "    alpha \\", "    alpha \\",
           f"    {SENTINEL}", "    @echo hi"]
    assert any("listed twice" in p for p in check(dup, "build")), (
        "selftest: missed a duplicate"
    )

    # The sentinel itself. Absent, the last real gate is the odd line again.
    no_sentinel = ["build: \\", "    alpha \\", "    beta", "    @echo hi"]
    assert any("missing the" in p for p in check(no_sentinel, "build")), (
        "selftest: missed an absent terminator"
    )

    # Present but not last terminates nothing — and would ALSO read as
    # out-of-order, so the message has to name the real fault.
    misplaced = ["build: \\", f"    {SENTINEL} \\", "    alpha \\",
                 "    beta", "    @echo hi"]
    probs = check(misplaced, "build")
    assert any("not LAST" in p for p in probs), (
        f"selftest: missed a misplaced terminator: {probs}"
    )

    # And it must not be counted as a gate: with it last, a two-gate list is
    # sorted, not "alpha, beta, _gate-list-end out of order".
    assert not any("not sorted" in p for p in check(good, "build")), (
        "selftest: the sentinel was sorted as if it were a gate"
    )

    # The block must end at the first line with no continuation, so a BODY line
    # that happens to look like a name is not read as a dependency.
    body = ["build: \\", "    alpha", "    zzz-not-a-dep", "    @echo hi"]
    s, e = dep_block(body, "build")
    assert (s, e) == (0, 2), f"selftest: block boundary wrong: {(s, e)}"

    # --- the ratchet (issue 1071) ---------------------------------------
    # A deletion, which every OTHER check here accepts: the list below is
    # sorted, one per line, terminated, and missing `beta`.
    assert ratchet({"alpha", "gamma"}, {"alpha", "beta", "gamma"}), (
        "selftest: a gate that left the registry was not reported"
    )
    # Growth is free — that is the whole point of a ratchet.
    assert ratchet({"alpha", "beta", "delta"}, {"alpha", "beta"}) == [], (
        "selftest: an ADDED gate was reported as a problem"
    )
    # And a delete-plus-add that nets to ZERO, which is #431's exact shape and
    # the case a count ratchet cannot see.
    assert ratchet({"alpha", "delta"}, {"alpha", "beta"}), (
        "selftest: one added and one removed netted out and passed"
    )
    # A missing baseline is a failure, not a silent pass: the ratchet would
    # otherwise disappear the moment someone deleted the file.
    assert ratchet({"alpha"}, None), "selftest: a missing baseline passed"

    # The set is FLAT across registries, so a gate moving from `fast` to
    # `build` is not a deletion.
    moved = ["fast-serial: \\", f"    {SENTINEL}", "    @echo f", "",
             "build-serial: \\", "    alpha \\", f"    {SENTINEL}", "    @echo b"]
    assert registry_names(moved) == {"alpha"}, (
        f"selftest: flat name set wrong: {registry_names(moved)}"
    )


def main(argv: list[str]) -> int:
    lines = JUSTFILE.read_text(encoding="utf-8").split("\n")

    if "--write-baseline" in argv:
        present = registry_names(lines)
        write_baseline(present)
        print(
            f"check-gate-lists: wrote {len(present)} gate name(s) to "
            f"{BASELINE.relative_to(REPO)}.\n"
            "Say in the commit message which gate was retired and why — this "
            "file exists\nbecause a deletion nobody meant to make passed every "
            "gate (issue 1071)."
        )
        return 0

    problems = [p for name in REGISTRIES for p in check(lines, name)]
    problems += ratchet(registry_names(lines), load_baseline())
    if problems:
        print("check-gate-lists: a gate registry is a conflict site again\n")
        print("\n".join(problems))
        print(
            "\nA gate registry may not SHRINK, and one gate per line, sorted.\n"
            "A retirement is deliberate:\n"
            "    python3 scripts/check/check-gate-lists.py --write-baseline\n"
            "and name the gate in the commit message.\n"
            "\n`just/check.just` is the file every PR\n"
            "that adds a gate must touch, and packing names onto shared lines\n"
            "turns that into a merge conflict for changes that do not overlap\n"
            "(issues 0883/0884, same shape, no union merge available here --\n"
            "a union of two authored recipe bodies is both of them, not one)."
        )
        return 1

    total = sum(
        len([n for g in names_per_line(lines, *dep_block(lines, name)) for n in g])
        for name in REGISTRIES
    )
    print(
        f"check-gate-lists OK — {len(REGISTRIES)} registry(ies), {total} gate(s), "
        "one per line and sorted."
    )
    return 0


if __name__ == "__main__":
    self_test()
    sys.exit(main(sys.argv[1:]))
