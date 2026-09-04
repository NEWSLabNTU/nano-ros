#!/usr/bin/env python3
"""Report which Python packages a lane needs and does not have — never install them.

nano-ros does not provision Python. It used to: `scripts/zephyr/setup.sh` ran
`pip3 install --user`, fell back to building a venv when PEP 668 refused, and
then installed Zephyr's `requirements-base.txt` into whichever interpreter it
had ended up with. Three interpreters could be in play (system, `--user`, the
fallback venv) and the script chose between them silently, so "setup succeeded"
did not tell you which Python the build would later use — and on a host where
that guess was wrong, the failure surfaced far away, four frames inside cmake as
`Error finding board: mps2`.

Provisioning a Python environment correctly is a distro-by-distro problem:
PEP 668 externally-managed interpreters, `--user` vs venv vs pipx, a venv that
must inherit system site-packages or shadow the tree's other tools, and
`python3-venv` not being installed by default on Debian. That is the user's
environment to own. What the project CAN own is saying precisely what is
missing, for which interpreter, and what a lane will do about it.

So: this never provisions an INTERPRETER — no venv creation, no choosing
between system/`--user`/pipx on your behalf.

It will, with `--install`, install DEV PACKAGES into the interpreter it just
probed (issue 0885). The distinction is the whole point: picking the Python is
a decision about your machine and stays yours, while `towncrier` or a pinned
`clang-format` inside a Python you already chose is just a tool the repo needs
in order to work. Only groups in `INSTALLABLE` may be installed this way —
BUILD groups (zephyr, px4, …) stay report-only, because those belong to an
environment the project does not own.

Usage:

    scripts/check-python-deps.py [--python PATH] [--quiet] GROUP [GROUP ...]
    scripts/check-python-deps.py --list

Exit codes:

    0  every requested group satisfied
    1  something missing (the report names it and how to get it)
    2  usage error, or the interpreter itself is unusable
"""

import argparse
import json
import os
import subprocess
import sys

# group -> (what it is for, [(import name, pip name)])
#
# Keyed on the IMPORT name, because that is the question ("will the build's
# `import pykwalify` work"), and carries the pip name separately because the two
# differ often enough to matter (`yaml` / `PyYAML`, `elftools` / `pyelftools`).
GROUPS = {
    # DELIBERATELY SHORTER than zephyr/scripts/requirements-base.txt.
    #
    # The group answers "will `west build` work here", so a module upstream
    # lists but our lanes never reach does not belong in it — a check that
    # reports a problem the build does not have teaches people to ignore it.
    # Measured: the tree's own `scripts/zephyr/.venv` has NO `intelhex` and
    # builds every Zephyr fixture, which is issue 0078's point one level down
    # (our flows are QEMU build-only, so the hex/flash half of base.txt is
    # never imported).
    #
    # These four are the ones a missing copy has actually broken: `pykwalify`
    # took down `list_boards.py` in the ROS distrobox and surfaced as
    # `Error finding board: mps2`; the rest are imported by the dts/build
    # scripts on every board.
    "zephyr-build": (
        "`west build` for any Zephyr board (the subset of requirements-base.txt our lanes import)",
        [
            ("elftools", "pyelftools"),
            ("yaml", "PyYAML"),
            ("pykwalify", "pykwalify"),
            ("packaging", "packaging"),
            # Zephyr 4.4 validates module `zephyr/module.yml` against a JSON
            # schema during CMake configure. Absent, west stops with
            # `Missing jsonschema dependency` and the build never starts — which
            # is how the nightly's `zephyr copy-out check (4.4)` and
            # `zephyr ci-both` cells died before compiling anything. 3.7 does
            # not import it, so this group grew a member the older line does not
            # need; that is the group's own rule — it lists what OUR lanes
            # import, and one of them now imports this.
            ("jsonschema", "jsonschema"),
        ],
    ),
    "west": (
        "the west meta-tool itself (workspace init/update, `west build`)",
        [("west", "west")],
    ),
    # PX4 ships its own `Tools/setup/requirements.txt`, and that file — not this
    # list — is the authority. These three are the ones `just px4 setup`
    # documented as the reason it was installing anything: kconfiglib for the
    # menuconfig step, pyros-genmsg + jinja2 for uORB topic-header generation.
    # The remediation names the upstream file too, because a subset can pass and
    # a later import still fail; what the group buys is catching the common case
    # BEFORE a long build instead of mid-way through it.
    "px4-build": (
        "`just px4 build-sitl-cpp` (subset of PX4's Tools/setup/requirements.txt)",
        [
            ("kconfiglib", "kconfiglib"),
            ("pyros_genmsg", "pyros-genmsg"),
            ("jinja2", "jinja2"),
        ],
    ),
    "cyclone-idl": (
        "scripts/cyclonedds/msg_to_cyclone_idl.py on a host with no ROS 2",
        [
            ("catkin_pkg", "catkin_pkg"),
            ("em", "empy==3.3.4"),
            ("lark", "lark"),
        ],
    ),
    "sdk-tools": (
        "scripts/sdk/{verify-index,check-qemu-source-features}.py on Python older than 3.11 (needs tomllib)",
        [("tomllib", "tomli")],
    ),
    # issue 0885 — the DEV utilities: tools a contributor runs, not a build.
    #
    # These were previously undeclared, which is why `clang-format==17.0.5`
    # existed only as a literal inside the CI Dockerfile and a contributor's
    # host version silently reformatted the tree differently. A version that
    # matters is a version that belongs in a group where `--list` shows it.
    #
    # Still REPORT-ONLY, like every group here: this module does not provision
    # Python and says so at the top. The value is that `just dev-tools` now
    # names what is missing and the exact `pip install` for it, instead of a
    # lane failing four frames deep in a tool nobody knew was required.
    "dev-tools": (
        "`just changelog*` (towncrier) and `just format` (clang-format)",
        [
            ("towncrier", "towncrier"),
            ("clang_format", "clang-format==17.0.5"),
        ],
    ),
}

