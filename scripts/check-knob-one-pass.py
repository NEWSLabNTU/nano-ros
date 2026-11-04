#!/usr/bin/env python3
"""phase-530 W7 (RFC-0120 D5) -- a configure takes ONE pass.

Issue 1002 measured a configure sequence that needed THREE passes before the
receive payload class settled, and four island builds shipped the stale,
larger value -- silent, because over-sizing always is. Its gate configured
until nothing moved. The fixed point that made several passes necessary
(`NanoRosReconfigure.cmake`, issue 0991) is deleted: `nros configure` seeds
every early reader before the host configure, and the late producers are held
to the seed. So the claim this gate makes is the stronger one:

    a configured build dir, configured AGAIN with nothing changed, moves no
    resolved knob.

A knob that moves on that second configure was not settled by the first --
the shape issue 1002 shipped -- and fails HERE. The pass count is 0, not a
bound to raise.

    check-knob-one-pass.py <build-dir> [--configure CMD]
    check-knob-one-pass.py --self-test
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
    as a changed value and it must not be treated as settled.
    """
    moved = []
    for k in sorted(set(a) | set(b)):
        if a.get(k) != b.get(k):
            moved.append((k, a.get(k), b.get(k)))
    return moved


def check(build_dir, configure_cmd):
    """Configure once more; any resolved knob that moves is a problem."""
    prev = snapshot(build_dir)
    if not prev:
        return ["no NROS_RESOLVED_* in %s/CMakeCache.txt -- configure it once "
                "before asking whether one pass settled it" % build_dir]
    rc = subprocess.run(configure_cmd, shell=True, capture_output=True).returncode
    if rc != 0:
        return ["the second configure FAILED (rc=%d): %s" % (rc, configure_cmd)]
    cur = snapshot(build_dir)
    moved = diff(prev, cur)
    if not moved:
        print("check-knob-one-pass: one configure settled it; %d resolved "
              "knob(s) unmoved by a second." % len(cur))
        return []
    problems = []
    for k, was, now in moved:
        problems.append(
            "%s moved %s -> %s on a second configure with nothing changed. "
            "The first configure's build ships the first value, and a stale "
            "value is often LARGER, so it over-sizes rather than failing -- "
            "silent (issue 1002)."
            % (k, was if was is not None else "<absent>",
               now if now is not None else "<absent>"))
    problems.append(
        "a configure did not settle in ONE pass. RFC-0120 D5: an early reader "
        "reads `nros configure`'s seed and a late producer is held to it; "
        "find the reader that read something else.")
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
        got = check(tmp, step)
        if not got:
            print("  self-test FAIL: a second configure that moved a value passed")
            failures += 1
        elif not quiet:
            print("  ok    a value a second configure moves is caught")

        # A configure that changes nothing: must PASS.
        with open(cache, "w") as fh:
            fh.write("NROS_RESOLVED_NROS_X:INTERNAL=7\n")
        got = check(tmp, "true")
        if got:
            print("  self-test FAIL: a stable sequence reported %d problem(s)" % len(got))
            failures += 1
        elif not quiet:
            print("  ok    a configure that moves nothing passes")

        # A value that APPEARS on the second configure is not settled either.
        with open(cache, "w") as fh:
            fh.write("NROS_RESOLVED_NROS_X:INTERNAL=7\n")
        late = os.path.join(tmp, "late.sh")
        with open(late, "w") as fh:
            fh.write("#!/bin/sh\nprintf 'NROS_RESOLVED_NROS_X:INTERNAL=7\\n"
                     "NROS_RESOLVED_NROS_Y:INTERNAL=9\\n' > %s\n" % cache)
        os.chmod(late, 0o755)
        got = check(tmp, late)
        if not got:
            print("  self-test FAIL: a knob APPEARING on the second configure passed")
            failures += 1
        elif not quiet:
            print("  ok    a knob that appears late is caught")

    if failures:
        print("check-knob-one-pass self-test: FAILED (%d)" % failures)
        return 1
    return 0


def main(argv):
    if len(argv) == 2 and argv[1] == "--self-test":
        return self_test()
    if len(argv) < 2:
        print("usage: check-knob-one-pass.py <build-dir> [--configure CMD]")
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
    # `cmake <build-dir>` re-configures from the cached source and settings;
    # `cmake -B <dir>` alone would take the CWD as the source.
    configure = "cmake '%s'" % build_dir
    if "--configure" in argv:
        configure = argv[argv.index("--configure") + 1]
    problems = check(build_dir, configure)
    if problems:
        print("check-knob-one-pass: a configure did not settle in one pass:")
        for p in problems:
            print("  - %s" % p)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
