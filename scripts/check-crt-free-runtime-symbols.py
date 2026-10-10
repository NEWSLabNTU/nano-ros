#!/usr/bin/env python3
"""Issue 1753 — a crt-free board image must define what its C runtime would have.

A bare-metal image links `-nostartfiles`: no `crt0.o`, no `crtbegin.o`. Two
things those objects supply are then the BOARD's job, and the rv-virt-threadx
TYPED carrier — whose generated entry holds `static ::rclcpp::Node` — failed to
link on both at once:

    relocation R_RISCV_PCREL_HI20 out of range ... references '__dso_handle'
    ... '__libc_fini'   ... '__fini_array_start'   ... '__fini_array_end'

R1  ARRAY BOUNDS. newlib's runtime names all three arrays' bounds —
    `__libc_init_array` reads `__preinit_array_*`, `__libc_fini_array` reads
    `__fini_array_*`. A board linker script that owns `.init_array` (and so has
    taken over section placement) must own `.preinit_array` and `.fini_array`
    too, with both bounds each. Left to the linker they resolve near address 0;
    on RISC-V medany that is ~2 GiB from `.text` and the link fails, on Cortex-M
    it is latent until an image reaches the exit path.

R2  THE C++ STATIC-OBJECT ABI. A static object with a destructor registers it
    through `__cxa_atexit(dtor, obj, &__dso_handle)`. With no crt, `__dso_handle`
    is undefined and libc's `__cxa_atexit` drags in the exit machinery above.
    The no-op definitions are correct only for an image that NEVER EXITS, and
    live once, in `nros-board-common/c/cxx_static_abi.c` (FreeRTOS had them since
    phase-370 W4; ThreadX RISC-V had none). Every board build that compiles the
    shared RTOS runner (`nros_rtos_run_components.c`) is RULED below: NEVER_EXITS
    builds must compile the stubs, HOSTED_CRT builds must not (their libc
    defines all four, and a second definition clashes). An unruled build, or a
    ruling whose file is gone, fails — so a new board is a decision, not a
    silent default.
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
RUNNER = "nros_rtos_run_components.c"
STUBS = "cxx_static_abi.c"

NEVER_EXITS = {
    "cmake/board/nano-ros-board-mps2-an385-freertos.cmake": "FreeRTOS on Cortex-M3, scheduler never returns",
    "cmake/board/nano-ros-board-mps3-an536-freertos.cmake": "FreeRTOS on Cortex-R52",
    "cmake/board/nano-ros-board-s32z270-freertos.cmake": "FreeRTOS on the S32Z RTU",
    "cmake/board/nano-ros-board-rv-virt-threadx.cmake": "ThreadX on RV64 QEMU virt (issue 1753)",
    "packages/boards/nros-board-freertos/build.rs": "the FreeRTOS cross family's cargo lane",
}
HOSTED_CRT = {
    "cmake/board/nano-ros-board-freertos-posix.cmake": "a Linux process: glibc's crt defines them",
    "cmake/board/nano-ros-board-threadx-linux.cmake": "a Linux process: glibc's crt defines them",
    "packages/boards/nros-board-nuttx-qemu/build.rs": "NuttX links its own libc runtime",
}

_SECTION = re.compile(r"^\s*\.(preinit_array|init_array|fini_array)\s*(?:\([^)]*\))?\s*:", re.M)
_BOUND = re.compile(r"\b(__(?:preinit|init|fini)_array_(?:start|end))\s*=")


def tracked(*patterns: str) -> list[str]:
    out = subprocess.run(["git", "ls-files", "-z", "--", *patterns], cwd=REPO,
                         capture_output=True, text=True, check=True).stdout
    return [p for p in out.split("\0") if p and not p.startswith("third-party/")
            and "/third-party/" not in p]


def strip_comments(text: str) -> str:
    return re.sub(r"/\*.*?\*/", "", text, flags=re.S)


def r1_offenders(text: str) -> list[str]:
    """What an `.init_array`-owning linker script lacks (R1)."""
    body = strip_comments(text)
    sections = set(_SECTION.findall(body))
    if "init_array" not in sections:
        return []
    missing = [f".{s}" for s in ("preinit_array", "fini_array") if s not in sections]
    bounds = set(_BOUND.findall(body))
    missing += sorted(f"{a}_array_{e}" for a in ("__preinit", "__init", "__fini")
                      for e in ("start", "end") if f"{a}_array_{e}" not in bounds)
    return missing


def _code(path: Path, text: str) -> str:
    """Comments stripped, per language — a ruling counts only a LISTED file."""
    if path.suffix == ".rs":
        return re.sub(r"//[^\n]*", "", text)
    return re.sub(r"#[^\n]*", "", text)


def r2_offenders(files: dict[str, str]) -> list[str]:
    """`files` maps repo path -> text of every board build file (R2)."""
    found = []
    runner_builds = {p for p, t in files.items() if RUNNER in _code(Path(p), t)}
    for p in sorted(runner_builds):
        has = STUBS in _code(Path(p), files[p])
        if p in NEVER_EXITS and not has:
            found.append(f"{p}: never exits ({NEVER_EXITS[p]}) but does not compile "
                         f"{STUBS} — a C++ static with a destructor will not link")
        elif p in HOSTED_CRT and has:
            found.append(f"{p}: hosted C runtime ({HOSTED_CRT[p]}) but compiles {STUBS}"
                         " — a second __dso_handle/__cxa_atexit")
        elif p not in NEVER_EXITS and p not in HOSTED_CRT:
            found.append(f"{p}: compiles {RUNNER} and is UNRULED — add it to NEVER_EXITS "
                         "or HOSTED_CRT in this script, with the reason")
    for p in sorted(set(NEVER_EXITS) | set(HOSTED_CRT)):
        if p not in runner_builds:
            found.append(f"{p}: ruled here but compiles no {RUNNER} (gone or moved) — "
                         "update the ruling so it cannot hold vacuously")
    return found


def self_test() -> None:
    good = """
  .preinit_array : { PROVIDE(__preinit_array_start = .); KEEP(*(.preinit_array)) PROVIDE(__preinit_array_end = .); }
  .init_array : { PROVIDE(__init_array_start = .); KEEP(*(.init_array)) PROVIDE(__init_array_end = .); }
  .fini_array : { PROVIDE(__fini_array_start = .); KEEP(*(.fini_array)) PROVIDE(__fini_array_end = .); }
