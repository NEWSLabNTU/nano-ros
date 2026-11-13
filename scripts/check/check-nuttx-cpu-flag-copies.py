#!/usr/bin/env python3
"""NuttX's CPU flags have one statement: the kernel export's `ARCHCPUFLAGS`.

A NuttX kernel states the CPU it was built for in its export's
`scripts/Make.defs` (`ARCHCPUFLAGS = -mcpu=cortex-a7 -mfloat-abi=hard ...`).
Every nano-ros compile and link for NuttX takes those flags from there:

  - the board build scripts read them (`nros_board_common::nuttx_export`);
  - cargo reads them from the `nros-cargo.toml` the kernel step writes beside
    the export, passed with `--config` by `nros build` and the cmake road;
  - the make road uses `$(ARCHCPUFLAGS)` itself.

A literal `-mcpu=` / `-mfpu=` / `-mfloat-abi=` / `-march=` / `-mabi=` on a NuttX
build path is a second copy, and a copy is how a kernel reconfigured for
another CPU leaves nano-ros compiling for the old one (the FFI app once
compiled with `vfpv3-d16` beside a `neon-vfpv4` kernel). This gate refuses one
in the files that build for NuttX. Comments are exempt: they may describe a
flag. There is no allowlist.

Usage::

    check-nuttx-cpu-flag-copies.py [--selftest]
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

# The files that build for NuttX, as git pathspecs.
SCOPE = [
    "packages/boards/nros-board-nuttx-qemu",
    "packages/boards/nros-board-common/src/nuttx_*.rs",
    "cmake/toolchain/*nuttx*",
    "cmake/board/*nuttx*",
    "cmake/platform/nano-ros-nuttx.cmake",
    "packages/api/nros-c/cmake/nros-nuttx.cmake",
    "scripts/nuttx",
    "integrations/nuttx",
]
# Not build inputs: prose, and the kernel configs (Kconfig, no flags).
SKIP_SUFFIXES = (".md", "defconfig")

FLAG = re.compile(r"(?<![\w$(])-m(cpu|fpu|float-abi|arch|abi)=[A-Za-z0-9]")
COMMENT = re.compile(r"^\s*(#|//|/\*|\*|;)")


def violations(path: str, text: str) -> list[str]:
    out = []
    for n, line in enumerate(text.splitlines(), 1):
        if COMMENT.match(line):
            continue
        # A trailing comment may name a flag too.
        code = re.split(r"\s(#|//)\s", line, maxsplit=1)[0]
        if FLAG.search(code):
            out.append(f"{path}:{n}: {line.strip()}")
    return out


def tracked_files() -> list[str]:
    res = subprocess.run(["git", "ls-files", "--", *SCOPE], cwd=ROOT,
                         capture_output=True, text=True, check=True)
    return [p for p in res.stdout.split() if not p.endswith(SKIP_SUFFIXES)]


def selftest() -> int:
    cases = [
        ('rustflags = ["-C", "link-arg=-mcpu=cortex-a7"]', True),
        ('CFLAGS_x = "-march=rv32imac -mabi=ilp32"', True),
        ('set(CMAKE_C_FLAGS_INIT "-mfpu=neon-vfpv4")', True),
        ("# the cortex-a7 -mcpu=cortex-a7 multilib", False),
        ("// -mfloat-abi=hard selects the multilib", False),
        ('CFLAGS_$(T)="$(ARCHCPUFLAGS)"', False),
        ('let x = 1; // -mcpu=cortex-a7 here', False),
        ("ARCHCPUFLAGS = ...", False),
    ]
    bad = [(line, want) for line, want in cases
           if bool(violations("f", line)) != want]
    for line, want in bad:
        print(f"  BAD {'refuse' if want else 'accept'}: {line}", file=sys.stderr)
    if bad:
        print("check-nuttx-cpu-flag-copies: selftest FAILED", file=sys.stderr)
        return 1
    print(f"check-nuttx-cpu-flag-copies: selftest OK ({len(cases)} cases)")
    return 0


def main() -> int:
    if "--selftest" in sys.argv:
        return selftest()
    import contextlib
    import io
    with contextlib.redirect_stdout(io.StringIO()):
        if selftest() != 0:
            print("check-nuttx-cpu-flag-copies: the selftest failed; run --selftest",
                  file=sys.stderr)
            return 1
    files = tracked_files()
    found = []
    for p in files:
        try:
            found += violations(p, (ROOT / p).read_text(errors="replace"))
        except (IsADirectoryError, FileNotFoundError):
            continue
    if found:
        print("check-nuttx-cpu-flag-copies: a NuttX build file restates a CPU flag. "
              "NuttX states them once, in its export's scripts/Make.defs "
              "(ARCHCPUFLAGS); read them from there (nros_board_common::nuttx_export, "
              "the export's nros-cargo.toml, or $(ARCHCPUFLAGS) on the make road):",
              file=sys.stderr)
        for line in found:
            print(f"  - {line}", file=sys.stderr)
        return 1
    print(f"check-nuttx-cpu-flag-copies: OK — {len(files)} NuttX build file(s), "
          "no restated CPU flag")
    return 0


if __name__ == "__main__":
    sys.exit(main())
