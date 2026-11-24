#!/usr/bin/env python3
"""The pinned make and ninja have ONE lookup (phase-489 W13).

`nros_pinned_make` / `nros_pinned_ninja` (`scripts/build/jobserver-pool.sh`)
answer "where is the pinned make / ninja": the store path (`nros dev sdk path`,
issue 0625), else a copy that passes the index row's smoke probe (the CI
images bake both under `/usr/local`, outside any store). A PATH prefix for
several tools goes through `nros_sdk_tool_path_prefix`
(`scripts/lib/sdk-tool-path.sh`), which asks the same two questions.

When W13 added the second answer, 20 sites in 11 files spelled the first one
themselves (`$(nros dev sdk path make)/bin/make`), so a baked make would have
been invisible to every one of them: the fixture drivers would have kept their
serial fallback in exactly the images that had the tool. This refuses a new
one.

Rule: no tracked shell or `just` file outside those two helpers names
`sdk path make` or `sdk path ninja`.

Rule 2 (run-matrix 45351389552, 2026-11-24): the make a CONFIGURE caches comes
from `nros_configure_make`, never a hand-spelled `nros_pinned_make || command -v
make`. Zephyr's native_sim runs that make by its cached ABSOLUTE path under
the fifo pool, and the silent fallback to a pre-4.4 make is what died
`invalid --jobserver-auth string`. The helper refuses that fallback when a
fifo jobserver is in MAKEFLAGS; the self-test drives it with fake makes.

Rule 3: a file that configures a Zephyr image (it names
`nros_zephyr_module_cmake_arg`, the one spelling of the module flag every
configure passes) also passes `-DMAKE=`. `west-fixtures.sh` passed none, which
is how its nine native_sim dirs cached /usr/bin/make. File-level on purpose:
the module flag and `-DMAKE=` sit in the same argument list, and a finer
reading would need a shell parser.

Run: python3 scripts/check/check-pinned-tool-one-lookup.py [--self-test]
"""

import re
import subprocess
import sys

LOOKUP = re.compile(r"\bsdk[- ]path\s+(make|ninja)\b")
FALLBACK = re.compile(r"nros_pinned_make\)\"?\s*\|\|\s*\w+=\"?\$\(command -v make")
HELPERS = {"scripts/build/jobserver-pool.sh", "scripts/lib/sdk-tool-path.sh"}


def problems(name, text):
    if name in HELPERS:
        return []
    out = []
    if "nros_zephyr_module_cmake_arg" in text and "-DMAKE=" not in text \
            and name != "scripts/lib/zephyr-module.sh":
        out.append(f"{name}: configures a Zephyr image (nros_zephyr_module_cmake_arg) and passes no "
                   "-DMAKE=; Zephyr's native_sim runs the make it caches by absolute path, so pass "
                   "-DMAKE=\"$(nros_configure_make)\"")
    for n, line in enumerate(text.splitlines(), 1):
        if line.lstrip().startswith("#"):
            continue
        if FALLBACK.search(line):
            out.append(f"{name}:{n}: a hand-spelled pinned-or-PATH make; use nros_configure_make "
                       "(it refuses a pre-4.4 make under a fifo jobserver)")
            continue
        if LOOKUP.search(line):
            out.append(f"{name}:{n}: {line.strip()}")
    return out


