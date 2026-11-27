#!/usr/bin/env python3
"""A `check cpp` lane compiles with the compiler it was GIVEN, never a literal one.

Issue 3016. The C++ runtime, scoped and header lanes take their compiler
from `NROS_CHECK_CXX` (bound to `_cxx` at the top of each recipe), and the
merge queue runs each of them twice, with `c++` and with `clang++`. A
compile spelled `c++ ...` instead of `"$_cxx" ...` is gcc on every runner
whatever the job asked for, so the clang pass would silently compile it with
gcc. That is the clang lane reading green over a TU it never compiled with
clang, which is how three clang-only defects sat unseen until 3016.

Two rules, both DERIVED from the files rather than listed here:

1. A recipe that binds `_cxx` names no literal C++ compiler (`c++`, `g++`,
   `clang++`) in command position. A comment and a flag (`-x c++`) are fine.
2. Every recipe a workflow runs under `NROS_CHECK_CXX=` binds `_cxx`, or
   the variable reaches nothing.

Usage::

    check-cpp-lane-compiler.py [--selftest]
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LANES = ROOT / "just" / "check" / "lanes.just"
WORKFLOWS = ROOT / ".github" / "workflows"

RECIPE = re.compile(r"^([a-z][a-z0-9_-]*)(?: [^:]*)?:(?!=)")
BINDS = re.compile(r'^\s*_cxx="\$\{NROS_CHECK_CXX:-[^}]+\}"')
# A literal C++ compiler where a COMMAND starts: at the start of a line, or
# after a shell operator / keyword that begins one.
BARE = re.compile(
    r"(?:^\s*|\bif\s+!?\s*|\bthen\s+|\bdo\s+|\belse\s+|&&\s*|\|\|\s*|;\s*|\$\(\s*|!\s+)"
    r"(c\+\+|g\+\+|clang\+\+)(?=\s|$)"
)
RUN_UNDER = re.compile(r"NROS_CHECK_CXX=\S+\s+just\s+check\s+([a-z][a-z0-9_-]*)")


def recipes(text):
    """{name: [(lineno, line), ...]} for every top-level recipe body."""
    out, cur = {}, None
    for n, line in enumerate(text.split("\n"), 1):
        m = RECIPE.match(line)
        if m:
            cur = m.group(1)
            out[cur] = []
            continue
        if cur is not None:
            out[cur].append((n, line))
    return out


def violations(lanes_text, workflow_texts):
    bodies = recipes(lanes_text)
    binding = {r for r, body in bodies.items() if any(BINDS.match(l) for _, l in body)}
    bad = []
    for r in sorted(binding):
        for n, line in bodies[r]:
            if line.lstrip().startswith("#"):
                continue
            m = BARE.search(line)
            if m:
                bad.append(
                    f"just/check/lanes.just:{n}: `{r}` compiles with a literal "
                    f"`{m.group(1)}` -- write \"$_cxx\", so the clang pass compiles it too"
                )
    for name, text in workflow_texts:
        for r in sorted(set(RUN_UNDER.findall(text))):
            if r not in bodies:
                bad.append(f"{name}: runs `just check {r}` under NROS_CHECK_CXX, and no such recipe exists")
            elif r not in binding:
                bad.append(
                    f"{name}: runs `just check {r}` under NROS_CHECK_CXX, but `{r}` never "
                    f"binds `_cxx`, so the variable selects nothing"
                )
    return bad, binding


def selftest():
    good = (
        "cpp-a:\n"
        "    #!/usr/bin/env bash\n"
        "    set -e\n"
        '    _cxx="${NROS_CHECK_CXX:-c++}"\n'
        '    "$_cxx" -std=c++14 x.cpp\n'
        '    if "$_cxx" -fsyntax-only y.cpp; then exit 1; fi\n'
        "    # c++ in a comment is fine\n"
        '    "$_cxx" -include h.hpp -x c++ /dev/null\n'
        "other:\n"
        "    c++ z.cpp\n"
    )
    wf_ok = [("wf.yml", "NROS_CHECK_CXX=clang++ just check cpp-a\n")]
    cases = [
        ("clean", good, wf_ok, 0),
        ("bare at line start", good.replace('"$_cxx" -std=c++14', "c++ -std=c++14"), wf_ok, 1),
        ("bare after if", good.replace('if "$_cxx"', "if c++"), wf_ok, 1),
        ("bare after if !", good.replace('if "$_cxx"', "if ! clang++"), wf_ok, 1),
        ("bare g++ after &&", good.replace('    "$_cxx" -std', '    true && g++ -std'), wf_ok, 1),
        ("run under, no binding", good, [("wf.yml", "NROS_CHECK_CXX=clang++ just check other\n")], 1),
        ("run under, no recipe", good, [("wf.yml", "NROS_CHECK_CXX=c++ just check nope\n")], 1),
    ]
    failed = 0
    for label, text, wfs, want in cases:
        got = len(violations(text, wfs)[0])
        if (got > 0) != (want > 0):
            print(f"check-cpp-lane-compiler self-test: `{label}` reported {got}, wanted {want}", file=sys.stderr)
            failed += 1
    return failed


def main(argv):
    failed = selftest()
    if failed:
        print(f"check-cpp-lane-compiler self-test: {failed} case(s) FAILED", file=sys.stderr)
        return 1
    if "--selftest" in argv:
        print("check-cpp-lane-compiler self-test: OK")
        return 0
    wfs = [(str(p.relative_to(ROOT)), p.read_text()) for p in sorted(WORKFLOWS.glob("*.yml"))]
    bad, binding = violations(LANES.read_text(), wfs)
    if not binding:
        print("check-cpp-lane-compiler: no recipe binds `_cxx` -- the rule has nothing to check, "
              "which means the binding was renamed", file=sys.stderr)
        return 1
    if bad:
        print(f"check-cpp-lane-compiler: {len(bad)} site(s)", file=sys.stderr)
        for b in bad:
            print("  " + b, file=sys.stderr)
        return 1
    print(f"check-cpp-lane-compiler: OK -- {len(binding)} recipe(s) take NROS_CHECK_CXX, none names a literal compiler")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