"""
    assert r1_offenders(good) == [], r1_offenders(good)
    # issue 1753's link.lds, in shape: `.init_array` alone.
    pre = "  .init_array : { PROVIDE(__init_array_start = .); KEEP(*(.init_array)) PROVIDE(__init_array_end = .); }\n"
    got = r1_offenders(pre)
    assert ".fini_array" in got and "__fini_array_start" in got and ".preinit_array" in got, got
    # A section named only in a COMMENT is not a section.
    commented = pre + "/* .fini_array : { __fini_array_start = .; } */\n"
    assert ".fini_array" in r1_offenders(commented), "a commented-out section must not count"
    assert r1_offenders("SECTIONS { .text : { *(.text) } }") == [], "no .init_array, no rule"

    ne, hc = next(iter(NEVER_EXITS)), next(iter(HOSTED_CRT))
    ok = {p: f'"{RUNNER}"\n"{STUBS}"\n' for p in NEVER_EXITS}
    ok.update({p: f'"{RUNNER}"\n' for p in HOSTED_CRT})
    assert r2_offenders(ok) == [], r2_offenders(ok)
    dropped = dict(ok, **{ne: f'"{RUNNER}"\n# "{STUBS}"\n'})
    assert any("never exits" in w for w in r2_offenders(dropped)), "a commented stub is no stub"
    hosted = dict(ok, **{hc: f'"{RUNNER}"\n"{STUBS}"\n'})
    assert any("hosted C runtime" in w for w in r2_offenders(hosted))
    new = dict(ok, **{"cmake/board/nano-ros-board-new.cmake": f'"{RUNNER}"\n'})
    assert any("UNRULED" in w for w in r2_offenders(new))
    gone = {k: v for k, v in ok.items() if k != ne}
    assert any("ruled here but" in w for w in r2_offenders(gone))


def main() -> int:
    self_test()
    problems = []
    scripts = tracked("*.ld", "*.lds")
    owners = 0
    for p in scripts:
        text = (REPO / p).read_text(encoding="utf-8", errors="replace")
        if "init_array" in _SECTION.findall(strip_comments(text)):
            owners += 1
        for m in r1_offenders(text):
            problems.append(f"R1 {p}: owns .init_array but lacks {m}")
    builds = tracked("cmake/board/*.cmake", "packages/boards/*/build.rs")
    files = {p: (REPO / p).read_text(encoding="utf-8", errors="replace") for p in builds}
    problems += [f"R2 {w}" for w in r2_offenders(files)]
    if problems:
        print("check-crt-free-runtime-symbols: a crt-free board image is missing "
              "what its C runtime would have defined (issue 1753):", file=sys.stderr)
        for w in problems:
            print(f"  {w}", file=sys.stderr)
        return 1
    print(f"check-crt-free-runtime-symbols OK — {owners} linker script(s) own all "
          f"three init/fini arrays; {len(NEVER_EXITS)} never-exiting build(s) compile "
          f"{STUBS}, {len(HOSTED_CRT)} hosted build(s) do not.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
