#!/usr/bin/env python3
"""A workflow must provision with `just setup <scope>`, not `just <scope> setup`.

The two spellings look interchangeable and are not:

    just setup zephyr     -> dispatcher: runs `_setup-common`, THEN `just zephyr setup`
    just zephyr setup     -> the module recipe ALONE

`_setup-common` is where the host facts every tier asserts get provisioned --
cross Rust targets, pinned corrosion, the in-tree CLI, `nros-launch-resolve`,
clang-format. The module spelling skips all of it silently: the recipe exists,
it succeeds, and the lane fails later on a precondition nothing provisioned.

That is not hypothetical. `nightly.yml`'s platform job used the module spelling
and carried a hand-rolled "Install cross targets from config/rust-targets.txt"
step to compensate -- a workaround for a defect one word wide. host-tests hit
the same class from the other direction and cost three CI rounds to unpick.

`check-preconditions-provisioned` asserts that `just setup` PROVIDES those
facts. This asserts that workflows INVOKE the form which runs them. Neither
implies the other.

EXEMPTIONS carry a reason and are checked in both directions: an exemption that
matches nothing is deleted, because a stale allow-list is how a gate quietly
stops covering what it names.

Run:  python3 scripts/check-workflow-setup-spelling.py [--self-test]
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WORKFLOWS = os.path.join(ROOT, ".github", "workflows")

# `just <scope> setup ...` occurrences that may stay, and why.
#
# The dispatcher takes a scope and nothing else -- `setup target="" tier=""`
# execs `just "$target" setup` with no argument passthrough -- so a call that
# must pass a FLAG has no dispatcher spelling available. These lanes therefore
# do not get `_setup-common`, and that is a known gap rather than an accident:
# giving the dispatcher argument passthrough would let them convert.
EXEMPT = {
    "just zephyr setup --skip-sdk": (
        "Passes `--skip-sdk` (the image bakes the SDK). The `setup` dispatcher "
        "has no argument passthrough, so no dispatcher spelling exists. These "
        "jobs provision the host facts themselves or do not need them."
    ),
}


def scope_tokens():
    """Platform scope names, from the same table the dispatcher consults."""
    path = os.path.join(ROOT, "scripts", "build", "scope.sh")
    try:
        with open(path, encoding="utf8") as fh:
            text = fh.read()
    except OSError:
        return set()
    names = set()
    for m in re.finditer(r'_NROS_SCOPE_PLATFORMS="([^"]*)"', text):
        names.update(re.findall(r"[a-z0-9_]+", m.group(1)))
    return names


def offenders(text, scopes):
    """[(lineno, line)] for executable `just <scope> setup` calls."""
    out = []
    for i, raw in enumerate(text.split("\n"), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        # Two shapes, and missing the second made this gate useless on the very
        # workflow that motivated it: nightly's platform job spells the scope as
        # a matrix expression, `just ${{ matrix.plat }} setup`. A templated scope
        # in that position is ALWAYS the module form -- there is nothing to
        # resolve, and the spelling alone is the defect. Caught by mutation-
        # testing this gate against the pre-conversion nightly.yml.
        m = re.search(r"just (\$\{\{[^}]*\}\}) setup\b(.*)$", line)
        if m:
            scope, rest = m.group(1), m.group(2).strip()
        else:
            m = re.search(r"just ([a-z0-9_]+) setup\b(.*)$", line)
            if not m:
                continue
            scope, rest = m.group(1), m.group(2).strip()
            if scope not in scopes:
                continue
        call = ("just %s setup %s" % (scope, rest)).strip()
        out.append((i, line, call))
    return out


def self_test():
    scopes = {"zephyr", "freertos"}
    t = "\n".join(
        [
            "      # a comment about just zephyr setup is not a call",
            "          just zephyr setup --skip-sdk",
            "          just setup zephyr",
            "          just freertos setup",
            "          just workspace setup",
            "          just ${{ matrix.plat }} setup",
        ]
    )
    got = offenders(t, scopes)
    assert [g[0] for g in got] == [2, 4, 6], got
    assert got[0][2] == "just zephyr setup --skip-sdk", got[0]
    assert got[1][2] == "just freertos setup", got[1]
    sys.stdout.write("check-workflow-setup-spelling self-test: OK\n")


def main():
    if "--self-test" in sys.argv:
        self_test()
        return 0
    self_test()

    scopes = scope_tokens()
    if not scopes:
        sys.stderr.write(
            "error: parsed ZERO platform scopes from scripts/build/scope.sh.\n"
            "This gate would then match nothing and pass vacuously.\n"
        )
        return 1

    problems = []
    seen_exempt = set()
    for fn in sorted(os.listdir(WORKFLOWS)):
        if not fn.endswith((".yml", ".yaml")):
            continue
        path = os.path.join(WORKFLOWS, fn)
        with open(path, encoding="utf8") as fh:
            text = fh.read()
        for lineno, line, call in offenders(text, scopes):
            if call in EXEMPT:
                seen_exempt.add(call)
                continue
            problems.append(
                "%s:%d uses the MODULE spelling\n"
                "      %s\n"
                "    `just <scope> setup` runs the module recipe alone and skips\n"
                "    `_setup-common`, so the cross Rust targets, pinned corrosion,\n"
                "    the CLI, the resolver and clang-format are never provisioned.\n"
                "    Use the dispatcher:  just setup <scope>" % (fn, lineno, line)
            )

    for call in EXEMPT:
        if call not in seen_exempt:
            problems.append(
                "STALE exemption %r matches no workflow line.\n"
                "    Delete it — an allow-list checked one way stops covering\n"
                "    what it claims to." % call
            )

    if problems:
        sys.stderr.write("check-workflow-setup-spelling: %d problem(s)\n\n" % len(problems))
        for p in problems:
            sys.stderr.write("  - %s\n\n" % p)
        return 1

    sys.stdout.write(
        "check-workflow-setup-spelling: OK — %d scope(s), %d exemption(s) all live.\n"
        % (len(scopes), len(EXEMPT))
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
