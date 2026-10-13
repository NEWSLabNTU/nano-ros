#!/usr/bin/env python3
"""phase-492 — user-facing C includes the C API only at upstream's paths.

Maintainer ruling (2026-10-13): `<nros/*.h>` from `nros-c` is INTERNAL (it
will be renamed into `rclc` later). A C user writes `<rclc/rclc.h>`,
`<rcl/rcl.h>`, `<rclc/executor.h>` ... so code ports from rclc / micro-ROS
unchanged. Every upstream-path header brings in the whole API, including the
nano-ros-only parts (`RCLC_C_COMPONENT`, `RCLC_CHECK_RET`, `RCLC_LOG_*`, the
`nros_app_main` entry hook).

Subject: what a user reads and copies —
  * C sources under `examples/` (copy-out templates);
  * ```c code blocks in `book/src/`, except `book/src/porting/`, which is
    written for port implementers.

Rule: none of them includes a header that lives in
`packages/api/nros-c/include/nros/`. The set is DERIVED from that directory,
so a port-seam header (`<nros/platform.h>`, `<nros/rmw_vtable.h>`,
`<nros/transport.h>` — owned by `nros-platform-api`, `nros-rmw-abi`,
`nros-board-cffi`) is outside the rule by construction, not by an allowlist.

The `nros new` C scaffolds are held by their own tests
(`cargo-nano-ros` integration + scaffold unit tests).

Run:  python3 scripts/check-c-user-includes.py
Gate: just check c-user-includes
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INTERNAL_DIR = "packages/api/nros-c/include/nros/"
INCLUDE = re.compile(r'^\s*#\s*include\s*[<"]nros/([A-Za-z0-9_./-]+\.h)[>"]', re.M)
FENCE = re.compile(r"^```c\s*$(.*?)^```\s*$", re.M | re.S)


def git_ls(*specs: str) -> list[str]:
    out = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "--", *specs],
        capture_output=True, text=True, check=True,
    ).stdout
    return [line for line in out.splitlines() if line]


def internal_headers() -> set[str]:
    return {p[len(INTERNAL_DIR):] for p in git_ls(INTERNAL_DIR + "*.h")}


def violations(texts: dict[str, str], internal: set[str]) -> list[str]:
    found = []
    for path, text in sorted(texts.items()):
        bodies = [m.group(1) for m in FENCE.finditer(text)] if path.endswith(".md") else [text]
        for body in bodies:
            for m in INCLUDE.finditer(body):
                if m.group(1) in internal:
                    found.append(f"{path}: #include <nros/{m.group(1)}>")
    return found


def self_test() -> bool:
    internal = {"app_main.h", "component.h"}
    texts = {
        "examples/a.c": "#include <rclc/rclc.h>\n#include <nros/platform.h>\n",
        "examples/b.c": "#include <nros/component.h>\n",
        "book/src/x.md": "text <nros/app_main.h>\n```c\n#include <nros/app_main.h>\n```\n",
        "book/src/y.md": "```cpp\n#include <nros/app_main.h>\n```\n",
    }
    got = violations(texts, internal)
    ok = got == ["book/src/x.md: #include <nros/app_main.h>", "examples/b.c: #include <nros/component.h>"]
    if not ok:
        print(f"check-c-user-includes: SELFTEST FAILED: {got}", file=sys.stderr)
    return ok


def main() -> int:
    if not self_test():
        return 1
    internal = internal_headers()
    if len(internal) < 20:
        print(f"check-c-user-includes: only {len(internal)} internal header(s) under "
              f"{INTERNAL_DIR} — the listing broke; refusing a vacuous pass", file=sys.stderr)
        return 1
    paths = git_ls("examples/*.c", "examples/*.h", "book/src/*.md")
    paths = [p for p in paths if not p.startswith("book/src/porting/")]
    texts = {p: (ROOT / p).read_text(errors="replace") for p in paths}
    bad = violations(texts, internal)
    if bad:
        print("check-c-user-includes: FAIL — user-facing C includes an internal nros-c header.\n"
              "  Include the API at upstream's paths (<rclc/rclc.h>, <rcl/rcl.h>, "
              "<rclc/executor.h>, ...); every one brings the whole API.\n  "
              + "\n  ".join(bad), file=sys.stderr)
        return 1
    print(f"check-c-user-includes: OK — {len(paths)} user-facing file(s) include no internal "
          f"nros-c header ({len(internal)} internal)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
