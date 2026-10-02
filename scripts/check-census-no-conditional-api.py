#!/usr/bin/env python3
"""phase-463 W5 I2 (issue 1419) -- the C++ surface is identical with the census on or off.

The census binary IS the boot binary (phase-463 W2): the native C++ umbrella
carries `metadata-mode`, every RTOS umbrella does not, and a component compiles
against ONE set of public headers in both. That holds only while no public
declaration of `nros-cpp` is conditional on the analysis modes. A header that
grew `#ifdef NROS_METADATA_MODE` around a declaration would make a component
compile differently on the census host than on the board -- the census would
then observe code that does not ship, which is the one thing it must never do.

So this reads every tracked header under `packages/api/nros-cpp/include/` and
refuses a preprocessor conditional (`#if`, `#ifdef`, `#ifndef`, `#elif`) whose
condition names the metadata, profile or census modes. Today there are none --
`nros_cpp_ffi.h` is cbindgen output with no feature `[defines]`, and the census
pair `nros_cpp_census_begin` / `nros_cpp_census_finish` is declared
unconditionally -- and this is what keeps it so. The SYMBOLS are conditional
(they exist only where `env` does), which is the RTOS image carrying zero bytes
of them; the DECLARATIONS are not.

The negative control runs on every invocation, on the normal path (phase-395):
a planted `#ifdef NROS_METADATA_MODE` must be refused, and the header guard of
`nros_cpp_ffi.h` must not be.

Usage: check-census-no-conditional-api.py
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
HEADER_DIR = "packages/api/nros-cpp/include"

# A preprocessor conditional, possibly indented (`#  if`), with its condition.
CONDITIONAL = re.compile(r"^\s*#\s*(if|ifdef|ifndef|elif)\b(?P<cond>.*)$")
# What an analysis-mode condition spells. Case-insensitive, because the Rust
# feature is `metadata-mode` and a C macro would be `NROS_METADATA_MODE`.
ANALYSIS = re.compile(r"metadata|profile[_-]?mode|census", re.IGNORECASE)

# The census pair must stay DECLARED: a gate over "no conditional" passes just
# as well when the declarations are deleted outright, and then the native entry
# no longer compiles the switch at all.
REQUIRED_DECLS = {
    "packages/api/nros-cpp/include/nros/nros_cpp_ffi.h": (
        "nros_cpp_census_begin",
        "nros_cpp_census_finish",
    ),
}


def offending_lines(text: str) -> list[tuple[int, str]]:
    """Every conditional line whose condition names an analysis mode."""
    hits = []
    for lineno, line in enumerate(text.splitlines(), start=1):
        m = CONDITIONAL.match(line)
        if m and ANALYSIS.search(m.group("cond")):
            hits.append((lineno, line.strip()))
    return hits


def self_test() -> None:
    planted = "int a;\n#ifdef NROS_METADATA_MODE\nint nros_cpp_x(void);\n#endif\n"
    if not offending_lines(planted):
        sys.exit(
            "check-census-no-conditional-api: FAIL -- self-test: a planted "
            "`#ifdef NROS_METADATA_MODE` was not refused, so this gate refuses nothing"
        )
    planted = "#  if defined(NROS_CENSUS)\n#endif\n"
    if not offending_lines(planted):
        sys.exit(
            "check-census-no-conditional-api: FAIL -- self-test: an indented "
            "`#  if defined(NROS_CENSUS)` was not refused"
        )
    benign = "#ifndef NROS_CPP_FFI_H\n#define NROS_CPP_FFI_H\n#if defined(__cplusplus)\n#endif\n"
    if offending_lines(benign):
        sys.exit(
            "check-census-no-conditional-api: FAIL -- self-test: a header guard was "
            "refused, so the gate would be red on a tree that is fine"
        )


def tracked_headers() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", "--", HEADER_DIR],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return [p for p in out.splitlines() if p.endswith((".h", ".hpp"))]


def main() -> int:
    self_test()
    headers = tracked_headers()
    if not headers:
        print(
            f"check-census-no-conditional-api: FAIL -- no tracked header under {HEADER_DIR}; "
            "the gate would report OK over nothing",
            file=sys.stderr,
        )
        return 2

    failures = []
    for rel in headers:
        text = (ROOT / rel).read_text(encoding="utf-8", errors="replace")
        for lineno, line in offending_lines(text):
            failures.append(f"  {rel}:{lineno}: {line}")
    for rel, names in REQUIRED_DECLS.items():
        text = (ROOT / rel).read_text(encoding="utf-8", errors="replace")
        for name in names:
            if re.search(rf"\b{re.escape(name)}\s*\(", text) is None:
                failures.append(
                    f"  {rel}: `{name}` is no longer declared -- the native entry's census "
                    "switch (issue 1419) calls it"
                )

    if failures:
        print(
            "check-census-no-conditional-api: FAIL -- the nros-cpp public surface is no longer "
            "identical with the census on and off (phase-463 W5 I2):",
            file=sys.stderr,
        )
        print("\n".join(failures), file=sys.stderr)
        print(
            "A declaration must not depend on `metadata-mode` / `profile-mode`: the census "
            "would then observe code the RTOS image does not compile. Keep the declaration "
            "unconditional and gate only the SYMBOL's body in Rust.",
            file=sys.stderr,
        )
        return 1
    print(
        f"check-census-no-conditional-api: OK -- {len(headers)} header(s), no analysis-mode "
        "conditional, census pair declared"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