# Groups whose absence is normal on many hosts, so a bare run does not imply
# they were requested.
DEFAULT_GROUPS = ["west", "zephyr-build"]

# Groups `--install` may write. DEV tooling only: these are the repo's own
# utilities inside whatever interpreter the user has already chosen. The build
# groups are deliberately absent — a Zephyr or PX4 environment is the user's to
# assemble, and installing into it silently is how three interpreters end up in
# play with nobody knowing which one a lane will use.
INSTALLABLE = {"dev-tools"}

PROBE = r"""
import importlib, json, sys
out = {"version": list(sys.version_info[:3]), "exe": sys.executable, "have": {}}
for name in json.loads(sys.argv[1]):
    try:
        importlib.import_module(name)
        out["have"][name] = True
    except Exception:
        out["have"][name] = False
print(json.dumps(out))
"""


# Groups whose lane resolves its interpreter through the in-repo Zephyr venv.
# Everything else defaults to the ambient interpreter — a PX4 or PlatformIO
# check answered against Zephyr's venv would report on a Python that lane never
# runs, which is the same wrong-interpreter mistake one level over.
ZEPHYR_GROUPS = {"west", "zephyr-build"}


def default_python(groups):
    """The interpreter the requested lane would actually use, when none is named.

    Must match `nros_zephyr_python` in scripts/build/zephyr-python.sh — a
    checker that answers for a DIFFERENT interpreter than the lane uses reports
    problems the build does not have, which is how a check earns being ignored.
    Two spellings of one candidate order, the way the riscv64 resolver carries
    three (shell/cmake/rust); keep them in step.

        NROS_PYTHON            the one knob
        scripts/zephyr/.venv   the conventional in-repo venv, used only when it
                               can actually import west — never on presence,
                               because a venv copied between hosts passes `-x`
                               and still cannot import its own packages
        this interpreter       whatever is running us
    """
    named = os.environ.get("NROS_PYTHON")
    if named:
        return named
    if not (set(groups) & ZEPHYR_GROUPS):
        return sys.executable
    repo = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    venv = os.path.join(repo, "scripts", "zephyr", ".venv", "bin", "python3")
    if os.access(venv, os.X_OK):
        try:
            if subprocess.run(
                [venv, "-c", "import west"], capture_output=True, timeout=60
            ).returncode == 0:
                return venv
        except (OSError, subprocess.SubprocessError):
            pass
    return sys.executable


def probe(python, modules):
    """Ask the TARGET interpreter, not this one.

    The interpreter that runs this script is not necessarily the one a lane will
    use — the Zephyr 4.4 line has its own `.venv312`, a container has its own
    system python, and `--python` is how a caller says which one the answer is
    about. Importing here would answer the wrong question convincingly.
    """
    try:
        res = subprocess.run(
            [python, "-c", PROBE, json.dumps(sorted(modules))],
            capture_output=True,
            text=True,
            timeout=120,
        )
    except (OSError, subprocess.SubprocessError) as e:
        return None, f"cannot run {python}: {e}"
    if res.returncode != 0:
        return None, f"{python} exited {res.returncode}: {res.stderr.strip()[:200]}"
    try:
        return json.loads(res.stdout), None
    except json.JSONDecodeError:
        return None, f"{python} produced no usable output: {res.stdout.strip()[:200]}"


