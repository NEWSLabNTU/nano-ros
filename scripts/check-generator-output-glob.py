#!/usr/bin/env python3
"""Issue 1857 -- a generator's outputs are its PREDICTED list, never a glob.

Codegen never deletes an output it no longer writes, and the directory it
writes into lives in a build tree that outlives renames (issue 1360's
persistent Zephyr workspace). So a `file(GLOB ...)` of that directory compiles
every orphan beside its replacement. It happened twice in one generator:

- phase-306 split `*_ffi.rs` into `*_types.rs` + `*_exports.rs`; the glob
  include!()d both (E0428) and was patched by deleting that one suffix;
- phase-492 renamed the C files to rosidl's names; the glob compiled
  yesterday's `std_msgs_msg_bool.c` beside `bool.c`, and the orphan failed the
  RFC-0090 codegen-version `#error`.

The rule: a tracked cmake file never globs a codegen output directory. Use
`_nros_predict_generated_outputs()` (cmake/NanoRosCodegenCore.cmake), which the
canonical generator already builds from.

An output directory is recognised by the variable the generators name it:
`_output_dir`, `OUTPUT_DIR`, or one of the `nano_ros_c` / `nano_ros_cpp`
roots. Full-line `#` comments are ignored. Runs its own negative control.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

# `file(GLOB` / `file(GLOB_RECURSE`, then anything up to the closing paren,
# naming a generator output root.
GLOB = re.compile(r"file\(\s*GLOB(?:_RECURSE)?\b([^)]*)\)", re.S)
OUTPUT_ROOT = re.compile(r"\$\{\s*_?output_dir\s*\}|\$\{\s*_?P?_?OUTPUT_DIR\s*\}|nano_ros_c(?:pp)?\b", re.I)


def strip_comments(text: str) -> str:
    return "\n".join("" if l.lstrip().startswith("#") else l for l in text.splitlines())


def scan_text(text: str) -> list[int]:
    code = strip_comments(text)
    hits = []
    for m in GLOB.finditer(code):
        if OUTPUT_ROOT.search(m.group(1)):
            hits.append(code.count("\n", 0, m.start()) + 1)
    return hits


def tracked_cmake() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", "-z", "--", "*.cmake", "*CMakeLists.txt", "*.cmake.in"],
        cwd=REPO, check=True, capture_output=True,
    ).stdout.decode()
    return sorted(p for p in out.split("\0") if p)


def self_test() -> bool:
    cases = [
        ('file(GLOB_RECURSE _generated_sources "${_output_dir}/*.c")', True),
        ('file(GLOB_RECURSE _x "${_output_dir}/*_ffi.rs")', True),
        ('file(GLOB _h\n    "${OUTPUT_DIR}/*.h")', True),
        ('file(GLOB _h "${CMAKE_CURRENT_BINARY_DIR}/nano_ros_c/*.h")', True),
        ('# file(GLOB_RECURSE _s "${_output_dir}/*.c")', False),
        ('file(GLOB _local_msg CONFIGURE_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/msg/*.msg")', False),
        ('set(_generated_sources ${_exp_src})', False),
    ]
    ok = True
    for text, want in cases:
        if bool(scan_text(text)) != want:
            print(f"  self-test: {text!r}: expected {'a hit' if want else 'no hit'}")
            ok = False
    return ok


def main() -> int:
    if not self_test():
        print("check-generator-output-glob: self-test FAILED")
        return 1
    files = tracked_cmake()
    if not files:
        print("check-generator-output-glob: examined 0 cmake files -- refusing to pass")
        return 1
    offenders = []
    for rel in files:
        try:
            text = (REPO / rel).read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        offenders += [f"  {rel}:{n}" for n in scan_text(text)]
    if offenders:
        print("check-generator-output-glob: FAIL -- a codegen output directory is globbed:")
        print("\n".join(offenders))
        print(
            "\n  Codegen never deletes an output it stopped writing, so a glob compiles\n"
            "  every orphan beside its replacement (issue 1857). Use the predicted\n"
            "  list from `_nros_predict_generated_outputs()` instead."
        )
        return 1
    print(f"check-generator-output-glob: OK ({len(files)} cmake file(s); no output-dir glob)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
