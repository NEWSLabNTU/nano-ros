#!/usr/bin/env python3
"""A knob is resolved EXACTLY ONCE.

phase-412 hit this three times, and every instance had the same shape: a knob
gained a derivable resolution and kept its old plain one further down the file.
The second call wins, and what it passes is the RAW Kconfig value -- which for
a derivable knob is the `-1` DERIVE SENTINEL. So the sentinel is resolved as
though an operator had stated it:

    NROS_RMW_SUBSCRIBER_SLOTS   derived 10, resolved -1
    NROS_EXECUTOR_MAX_NODES     derived  4, resolved -1
    ZPICO_MAX_SUBSCRIBERS       resolved from raw CONFIG_ before the resolver ran

None was visible to any other gate: the derived value was RIGHT, the resolver
RAN, and the number that reached the build came from the wrong call. The rule
is "converting a knob to derivable means REMOVING its old call, not just adding
a new one", and a rule nobody can check is a rule that gets forgotten -- it was,
three times, by the same person in the same file.

    check-knob-resolved-once.py [files...]
    check-knob-resolved-once.py --self-test
"""
import re
import sys
import tempfile

CALL = re.compile(
    r"_nros_resolve(?:_derivable)?_knob\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)", re.M)

DEFAULT_FILES = ["zephyr/cmake/nros_cargo_build.cmake"]


def scan(text):
    """knob -> list of BRANCH PATHS at which it is resolved.

    Branch-aware, because the first version was not and reported two false
    positives on its first real run: ZPICO_TX_BATCH and ZPICO_TX_SPLIT_LOCK are
    each resolved in the `if` arm and again in the `else` arm of one
    conditional, so exactly one call ever executes. A checker that flags a
    correct idiom trains people to ignore it, which is worse than not having it.

    A path is the list of (block id, arm index) entered to reach the call. Two
    calls are EXCLUSIVE when they take different arms of a block they share;
    otherwise both can run and the later one wins.

    A call that BUILDS its name (`${_pool}`) is not matched, deliberately: a
    computed name is invisible to every static check here, and
    `check-kconfig-knob-forwarding` already rejects that shape.
    """
    out = {}
    path = []
    next_block = [0]
    for line in text.splitlines():
        stripped = line.strip()
        low = stripped.lower()
        if low.startswith("if("):
            next_block[0] += 1
            path.append([next_block[0], 0])
        elif low.startswith("elseif(") or low.startswith("else("):
            if path:
                path[-1][1] += 1
        elif low.startswith("endif("):
            if path:
                path.pop()
        for m in CALL.finditer(stripped):
            out.setdefault(m.group(1), []).append([tuple(p) for p in path])
    return out


def exclusive(a, b):
    """True when two branch paths cannot both execute."""
    for (blk_a, arm_a), (blk_b, arm_b) in zip(a, b):
        if blk_a != blk_b:
            return False
        if arm_a != arm_b:
            return True
    return False


def check(paths):
    problems = []
    for path in paths:
        try:
            with open(path, encoding="utf8") as fh:
                text = fh.read()
        except OSError as e:
            problems.append("cannot read %s: %s" % (path, e))
            continue
        # Strip comments: the fix for each past instance left the old call
        # QUOTED in a comment explaining what it cost, and a checker that
        # counted those would fail on the very commit that fixed the bug.
        stripped = "\n".join(
            line for line in text.splitlines() if not line.lstrip().startswith("#"))
        for knob, paths in sorted(scan(stripped).items()):
            reachable = [
                p for i, p in enumerate(paths)
                if not any(exclusive(p, q) for j, q in enumerate(paths) if i != j)
            ]
            n = len(reachable)
            if n > 1:
                problems.append(
                    "%s is resolved %d times in %s. The LAST call wins, and if "
                    "it passes the raw Kconfig value that is the `-1` DERIVE "
                    "SENTINEL -- resolved as though someone had stated it. "
                    "Converting a knob to derivable means REMOVING its old "
                    "_nros_resolve_knob call." % (knob, n, path))
    return problems


def self_test(quiet=False):
    cases = [
        ("_nros_resolve_knob(NROS_A \"x\")\n", 0, "one plain call"),
        ("_nros_resolve_derivable_knob(NROS_A \"x\" D)\n", 0, "one derivable call"),
        ("_nros_resolve_derivable_knob(NROS_A \"x\" D)\n"
         "_nros_resolve_knob(NROS_A \"${CONFIG_NROS_A}\")\n", 1,
         "the real defect: derivable plus a leftover plain call"),
        ("_nros_resolve_knob(NROS_A \"x\")\n"
         "_nros_resolve_knob(NROS_A \"y\")\n", 1, "two plain calls"),
        ("# _nros_resolve_knob(NROS_A \"x\")\n"
         "_nros_resolve_derivable_knob(NROS_A \"x\" D)\n", 0,
         "a commented-out old call does not count"),
        # The false positive this gate produced on its FIRST real run. Both
        # arms of one conditional resolve the same knob; exactly one executes.
        ("if(CONFIG_X)\n"
         "    _nros_resolve_knob(ZPICO_A \"1\")\n"
         "else()\n"
         "    _nros_resolve_knob(ZPICO_A \"0\")\n"
         "endif()\n", 0, "if/else arms are exclusive, not duplicates"),
        # ...but a call OUTSIDE the conditional and one inside it can both run.
        ("_nros_resolve_knob(ZPICO_B \"1\")\n"
         "if(CONFIG_X)\n"
         "    _nros_resolve_knob(ZPICO_B \"0\")\n"
         "endif()\n", 1, "inside plus outside a conditional both reachable"),
    ]
    failures = 0
    for text, want, name in cases:
        with tempfile.NamedTemporaryFile("w", suffix=".cmake", delete=False) as fh:
            fh.write(text)
            path = fh.name
        got = len(check([path]))
        ok = (got >= 1) if want else (got == 0)
        if not ok:
            print("  self-test FAIL: %s -- got %d, want %s"
                  % (name, got, "at least 1" if want else "0"))
            failures += 1
        elif not quiet:
            print("  ok    %s" % name)
    if failures:
        print("check-knob-resolved-once self-test: FAILED (%d)" % failures)
        return 1
    return 0


def main(argv):
    if len(argv) == 2 and argv[1] == "--self-test":
        return self_test()
    # Always, not only behind the flag: a negative control nobody runs
    # decays into a comment, and this rule's whole job is to fire. Same
    # shape as `scripts/check-knob-delivery.py`; gated by
    # `check-gate-selftests`. It runs BEFORE the real check so a gate that
    # has stopped matching anything cannot report success.
    rc = self_test(quiet=True)
    if rc:
        return rc
    paths = argv[1:] or DEFAULT_FILES
    problems = check(paths)
    if problems:
        print("check-knob-resolved-once: a knob is resolved more than once:")
        for p in problems:
            print("  - %s" % p)
        return 1
    print("check-knob-resolved-once: OK (%s)" % ", ".join(paths))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
