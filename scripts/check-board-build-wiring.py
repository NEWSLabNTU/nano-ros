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

## The second rule: a board may not spell an ISA/ABI flag (phase-471 W2)

The rule above asks whether a board *reaches* `nros-board-common`. Every
FreeRTOS overlay already did — while carrying the whole recipe, 149 to 243
lines of it, so the answer was yes and the duplication was total. Reaching is
not delegating, and a gate that cannot tell them apart is not measuring what it
says it measures.

The stronger question that is still a PROPERTY and not a proxy for length:
**does this board choose a compiler target?** An ISA/ABI flag —
`-mcpu=` `-march=` `-mabi=` `-mfpu=` `-mthumb` `-mfloat-abi=` `-mcmodel=` — is
the one thing a board can write that decides what the compiler produces, and
it is the thing the family builder exists to make one answer:
`arch_flags::cflags_for_target` reads the `[arch.*]` profile, `FREERTOS_CFLAGS`
overrides it at RFC-0049 rung 1, and a board that also writes the flags out has
a second answer that cannot see either.

It is not a line-count heuristic, it is falsifiable at a line, and it is
exactly the defect phase-471 W0 measured: `gcc_print_file` was copied verbatim
into three FreeRTOS board scripts, each with its own hardcoded `-mcpu` list,
and the copies could not see `FREERTOS_CFLAGS` — so a board pointed at another
CPU through that variable compiled its C for the new one and linked the old
one's newlib. W2 folded the three into the runner, which reads the flags from
the same function the compile does.

Comments are stripped before the scan: naming a flag while EXPLAINING one is
not choosing a target, and five board scripts legitimately do (issue 0478's
note appears four times in `nros-board-freertos` alone).

## Exemptions live at the site

A board that must use `cc::Build` without the shared modules, or must spell its
own arch flags, says so on the line above it:

    // nros-board-common-exempt: <reason>
    // nros-board-arch-flags-exempt: <reason>

Next to the code rather than in a distant list, so the reason is read by
whoever is changing the thing it excuses. A reason is REQUIRED — a bare marker
does not count. There is one today: `nros-board-threadx`, whose RISC-V64 flags
duplicate `threadx_qemu_riscv64_build`'s and whose acceptance is a RISC-V64
build rather than a gate, tracked as issue 1562.

Usage: check-board-build-wiring.py [--self-test] [--list]
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path
import sys as _w3_sys  # noqa: E402
from pathlib import Path as _W3Path  # noqa: E402
_w3_sys.path.insert(0, str(_W3Path(__file__).resolve().parent / "lib"))
import comments  # noqa: E402  phase-472 W3 — the one comment stripper

ROOT = Path(__file__).resolve().parent.parent
BOARDS = ROOT / "packages/boards"

# The C compiler driver. A board reaching for this is choosing flags.
CC_RE = re.compile(r"\bcc::Build\b")
COMMON_RE = re.compile(r"\bnros_board_common::")
EXEMPT_RE = re.compile(r"//\s*nros-board-common-exempt:\s*(\S.*)")

# phase-471 W2 — the flags that decide what the compiler PRODUCES. `-mthumb`
# takes no `=`; the rest are prefixes of a value.
ARCH_FLAG_RE = re.compile(r"-m(?:cpu|arch|float-abi|abi|fpu|cmodel|tune)=|-mthumb\b")
ARCH_EXEMPT_RE = re.compile(r"//\s*nros-board-arch-flags-exempt:\s*(\S.*)")


def strip_comments(text: str) -> str:
    """Rust source with `//`-comments removed, string literals preserved.

    A flag NAMED in a comment is a flag being explained, not chosen, and five
    board scripts legitimately do that (issue 0478's note is in four places).
    Scanning the raw text would report every one of them, which is 0196's
    wider-than-the-rule shape: a false report, fixed by deriving the subject
    rather than by widening an allowlist.
    """
    # phase-472 W3 — the shared stripper (scripts/lib/comments.py).
    return comments.strip_comments(text, "rust")


def board_build_scripts() -> list[Path]:
    out = subprocess.run(
        ["git", "ls-files", "packages/boards/*/build.rs"],
        cwd=ROOT, capture_output=True, text=True, check=True,
    )
    return [ROOT / p for p in out.stdout.split() if p]


