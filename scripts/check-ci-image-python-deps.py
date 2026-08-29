#!/usr/bin/env python3
"""The zephyr CI image must install every Python module its own checker demands.

Issue 0878. `scripts/check-python-deps.py` decides whether a host can run
`west build`, and its `west` + `zephyr-build` groups are what
`just zephyr setup` enforces. The zephyr CI image did not install them, so every
Zephyr nightly cell failed in the setup step before a single build ran — 21 of
21 red, which also meant the lane could not report the real regression it
contained (issue 0876).

The image now installs them. This gate is what stops the two lists drifting
apart again, because a second hand-maintained copy of a dependency list is
exactly how that happens: adding a module to `GROUPS` without adding it to the
Dockerfile puts the image back where it started, and the failure appears a day
later in a lane nobody is watching.

WHAT IS CHECKED

1. Every pip name in `GROUPS["west"] + GROUPS["zephyr-build"]` appears in the
   Dockerfile's `pip3 install` block.
2. Every one of them is PINNED (`name==version`). An unpinned install at
   image-build time means two builds of the same tag ship different tooling
   with no signal — the floating half of the drift axis in
   `docs/development/ci-image-provisioning.md`, and the reason `uv` and `just`
   were pinned in ci-base.
3. The import-check line in the Dockerfile actually imports what was installed,
   so a package that installs but cannot be imported fails at BUILD time rather
   than in a lane.

WHAT IS NOT CHECKED

Whether the pinned versions are current, or mutually compatible. That is a
judgement, and a gate that guessed at it would fail on every legitimate bump.
The `python3 -c 'import ...'` line in the Dockerfile is what catches an
incompatible set, at image-build time, which is where it belongs.

Usage::

    check-ci-image-python-deps.py [--selftest]
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DOCKERFILE = os.path.join(ROOT, "ci", "docker", "zephyr-ros", "Dockerfile")
DEPS_SCRIPT = os.path.join(ROOT, "scripts", "check-python-deps.py")

# The groups the zephyr lanes enforce. `just zephyr setup` fails on exactly
# these; a group the image does not need (px4) is deliberately absent.
REQUIRED_GROUPS = ("west", "zephyr-build")

PIP_BLOCK = re.compile(r"RUN\s+pip3\s+install\s+(.*?)(?:\n(?!\s)|\Z)", re.S)
PINNED = re.compile(r"^([A-Za-z0-9][A-Za-z0-9._-]*)==([^\s\\]+)$")
IMPORT_LINE = re.compile(r"python3\s+-c\s+'import\s+([^']+)'")


def wanted_from_deps_script(text):
    """[(import_name, pip_name)] for REQUIRED_GROUPS, read from the SSoT."""
    ns = {}
    exec(compile(text, DEPS_SCRIPT, "exec"), ns)  # noqa: S102 - our own file
    groups = ns["GROUPS"]
    out = []
    for g in REQUIRED_GROUPS:
        if g not in groups:
            raise KeyError(g)
        out.extend(groups[g][1])
    return out


def installed_from_dockerfile(text):
    """({pip_name: version_or_None}, [imported_names])."""
    pkgs = {}
    for m in PIP_BLOCK.finditer(text):
        # Stop at the first `&&`: everything after it is a VERIFICATION command
        # (`python3 -c ...`, `west --version`), not an install argument. Reading
        # past it made this gate report `west` as an unpinned package because
        # the RUN ends `&& west --version` — a false positive on the very file
        # it was written for.
        args = m.group(1).replace("\\", " ").split("&&")[0]
        for tok in args.split():
            if tok.startswith("-") or tok in ("pip3", "install"):
                continue
            pin = PINNED.match(tok)
            if pin:
                pkgs[pin.group(1)] = pin.group(2)
            elif re.match(r"^[A-Za-z0-9][A-Za-z0-9._-]*$", tok):
                pkgs[tok] = None
    imported = []
    for m in IMPORT_LINE.finditer(text):
        imported.extend(x.strip() for x in m.group(1).split(","))
    return pkgs, imported


def check(deps_text, docker_text):
    """[] when clean, else a list of human-readable problems."""
    wanted = wanted_from_deps_script(deps_text)
    pkgs, imported = installed_from_dockerfile(docker_text)
    problems = []

    lower = {k.lower(): v for k, v in pkgs.items()}
    for imp, pip in wanted:
        if pip.lower() not in lower:
            problems.append(
                f"`{pip}` is required by check-python-deps.py "
                f"({'/'.join(REQUIRED_GROUPS)}) but the image never installs it"
            )
        elif lower[pip.lower()] is None:
            problems.append(f"`{pip}` is installed UNPINNED — write `{pip}==<version>`")
        if imp not in imported:
            problems.append(
                f"`import {imp}` is missing from the Dockerfile's verification line "
                f"— an install that cannot be imported would pass image build"
            )
    return problems


def main():
    if "--selftest" in sys.argv:
        return selftest(verbose=True)
    # Always, not only behind the flag: a negative control nobody runs decays
    # into a comment.
    selftest()

    with open(DEPS_SCRIPT, encoding="utf8") as fh:
        deps_text = fh.read()
    with open(DOCKERFILE, encoding="utf8") as fh:
        docker_text = fh.read()

    problems = check(deps_text, docker_text)
    if problems:
        print("check-ci-image-python-deps: the zephyr CI image and its own "
              "checker disagree:\n", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        print(
            "\n  scripts/check-python-deps.py is the single source. The image must\n"
            "  install every module the `west` + `zephyr-build` groups demand,\n"
            "  pinned, and verify the imports in the same RUN.\n"
            "\n"
            "  Issue 0878: when it did not, every Zephyr nightly cell failed in\n"
            "  `just zephyr setup` before any build — and a lane that is\n"
            "  uniformly red cannot report a regression, which is how issue 0876\n"
            "  rode in unnoticed.\n"
            "\n"
            "  Edit ci/docker/zephyr-ros/Dockerfile, and bump the `-rN` tag in\n"
            "  .github/workflows/images.yml with it.",
            file=sys.stderr,
        )
        return 1

    n = len(wanted_from_deps_script(deps_text))
    print(f"check-ci-image-python-deps OK — the zephyr image installs and imports "
          f"all {n} module(s) its checker requires, pinned.")
    return 0


def selftest(verbose=False):
    """Prove it can fail. Runs on every invocation."""
    ok = fail = 0

    def chk(desc, cond):
        nonlocal ok, fail
        if verbose or not cond:
            print(f"  {'ok   ' if cond else 'FAIL '} {desc}")
        ok += 1 if cond else 0
        fail += 0 if cond else 1

    deps = (
        "GROUPS = {\n"
        "    'zephyr-build': ('why', [('elftools', 'pyelftools'), ('yaml', 'PyYAML')]),\n"
        "    'west': ('why', [('west', 'west')]),\n"
        "    'px4': ('why', [('jinja2', 'jinja2')]),\n"
        "}\n"
    )
    good = (
        "RUN pip3 install --no-cache-dir \\\n"
        "        west==1.2.0 \\\n"
        "        pyelftools==0.31 \\\n"
        "        PyYAML==6.0.2 \\\n"
        "    && python3 -c 'import west, elftools, yaml'\n"
        "\nENV FOO=bar\n"
    )
    chk("a complete, pinned, verified image is clean", check(deps, good) == [])
    chk("a MISSING package is caught",
        any("never installs" in p for p in check(deps, good.replace("        PyYAML==6.0.2 \\\n", ""))))
    chk("an UNPINNED package is caught",
        any("UNPINNED" in p for p in check(deps, good.replace("PyYAML==6.0.2", "PyYAML"))))
    chk("a package installed but NOT imported is caught",
        any("verification line" in p
            for p in check(deps, good.replace("import west, elftools, yaml", "import west, elftools"))))
    chk("a group the image does not need (px4) is not demanded",
        not any("jinja2" in p for p in check(deps, good)))
    chk("an image with no pip block at all fails rather than passing vacuously",
        len(check(deps, "ENV FOO=bar\n")) >= 3)
    # The real Dockerfile ends its RUN with `&& west --version`. Reading past
    # the `&&` saw a bare `west` token and called the package unpinned.
    chk("a verification command after `&&` is not read as a package",
        check(deps, good.replace("import west, elftools, yaml'",
                                 "import west, elftools, yaml' \\\n    && west --version")) == [])

    if verbose:
        print(f"\n{ok} passed, {fail} failed")
    if fail:
        print("check-ci-image-python-deps self-test: FAILED", file=sys.stderr)
        raise SystemExit(1)
    return 0


if __name__ == "__main__":
    sys.exit(main())
