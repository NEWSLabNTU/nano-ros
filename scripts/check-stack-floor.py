#!/usr/bin/env python3
"""Assert a floor on the runtime stack of ESP32 images (issues 0190, 1052).

On `qemu-esp32-baremetal` the stack is not declared — it is the LINKER
LEFTOVER. `link.x` fills DRAM up to `_stack_start` (0x3fcce400) and `.bss`
grows up from below, so `_stack_end` is wherever `.bss` happens to end and

    stack = _stack_start - _stack_end

shrinks by exactly one byte for every byte any static gains. There is no
runtime stack-overflow guard on this target, so an overflow does not trap:
it writes frames down into `.bss` and the image dies later, somewhere else,
as a wild jump.

That is issue 0190, and `nros-board-esp32-qemu/src/node.rs` closes its
explanation with the instruction this script exists to enforce:

    Check `.stack` in `readelf -S` after changing ANY large static — there
    is no runtime stack-overflow guard on this target.

Nobody could run that instruction on a schedule. Issue 1052 is what happened
next: the comment budgets a ~67 KB stack, the shipped `esp32_qemu_talker`
had 18,572 B, and it faulted with `sp` 2,548 B outside the stack, inside
`nros_smoltcp::TCP_RX_BUFFER_0`. The measurement was always one command away
and the failure looked like a zenoh-pico bug for as long as nobody took it.

Read the numbers, not just the verdict: this prints every image's stack, so
a shrink that stays above the floor is still visible in a diff.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path

# --------------------------------------------------------------------------
# Floors, per board.
#
# NOT a ratchet over what the tree happens to link today. 18,572 B crashes and
# 22,060 B does not, but 22,060 is not evidence of safety — the listener merely
# runs a shallower call chain than the publisher path. The number to respect is
# the one node.rs and issue 0064 measured for the zenoh-pico handshake + nested
# smoltcp poll path (≈98 KB deep at its worst; ~67 KB budgeted). A floor set
# just above the last crash would bless the next one.
# --------------------------------------------------------------------------
FLOORS = {
    "qemu-esp32-baremetal": 32 * 1024,
}

# Images that do NOT meet their board floor and are allowed to link anyway,
# each with the issue that tracks getting it there. An entry here is a DEBT
# ON RECORD, not an exemption from the rule: the recorded value is the worst
# stack the image may have, so it can only improve, and removing the entry is
# what closing the issue looks like.
#
# The repo's rule for a real-but-open gap (CLAUDE.md, the rmw-api-parity map):
# it carries a TRACKED ISSUE ID in the reason, and that is what --check
# tolerates — nothing else.
DEBT: dict[str, tuple[int, str, str]] = {
    # Empty, and that is the point: both ESP32 images now clear the floor on
    # what they DECLARE (issue 1052 — talker 49,148 B, listener 52,636 B, from
    # 18,572 and 22,060). An entry here is a debt on record, never a waiver:
    # the value is a CEILING that may only fall, so the gate fails if an image
    # regresses toward it, and deleting the entry is what closing the issue
    # looks like. That is how the listener left this table — it was added at
    # 22,060 B, the fix took it to 52,636 B, and the gate refused the rise until
    # the row was removed.
}


class Failure(Exception):
    pass


def find_nm() -> str:
    """Locate a RISC-V `nm`, or fail LOUDLY.

    A missing tool must never read as a passing check. During issue 1052 a
    probe spelled `riscv32-esp-elf-nm ... 2>/dev/null | grep -c <sym>` was
    used to decide whether a symbol was present; the tool was not on PATH,
    the redirect ate "command not found", `grep -c` returned 0, and the 0 was
    read as "absent". It returns 0 for every input, so it discriminated
    nothing — and it shipped in a published conclusion. Hence: resolve
    explicitly, raise if absent.
    """
    for name in ("riscv32-esp-elf-nm", "riscv64-unknown-elf-nm", "llvm-nm"):
        found = shutil.which(name)
        if found:
            return found
    for root in (Path.home() / ".espressif" / "tools" / "riscv32-esp-elf",):
        if root.is_dir():
            for cand in sorted(root.glob("*/riscv32-esp-elf/bin/riscv32-esp-elf-nm")):
                if os.access(cand, os.X_OK):
                    return str(cand)
    raise Failure(
        "no RISC-V `nm` found (tried riscv32-esp-elf-nm, riscv64-unknown-elf-nm, "
        "llvm-nm, and ~/.espressif/tools/riscv32-esp-elf/*/riscv32-esp-elf/bin/).\n"
        "Refusing to report a verdict without one — a missing tool is not a pass."
    )


def parse_stack_symbols(nm_output: str) -> tuple[int, int]:
    """Extract (_stack_end, _stack_start) from `nm` output.

    Pure text in, numbers out, so the logic is testable without an ELF.
    """
    end = start = None
    for line in nm_output.splitlines():
        parts = line.split()
        if len(parts) < 3:
            continue
        sym = parts[-1]
        if sym == "_stack_end":
            end = int(parts[0], 16)
        elif sym == "_stack_start":
            start = int(parts[0], 16)
    if end is None or start is None:
        missing = " and ".join(
            n for n, v in (("_stack_end", end), ("_stack_start", start)) if v is None
        )
        raise Failure(
            f"{missing} not found in the symbol table.\n"
            "On this board the stack has no section — it is the gap between these "
            "two linker symbols. Without them there is nothing to measure, which "
            "is a failure to answer, not an answer."
        )
    if start <= end:
        raise Failure(
            f"_stack_start (0x{start:08x}) is not above _stack_end (0x{end:08x}); "
            "the stack region is inverted or empty."
        )
    return end, start


def stack_size(elf: Path, nm: str) -> tuple[int, int, int]:
    proc = subprocess.run(
        [nm, str(elf)], capture_output=True, text=True, check=False
    )
    if proc.returncode != 0:
        raise Failure(f"{nm} failed on {elf}:\n{proc.stderr.strip()}")
    end, start = parse_stack_symbols(proc.stdout)
    return start - end, end, start


def board_of(elf: Path) -> str | None:
    """Which board's floor applies. Path-based: these images are built into
    `build/cargo-fixtures/<board>[-<group cksum>]/…` (phase-340 group dirs)."""
    for part in elf.parts:
        for board in FLOORS:
            if part == board or part.startswith(board + "-"):
                return board
    return None


def verdict(elf: Path, size: int, board: str) -> tuple[bool, str]:
    floor = FLOORS[board]
    name = elf.name
    if name in DEBT:
        cap, issue, reason = DEBT[name]
        if size > cap:
            return False, (
                f"{name}: stack {size:,} B EXCEEDS its recorded debt of {cap:,} B.\n"
                f"    The entry in DEBT is a ceiling that may only fall. It rose, so "
                f"either something regressed or the entry is stale — re-measure and "
                f"lower it, or delete it if the image now clears the {floor:,} B floor."
            )
        return True, (
            f"{name}: {size:,} B — below the {floor:,} B floor, tracked by issue "
            f"{issue} (cap {cap:,} B)\n      {reason}"
        )
    if size < floor:
        return False, (
            f"{name}: stack {size:,} B is BELOW the {board} floor of {floor:,} B.\n"
            f"    The stack on this board is the linker leftover after `.bss`, so a "
            f"static that grew took this directly. There is no runtime overflow "
            f"guard here: the image will not trap, it will write frames into `.bss` "
            f"and die later as a wild jump (issues 0190, 1052).\n"
            f"    Fix the static, do not lower the floor. `just mem-report <elf>` "
            f"ranks what is in `.bss`; the usual answer is a pool sized on a "
            f"configured maximum rather than on what the node declares.\n"
            f"    If it must ship anyway, add it to DEBT in {Path(__file__).name} "
            f"with a tracked issue id."
        )
    return True, f"{name}: {size:,} B (floor {floor:,} B)"


def check(paths: list[str]) -> int:
    # Run the selftest on the NORMAL path, not only under `--selftest`.
    # It is pure text/arithmetic and costs nothing, and it means the controls
    # cannot rot into decoration while the gate keeps reporting verdicts —
    # `check-gate-selftests` enforces this repo-wide.
    selftest(quiet=True)
    nm = find_nm()
    failures = []
    checked = 0
    for p in paths:
        elf = Path(p)
        if not elf.is_file():
            raise Failure(f"{elf}: not a file. Refusing to skip it silently.")
        board = board_of(elf)
        if board is None:
            raise Failure(
                f"{elf}: no board in this path matches a floor in FLOORS "
                f"({', '.join(sorted(FLOORS))}).\n"
                "Passing an image this script cannot classify is how a check goes "
                "quiet — name the board or do not pass the file."
            )
        size, end, start = stack_size(elf, nm)
        ok, msg = verdict(elf, size, board)
        checked += 1
        print(f"  {'[OK]  ' if ok else '[FAIL]'} {msg}")
        if not ok:
            failures.append(msg)
    if checked == 0:
        raise Failure("no images were checked; a check over nothing is not a pass.")
    if failures:
        print(f"\ncheck-stack-floor: {len(failures)} image(s) below floor", file=sys.stderr)
        return 1
    print(f"check-stack-floor: {checked} image(s) OK")
    return 0


def claims() -> int:
    """What this gate asserts, printable without any artifact."""
    print("check-stack-floor asserts, for every image it is given:")
    for board, floor in sorted(FLOORS.items()):
        print(f"  {board}: _stack_start - _stack_end >= {floor:,} B")
    print("\nStack is the linker leftover after `.bss` on these boards; there is no")
    print("runtime overflow guard, so an overflow corrupts `.bss` instead of trapping.")
    if DEBT:
        print("\nRecorded debt (below floor, tracked, ceiling may only fall):")
        for name, (cap, issue, reason) in sorted(DEBT.items()):
            print(f"  {name}: <= {cap:,} B, issue {issue}")
            print(f"      {reason}")
    return 0


def selftest(quiet: bool = False) -> int:
    """Controls, including the negative ones.

    A gate is only worth its runtime if it FAILS on the thing it exists to
    catch, so the below-floor and missing-symbol cases are asserted here, not
    just the passing one.
    """
    nm_ok = "3fcc2404 A _stack_end\n3fcce400 A _stack_start\n"
    end, start = parse_stack_symbols(nm_ok)
    assert start - end == 49_148, start - end

    # tolerate other symbols and orderings
    noise = "42051d70 t some_fn\n3fcce400 A _stack_start\n0 n x\n3fcc9b74 A _stack_end\n"
    end, start = parse_stack_symbols(noise)
    assert start - end == 18_572, start - end

    # NEGATIVE: symbols absent must raise, never default to a size
    for bad in ("", "42051d70 t some_fn\n", "3fcce400 A _stack_start\n"):
        try:
            parse_stack_symbols(bad)
        except Failure:
            pass
        else:
            raise AssertionError(f"missing stack symbols accepted: {bad!r}")

    # NEGATIVE: inverted region must raise
    try:
        parse_stack_symbols("3fcce400 A _stack_end\n3fcc2404 A _stack_start\n")
    except Failure:
        pass
    else:
        raise AssertionError("inverted stack region accepted")

    board = "qemu-esp32-baremetal"
    floor = FLOORS[board]

    # the shipped talker that crashed must FAIL
    ok, msg = verdict(Path("esp32_qemu_talker"), 18_572, board)
    assert not ok, msg
    assert "BELOW" in msg

    # the same image with the pools fixed must PASS
    ok, _ = verdict(Path("esp32_qemu_talker"), 49_148, board)
    assert ok

    # exactly at the floor passes; one byte under fails
    assert verdict(Path("esp32_qemu_talker"), floor, board)[0]
    assert not verdict(Path("esp32_qemu_talker"), floor - 1, board)[0]

    # The DEBT mechanism, exercised with a SYNTHETIC entry. DEBT is empty now
    # that both real images clear the floor, and an untested mechanism is how
    # the next entry gets added wrong — so inject one rather than letting the
    # controls lapse with the table.
    DEBT["_selftest_image"] = (22_060, "1052", "synthetic, selftest only")
    try:
        ok, _ = verdict(Path("_selftest_image"), 22_060, board)
        assert ok, "an image at exactly its cap must pass"
        ok, msg = verdict(Path("_selftest_image"), 22_061, board)
        assert not ok and "EXCEEDS" in msg, msg
        # below the cap is fine — the ceiling may always fall
        assert verdict(Path("_selftest_image"), 1_000, board)[0]
    finally:
        del DEBT["_selftest_image"]

    # board resolution must see phase-340 group dirs, and must NOT invent a
    # board for a path it does not recognise
    assert board_of(Path(f"build/cargo-fixtures/{board}/x/y/talker")) == board
    assert board_of(Path(f"build/cargo-fixtures/{board}-4118800323/x/talker")) == board
    assert board_of(Path("build/cargo-fixtures/some-other-board/x/talker")) is None

    # bin-name parsing: [package] name plus every [[bin]] name, comments
    # stripped, other sections ignored
    import tempfile

    with tempfile.TemporaryDirectory() as td:
        leaf = Path(td)
        (leaf / "Cargo.toml").write_text(
            '[package]\nname = "esp32_qemu_talker"  # trailing comment\n'
            'version = "0.1.0"\n\n'
            '[dependencies]\nname_like_key = "1"\n\n'
            '[[bin]]\nname = "esp32_qemu_talker"\npath = "src/main.rs"\n\n'
            '[[bin]]\nname = "second-bin"\n'
        )
        got = bin_names(leaf)
        assert got == ["esp32_qemu_talker", "second-bin"], got

        # NEGATIVE: a manifest with no name must raise, not return []
        (leaf / "Cargo.toml").write_text('[dependencies]\nfoo = "1"\n')
        try:
            bin_names(leaf)
        except Failure:
            pass
        else:
            raise AssertionError("manifest with no name accepted")

    if not quiet:
        print("check-stack-floor: selftest OK")
    return 0


def bin_names(leaf: Path) -> list[str]:
    """Binary names a leaf produces, from its own `Cargo.toml`.

    Needed because phase-340 group dirs are SHARED: several rows build into one
    `build/cargo-fixtures/<group>/` and the directory therefore holds sibling
    rows' binaries, plus binaries left by earlier env combinations that no row
    produces any more. Globbing it checks other people's artifacts — during
    development that failed the listener's build over a stale talker ELF from
    before the fix under test.
    """
    manifest = leaf / "Cargo.toml"
    if not manifest.is_file():
        raise Failure(f"{manifest}: not found; cannot determine this row's binaries.")
    names: list[str] = []
    section = None
    for raw in manifest.read_text().splitlines():
        line = raw.split("#", 1)[0].strip()
        if line.startswith("[") and line.endswith("]"):
            section = line.strip("[]")
            continue
        if section not in ("package", "bin") or "=" not in line:
            continue
        key, _, val = line.partition("=")
        if key.strip() != "name":
            continue
        val = val.strip().strip('"').strip("'")
        if val and val not in names:
            names.append(val)
    if not names:
        raise Failure(
            f"{manifest}: no `name` under [package] or [[bin]].\n"
            "Refusing to fall back to a directory glob — that would check other "
            "rows' artifacts in the shared group dir."
        )
    return names


def check_row(leaf: str, adir: str) -> int:
    """Check the binaries THIS row produces, inside a shared artifact dir."""
    leafp, adirp = Path(leaf), Path(adir)
    if not adirp.is_dir():
        raise Failure(f"{adirp}: artifact dir does not exist.")
    wanted = set()
    for n in bin_names(leafp):
        wanted.update({n, n.replace("-", "_"), n.replace("_", "-")})
    found = [
        f
        for f in adirp.glob("*/*/*")
        if f.is_file() and os.access(f, os.X_OK) and f.name in wanted
    ]
    if not found:
        raise Failure(
            f"{leafp}: none of {sorted(wanted)} found under {adirp}.\n"
            "The row was just built, so its binary must be there. Refusing to "
            "report a pass over an empty set."
        )
    return check([str(f) for f in sorted(found)])


def main(argv: list[str]) -> int:
    args = argv[1:]
    if not args:
        print(__doc__)
        print(
            "usage: check-stack-floor.py <elf>...\n"
            "       check-stack-floor.py --row <leaf-dir> <artifact-dir>\n"
            "       check-stack-floor.py --selftest | --claims"
        )
        return 2
    if args[0] == "--selftest":
        return selftest()
    if args[0] == "--claims":
        return claims()
    if args[0] == "--row":
        if len(args) != 3:
            raise Failure("--row takes exactly <leaf-dir> <artifact-dir>")
        return check_row(args[1], args[2])
    return check(args)


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv))
    except Failure as exc:
        print(f"check-stack-floor: {exc}", file=sys.stderr)
        sys.exit(1)