def main():
    ap = argparse.ArgumentParser(add_help=True)
    ap.add_argument("groups", nargs="*", default=None)
    ap.add_argument("--python", default=None)
    ap.add_argument("--list", action="store_true", help="print the known groups and exit")
    ap.add_argument("--quiet", action="store_true", help="print only when something is missing")
    ap.add_argument(
        "--install", action="store_true",
        help="install what is missing INTO the probed interpreter (dev groups only)",
    )
    args = ap.parse_args()

    if args.list:
        for name, (why, mods) in GROUPS.items():
            print(f"{name}\n    {why}\n    {' '.join(m for _, m in mods)}")
        return 0

    groups = args.groups or DEFAULT_GROUPS
    python = args.python or default_python(groups)
    unknown = [g for g in groups if g not in GROUPS]
    if unknown:
        sys.stderr.write(
            f"check-python-deps: unknown group(s): {', '.join(unknown)}\n"
            f"  known: {', '.join(GROUPS)}\n"
        )
        return 2

    # Refuse UP FRONT, before probing. Deciding this inside the "something is
    # missing" branch made the answer depend on the host: `--install
    # zephyr-build` printed OK on a machine that happened to have those
    # packages, and refused on one that did not. A permission question must not
    # have two answers.
    if args.install:
        refused = [g for g in groups if g not in INSTALLABLE]
        if refused:
            sys.stderr.write(
                f"check-python-deps: --install refused for {', '.join(refused)}.\n"
                f"  Installable groups: {', '.join(sorted(INSTALLABLE))}\n"
                "  A build environment (zephyr, px4, …) is yours to assemble; this\n"
                "  tool installs only the repo's OWN dev utilities.\n"
            )
            return 2

    wanted = {imp: pipname for g in groups for imp, pipname in GROUPS[g][1]}
    info, err = probe(python, wanted)
    if info is None:
        sys.stderr.write(f"check-python-deps: {err}\n")
        return 2

    ver = ".".join(str(p) for p in info["version"])
    missing = sorted(imp for imp, ok in info["have"].items() if not ok)

    # `tomllib` is stdlib from 3.11, so on a modern interpreter the sdk-tools
    # group is satisfied by the version rather than by a package.
    if "tomllib" in missing and tuple(info["version"][:2]) >= (3, 11):
        missing.remove("tomllib")

    if not missing:
        if not args.quiet:
            print(
                f"python-deps: OK — {info['exe']} (Python {ver}) has "
                f"{', '.join(groups)}"
            )
        return 0

    if args.install:
        pkgs = sorted(set(wanted[m] for m in missing))
        print(
            f"python-deps: installing {', '.join(pkgs)}\n"
            f"  into: {info['exe']} (Python {ver})"
        )
        # `--only-binary :none:` is NOT passed and no index is pinned: this is a
        # plain pip into the interpreter the caller already chose. If that
        # interpreter is PEP 668 externally-managed, pip refuses and says so far
        # better than a pre-flight guess could — so let it, and translate.
        rc = subprocess.run(
            [info["exe"], "-m", "pip", "install", *pkgs]
        ).returncode
        if rc != 0:
            sys.stderr.write(
                "\npython-deps: pip declined.\n"
                "  On a PEP 668 host (Arch, Fedora, Debian 12+) the system interpreter\n"
                "  is externally managed and will refuse. That is not a bug to work\n"
                "  around silently — pick an interpreter you own and re-run:\n\n"
                "      python3 -m venv --system-site-packages .venv\n"
                "      . .venv/bin/activate && just dev-tools --install\n\n"
                "  or install for your user only:\n\n"
                f"      {info['exe']} -m pip install --user {' '.join(pkgs)}\n"
            )
            return 1
        # Re-probe rather than assume: a pip that exits 0 has still been seen to
        # leave an import failing (wrong interpreter, --user vs venv shadowing).
        info2, err2 = probe(python, wanted)
        if info2 is None:
            sys.stderr.write(f"check-python-deps: re-probe failed: {err2}\n")
            return 2
        still = sorted(i for i, ok in info2["have"].items() if not ok)
        if "tomllib" in still and tuple(info2["version"][:2]) >= (3, 11):
            still.remove("tomllib")
        if still:
            sys.stderr.write(
                "\npython-deps: pip reported success but these still do not import:\n"
                f"      {', '.join(still)}\n"
                f"  interpreter: {info2['exe']}\n"
                "  Usually a second interpreter is shadowing this one on PATH.\n"
            )
            return 1
        print(f"python-deps: OK — {', '.join(groups)} now satisfied")
        return 0

    sys.stderr.write(
        f"python-deps: MISSING for {', '.join(groups)}\n"
        f"  interpreter: {info['exe']} (Python {ver})\n"
    )
    for g in groups:
        why, mods = GROUPS[g]
        gone = [(i, p) for i, p in mods if i in missing]
        if gone:
            sys.stderr.write(f"  [{g}] {why}\n")
            for imp, pipname in gone:
                sys.stderr.write(f"      import {imp:<12} (pip: {pipname})\n")
    sys.stderr.write(
        "\n  nano-ros does not install these: choosing between a distro package,\n"
        "  `pip --user`, and a venv is a decision about YOUR interpreter, and on a\n"
        "  PEP 668 host (Arch, Fedora, Debian 12+) pip refuses `--user` outright.\n"
        "  Any of these works; use whichever suits the host:\n\n"
        f"      <distro pkg manager>   e.g. python-pykwalify, python-pyelftools\n"
        f"      pip install --user     {' '.join(sorted(set(wanted[m] for m in missing)))}\n"
        "      python3 -m venv --system-site-packages .venv && . .venv/bin/activate\n"
        f"                             && pip install {' '.join(sorted(set(wanted[m] for m in missing)))}\n\n"
        "  Then point the lane at that interpreter if it is not the default:\n"
        "      NROS_PYTHON=/path/to/python3\n"
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
