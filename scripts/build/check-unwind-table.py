#!/usr/bin/env python3
"""A host executable that carries unwind data must carry its search table.

Issue 2664. A native-simulator Zephyr image (`zephyr.exe`) is a host process,
and native_sim ends a thread with `pthread_exit`, which unwinds that thread's
stack. The unwinder finds each frame's FDE through `.eh_frame_hdr`'s binary
search table. When ld cannot parse an input's `.eh_frame` it says so ("error
in <object>(.eh_frame); no .eh_frame_hdr table will be created"), as a
warning, and links anyway: the header is written with no table, the unwinder
falls back to walking `.eh_frame` in order, and on the unparsable entry it
aborts with no message. Whether the walk reaches that entry depends on layout,
so an image boots or dies at random with unrelated code changes.

This refuses such an executable, so the fixture build fails at the image
instead of a runtime cell failing later with "exited with code 1" and no
output. An executable with no `.eh_frame` at all needs no table and passes.

Usage:
    check-unwind-table.py [--log <build log>] <executable>...
    check-unwind-table.py --self-test

Exit 0 when every executable is sound, 1 when one is not, 2 on a usage or
read error. With `--log`, the refusal quotes ld's own line from that log.
"""

from __future__ import annotations

import argparse
import shutil
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

# DW_EH_PE value formats (the low nibble) and their sizes. 0xff is "omit".
_PE_SIZE = {0x02: 2, 0x03: 4, 0x04: 8, 0x0A: 2, 0x0B: 4, 0x0C: 8}
DW_EH_PE_OMIT = 0xFF
LD_LINE = "no .eh_frame_hdr table will be created"


def sections(data: bytes) -> tuple[dict[str, tuple[int, int]], str]:
    """({name: (file offset, size)}, struct byte order) of a 64- or 32-bit ELF."""
    if data[:4] != b"\x7fELF":
        raise ValueError("not an ELF file")
    is64 = data[4] == 2
    end = "<" if data[5] == 1 else ">"
    if is64:
        shoff, = struct.unpack_from(end + "Q", data, 0x28)
        shentsize, shnum, shstrndx = struct.unpack_from(end + "HHH", data, 0x3A)
    else:
        shoff, = struct.unpack_from(end + "I", data, 0x20)
        shentsize, shnum, shstrndx = struct.unpack_from(end + "HHH", data, 0x2E)

    def header(i: int) -> tuple[int, int, int]:
        base = shoff + i * shentsize
        if is64:
            name, = struct.unpack_from(end + "I", data, base)
            off, size = struct.unpack_from(end + "QQ", data, base + 0x18)
        else:
            name, = struct.unpack_from(end + "I", data, base)
            off, size = struct.unpack_from(end + "II", data, base + 0x10)
        return name, off, size

    _, stroff, _ = header(shstrndx)
    out = {}
    for i in range(shnum):
        name_off, off, size = header(i)
        name = data[stroff + name_off:data.index(b"\0", stroff + name_off)].decode()
        out[name] = (off, size)
    return out, end


def verdict(path: Path) -> str:
    """'' when `path` is sound, else why not."""
    data = path.read_bytes()
    secs, end = sections(data)
    if secs.get(".eh_frame", (0, 0))[1] == 0:
        return ""
    if ".eh_frame_hdr" not in secs:
        return "it has unwind data (.eh_frame) and no .eh_frame_hdr section"
    off, size = secs[".eh_frame_hdr"]
    hdr = data[off:off + size]
    if len(hdr) < 4:
        return f"its .eh_frame_hdr is {len(hdr)} byte(s), shorter than the header"
    _version, ptr_enc, count_enc, table_enc = hdr[0], hdr[1], hdr[2], hdr[3]
    if count_enc == DW_EH_PE_OMIT or table_enc == DW_EH_PE_OMIT:
        return (f"its .eh_frame_hdr ({size} bytes) has NO search table "
                "(the FDE count is omitted)")
    at = 4
    ptr_size = _PE_SIZE.get(ptr_enc & 0x0F)
    count_size = _PE_SIZE.get(count_enc & 0x0F)
    if ptr_size is None or count_size is None:
        return (f"its .eh_frame_hdr uses an encoding this check does not read "
                f"(0x{ptr_enc:02x}/0x{count_enc:02x}), so it cannot be shown sound")
    at += ptr_size
    if len(hdr) < at + count_size:
        return f"its .eh_frame_hdr ({size} bytes) ends before its FDE count"
    fmt = {2: "H", 4: "I", 8: "Q"}[count_size]
    count, = struct.unpack_from(end + fmt, hdr, at)
    if count == 0:
        return f"its .eh_frame_hdr ({size} bytes) indexes 0 FDEs"
    return ""


