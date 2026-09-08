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

import json
import subprocess
import sys

PACKAGE = "nros-rmw-xrce-cffi"


def main() -> int:
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
    out_dir = None
    for line in proc.stdout.splitlines():
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue
        if msg.get("reason") != "build-script-executed":
            continue
        if PACKAGE + "#" not in msg.get("package_id", ""):
            continue
        out_dir = msg.get("out_dir")
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
