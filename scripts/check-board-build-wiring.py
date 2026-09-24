#!/usr/bin/env python3
"""A board build script that compiles C routes through `nros-board-common`.

phase-468 W3.

`packages/boards/nros-board-common` is the shared build-script crate: the
per-RTOS-family builders (`freertos_build`, `nuttx_platform_build`,
`threadx_sources`) over shared `arch_flags`, `base_config`, `policy` and
`host_probe`. A board that reaches for `cc::Build` on its own is choosing its
own compiler flags, include set and vendored-source resolution — the three
things those modules exist to make one answer.

## Measured before this was written

Eight board build scripts compile C. All eight already comply:

    cc::Build directly, plus nros_board_common:
      nros-board-freertos              freertos_build, freertos_config,
                                       host_probe, nros_build_paths,
                                       platform_config
      nros-board-mps2-an385-freertos   freertos_build, host_probe
      nros-board-mps3-an536-freertos   freertos_build, host_probe
      nros-board-s32z270-freertos      freertos_build, host_probe
      nros-board-threadx               nros_build_paths, threadx_sources
      nros-board-threadx-linux         nros_build_paths, threadx_sources

    C compiled entirely THROUGH the helper (no direct cc::Build):
      nros-board-nuttx-qemu            nuttx_platform_build, nuttx_image_link
      nros-board-threadx-qemu-riscv64  threadx_qemu_riscv

So this gate lands green and its job is to keep that true, not to fix a break.
That is the honest description: a regression gate, not a repair.

## What it does NOT cover, and why

Three board crates have a `build.rs` that compiles no C:
`mps2-an385-pac`, `nros-board-mps2-an385` and `nros-board-nuttx`. The first two
emit a linker script (`memory.x` / `device.x`) into `OUT_DIR`; the third only
declares `rerun-if-env-changed` for two value knobs. Emitting a linker script
is a different job from compiling a vendored RTOS, and phase-468 lists them as
non-goals — so the rule keys on "compiles C", never on "has a build.rs".

## Exemptions live at the site

A board that must use `cc::Build` without the shared modules says so on the
line above it:

    // nros-board-common-exempt: <reason>

Next to the code rather than in a distant list, so the reason is read by
whoever is changing the thing it excuses. There are none today.

Usage: check-board-build-wiring.py [--self-test] [--list]
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BOARDS = ROOT / "packages/boards"

# The C compiler driver. A board reaching for this is choosing flags.
CC_RE = re.compile(r"\bcc::Build\b")
COMMON_RE = re.compile(r"\bnros_board_common::")
EXEMPT_RE = re.compile(r"//\s*nros-board-common-exempt:\s*(\S.*)")


def board_build_scripts() -> list[Path]:
    out = subprocess.run(
        ["git", "ls-files", "packages/boards/*/build.rs"],
        cwd=ROOT, capture_output=True, text=True, check=True,
    )
    return [ROOT / p for p in out.stdout.split() if p]


def classify(path: Path) -> tuple[bool, bool, str | None]:
    text = path.read_text(errors="replace")
    m = EXEMPT_RE.search(text)
    return bool(CC_RE.search(text)), bool(COMMON_RE.search(text)), (m.group(1).strip() if m else None)


def run(list_only: bool) -> int:
    scripts = board_build_scripts()
    if not scripts:
        print(
            "check-board-build-wiring: found NO board build scripts.\n"
            "  That is not a pass — this tree has eleven. Discovery broke.",
            file=sys.stderr,
        )
        return 1

    bad: list[str] = []
    compiles = 0
    for p in sorted(scripts):
        rel = p.relative_to(ROOT)
        uses_cc, uses_common, exempt = classify(p)
        if list_only:
            tag = "cc" if uses_cc else "--"
            print(f"  {tag}  common={'yes' if uses_common else 'no ':3} {rel}"
                  + (f"  EXEMPT: {exempt}" if exempt else ""))
        if not uses_cc:
            continue
        compiles += 1
        if uses_common or exempt:
            continue
        bad.append(
            f"{rel}: uses `cc::Build` and nothing from `nros_board_common`.\n"
            f"      A board that compiles C on its own picks its own compiler flags,\n"
            f"      include set and vendored-source resolution — the three things\n"
            f"      `arch_flags` / `base_config` / the per-family builders exist to\n"
            f"      make ONE answer. Route through them, or state why not on the line\n"
            f"      above: `// nros-board-common-exempt: <reason>`."
        )

    if list_only:
        return 0

    if bad:
        print("check-board-build-wiring: FAILED (phase-468 W3)", file=sys.stderr)
        for b in bad:
            print(f"  - {b}", file=sys.stderr)
        return 1

    print(
        f"check-board-build-wiring: OK — {len(scripts)} board build script(s), "
        f"{compiles} compile C, every one of those routed through nros-board-common."
    )
    return 0


def self_test() -> bool:
    ok = True

    def chk(label: str, cond: bool, detail: str = "") -> None:
        nonlocal ok
        print(f"  {'ok ' if cond else 'FAIL'} {label}" + ("" if cond else f" — {detail}"))
        if not cond:
            ok = False

    chk("a cc::Build use is detected", bool(CC_RE.search("let mut b = cc::Build::new();")))
    chk("a helper call is detected", bool(COMMON_RE.search("nros_board_common::freertos_build::run();")))
    chk(
        "an exemption is read with its reason",
        (EXEMPT_RE.search("// nros-board-common-exempt: vendor ships its own flags")
         or type("x", (), {"group": lambda *_: ""})()).group(1).strip()
        == "vendor ships its own flags",
    )
    chk("an exemption with NO reason does not count", EXEMPT_RE.search("// nros-board-common-exempt:") is None)
    # Discovery must find the real tree, or every run passes over nothing.
    chk("discovery finds board build scripts", len(board_build_scripts()) > 0)
    return ok


if __name__ == "__main__":
    if "--self-test" in sys.argv:
        sys.exit(0 if self_test() else 1)
    if not self_test():
        print("check-board-build-wiring: SELF-TEST FAILED", file=sys.stderr)
        sys.exit(1)
    sys.exit(run("--list" in sys.argv))