def ld_lines(log: Path | None) -> list[str]:
    if log is None or not log.is_file():
        return []
    return [line.strip() for line in log.read_text(errors="replace").splitlines()
            if LD_LINE in line]


def report(paths: list[Path], log: Path | None) -> int:
    bad = []
    for p in paths:
        try:
            why = verdict(p)
        except (OSError, ValueError, struct.error, IndexError) as e:
            print(f"check-unwind-table: cannot read {p}: {e}", file=sys.stderr)
            return 2
        if why:
            bad.append((p, why))
    if not bad:
        return 0
    for p, why in bad:
        print(f"check-unwind-table: {p}: {why}.", file=sys.stderr)
    lines = ld_lines(log)
    if lines:
        print("  ld said, while linking it:", file=sys.stderr)
        for line in lines[:4]:
            print(f"    {line}", file=sys.stderr)
    print("  Without the table, a thread exit unwinds by walking .eh_frame in order\n"
          "  and aborts on the entry ld could not parse, so the image dies with no\n"
          "  message, or not, depending on layout. Find the input ld named and keep\n"
          "  its .eh_frame parsable; do not ship the image.", file=sys.stderr)
    return 1


def self_test() -> int:
    cc = shutil.which("cc") or shutil.which("gcc")
    if cc is None:
        print("check-unwind-table self-test: FAIL, no host C compiler (cc)")
        return 1
    failures = []
    with tempfile.TemporaryDirectory() as d:
        src = Path(d) / "t.c"
        src.write_text("int main(void) { return 0; }\n")
        good = Path(d) / "good"
        subprocess.run([cc, "-o", str(good), str(src)], check=True)
        if verdict(good):
            failures.append(f"a normal executable was refused: {verdict(good)}")
        # The state ld leaves after "no .eh_frame_hdr table will be created":
        # the header stays, its FDE count and table encodings are omitted.
        data = bytearray(good.read_bytes())
        off, _ = sections(bytes(data))[0][".eh_frame_hdr"]
        data[off + 2] = DW_EH_PE_OMIT
        data[off + 3] = DW_EH_PE_OMIT
        empty = Path(d) / "empty"
        empty.write_bytes(bytes(data))
        if "NO search table" not in verdict(empty):
            failures.append("a header with no search table was not refused")
        # A zero FDE count in a well-formed header.
        data = bytearray(good.read_bytes())
        enc = data[off + 1] & 0x0F
        count_at = off + 4 + _PE_SIZE[enc]
        data[count_at:count_at + 4] = b"\0\0\0\0"
        zero = Path(d) / "zero"
        zero.write_bytes(bytes(data))
        if "0 FDEs" not in verdict(zero):
            failures.append("a header indexing 0 FDEs was not refused")
        nohdr = Path(d) / "nohdr"
        subprocess.run([cc, "-Wl,--no-eh-frame-hdr", "-o", str(nohdr), str(src)],
                       check=True)
        if "no .eh_frame_hdr section" not in verdict(nohdr):
            failures.append("unwind data with no .eh_frame_hdr was not refused")
        log = Path(d) / "build.log"
        log.write_text(f"/usr/bin/ld: error in x.o(.eh_frame); {LD_LINE}\n")
        if not ld_lines(log):
            failures.append("ld's line was not found in a log that has it")
    if failures:
        for f in failures:
            print(f"check-unwind-table self-test: FAIL, {f}")
        return 1
    print("check-unwind-table self-test: OK (5 cases)")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--log", type=Path, help="the build log, to quote ld's line")
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("executables", nargs="*", type=Path)
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    if not args.executables:
        ap.error("name at least one executable")
    return report(args.executables, args.log)


if __name__ == "__main__":
    sys.exit(main())
