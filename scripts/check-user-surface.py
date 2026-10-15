#!/usr/bin/env python3
"""phase-492 / phase-499 — user-facing C and C++ use only upstream's surface.

Maintainer ruling (2026-10-13, RFC-0089 amendment "the user surface is
upstream's"): `<nros/*.hpp>` is INTERNAL (to be renamed into `rclcpp` later),
and since phase-501 the C API's internal headers are `<rclc/detail/*.h>`. A C user includes `<rclc/rclc.h>`, `<rcl/rcl.h>`,
`<rclc/executor.h>` ...; a C++ user `<rclcpp/rclcpp.hpp>`. Each umbrella
carries the nano-ros-only parts too. And the program writes `main`: the build
renames it and reaches it from the platform's entry, so no user file names
the internal link contract `nros_app_main` or the `NROS_APP_MAIN_REGISTER()`
shim.

Subject: what a user reads and copies —
  * C and C++ sources under `examples/` (copy-out templates);
  * ```c / ```cpp code blocks in `book/src/`, except `book/src/porting/`,
    which is written for port implementers.

Rules:
  1. No include of a header that lives in `packages/api/nros-c/include/rclc/detail/`,
     `packages/api/nros-c/include/nros/` (the generated headers, until
     phase-501 W2) or `packages/api/nros-cpp/include/nros/`. The set is DERIVED
     from those directories, so a port-seam header (`<nros/platform.h>`,
     `<nros/rmw_vtable.h>`, `<nros/transport.h>`, owned by other packages) is
     outside the rule by construction, not by an allowlist.
  2. No `nros_app_main` / `NROS_APP_MAIN_REGISTER*` in CODE (comments are
     stripped: a note explaining the mechanism is not a use).

The `nros new` scaffolds are held by their own tests (`cargo-nano-ros`).

Run:  python3 scripts/check-user-surface.py
Gate: just check user-surface
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
# (directory, the include spelling its headers are reached by)
INTERNAL_DIRS = (
    ("packages/api/nros-c/include/rclc/detail/", "rclc/detail/"),
    ("packages/api/nros-c/include/nros/", "nros/"),
    ("packages/api/nros-cpp/include/nros/", "nros/"),
)
INCLUDE = re.compile(r'^\s*#\s*include\s*[<"]((?:nros|rclc)/[A-Za-z0-9_./-]+\.hp?p?)[>"]', re.M)
ENTRY = re.compile(r"\b(nros_app_main|NROS_APP_MAIN_REGISTER\w*)\b")
FENCE = re.compile(r"^```(?:c|cpp|c\+\+)\s*$(.*?)^```\s*$", re.M | re.S)
COMMENT = re.compile(r"/\*.*?\*/|//[^\n]*", re.S)
SOURCES = ("examples/*.c", "examples/*.h", "examples/*.cpp", "examples/*.hpp", "examples/*.cc")


def git_ls(*specs: str) -> list[str]:
    out = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "--", *specs],
        capture_output=True, text=True, check=True,
    ).stdout
    return [line for line in out.splitlines() if line]


def internal_headers() -> set[str]:
    found: set[str] = set()
    for d, spelling in INTERNAL_DIRS:
        found |= {spelling + p[len(d):] for p in git_ls(d + "*.h", d + "*.hpp")}
    return found


def violations(texts: dict[str, str], internal: set[str]) -> list[str]:
    found = []
    for path, text in sorted(texts.items()):
        bodies = [m.group(1) for m in FENCE.finditer(text)] if path.endswith(".md") else [text]
        for body in bodies:
            for m in INCLUDE.finditer(body):
                if m.group(1) in internal:
                    found.append(f"{path}: #include <{m.group(1)}>")
            for name in sorted(set(ENTRY.findall(COMMENT.sub(" ", body)))):
                found.append(f"{path}: names {name} (write `main`)")
    return found


def self_test() -> bool:
    internal = {"rclc/detail/app_main.h", "nros/nros_generated.h", "nros/nros.hpp"}
    texts = {
        "examples/a.c": "#include <rclc/rclc.h>\n#include <nros/platform.h>\n// nros_app_main is internal\n",
        "examples/b.c": "#include <rclc/component.h>\n#include <nros/nros_generated.h>\n"
                        "int nros_app_main(int c, char** v);\n",
        "book/src/x.md": "text <rclc/detail/app_main.h>\n```c\n#include <rclc/detail/app_main.h>\n```\n",
        "book/src/y.md": "```cpp\n#include <nros/nros.hpp>\nNROS_APP_MAIN_REGISTER()\n```\n",
    }
    got = violations(texts, internal)
    want = [
        "book/src/x.md: #include <rclc/detail/app_main.h>",
        "book/src/y.md: #include <nros/nros.hpp>",
        "book/src/y.md: names NROS_APP_MAIN_REGISTER (write `main`)",
        "examples/b.c: #include <nros/nros_generated.h>",
        "examples/b.c: names nros_app_main (write `main`)",
    ]
    if got != want:
        print(f"check-user-surface: SELFTEST FAILED:\n  got  {got}\n  want {want}", file=sys.stderr)
        return False
    return True


def main() -> int:
    if not self_test():
        return 1
    internal = internal_headers()
    if len(internal) < 60:
        print(f"check-user-surface: only {len(internal)} internal header(s) — the listing "
              f"broke; refusing a vacuous pass", file=sys.stderr)
        return 1
    paths = git_ls(*SOURCES, "book/src/*.md")
    paths = [p for p in paths if not p.startswith("book/src/porting/")]
    texts = {p: (ROOT / p).read_text(errors="replace") for p in paths}
    bad = violations(texts, internal)
    if bad:
        print("check-user-surface: FAIL — user-facing C/C++ reaches the internal surface.\n"
              "  Include the API at upstream's paths (<rclc/rclc.h>, <rcl/rcl.h>, "
              "<rclcpp/rclcpp.hpp>, ...) and write `main`.\n  "
              + "\n  ".join(bad), file=sys.stderr)
        return 1
    print(f"check-user-surface: OK — {len(paths)} user-facing file(s) use only the upstream "
          f"surface ({len(internal)} internal headers)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