def self_test():
    cases = [
        ("a hand-built store path", 'make_bin="$(nros dev sdk path make)/bin/make"\n', True),
        ("a ninja PATH prefix", 'export PATH="$(nros dev sdk path ninja)/bin:$PATH"\n', True),
        ("the helper", 'make_bin="$(nros_pinned_make)"\n', False),
        ("a comment", "# was $(nros dev sdk path make)\n", False),
        ("another tool", 'd="$(nros dev sdk path qemu)"\n', False),
        ("a hand-spelled fallback", 'make_bin="$(nros_pinned_make)" || make_bin="$(command -v make)"\n', True),
        ("the configure helper", 'make_bin="$(nros_configure_make)"\n', False),
        ("a Zephyr configure with no -DMAKE", 'args+=("$(nros_zephyr_module_cmake_arg "$r")")\n', True),
        ("a Zephyr configure with -DMAKE", 'args+=("$(nros_zephyr_module_cmake_arg "$r")" "-DMAKE=$m")\n', False),
    ]
    bad = 0
    for desc, text, want in cases:
        got = bool(problems("x.sh", text))
        print(f"  {'ok  ' if got == want else 'FAIL'} {desc}: {'refused' if got else 'accepted'}")
        bad += got != want
    if problems("scripts/build/jobserver-pool.sh", cases[0][1]):
        print("  FAIL the helper itself is refused")
        bad += 1
    bad += drive_configure_make()
    return 1 if bad else 0


def drive_configure_make():
    """Drive nros_configure_make with fake makes: no pinned make, a 4.3 on PATH."""
    import os
    import tempfile
    bad = 0
    with tempfile.TemporaryDirectory() as tmp:
        for name, ver in (("old", "4.3"), ("new", "4.4.1")):
            d = os.path.join(tmp, name)
            os.mkdir(d)
            with open(os.path.join(d, "make"), "w") as f:
                f.write(f"#!/bin/sh\necho 'GNU Make {ver}'\n")
            os.chmod(os.path.join(d, "make"), 0o755)
        fifo = os.path.join(tmp, "fifo")
        os.mkfifo(fifo)
        script = ('source scripts/build/jobserver-pool.sh; nros_pinned_make() { return 1; }; '
                  'nros_configure_make')
        cases = [
            ("4.3 on PATH, no jobserver: used", "old", "", True),
            ("4.3 on PATH, fifo jobserver: REFUSED", "old", f"-j4 --jobserver-auth=fifo:{fifo}", False),
            ("4.4 on PATH, fifo jobserver: used", "new", f"-j4 --jobserver-auth=fifo:{fifo}", True),
        ]
        for desc, which, flags, want_ok in cases:
            env = dict(os.environ, PATH=os.path.join(tmp, which) + os.pathsep + "/usr/bin:/bin",
                       MAKEFLAGS=flags)
            r = subprocess.run(["bash", "-c", script], env=env, capture_output=True, text=True)
            ok = r.returncode == 0 and r.stdout.strip().endswith(f"{which}/make")
            print(f"  {'ok  ' if ok == want_ok else 'FAIL'} nros_configure_make: {desc}")
            bad += ok != want_ok
    return bad


def main(argv):
    if argv == ["--self-test"]:
        return self_test()
    import contextlib
    import io
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        rc = self_test()
    if rc:
        sys.stderr.write(buf.getvalue())
        print("check-pinned-tool-one-lookup self-test: FAILED", file=sys.stderr)
        return 1
    files = subprocess.run(
        ["git", "ls-files", "-z", "*.sh", "*.just", "justfile"],
        capture_output=True, check=True,
    ).stdout.decode().split("\0")
    found = []
    for f in filter(None, files):
        try:
            found += problems(f, open(f, encoding="utf-8").read())
        except (FileNotFoundError, UnicodeDecodeError):
            continue
    if found:
        print("check-pinned-tool-one-lookup: FAILED", file=sys.stderr)
        for p in found:
            print("  " + p, file=sys.stderr)
        print("\nUse nros_pinned_make / nros_pinned_ninja (scripts/build/jobserver-pool.sh),\n"
              "or nros_sdk_tool_path_prefix make ninja (scripts/lib/sdk-tool-path.sh) for a PATH prefix:\n"
              "they also accept a baked copy that passes the index smoke probe (phase-489 W13).",
              file=sys.stderr)
        return 1
    print("check-pinned-tool-one-lookup: OK (the pinned make and ninja have one lookup)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
