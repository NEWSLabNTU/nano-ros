#!/usr/bin/env python3
"""Issue 1002 -- a configure sequence must reach a FIXPOINT.

Issue 0991 established that a clean build dir derives over the wrong basis on
the first configure. Its remedy is "configure again", and everything in the
tree says TWO passes. Two is not enough for a knob whose resolved value is
cached: the fragment updates on pass 2 and the knob resolved from it on pass 3.

Measured on the reference island, watching the receive payload class:

    configure 1   fragment placeholder   delivered 1496
    configure 2   fragment 880           delivered 1496
    configure 3   fragment 880           delivered 880

Four consecutive island builds shipped the stale value. Nothing broke, because
the stale value is the CLOSURE basis and therefore LARGER -- over-sized,
silent, green. That is exactly why it survived, and why counting passes is the
wrong instrument: the count is folklore, it changes whenever a chain gets
longer, and being WRONG about it looks identical to being right.

So this gate does not count passes. It configures until nothing moves, and
fails if that takes more passes than it was told to allow. A knob that gains a
longer chain fails HERE rather than shipping a stale value for four builds.

    check-knob-fixpoint.py <build-dir> [--max-passes N] [--configure CMD]
    check-knob-fixpoint.py --self-test
"""
import os
import re
import subprocess
import sys
import tempfile


def snapshot(build_dir):
    """Every NROS_RESOLVED_* the cache holds, as a dict."""
    out = {}
    path = os.path.join(build_dir, "CMakeCache.txt")
    if not os.path.exists(path):
        return out
    with open(path, encoding="utf8", errors="ignore") as fh:
        for line in fh:
            m = re.match(r"^(NROS_RESOLVED_[A-Z0-9_]+):[A-Z]+=(.*)$", line.strip())
            if m:
                out[m.group(1)] = m.group(2)
    return out


def diff(a, b):
    """Names whose value MOVED, plus names that appeared or vanished.

    An appearance counts: a knob that shows up on pass 3 was absent on pass 2,
    and a consumer reading it on pass 2 got a default. That is the same defect
    as a changed value and it must not be treated as convergence.
    """
    moved = []
    for k in sorted(set(a) | set(b)):
        if a.get(k) != b.get(k):
            moved.append((k, a.get(k), b.get(k)))
    return moved


def check(build_dir, configure_cmd, max_passes):
    problems = []
    prev = snapshot(build_dir)
    if not prev:
        return ["no NROS_RESOLVED_* in %s/CMakeCache.txt -- configure it once "
                "before asking whether it has converged" % build_dir]
    for n in range(1, max_passes + 1):
        rc = subprocess.run(configure_cmd, shell=True, capture_output=True).returncode
        if rc != 0:
            return ["configure pass %d FAILED (rc=%d): %s" % (n, rc, configure_cmd)]
        cur = snapshot(build_dir)
        moved = diff(prev, cur)
        if not moved:
            print("check-knob-fixpoint: converged after %d extra configure(s); "
                  "%d resolved knob(s) stable." % (n, len(cur)))
            return []
        prev = cur
    for k, was, now in moved:
        problems.append(
            "%s moved %s -> %s on configure pass %d. A build that stops "
            "earlier ships the stale value, and the stale value is often "
            "LARGER, so it over-sizes rather than failing -- silent."
            % (k, was if was is not None else "<absent>",
               now if now is not None else "<absent>", max_passes))
    problems.append(
        "the configure sequence had NOT converged after %d extra pass(es). "
        "Issue 1002: do not raise this number without asking why the chain "
        "got longer." % max_passes)
    return problems


def self_test(quiet=False):
    """The gate must FAIL on a sequence that does not settle, and pass on one
    that does. Without both, a gate that stopped comparing anything reports
    success forever."""
    failures = 0
    with tempfile.TemporaryDirectory() as tmp:
        cache = os.path.join(tmp, "CMakeCache.txt")

        # A configure that keeps changing a value: must FAIL.
        with open(cache, "w") as fh:
            fh.write("NROS_RESOLVED_NROS_X:INTERNAL=1\n")
        step = os.path.join(tmp, "step.sh")
        with open(step, "w") as fh:
            fh.write("#!/bin/sh\nn=$(cat %s/n 2>/dev/null || echo 1)\n"
                     "n=$((n+1)); echo $n > %s/n\n"
                     "echo NROS_RESOLVED_NROS_X:INTERNAL=$n > %s\n" % (tmp, tmp, cache))
        os.chmod(step, 0o755)
        got = check(tmp, step, 2)
        if not got:
            print("  self-test FAIL: a never-settling sequence reported convergence")
            failures += 1
        elif not quiet:
            print("  ok    a value that keeps moving is caught")

        # A configure that changes nothing: must PASS.
        with open(cache, "w") as fh:
            fh.write("NROS_RESOLVED_NROS_X:INTERNAL=7\n")
        got = check(tmp, "true", 2)
        if got:
            print("  self-test FAIL: a stable sequence reported %d problem(s)" % len(got))
            failures += 1
        elif not quiet:
            print("  ok    a stable sequence converges")

        # A value that APPEARS late is not convergence either.
        with open(cache, "w") as fh:
            fh.write("NROS_RESOLVED_NROS_X:INTERNAL=7\n")
        late = os.path.join(tmp, "late.sh")
        with open(late, "w") as fh:
            fh.write("#!/bin/sh\nprintf 'NROS_RESOLVED_NROS_X:INTERNAL=7\\n"
                     "NROS_RESOLVED_NROS_Y:INTERNAL=9\\n' > %s\n" % cache)
        os.chmod(late, 0o755)
        got = check(tmp, late, 1)
        if not got:
            print("  self-test FAIL: a knob APPEARING late reported convergence")
            failures += 1
        elif not quiet:
            print("  ok    a knob that appears late is caught")

    if failures:
        print("check-knob-fixpoint self-test: FAILED (%d)" % failures)
        return 1
    return 0


def main(argv):
    if len(argv) == 2 and argv[1] == "--self-test":
        return self_test()
    if len(argv) < 2:
        print("usage: check-knob-fixpoint.py <build-dir> [--max-passes N] "
              "[--configure CMD]")
        return 2
    # Always, not only behind the flag: a negative control nobody runs
    # decays into a comment, and this rule's whole job is to fire. Same
    # shape as `scripts/check-knob-delivery.py`; gated by
    # `check-gate-selftests`. It runs BEFORE the real check so a gate that
    # has stopped matching anything cannot report success.
    rc = self_test(quiet=True)
    if rc:
        return rc
    build_dir = argv[1]
    max_passes = 2
    configure = "cmake -B %s" % build_dir
    if "--max-passes" in argv:
        max_passes = int(argv[argv.index("--max-passes") + 1])
    if "--configure" in argv:
        configure = argv[argv.index("--configure") + 1]
    problems = check(build_dir, configure, max_passes)
    if problems:
        print("check-knob-fixpoint: the configure sequence has not converged:")
        for p in problems:
            print("  - %s" % p)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
