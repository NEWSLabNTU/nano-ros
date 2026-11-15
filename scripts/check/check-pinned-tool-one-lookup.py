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

Run: python3 scripts/check/check-pinned-tool-one-lookup.py [--self-test]
"""

import re
import subprocess
import sys

LOOKUP = re.compile(r"\bsdk[- ]path\s+(make|ninja)\b")
HELPERS = {"scripts/build/jobserver-pool.sh", "scripts/lib/sdk-tool-path.sh"}


def problems(name, text):
    if name in HELPERS:
        return []
    out = []
    for n, line in enumerate(text.splitlines(), 1):
        if line.lstrip().startswith("#"):
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
    ]
    bad = 0
    for desc, text, want in cases:
        got = bool(problems("x.sh", text))
        print(f"  {'ok  ' if got == want else 'FAIL'} {desc}: {'refused' if got else 'accepted'}")
        bad += got != want
    if problems("scripts/build/jobserver-pool.sh", cases[0][1]):
        print("  FAIL the helper itself is refused")
        bad += 1
    return 1 if bad else 0


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
