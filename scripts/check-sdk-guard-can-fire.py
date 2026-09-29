#!/usr/bin/env python3
"""A guard on an always-exported SDK variable can never fire — issue 0955.

Six platform-lane skip guards were written:

    if [ -z "${NUTTX_DIR:-}" ] && [ ! -d third-party/nuttx/nuttx ]; then
        nros_lane_skip_note nuttx "NUTTX_DIR unset and ... absent"; exit 0
    fi

`just/sdk-env.just` EXPORTS all 23 such variables with a default, so `-z` is
never true, so the `&&` is never true, and the skip could never fire. Under a
broad lane that step did not skip — it walked into cmake and FAILED where the
author intended a skip, so an unprovisioned host got a cmake-level error instead
of `== nuttx == SKIPPED (...)`.

Unusually for this repo, that breaks the OTHER way: it does not launder a
failure into a pass, it turns an intended skip into a confusing red. Which is
also why nothing caught it — every gate here looks for the first direction.

The fix was one shared helper, `nros_sdk_missing VAR marker`, testing the
RESOLVED directory the way the working guards in the same files already did.
This keeps a seventh site from reintroducing the dead form by copying a
neighbour — the #282 -> #326 shape CLAUDE.md files under "one shared helper
rather than a second spelling".

Buildless: the export list and the guards are both plain text.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SDK_ENV = os.path.join(ROOT, "just", "sdk-env.just")

EXPORT = re.compile(r"^export\s+([A-Z0-9_]+)\s*:=", re.M)
# `-z` on the variable, in every spelling: `"${V:-}"`, `"${V-}"`, `"${V}"`,
# `"$V"`, and unquoted. Braced-only was the first version, so `[ -z "$NUTTX_DIR" ]`
# — the same dead guard — read as clean (phase-472 W2).
GUARD = re.compile(r'-z\s+"?\$(?:\{([A-Z0-9_]+)(?::?-)?\}|([A-Z0-9_]+)\b)"?')

sys.path.insert(0, os.path.join(ROOT, "scripts", "lib"))
from check_just_sources import just_sources  # noqa: E402 — phase-472 W2


def exported_vars(text):
    """Variables sdk-env.just exports — every one of them always has a value."""
    return set(EXPORT.findall(text))


def dead_guards(text, exported):
    """[(lineno, var, line)] for `-z` tests on a variable that is always set."""
    out = []
    for i, line in enumerate(text.split("\n"), 1):
        if line.lstrip().startswith("#"):
            continue
        for var in (a or b for a, b in GUARD.findall(line)):
            if var in exported:
                out.append((i, var, line.strip()))
    return out


def selftest():
    """Both verdicts, on the normal path — phase-395."""
    exported = exported_vars(
        'export NUTTX_DIR := env("NUTTX_DIR", "x")\n'
        'export THREADX_DIR := env("THREADX_DIR", "y")\n')
    assert exported == {"NUTTX_DIR", "THREADX_DIR"}, exported

    bad = 'if [ -z "${NUTTX_DIR:-}" ] && [ ! -d third-party/nuttx/nuttx ]; then'
    assert dead_guards(bad, exported), "a -z on an exported var must be caught"

    # The fixed shape, and the working idiom it was modelled on.
    assert not dead_guards('if nros_sdk_missing NUTTX_DIR include; then', exported)
    assert not dead_guards('if [ ! -d "$NUTTX_DIR/include" ]; then', exported)

    # A -z on a variable sdk-env does NOT export is a legitimate question.
    assert not dead_guards('if [ -z "${SOME_LOCAL:-}" ]; then', exported)

    # A comment describing the defect is prose, not policy.
    assert not dead_guards('    # was: [ -z "${NUTTX_DIR:-}" ]', exported)

    # phase-472 W2 — the unbraced spelling is the same dead guard.
    assert dead_guards('if [ -z "$NUTTX_DIR" ]; then', exported)
    assert dead_guards('if [ -z $THREADX_DIR ]; then', exported)
    assert not dead_guards('if [ -z "$NUTTX_DIRS" ]; then', exported)

    # phase-472 W2 — the population is the justfile GRAPH: the root `justfile`
    # and a file behind `mod check` + `import` are both read.
    import tempfile
    with tempfile.TemporaryDirectory() as tmp:
        os.makedirs(os.path.join(tmp, "just", "check"))
        for rel, text in (
            ("justfile", "mod check 'just/check.just'\nr:\n    [ -z \"$NUTTX_DIR\" ]\n"),
            (os.path.join("just", "check.just"), "import 'check/a.just'\n"),
            (os.path.join("just", "check", "a.just"), "g:\n    [ -z \"${NUTTX_DIR:-}\" ]\n"),
        ):
            with open(os.path.join(tmp, rel), "w", encoding="utf-8") as fh:
                fh.write(text)
        found = {rel for rel, *_ in scan(tmp, exported)}
        assert found == {"justfile", os.path.join("just", "check", "a.just")}, found


def scan(root, exported):
    """[(rel, lineno, var, line)] over every justfile `just` loads (phase-472 W2).

    The graph, not `just/*.just`: that listing read neither the root `justfile`
    nor the 13 files of `mod check` under `just/check/`.
    """
    out = []
    for path in just_sources(root):
        with open(path, encoding="utf-8") as fh:
            text = fh.read()
        out += [(os.path.relpath(path, root), *d) for d in dead_guards(text, exported)]
    return out


def main():
    selftest()
    with open(SDK_ENV, encoding="utf-8") as fh:
        exported = exported_vars(fh.read())
    if not exported:
        sys.exit("check-sdk-guard-can-fire: no exports found in just/sdk-env.just")

    problems = [
        f"{rel}:{lineno}: tests `-z` on ${var}, which "
        f"sdk-env.just always exports — this guard can never fire.\n"
        f"      {line}\n"
        f"      Use: nros_sdk_missing {var} <marker-subdir>"
        for rel, lineno, var, line in scan(ROOT, exported)
    ]
    if problems:
        sys.stderr.write("check-sdk-guard-can-fire: FAILED\n")
        for p in problems:
            sys.stderr.write(f"  {p}\n")
        return 1
    print(f"sdk guards can fire: OK ({len(exported)} exported var(s), no "
          f"`-z` guard on any of them)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
