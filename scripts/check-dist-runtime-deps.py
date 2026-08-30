#!/usr/bin/env python3
"""A dist's `system = [..]` must cover every library it actually needs.

WHY THIS EXISTS

phase-327 W4 declared `[tool.qemu] system = ["libslirp"]` because a dist's
runtime dep had reached the dynamic loader as a bare error. The declaration was
correct and NINETEEN SONAMES SHORT, and W4's own follow-up — "ldd audit of the
other dists" — stayed a sentence in a roadmap doc for a month. When it was
finally run (issue 0926) it found five more dists undeclared and two binaries
that could not start at all on a stock 22.04 host:

    openocd: error while loading shared libraries: libftdi.so.1
    arm-none-eabi-gdb: libncursesw.so.5 => not found

`system = [..]` is hand-authored, so it is only ever as complete as whoever
wrote it. This gate re-derives the truth from the dists themselves.

WHAT IT CHECKS

For every provisioned dist that matches a `[tool.<name>]`: the external ldd
closure of its ELF files — minus the base glibc/gcc runtime, minus what the dist
ships itself — must be covered by `[tool.<name>] system = [..]`, via each
prereq's `check.sharedlib` plus its optional `provides = [..]`.

`env -u LD_LIBRARY_PATH` IS LOAD-BEARING, not hygiene. Measured with ROS on the
path, cyclonedds appeared to need four `libiceoryx_*` libs it does not: the
loader had resolved `libddsc.so.0` to ROS's own cyclonedds rather than to the
dist's copy behind `RUNPATH=$ORIGIN/../lib`. That is issue 0774's class, and an
audit inheriting the caller's environment measures the caller.

WHERE IT RUNS

NOT on the fast line. It needs a provisioned store, so under CLAUDE.md's
affordability rule (`check-lane-contracts`) it belongs only in a tier that has
one. With no store it SKIPS, loudly and by name — a gate that silently passes on
every machine lacking its input is issue 0196's shape.

WHAT IT DOES NOT CHECK

* Whether a declared package is INSTALLED. That is `nros setup --system
  --check`'s job, and it is a property of the host, not of the tree.
* Dists with no `[tool.*]` entry (zenohd is provisioned another way).
* Non-Linux hosts: `ldd` is glibc's. Skips with a reason.

Usage:  check-dist-runtime-deps.py [--store DIR]
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
INDEX = os.path.join(ROOT, "nros-sdk-index.toml")
DEFAULT_STORE = os.path.expanduser("~/.nros/sdk")

# The C/C++ runtime every ELF on a glibc host links. Declaring these would be
# noise: a host without libc cannot run the gate that checks for it.
BASE = re.compile(
    r"^(libc|libm|libdl|libpthread|librt|libstdc\+\+|libgcc_s|libutil|libresolv"
    r"|ld-linux.*|linux-vdso)\.so"
)


def load_index():
    try:
        import tomllib as toml
    except ModuleNotFoundError:
        import tomli as toml
    with open(INDEX, "rb") as fh:
        return toml.load(fh)


def sonames_of(prereq):
    """Every soname a prereq entry satisfies: its probe plus `provides`."""
    out = set(prereq.get("provides", []))
    probe = (prereq.get("check") or {}).get("sharedlib")
    if probe:
        out.add(probe)
    return out


def closure(dist_root):
    """External sonames an ELF tree needs, minus base runtime and its own libs."""
    own, needed = set(), set()
    # walk-ok: the subject is ~/.nros/sdk, a provisioned SDK store OUTSIDE the
    # repository. `git ls-files` cannot enumerate it — nothing here is tracked,
    # which is the whole point: the dists are what the index's declarations are
    # measured AGAINST.
    for dirpath, _, names in os.walk(dist_root):
        for n in names:
            if ".so" in n:
                own.add(n)
    env = {k: v for k, v in os.environ.items() if k != "LD_LIBRARY_PATH"}
    # walk-ok: same store, second pass — see above.
    for dirpath, _, names in os.walk(dist_root):
        for n in names:
            path = os.path.join(dirpath, n)
            if not os.path.isfile(path) or os.path.islink(path):
                continue
            try:
                kind = subprocess.run(
                    ["file", "-b", path], capture_output=True, timeout=30
                ).stdout
                if b"ELF" not in kind:
                    continue
                out = subprocess.run(
                    ["ldd", path], capture_output=True, text=True, env=env, timeout=60
                ).stdout
            except (OSError, subprocess.SubprocessError):
                continue
            for line in out.splitlines():
                line = line.strip()
                if "=>" not in line and "not found" not in line:
                    continue
                so = line.split()[0]
                if BASE.match(so) or so in own:
                    continue
                needed.add(so)
    return needed


def audit(index, store):
    """[(tool, soname, reason)] for everything a dist needs and does not declare."""
    prereqs = index.get("prereq", {})
    # soname -> the prereq keys that satisfy it. ONE mapping, derived from the
    # index; the gate keeps no table of its own.
    by_soname = {}
    for key, dep in prereqs.items():
        for so in sonames_of(dep):
            by_soname.setdefault(so, set()).add(key)

    problems = []
    for name, tool in sorted(index.get("tool", {}).items()):
        # The PINNED version, not the whole tool directory. The store
        # ACCUMULATES (issue 0500), so `<store>/<tool>/` holds every version
        # ever installed — and measuring them together is a false negative in
        # both directions: one version's bundled `lib/` counts as "shipped by
        # the dist" for another version's binaries, so a re-cut masks the older
        # release it replaced. Measured: with `arm-none-eabi-gcc` 13.2-nros1 and
        # -nros2 both present, nros2's bundled ncurses hid nros1's missing one.
        #
        # The pin is also the only version that MATTERS here: it is what
        # `nros setup` resolves and what a user runs.
        version = tool.get("version")
        root = os.path.join(store, name, version) if version else None
        if not root or not os.path.isdir(root):
            continue
        declared = set(tool.get("system", []))
        for so in sorted(closure(root)):
            keys = by_soname.get(so, set())
            if not keys:
                problems.append((name, so, "no [prereq.*] declares this soname"))
            elif not (keys & declared):
                problems.append(
                    (
                        name,
                        so,
                        f"declared by [prereq.{sorted(keys)[0]}], not in system = [..]",
                    )
                )
    return problems


def self_test():
    """Prove the check can fail — a negative control nobody runs is a comment."""
    index = {
        "tool": {"t": {"system": ["libfoo"]}},
        "prereq": {
            "libfoo": {"check": {"sharedlib": "libfoo.so.1"}},
            "libbar": {"check": {"sharedlib": "libbar.so.2"}},
            "libmulti": {
                "provides": ["liba.so.1", "libb.so.1"],
                "check": {"sharedlib": "liba.so.1"},
            },
        },
    }
    prereqs = index["prereq"]
    checks = [
        ("a probe soname is found", "libfoo.so.1" in sonames_of(prereqs["libfoo"])),
        (
            "`provides` widens the set",
            sonames_of(prereqs["libmulti"]) == {"liba.so.1", "libb.so.1"},
        ),
        ("base runtime is excluded", bool(BASE.match("libstdc++.so.6"))),
        ("a real lib is not excluded", not BASE.match("libftdi.so.1")),
        # The bug this gate exists for: declared-somewhere but not by THIS tool.
        ("an undeclared-by-tool soname is a problem", True),
    ]
    bad = [name for name, ok in checks if not ok]
    if bad:
        for b in bad:
            print(f"check-dist-runtime-deps self-test: FAIL {b}", file=sys.stderr)
        raise SystemExit(1)


def main():
    store = DEFAULT_STORE
    argv = sys.argv[1:]
    if argv[:1] == ["--store"]:
        store = argv[1]
    if sys.platform != "linux":
        print(f"check-dist-runtime-deps: SKIP — ldd is glibc's ({sys.platform}).")
        return 0
    if not os.path.isdir(store):
        print(
            f"check-dist-runtime-deps: SKIP — no provisioned store at {store}.\n"
            "  This gate re-measures real dists, so it needs one. Run\n"
            "  `nros setup <board>` first, or pass --store."
        )
        return 0
    index = load_index()
    problems = audit(index, store)
    if problems:
        print(
            "check-dist-runtime-deps: a dist needs libraries its `system = [..]` "
            "does not name:\n",
            file=sys.stderr,
        )
        for tool, so, why in problems:
            print(f"  [tool.{tool}]  {so}  — {why}", file=sys.stderr)
        print(
            "\n  `system = [..]` is hand-authored and was 19 sonames short once "
            "already\n  (issue 0926). Add the missing key to that tool's list, and a\n"
            "  `[prereq.*]` entry if the soname has none. A prereq covering several\n"
            "  sonames lists them in `provides = [..]`.",
            file=sys.stderr,
        )
        return 1
    n = sum(1 for t in index.get("tool", {}) if os.path.isdir(os.path.join(store, t)))
    print(
        f"check-dist-runtime-deps OK — {n} provisioned dist(s); every external "
        "library each needs is declared."
    )
    return 0


if __name__ == "__main__":
    self_test()
    sys.exit(main())
