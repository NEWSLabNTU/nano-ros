#!/usr/bin/env python3
"""Issue 2032 — a `nros_posix_*` symbol is reached only by a build that has the POSIX port.

`nros_posix_*` names are the POSIX port's own (`nros-platform-posix`). They are
not part of `<nros/platform.h>`, so no other port defines them. Zephyr
`native_sim`, and the host tiers of ThreadX and FreeRTOS, target a Linux triple
without that port. `cfg(unix)` is therefore true there, and `cfg(unix)` is the
wrong gate. A reference compiled into such an image makes the linker satisfy
it from whichever archive carries a POSIX `platform.o`: `nros-rmw-xrce-cffi`
bundles one on any POSIX-host target. Every other symbol in that member then
collides with the image's own port.

It has happened three times:
- issue 1776, the termination guard;
- issue 2032's first route, an ABI function the Zephyr kernel arm lacked
  (`check-platform-port-arms` keeps that one out);
- issue 2032's second route, `nros_posix_apply_current_deadline`. Issue 2018
  added it to nros-cpp's native level runner four minutes after the first
  fix landed, and every Zephyr native_sim XRCE image failed to link in
  `run-matrix` and the tier-2 nightly.

`check-platform-port-arms` compares `nros_platform_*` functions across a
port's arms, so a `nros_posix_*` name is outside what it can see.

THE RULE. Outside the POSIX port itself, a Rust `extern "C"` block that
declares a `nros_posix_*` function must carry `#[cfg(feature =
"platform-posix")]`, the feature that selects that port. The shared spelling
is in `nros-platform` (`termination`, `sched`), and callers go through it. A
crate that is POSIX-only by construction is listed in `POSIX_ONLY_CRATES`
with the reason. C and C++ sources outside the port may not reference a
`nros_posix_*` name at all.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

PORT_DIR = "packages/platform/nros-platform-posix/"

# A crate whose every build has the POSIX port, so a bare reference is safe.
POSIX_ONLY_CRATES = {
    "packages/boards/nros-board-linux/": (
        "the Linux host board: its builds select `platform-posix`, and no "
        "Zephyr or RTOS image links it"
    ),
}

DECL_RE = re.compile(r"\bfn\s+(nros_posix_[A-Za-z0-9_]+)\s*\(")
EXTERN_RE = re.compile(r'^\s*(?:pub\s+)?(?:unsafe\s+)?extern\s+"C"\s*\{')
CFG_POSIX_RE = re.compile(r'#\[cfg\(\s*feature\s*=\s*"platform-posix"\s*\)\]')
C_REF_RE = re.compile(r"\bnros_posix_[A-Za-z0-9_]+\b")


def rust_findings(rel: str, text: str) -> list[str]:
    """One message per `nros_posix_*` declaration in an extern block lacking the gate."""
    out = []
    lines = text.splitlines()
    block_start = None  # index of the current extern "C" block's opening line
    depth = 0
    for i, line in enumerate(lines):
        code = line.split("//", 1)[0]
        if block_start is None and EXTERN_RE.match(code):
            block_start, depth = i, 0
        if block_start is not None:
            m = DECL_RE.search(code)
            if m and not gated(lines, block_start):
                out.append(
                    f"{rel}:{i + 1}: declares {m.group(1)} without "
                    f'#[cfg(feature = "platform-posix")] on its extern block.\n'
                    f"      Zephyr native_sim and the ThreadX/FreeRTOS host tiers are "
                    f"`unix` with no POSIX port, so the linker pulls the POSIX "
                    f"platform.o and every port symbol collides (issue 2032). Call "
                    f"the gated helper in `nros-platform` (`sched`, `termination`) "
                    f"instead."
                )
            depth += code.count("{") - code.count("}")
            if depth <= 0 and "{" in "".join(lines[block_start : i + 1]):
                block_start = None
    return out


def gated(lines: list[str], block_start: int) -> bool:
    """True when the attributes directly above the extern block include the gate."""
    j = block_start - 1
    while j >= 0:
        s = lines[j].strip()
        if s.startswith("#["):
            if CFG_POSIX_RE.search(s):
                return True
            j -= 1
            continue
        if s.startswith("//") or s == "":
            j -= 1
            continue
        return False
    return False


def c_findings(rel: str, text: str) -> list[str]:
    out = []
    for i, line in enumerate(text.splitlines()):
        code = line.split("//", 1)[0]
        if code.lstrip().startswith(("*", "/*")):
            continue
        m = C_REF_RE.search(code)
        if m:
            out.append(
                f"{rel}:{i + 1}: references {m.group(0)}, a POSIX-port-private name, "
                f"outside the POSIX port (issue 2032)."
            )
    return out


def self_test() -> None:
    """Negative controls: each rule fires on the shape it forbids."""
    bare = 'unsafe extern "C" {\n    fn nros_posix_apply_current_deadline() -> i32;\n}\n'
    assert len(rust_findings("a.rs", bare)) == 1, "an ungated declaration must fire"
    cfg_unix = '#[cfg(unix)]\nunsafe extern "C" {\n    fn nros_posix_x();\n}\n'
    assert len(rust_findings("a.rs", cfg_unix)) == 1, "cfg(unix) is not the gate"
    good = (
        '#[cfg(feature = "platform-posix")]\n'
        "// a comment between\n"
        'unsafe extern "C" {\n    fn nros_posix_x();\n}\n'
    )
    assert rust_findings("a.rs", good) == []
    other = 'unsafe extern "C" {\n    fn nros_platform_clock_ns() -> u64;\n}\n'
    assert rust_findings("a.rs", other) == [], "ABI names are not this rule's"
    # A gated block followed by an ungated one: only the second fires.
    two = good + bare
    assert len(rust_findings("a.rs", two)) == 1
    assert len(c_findings("a.c", "int r = nros_posix_x();\n")) == 1
    assert c_findings("a.c", "/* nros_posix_x is the port's */\n") == []


def tracked(*globs: str) -> list[str]:
    r = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "--", *globs],
        capture_output=True, text=True, check=False,
    )
    if r.returncode != 0:
        sys.exit(f"check-posix-port-private-symbols: `git ls-files` failed:\n  {r.stderr.strip()}")
    return [x for x in r.stdout.splitlines() if x]


def exempt(rel: str) -> bool:
    return rel.startswith(PORT_DIR) or any(rel.startswith(c) for c in POSIX_ONLY_CRATES)


def main(argv: list[str]) -> int:
    self_test()
    if "--self-test" in argv:
        print("check-posix-port-private-symbols self-test: OK")
        return 0
    errors, rs, cs = [], 0, 0
    for rel in tracked("packages/*.rs"):
        if exempt(rel) or "/generated/" in rel:
            continue
        rs += 1
        text = (ROOT / rel).read_text(errors="replace")
        if "nros_posix_" in text:
            errors += rust_findings(rel, text)
    for rel in tracked("packages/*.c", "packages/*.h", "packages/*.cpp", "packages/*.hpp"):
        if exempt(rel) or "/third-party/" in rel:
            continue
        cs += 1
        text = (ROOT / rel).read_text(errors="replace")
        if "nros_posix_" in text:
            errors += c_findings(rel, text)
    if rs == 0 or cs == 0:
        sys.exit("check-posix-port-private-symbols: scanned no files — an empty scan passes vacuously.")
    if errors:
        print("check-posix-port-private-symbols: FAIL", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        return 1
    print(
        f"check-posix-port-private-symbols: OK — {rs} Rust and {cs} C/C++ file(s); "
        f"every nros_posix_* reference outside the port is behind platform-posix."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
