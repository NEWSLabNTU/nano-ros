#!/usr/bin/env python3
"""Refuse a NuttX export whose archives hold an object of another machine.

Issues 2773 / 2807: an arch switch in the shared NuttX tree whose distclean
did not complete left the previous arch's objects behind, `ar` (which checks
no machine type) archived them into the next arch's libraries, and the export
key was written anyway, so the poisoned export read as current. `ld` then
reported `file in wrong format` or `cannot find -lboards`, naming neither the
member nor how it got there.

    check-export-machine.py <export-dir> <arch>     # arch: arm | riscv
    check-export-machine.py                         # self-test only (the gate)

Every run starts with the self-test (a planted foreign member in a synthetic
archive), so a checker that stopped seeing foreign members cannot pass an
export.

Reads every `*.a` under `<export-dir>` (ar format parsed here, no binutils
needed) and every ELF member's `e_machine`. Exit 1 names each foreign member.
"""

import struct
import sys
import tempfile
from pathlib import Path

# ELF e_machine per NuttX CONFIG_ARCH, with the `-` dropped as build-nuttx.sh
# drops it (`risc-v` -> `riscv`).
MACHINES = {"arm": 40, "riscv": 243}
NAMES = {40: "ARM", 243: "RISC-V", 62: "x86-64", 3: "x86", 183: "AArch64"}


def members(data: bytes):
    """Yield (name, payload) for each member of a System V / GNU ar archive."""
    if not data.startswith(b"!<arch>\n"):
        return
    off, longnames = 8, b""
    while off + 60 <= len(data):
        hdr = data[off:off + 60]
        name = hdr[0:16].decode("latin-1").rstrip()
        size = int(hdr[48:58].decode("latin-1").strip() or 0)
        body = data[off + 60:off + 60 + size]
        off += 60 + size + (size & 1)
        if name == "//":
            longnames = body
            continue
        if name in ("/", "/SYM64/") or name.startswith("__.SYMDEF"):
            continue
        if name.startswith("/") and name[1:].isdigit():
            i = int(name[1:])
            name = longnames[i:longnames.index(b"\n", i)].decode("latin-1")
        yield name.rstrip("/"), body


def machine(body: bytes):
    """An ELF object's e_machine, or None for a non-ELF member."""
    if len(body) < 20 or body[:4] != b"\x7fELF" or body[5] not in (1, 2):
        return None
    return struct.unpack_from("<H" if body[5] == 1 else ">H", body, 18)[0]


def foreign(export: Path, want: int):
    out = []
    for lib in sorted(export.rglob("*.a")):  # walk-ok: untracked build output in one export dir
        for name, body in members(lib.read_bytes()):
            m = machine(body)
            if m is not None and m != want:
                out.append((lib.relative_to(export), name, m))
    return out


def check(export: Path, arch: str, quiet: bool = False) -> int:
    want = MACHINES.get(arch)
    if want is None:
        print(f"check-export-machine: unknown arch {arch!r} (know {sorted(MACHINES)})",
              file=sys.stderr)
        return 2
    if not any(export.rglob("*.a")):  # walk-ok: untracked build output in one export dir
        # A missing or empty export is never "clean": the check would pass
        # anything it could not read.
        if not quiet:
            print(f"check-export-machine: no archive under {export}", file=sys.stderr)
        return 1
    bad = foreign(export, want)
    if not bad:
        return 0
    if quiet:
        return 1
    print(f"check-export-machine: {len(bad)} archive member(s) in {export} are not "
          f"{NAMES[want]} objects (the previous arch's objects "
          "survived the reconfigure's clean):", file=sys.stderr)
    for lib, name, m in bad[:40]:
        print(f"  {lib}({name}): {NAMES.get(m, f'EM {m}')}", file=sys.stderr)
    if len(bad) > 40:
        print(f"  ... and {len(bad) - 40} more", file=sys.stderr)
    return 1


def _self_test(verbose: bool) -> int:
    def elf(em: int) -> bytes:
        return b"\x7fELF\x01\x01\x01" + b"\0" * 11 + struct.pack("<H", em) + b"\0" * 32

    def ar(entries) -> bytes:
        out, table = [b"!<arch>\n"], b""
        for name, body in entries:
            if len(name) > 15:
                ref = f"/{len(table)}"
                table += name.encode() + b"/\n"
                name = ref
            else:
                name += "/"
            out.append(f"{name:<16}{0:<12}{0:<6}{0:<6}{644:<8}{len(body):<10}`\n".encode()
                       + body + (b"\n" if len(body) & 1 else b""))
        if table:
            out.insert(1, f"{'//':<16}{'':<32}{len(table):<10}`\n".encode() + table
                       + (b"\n" if len(table) & 1 else b""))
        return b"".join(out)

    fails = 0

    def case(what, cond):
        nonlocal fails
        if verbose or not cond:
            print(f"  {'ok  ' if cond else 'FAIL'} {what}", file=sys.stderr)
        fails += not cond

    with tempfile.TemporaryDirectory() as t:
        root = Path(t)
        (root / "libs").mkdir()
        (root / "libs/libboards.a").write_bytes(ar([("boardctl.o", elf(243)),
                                                    ("dummy.o", elf(40))]))
        (root / "libs/libc.a").write_bytes(ar([("a_member_name_past_15.o", elf(40)),
                                               ("notes.txt", b"not elf")]))
        bad = foreign(root, MACHINES["riscv"])
        case("an ARM member in a riscv export is named",
             ("libboards.a", "dummy.o") in {(str(l.name), n) for l, n, _ in bad})
        case("a long (GNU //) member name resolves",
             any(n == "a_member_name_past_15.o" for _, n, _ in bad))
        case("a non-ELF member is not judged", all(n != "notes.txt" for _, n, _ in bad))
        case("the riscv member is not reported", all(n != "boardctl.o" for _, n, _ in bad))
        case("the arm export is clean apart from its riscv member",
             [n for _, n, _ in foreign(root, MACHINES["arm"])] == ["boardctl.o"])
        case("check() fails a poisoned export", check(root, "riscv", quiet=True) == 1)
        (root / "libs/libboards.a").write_bytes(ar([("boardctl.o", elf(243))]))
        (root / "libs/libc.a").write_bytes(ar([("lib_strchr.o", elf(243))]))
        case("check() passes a clean export", check(root, "riscv", quiet=True) == 0)
        case("an export with no archive is refused, not passed",
             check(root / "nowhere", "riscv", quiet=True) == 1)
    if fails:
        print(f"check-export-machine self-test: {fails} FAILED", file=sys.stderr)
    return 1 if fails else 0


def main(argv) -> int:
    args = [a for a in argv[1:] if a != "--self-test"]
    if args and len(args) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    if _self_test(verbose=not args):
        return 1
    if not args:
        print("check-export-machine: OK (self-test: a foreign archive member is named, "
              "long GNU names resolve, non-ELF members are not judged)")
        return 0
    return check(Path(args[0]), args[1])


if __name__ == "__main__":
    sys.exit(main(sys.argv))
