#!/usr/bin/env python3
"""Does a split C++ build gate write only into the directory it creates?

`check::cpp` was split into four build gates (issue 2683), each writing its
probes into its own `$gen/nros-cpp-tests-<suffix>` dir. Four sites kept the
old shared `$gen/nros-cpp-tests/`, which no split gate creates, so on a fresh
tree `cpp-headers` and `cpp-runtime` failed at the first write ("can't create
... No such file or directory"), and on a used tree they passed off whatever an
earlier run had left. The answer depended on what was on disk, which is why
this reads the RECIPE, not the disk: it holds the same on every tree.

Rule, per gate `cpp-<suffix>` in just/check/lanes.just: every
`$gen/nros-cpp-tests…` token names `nros-cpp-tests-<suffix>`, and the recipe
runs `mkdir -p "$gen/nros-cpp-tests-<suffix>"` before its first other use.

    cpp_gate_own_dirs.py <gate>...     check those gates (exit 1 on a breach)
    cpp_gate_own_dirs.py --self-test   negative control (also run before
                                       every check)
"""

import re
import sys
from pathlib import Path

LANES = Path(__file__).resolve().parents[2] / "just" / "check" / "lanes.just"
TOKEN = re.compile(r"\$gen/(nros-cpp-tests[A-Za-z0-9_-]*)")


def recipe_body(text, gate):
    lines = text.splitlines()
    head = re.compile(r"^" + re.escape(gate) + r"(\s[^:]*)?:")
    for i, line in enumerate(lines):
        if head.match(line):
            body = []
            for nxt in lines[i + 1:]:
                if nxt and not nxt[0].isspace():
                    break
                body.append(nxt)
            return body
    return None


def breaches(text, gate):
    body = recipe_body(text, gate)
    if body is None:
        return [f"{gate}: no such recipe in {LANES.name}"]
    own = "nros-cpp-tests-" + gate.removeprefix("cpp-")
    made = False
    out = []
    for n, line in enumerate(body, 1):
        for name in TOKEN.findall(line):
            if name != own:
                out.append(f"{gate}: body line {n} writes $gen/{name}, "
                           f"not its own $gen/{own}: {line.strip()}")
            elif not made:
                if re.search(r'mkdir -p "\$gen/' + re.escape(own) + r'[/"]',
                             line):
                    made = True
                else:
                    out.append(f"{gate}: body line {n} uses $gen/{own} "
                               f"before `mkdir -p` creates it: {line.strip()}")
    return out


def self_test():
    good = ('cpp-x:\n    set -e\n    mkdir -p "$gen/nros-cpp-tests-x"\n'
            '    cc -o "$gen/nros-cpp-tests-x/a"\nother:\n'
            '    cc -o "$gen/nros-cpp-tests/b"\n')
    shared = good.replace('"$gen/nros-cpp-tests-x/a"',
                          '"$gen/nros-cpp-tests/a"')
    early = ('cpp-x:\n    cc -o "$gen/nros-cpp-tests-x/a"\n'
             '    mkdir -p "$gen/nros-cpp-tests-x"\n')
    cases = [(good, 0), (shared, 1), (early, 1)]
    for i, (text, want) in enumerate(cases):
        got = len(breaches(text, "cpp-x"))
        if got != want:
            print(f"self-test case {i}: {got} breach(es), want {want}")
            return 1
    return 0


def main(argv):
    if argv == ["--self-test"]:
        rc = self_test()
        print("cpp_gate_own_dirs self-test: " + ("ok" if rc == 0 else "FAILED"))
        return rc
    if not argv:
        print("usage: cpp_gate_own_dirs.py <gate>... | --self-test")
        return 2
    if self_test() != 0:
        return 1
    text = LANES.read_text()
    found = [b for gate in argv for b in breaches(text, gate)]
    for b in found:
        print("FAIL " + b)
    if found:
        print("A split C++ gate must write only into the dir it creates, "
              "or a fresh tree fails it.")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
