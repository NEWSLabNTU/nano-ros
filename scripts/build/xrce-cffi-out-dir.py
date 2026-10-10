#!/usr/bin/env python3
"""Build `nros-rmw-xrce-cffi` and print the OUT_DIR its build script ran in.

Issue 1238 — `packages/rmw/xrce/nros-rmw-xrce/CMakeLists.txt` links the
vendored archive that cargo lane builds instead of compiling the sources a
second time (phase-420 W9 step 4), so a configure needs a path that only cargo
knows: the OUT_DIR is fingerprint-named, and the fingerprint moves whenever
anything in the crate's dependency closure changes.

ONE spelling of that lookup, because there were two and neither ran: the
CMakeLists prints a by-hand pipeline in its FATAL_ERROR and `just check
rmw-xrce` passed no `-D` at all, so the lane could only pass against a CMake
cache some earlier hand-run had primed -- and a stale cache names a directory
cargo has since replaced, which is the same error one line down.

Prints the directory on stdout. Everything else goes to stderr so the caller
can use `$(...)` directly.
"""

import os
import subprocess
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))
# phase-484 W6 (RFC-0103 D8) — the one reader of cargo's JSON stream (it
# matches the package by NAME; the `<name>#` substring this used to test also
# matched any package whose name ended in it).
from cargo_out_dir import out_dirs, self_test  # noqa: E402

PACKAGE = "nros-rmw-xrce-cffi"


def main() -> int:
    self_test()
    cmd = [
        "cargo",
        "build",
        "-p",
        PACKAGE,
        "--message-format=json-render-diagnostics",
    ] + sys.argv[1:]
    proc = subprocess.run(cmd, capture_output=True, text=True)
    sys.stderr.write(proc.stderr)
    if proc.returncode != 0:
        return proc.returncode

    # `build-script-executed` is emitted for a FRESH unit too -- cargo replays
    # the recorded output rather than re-running the script -- so this does not
    # depend on the crate having been rebuilt by this invocation.
    found = out_dirs(proc.stdout.splitlines(), PACKAGE)
    out_dir = found[-1] if found else None
    if not out_dir:
        sys.stderr.write(
            "xrce-cffi-out-dir: cargo built %s and emitted no `build-script-executed`\n"
            "  message for it. Nothing can be linked without the OUT_DIR, so this is a\n"
            "  hard failure rather than a guessed path (issue 1238).\n" % PACKAGE
        )
        return 1
    print(out_dir)
    return 0


if __name__ == "__main__":
    sys.exit(main())
