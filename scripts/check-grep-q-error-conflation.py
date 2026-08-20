#!/usr/bin/env python3
"""Forbid NEW `grep -q` conditionals that cannot tell an error from a non-match.

Issue 0726. `grep` exits 1 for "no match" and >=2 for an error. Both natural
spellings conflate them, in opposite directions:

    if ! … grep -q PAT …    an error becomes a FINDING that is not real
    if   … grep -q PAT …    an error makes the check silently NOT FIRE

The first was a live defect: under a 32-way gate fan-out a forked grep failed to
start and `check-rmw-force-link-anchor` reported a missing force-link anchor for
an example that has one. Only ever green->red under load, which is the direction
that teaches people to stop believing a gate.

The fix is `nros_grep_q` (scripts/lib/grep-q.sh): 0 match, 1 no-match, exit 2 on
tool failure.

This gate is a RATCHET, not a cleanup. The sweep found 87 pre-existing sites, and
converting them blind would churn 87 diffs to fix an unknown fraction — for many
the searched text is certainly present-or-absent and no error is possible. So the
existing sites are baselined by COUNT PER FILE and the gate fails when a file
grows a new one. Lowering a baseline is always allowed; raising it is the thing
that needs a reason.
"""

import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BASELINE = os.path.join(ROOT, "scripts", "grep-q-baseline.json")

# A `grep -q` whose STATUS drives control flow. A bare `grep -q` on its own line
# (status discarded, or captured into a variable the caller then inspects) is
# not this defect — that is the shape the fix itself uses.
COND = re.compile(
    r"""(?x)
    (?: \bif\s+!?\s* .* \bgrep\s+-[a-zA-Z]*q )      # if [!] … grep -q
  | (?: \bgrep\s+-[a-zA-Z]*q .* \|\| )              # grep -q … || …
  | (?: \bgrep\s+-[a-zA-Z]*q .* \&\& )              # grep -q … && …
  | (?: \bwhile\s+!?\s* .* \bgrep\s+-[a-zA-Z]*q )   # while [!] … grep -q
    """
)

SUFFIXES = (".sh", ".just", ".py")


def tracked():
    out = subprocess.run(
        ["git", "-C", ROOT, "ls-files", "--", "scripts", "just", "justfile"],
        capture_output=True, text=True, check=True,
    ).stdout.split()
    return [f for f in out if f.endswith(SUFFIXES) or f == "justfile"]


def count(rel):
    path = os.path.join(ROOT, rel)
    try:
        with open(path, encoding="utf-8", errors="replace") as fh:
            lines = fh.readlines()
    except OSError:
        return 0
    n = 0
    for line in lines:
        stripped = line.lstrip()
        if stripped.startswith("#"):
            continue          # prose about the rule is not a violation
        if "nros_grep_q" in line:
            continue          # the fix
        if COND.search(line):
            n += 1
    return n


def scan():
    me = os.path.relpath(os.path.abspath(__file__), ROOT)
    return {f: c for f in tracked() if f != me and (c := count(f))}


def self_test():
    """Both directions — a checker that stopped checking passes silently."""
    good = [
        "grep -q foo bar.txt; rc=$?",
        "nros_grep_q \"$pat\" \"$f\"",
        "# if ! grep -q foo; then   (prose)",
    ]
    bad = [
        'if ! printf "%s" "$t" | grep -q "$pat"; then',
        "if grep -q foo bar; then",
        "grep -q foo bar || fail=1",
        "grep -q foo bar && continue",
    ]
    fails = []
    for s in good:
        st = s.lstrip()
        if not st.startswith("#") and "nros_grep_q" not in s and COND.search(s):
            fails.append(f"false positive: {s}")
    for s in bad:
        if not COND.search(s):
            fails.append(f"MISSED: {s}")
    if fails:
        print("check-grep-q-error-conflation --self-test FAILED:")
        print("\n".join("  " + f for f in fails))
        return 1
    print(f"check-grep-q-error-conflation --self-test: "
          f"{len(good) + len(bad)} case(s) OK")
    return 0


def main():
    if "--self-test" in sys.argv:
        return self_test()
    if self_test():
        return 1

    current = scan()
    if "--write-baseline" in sys.argv:
        with open(BASELINE, "w", encoding="utf-8") as fh:
            json.dump(current, fh, indent=2, sort_keys=True)
            fh.write("\n")
        print(f"wrote baseline: {sum(current.values())} site(s) "
              f"across {len(current)} file(s)")
        return 0

    try:
        with open(BASELINE, encoding="utf-8") as fh:
            base = json.load(fh)
    except OSError:
        print(f"FAIL: missing baseline {BASELINE}; "
              f"regenerate with --write-baseline", file=sys.stderr)
        return 1

    grew = []
    for f, n in sorted(current.items()):
        was = base.get(f, 0)
        if n > was:
            grew.append((f, was, n))
    if grew:
        print("FAIL: new `grep -q` conditional(s) that cannot distinguish a")
        print("      tool ERROR (exit >=2) from a NON-MATCH (exit 1):")
        for f, was, n in grew:
            print(f"  {f}: {was} -> {n}")
        print()
        print("  Source scripts/lib/grep-q.sh and use `nros_grep_q`:")
        print("    nros_grep_q \"$pat\" \"$file\"; case $? in 0) ;; 1) finding ;; esac")
        print("  It exits 2 on a tool failure instead of reporting a finding.")
        print()
        print("  Issue 0726: a forked grep that failed to start under a 32-way")
        print("  fan-out was reported as a missing force-link anchor — a false,")
        print("  specific claim, and only ever under load.")
        return 1

    total = sum(current.values())
    shrank = sum(1 for f, n in current.items() if n < base.get(f, 0))
    gone = sum(1 for f in base if f not in current)
    msg = (f"check-grep-q-error-conflation: OK ({total} baselined site(s), "
           f"no file grew one)")
    if shrank or gone:
        msg += f" — {shrank + gone} file(s) improved; rerun --write-baseline"
    print(msg)
    return 0


if __name__ == "__main__":
    sys.exit(main())
