#!/usr/bin/env python3
"""A board name states the reach of its build — RFC-0093, phase-437 W1.

WHAT THIS ASSERTS

RFC-0093's rule is that a board name is a CLAIM about where its artifact runs,
and the claim is true. Two reaches exist:

  SYSTEM  — no hardware assumption. `freertos-posix` sets no `CMAKE_C_COMPILER`
            and no linker script; `threadx-linux` runs as a userspace process.
            These travel across machines, and their names must name no part and
            no machine.
  TARGET  — one silicon part or one machine model. Pinned by a linker script, a
            memory base, a board defconfig or a peripheral address. Its name
            must NAME that part or machine — not an arch, not an emulator.

The reach is MEASURED from the build, never declared: this reads the cmake
overlay and the `packages/boards/**` directories the overlay itself names, and
looks for the pins. A board that pins nothing is a system board.

WHY A NAME MADE OF ARCH + EMULATOR IS THE FAILURE

`riscv64-qemu` reads "riscv64, under QEMU" and is one machine: 16550 UART at
`0x10000000`, CLINT at `0x02000000`, virtio-MMIO at `0x10001000`, and a write of
`0x5555` to QEMU's `test-finisher` at `0x100000` — a device no silicon has. A
user reads that name to decide whether their RISC-V64 board is supported, and
the answer it gives is wrong.

So for a TARGET board the rule is: at least one segment of the name must be
outside {arch tokens} u {emulator tokens} u {stack tokens}. That segment is the
part or the machine. `mps2-an385-freertos` has `mps2`/`an385`; `riscv64-qemu`
has nothing.

THE VENDOR EXEMPTION (RFC-0093 R4, which OUTRANKS R5)

A name that IS the vendor's own board id passes whatever it is made of.
`qemu-armv7a-nuttx` looks like emulator+arch+stack and is exactly NuttX's
`CONFIG_ARCH_BOARD="qemu-armv7a"`. A borrowed name is checkable against
upstream; an invented one is a third vocabulary, which is what this gate exists
to prevent. The vendor names are HARVESTED from the defconfigs the overlays
name, never listed here.

QEMU machine names (`virt`) count as MACHINE tokens, not emulator tokens: the
machine is what the build targets, the emulator is how we run it. That is
RFC-0093 R3, and it is why `rv-virt-threadx` passes with no vendor name to
borrow — ThreadX ships no board layer at all.

WHAT IT DOES NOT ENFORCE, AND WHY

RFC-0093 R2 (`<where>-<stack>`, stack always last) is a SHAPE rule, and this
gate does not check it. `mps2-an385` and `esp32-c3` carry no stack suffix and
pass here, while RFC-0093 §4 still renames them to `mps2-an385-baremetal` and
`esp32-c3-baremetal`.

The line is deliberate: R1/R3/R4 are about a name stating something FALSE —
`riscv64-qemu` tells a user their RISC-V64 board is supported — and a gate is
worth its runtime for those. A missing stack suffix misleads nobody; it makes a
family harder to sort. Enforcing it would also need `native`, `posix`, `zephyr`
and `fvp-aemv8r-smp` carved out, and an allow-list carved out of a shape rule
is how a gate stops meaning anything.

So R2 is phase-437's job and not this gate's, and the phase says so.

RATCHET

The violations that exist today are recorded in
`.config/board-name-reach-baseline.txt` and may only SHRINK: phase-437 W4-W6
remove them. A name not in the baseline must obey the rule.

Run:  python3 scripts/check-board-name-reach.py [--self-test] [--write-baseline]
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OVERLAY_DIR = os.path.join(ROOT, "cmake", "board")
BASELINE = os.path.join(ROOT, ".config", "board-name-reach-baseline.txt")

# An ISA or CPU family. Naming one is a claim about every machine that has it.
ARCH = {
    "arm", "armv7a", "armv7", "armv8", "armv8r", "aarch64", "arm64",
    "riscv", "riscv32", "riscv64", "rv32", "rv64", "x86", "x86_64", "i386",
    "cortex", "m0", "m3", "m4", "m7", "r5", "r52", "a7", "a53",
    "thumbv7m", "thumbv7em",
}
# How we RUN a machine, never what the build targets (RFC-0093 R3).
EMULATOR = {"qemu", "fvp", "renode", "sim", "simulator"}
# Which stack runs there — the suffix, never the "where" (R2).
STACK = {
    "baremetal", "bare-metal", "freertos", "nuttx", "threadx", "zephyr",
    "posix", "linux", "native", "rtos",
}
# QEMU machine models. A machine IS a target; only the emulator hosting it is
# not. Harvested from the `-M <machine>` arguments the recipes pass.
MACHINE = {"virt", "lm3s6965evb", "mps2-an385", "esp32c3"}

INDEX = os.path.join(ROOT, "nros-sdk-index.toml")
HOST_ARCH = {"x86_64", "aarch64-host"}


def index_boards(path):
    """{key: (arch, platform)} for every `[board.*]` in the SDK index.

    The index namespace is what `nros setup <board>` looks up, and it is where
    the emulator-first spellings live — so a gate that read only the cmake
    overlays would miss `qemu-arm-freertos` entirely, which is half the point.
    """
    out = {}
    try:
        with open(path, encoding="utf8") as fh:
            text = fh.read()
    except OSError:
        return out
    for m in re.finditer(r"^\[board\.([a-z0-9._-]+)\]\n((?:(?!^\[).*\n)*)", text, re.M):
        body = m.group(2)
        a = re.search(r'^arch\s*=\s*"([^"]*)"', body, re.M)
        p = re.search(r'^platform\s*=\s*"([^"]*)"', body, re.M)
        out[m.group(1)] = (a.group(1) if a else "", p.group(1) if p else "")
    return out


LD_HINT = re.compile(r"\.lds?\b|\.x\"|LINKER_SCRIPT")
MMIO = re.compile(r"0x[0-9a-fA-F]{6,}")
DEFCONFIG_PIN = re.compile(r"^CONFIG_(RAM_START|ARCH_BOARD)\b", re.M)
VENDOR_BOARD = re.compile(r'^CONFIG_ARCH_BOARD="([^"]+)"', re.M)
BOARD_DIR_REF = re.compile(r"packages/boards/[A-Za-z0-9/_.-]+")


def overlays(root):
    """{board name: overlay path} from `cmake/board/nano-ros-board-<name>.cmake`."""
    out = {}
    try:
        names = os.listdir(os.path.join(root, "cmake", "board"))
    except OSError:
        return out
    for n in sorted(names):
        m = re.fullmatch(r"nano-ros-board-(.+)\.cmake", n)
        if m:
            out[m.group(1)] = os.path.join(root, "cmake", "board", n)
    return out


def scan(root, board, overlay_path):
    """(pinned, why, vendor_names) — is this board tied to one target?

    Read from the OVERLAY ALONE, deliberately. The obvious richer version —
    walk the `packages/boards/**` directories the overlay names and look for
    linker scripts and MMIO literals — was written first and is WRONG, because
    those directories are SHARED: `nros-board-freertos` is referenced by four
    boards and `nros-board-common` by two, so the FreeRTOS Cortex-M linker
    script made `freertos-posix` look pinned, and a peripheral address in
    `threadx_hooks.c` did the same to `threadx-linux`. Both of those travel;
    attributing a shared crate's pins to every consumer inverts the answer.

    The overlay is the one file that belongs to exactly one board, and it is
    where a board declares what it pins: a linker script, or the board
    defconfig its RTOS builds from.

    This is CONSERVATIVE in one direction only. A board whose toolchain comes
    from an external SDK (ESP-IDF supplies ESP32-C3's linking) pins nothing
    here and reads as a system board — so the gate can call a target board
    portable, never the reverse. That is the safe direction: the SYSTEM rule
    then forbids naming a machine or an emulator, which is what catches
    `qemu-esp32-baremetal` regardless of how it is classified.
    """
    try:
        with open(overlay_path, encoding="utf8", errors="replace") as fh:
            overlay = fh.read()
    except OSError:
        return False, [], set()

    why, vendor = [], set()
    if LD_HINT.search(overlay):
        why.append("its overlay names a linker script")

    # `set(NROS_NUTTX_DEFCONFIG "${...}/nuttx-config/arm/defconfig")` — the
    # board defconfig an RTOS build is configured from. The path is per-board
    # even when the crate holding it is shared.
    for m in re.finditer(r'set\(\s*\w*DEFCONFIG\s+"([^"]+)"', overlay):
        rel = re.sub(r"\$\{[^}]+\}/?", "", m.group(1))
        why.append(f"its overlay names the board defconfig {rel}")
        for dirpath, _dn, filenames in os.walk(os.path.join(root, "packages", "boards")):
            if "defconfig" in filenames and dirpath.endswith(os.path.dirname(rel)):
                try:
                    with open(os.path.join(dirpath, "defconfig"), encoding="utf8") as fh:
                        vendor |= set(VENDOR_BOARD.findall(fh.read()))
                except OSError:
                    pass
    return bool(why), why[:4], vendor


def segments(name):
    return [s for s in re.split(r"[-_]", name) if s]


def verdict(board, pinned, vendor):
    """(ok, message). The name rules of RFC-0093 §5."""
    segs = segments(board)
    known = ARCH | EMULATOR | STACK
    # R4 — a vendor board id passes whatever it is made of, and it outranks R5.
    for v in vendor:
        if board == v or board.startswith(v + "-"):
            return True, f"{board}: names upstream's `{v}` (R4)"

    if not pinned:
        # SYSTEM: must name no machine and no emulator.
        bad = [s for s in segs if s in EMULATOR or s in MACHINE]
        if bad:
            return False, (
                f"{board}: pins no linker script, defconfig or peripheral "
                f"address, so it travels — but its name says {bad}. A system "
                f"board must name no machine and no emulator (RFC-0093 R1/R3)."
            )
        return True, f"{board}: system board, names no target"

    # TARGET: something in the name must BE the part or the machine.
    named = [s for s in segs if s in MACHINE or s not in known]
    if not named:
        return False, (
            f"{board}: pinned to one target, but every segment of its name is "
            f"an arch, an emulator or a stack — so it names no part and no "
            f"machine, and promises reach it does not have (RFC-0093 R1)."
        )
    if "qemu" in segs and not any(board.startswith(v) for v in vendor):
        return False, (
            f"{board}: names the emulator. QEMU is how a machine is RUN, not "
            f"what the build targets — name the machine ({', '.join(named)}) "
            f"or borrow upstream's board id (RFC-0093 R3/R4)."
        )
    return True, f"{board}: target board, names {', '.join(named)}"


def read_baseline():
    try:
        with open(BASELINE, encoding="utf8") as fh:
            return {
                l.split("#", 1)[0].strip()
                for l in fh
                if l.strip() and not l.strip().startswith("#")
            }
    except OSError:
        return set()


def self_test():
    assert segments("mps2-an385-freertos") == ["mps2", "an385", "freertos"]
    # a pinned board made only of arch + emulator names no target
    ok, msg = verdict("riscv64-qemu", True, set())
    assert not ok and "names no part" in msg, msg
    # …and one that names a part passes
    ok, _ = verdict("mps2-an385-freertos", True, set())
    assert ok
    # emulator + arch + stack, but it IS upstream's board id -> R4 wins
    ok, msg = verdict("qemu-armv7a-nuttx", True, {"qemu-armv7a"})
    assert ok and "(R4)" in msg, msg
    # the same name WITHOUT the vendor declaration must fail
    ok, _ = verdict("qemu-armv7a-nuttx", True, set())
    assert not ok
    # a QEMU machine model is a target, so `virt` is a legal "where"
    ok, _ = verdict("rv-virt-threadx", True, set())
    assert ok
    # a system board may not name a machine or an emulator
    ok, _ = verdict("freertos-posix", False, set())
    assert ok
    ok, msg = verdict("qemu-posix", False, set())
    assert not ok and "travels" in msg, msg
    # naming the emulator fails even when a part is present
    ok, msg = verdict("qemu-arm-freertos", True, set())
    assert not ok, msg
    sys.stdout.write("check-board-name-reach self-test: OK\n")


def main():
    if "--self-test" in sys.argv:
        self_test()
        return 0
    self_test()

    found = overlays(ROOT)
    if not found:
        sys.stderr.write(
            "error: no cmake/board/nano-ros-board-*.cmake found. This gate\n"
            "would then pass over an empty set, which is not a pass.\n"
        )
        return 1

    baseline = read_baseline()
    violations = {}
    vendors_seen = set()
    for board, path in sorted(found.items()):
        pinned, why, vendor = scan(ROOT, board, path)
        vendors_seen |= vendor
        ok, msg = verdict(board, pinned, vendor)
        if not ok:
            violations[board] = (msg, why)

    # The INDEX namespace too. A cross `arch` means the entry is tied to a
    # target — the index has no linker script to read, but it does say what it
    # builds for, and `arch = "x86_64"` is the host. `zephyr` declares no arch
    # (Zephyr supplies its own boards), so it is a meta entry and out of scope.
    idx = index_boards(INDEX)
    if not idx:
        sys.stderr.write("error: no [board.*] entries read from the SDK index.\n")
        return 1
    for board, (arch, _platform) in sorted(idx.items()):
        if board in found or not arch or arch == "-":
            continue
        pinned = arch not in HOST_ARCH
        ok, msg = verdict(board, pinned, vendors_seen)
        if not ok:
            violations[board] = (msg, [f"index entry declares arch = \"{arch}\""])

    if "--write-baseline" in sys.argv:
        with open(BASELINE, "w", encoding="utf8") as fh:
            fh.write(
                "# phase-437 W1 — board names that do not yet state their reach\n"
                "# (RFC-0093). A RATCHET: it may only SHRINK. W4-W6 empty it.\n"
                "#\n"
                "# Regenerate ONLY to record a rename that removed one:\n"
                "#     python3 scripts/check-board-name-reach.py --write-baseline\n"
            )
            for b in sorted(violations):
                fh.write(f"{b}\n")
        print(f"wrote baseline: {len(violations)} known violation(s)")
        return 0

    new = sorted(set(violations) - baseline)
    stale = sorted(baseline - set(violations))
    if new or stale:
        sys.stderr.write("check-board-name-reach: the name set drifted.\n\n")
        for b in new:
            msg, why = violations[b]
            sys.stderr.write(f"  {msg}\n")
            for w in why:
                sys.stderr.write(f"      pinned by: {w}\n")
            sys.stderr.write("\n")
        for b in stale:
            sys.stderr.write(
                f"  {b}: listed as violating and now obeys the rule — delete its\n"
                f"      line. This file may only shrink.\n\n"
            )
        return 1

    print(
        f"check-board-name-reach: OK — {len(found)} overlay(s) + {len(idx)} index "
        f"entr(ies); "
        f"{len(violations)} known violation(s) awaiting phase-437 W4-W6."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