class Board:
    """What the two rules ask of one board build script."""

    def __init__(self, path: Path) -> None:
        text = path.read_text(errors="replace")
        code = strip_comments(text)
        m = EXEMPT_RE.search(text)
        a = ARCH_EXEMPT_RE.search(text)
        self.uses_cc = bool(CC_RE.search(code))
        self.uses_common = bool(COMMON_RE.search(code))
        self.exempt = m.group(1).strip() if m else None
        self.arch_exempt = a.group(1).strip() if a else None
        self.arch_flags = sorted({hit.rstrip("=") for hit in ARCH_FLAG_RE.findall(code)})


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
    arch_exempted = 0
    for p in sorted(scripts):
        rel = p.relative_to(ROOT)
        b = Board(p)
        if list_only:
            tag = "cc" if b.uses_cc else "--"
            flags = ",".join(b.arch_flags) if b.arch_flags else "-"
            print(f"  {tag}  common={'yes' if b.uses_common else 'no ':3} arch={flags:20} {rel}"
                  + (f"  EXEMPT: {b.exempt}" if b.exempt else "")
                  + (f"  ARCH-EXEMPT: {b.arch_exempt}" if b.arch_exempt else ""))

        # Rule 2 (phase-471 W2) — does this board choose a compiler target?
        # Independent of rule 1: a board that delegates every compile can still
        # hand a toolchain probe its own flag list, which is exactly how the
        # three FreeRTOS `gcc_print_file` copies stayed invisible.
        if b.arch_flags:
            if b.arch_exempt:
                arch_exempted += 1
            else:
                bad.append(
                    f"{rel}: spells its own ISA/ABI flag(s): {', '.join(b.arch_flags)}.\n"
                    f"      A board that writes the flags out has a SECOND answer to\n"
                    f"      'what is this compiling for', and it cannot see the first:\n"
                    f"      the `[arch.*]` profile `arch_flags::cflags_for_target` reads,\n"
                    f"      nor the RFC-0049 rung-1 env override above it. That is how\n"
                    f"      three FreeRTOS boards compiled their C for one CPU and linked\n"
                    f"      another's newlib (phase-471 W2). Take the flags from the same\n"
                    f"      function the compile does — in `nros-board-common` — or state\n"
                    f"      why not on the line above:\n"
                    f"      `// nros-board-arch-flags-exempt: <reason, with an issue id>`."
                )

        # Rule 1 (phase-468 W3) — does a C-compiling board reach the shared crate?
        if not b.uses_cc:
            continue
        compiles += 1
        if b.uses_common or b.exempt:
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
        print("check-board-build-wiring: FAILED (phase-468 W3 + phase-471 W2)", file=sys.stderr)
        for b0 in bad:
            print(f"  - {b0}", file=sys.stderr)
        return 1

    print(
        f"check-board-build-wiring: OK — {len(scripts)} board build script(s), "
        f"{compiles} compile C, every one of those routed through nros-board-common; "
        f"none spells its own ISA/ABI flags ({arch_exempted} exempted with a reason)."
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

    # phase-471 W2 — rule 2. Each case is a shape that existed in this tree.
    for flag in ("-mcpu=cortex-m3", "-mthumb", "-mfpu=neon-fp-armv8",
                 "-mfloat-abi=hard", "-march=rv64gc", "-mabi=lp64d",
                 "-mcmodel=medany"):
        chk(f"an arch flag is detected: {flag}",
            bool(ARCH_FLAG_RE.search(f'.flag("{flag}")')))
    chk("a non-arch -m flag is NOT an arch flag",
        not ARCH_FLAG_RE.search('.flag("-mno-omit-leaf-frame-pointer")'))
    chk("a plain warning flag is not an arch flag",
        not ARCH_FLAG_RE.search('.flag("-ffunction-sections").flag("-Wno-sign-compare")'))
    # The reason comments are stripped: naming a flag while explaining one is
    # what five board scripts legitimately do, and reporting them would be a
    # reach WIDER than the rule (0196 the other way round).
    chk("a flag named in a line comment is not a choice",
        not ARCH_FLAG_RE.search(strip_comments('// cc-rs passes -mcpu=cortex-m3 here\nlet x = 1;')))
    chk("a flag named in a doc comment is not a choice",
        not ARCH_FLAG_RE.search(strip_comments('//! defaults to `-mcpu=cortex-m3 -mthumb`\n')))
    chk("a flag in a STRING survives comment stripping",
        bool(ARCH_FLAG_RE.search(strip_comments('let f = "-mcpu=cortex-r52"; // was -march=rv64gc'))))
    chk("a `//` inside a string literal does not eat the rest of the line",
        strip_comments('let u = "http://x"; let f = "-mthumb";').count('"') == 4)
    chk(
        "an arch exemption is read with its reason",
        (ARCH_EXEMPT_RE.search("// nros-board-arch-flags-exempt: issue 1562, needs a riscv64 build")
         or type("x", (), {"group": lambda *_: ""})()).group(1).strip()
        == "issue 1562, needs a riscv64 build",
    )
    chk("an arch exemption with NO reason does not count",
        ARCH_EXEMPT_RE.search("// nros-board-arch-flags-exempt:") is None)

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
