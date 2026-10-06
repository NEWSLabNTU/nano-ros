#!/usr/bin/env python3
"""Decode the nano-ros contract-violation record out of a target memory dump.

phase-474 I1. The executor's monitors write every STORED violation into a
fixed RAM record, `NROS_VIOLATION_RECORD`
(`packages/core/nros-node/src/executor/monitor.rs`, `ViolationRecord`), for a
board whose console reaches nobody. The record is a `#[no_mangle]` static that
exists when the image was built with the boot report (`NROS_BOOT_REPORT=1`,
Zephyr `CONFIG_NROS_BOOT_REPORT=y`), it is never drained, and it keeps the
LATEST `capacity` entries with the running total.

    python3 scripts/read-violation-record.py --addr-only build/zephyr/zephyr.elf
    pyocd commander -t s32k344 -c "savemem ADDR LEN violations.bin"
    python3 scripts/read-violation-record.py build/zephyr/zephyr.elf violations.bin

Reading needs no halt: every word is a `u32` written with atomics, and a slot's
`seq` is written last, so a slot whose `seq` disagrees with its position is
reported as mid-write rather than decoded.

Layout (version 1, little-endian u32 words):

    0 magic "NRVR"   1 version   2 capacity   3 slot_words (7)
    4 total          5 head      6 dropped    7 suppressed_before_arm
    8 armed          9 reserved
    10.. capacity x slot: seq, rule, fqn_hash, measured, declared,
                          fqn_addr, fqn_len

`rule` is an index into RULES below plus one (monitor.rs `RULE_IDS`, append
only). `fqn_hash` is 32-bit FNV-1a of the endpoint ref; `fqn_addr`/`fqn_len`
point at its text in the image's rodata, which the ELF holds.

`--self-test` checks the decoder against a synthetic record.
"""

from __future__ import annotations

import argparse
import importlib.util
import struct
import sys
from pathlib import Path

SYMBOL = "NROS_VIOLATION_RECORD"
MAGIC = 0x4E525652  # "NRVR"; monitor.rs RECORD_MAGIC
KNOWN_VERSION = 1  # monitor.rs RECORD_VERSION
HEADER_WORDS = 10  # monitor.rs RECORD_HEADER_WORDS
SLOT_WORDS = 7  # monitor.rs RECORD_SLOT_WORDS
HEADER = (
    "magic",
    "version",
    "capacity",
    "slot_words",
    "total",
    "head",
    "dropped",
    "suppressed_before_arm",
    "armed",
    "reserved",
)
SLOT = ("seq", "rule", "fqn_hash", "measured", "declared", "fqn_addr", "fqn_len")
# monitor.rs RULE_IDS, in wire order (code = index + 1).
RULES = (
    "rate-hierarchy-runtime",
    "max-age-runtime",
    "max-latency-runtime",
    "deadline-miss-runtime",
    "stack-headroom-runtime",
    "alive-supervision-runtime",
    "silence-runtime",
    "timer-overrun-runtime",
    "release-jitter-runtime",
)


def _boot_report_helpers():
    """`resolve_symbol` and `elf_bytes` from read-boot-report.py: one ELF
    reader for both records rather than a second copy."""
    path = Path(__file__).with_name("read-boot-report.py")
    spec = importlib.util.spec_from_file_location("read_boot_report", path)
    mod = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(mod)
    return mod


def fnv1a32(text: str) -> int:
    h = 0x811C9DC5
    for b in text.encode():
        h = ((h ^ b) * 0x01000193) & 0xFFFFFFFF
    return h


def rule_name(code: int) -> str:
    return RULES[code - 1] if 1 <= code <= len(RULES) else f"rule#{code}"


def decode(blob: bytes) -> tuple[dict[str, int], list[dict[str, int]]]:
    if len(blob) < HEADER_WORDS * 4:
        raise SystemExit(f"dump is {len(blob)} B, shorter than the {HEADER_WORDS * 4} B header")
    hdr = dict(zip(HEADER, struct.unpack_from(f"<{HEADER_WORDS}I", blob, 0)))
    if hdr["magic"] != MAGIC:
        raise SystemExit(f"magic {hdr['magic']:#010x} is not NRVR: not a violation record")
    if hdr["version"] != KNOWN_VERSION:
        raise SystemExit(
            f"record version {hdr['version']}, this decoder knows {KNOWN_VERSION}: "
            "update scripts/read-violation-record.py with monitor.rs"
        )
    if hdr["slot_words"] != SLOT_WORDS:
        raise SystemExit(f"slot_words {hdr['slot_words']} != {SLOT_WORDS}")
    cap = hdr["capacity"]
    need = (HEADER_WORDS + cap * SLOT_WORDS) * 4
    if len(blob) < need:
        raise SystemExit(f"dump is {len(blob)} B, the record is {need} B ({cap} slots)")
    slots = []
    for i in range(cap):
        off = (HEADER_WORDS + i * SLOT_WORDS) * 4
        slots.append(dict(zip(SLOT, struct.unpack_from(f"<{SLOT_WORDS}I", blob, off))))
    return hdr, slots


def newest_first(hdr: dict[str, int], slots: list[dict[str, int]]) -> list[tuple[int, dict | None]]:
    """(seq, slot or None when overwritten/mid-write), newest first."""
    total, cap = hdr["total"], hdr["capacity"]
    out = []
    for k in range(min(total, cap)):
        seq = total - k
        s = slots[(seq - 1) % cap]
        out.append((seq, s if s["seq"] == seq else None))
    return out


def report(hdr, slots, elf: Path | None, helpers=None) -> str:
    lines = [
        f"violation record: total={hdr['total']} dropped={hdr['dropped']} "
        f"capacity={hdr['capacity']} suppressed_before_arm={hdr['suppressed_before_arm']} "
        f"armed_executors={hdr['armed']}"
    ]
    if hdr["total"] == 0:
        lines.append("  no violation stored since boot")
    for seq, s in newest_first(hdr, slots):
        if s is None:
            lines.append(f"  #{seq}: (being written or overwritten)")
            continue
        fqn = None
        if helpers is not None and elf is not None:
            raw = helpers.elf_bytes(elf, s["fqn_addr"], s["fqn_len"])
            if raw is not None:
                fqn = raw.decode("utf-8", "replace")
        name = fqn if fqn is not None else f"fqn#{s['fqn_hash']:08x}"
        lines.append(
            f"  #{seq}: {rule_name(s['rule'])} {name} "
            f"measured={s['measured']} declared={s['declared']}"
        )
    return "\n".join(lines)


def make_record(cap: int, entries: list[tuple[str, str, int, int]], **hdr_over: int) -> bytes:
    words = [MAGIC, KNOWN_VERSION, cap, SLOT_WORDS, 0, 0, 0, 0, 0, 0] + [0] * (cap * SLOT_WORDS)
    total = 0
    for rule, fqn, measured, declared in entries:
        total += 1
        i = (total - 1) % cap
        base = HEADER_WORDS + i * SLOT_WORDS
        words[base : base + SLOT_WORDS] = [
            total,
            RULES.index(rule) + 1,
            fnv1a32(fqn),
            measured,
            declared,
            0,
            0,
        ]
    words[4] = total
    words[5] = total % cap
    words[6] = max(0, total - cap)
    for k, v in hdr_over.items():
        words[HEADER.index(k)] = v
    return struct.pack(f"<{len(words)}I", *words)


def self_test(quiet: bool = False) -> int:
    assert fnv1a32("") == 0x811C9DC5 and fnv1a32("a") == 0xE40C292C
    entries = [("timer-overrun-runtime", "timer", m, 0) for m in range(1, 6)]
    hdr, slots = decode(make_record(3, entries, suppressed_before_arm=8, armed=1))
    assert hdr["total"] == 5 and hdr["dropped"] == 2, hdr
    got = [(seq, s["measured"]) for seq, s in newest_first(hdr, slots)]
    assert got == [(5, 5), (4, 4), (3, 3)], got
    text = report(hdr, slots, None)
    assert "suppressed_before_arm=8" in text and "#5: timer-overrun-runtime" in text, text
    hdr, slots = decode(make_record(4, []))
    assert "no violation stored" in report(hdr, slots, None)
    if not quiet:
        print("read-violation-record: self-test ok", file=sys.stderr)
    return 0


def check_layout(monitor_rs: Path) -> int:
    """The decoder's constants and rule table against monitor.rs -- the two
    halves of one wire format, which must move together."""
    import re

    src = monitor_rs.read_text()
    errs = []

    def const(name: str) -> int:
        m = re.search(rf"pub const {name}: u32 = ([0-9a-fx_]+);", src)
        if not m:
            errs.append(f"{name} not found in {monitor_rs}")
            return -1
        return int(m.group(1).replace("_", ""), 0)

    for name, mine in (
        ("RECORD_MAGIC", MAGIC),
        ("RECORD_VERSION", KNOWN_VERSION),
        ("RECORD_HEADER_WORDS", HEADER_WORDS),
        ("RECORD_SLOT_WORDS", SLOT_WORDS),
    ):
        theirs = const(name)
        if theirs != mine:
            errs.append(f"{name}: monitor.rs {theirs:#x}, decoder {mine:#x}")
    m = re.search(r"pub const RULE_IDS: \[&str; \d+\] = \[(.*?)\];", src, re.S)
    rules = tuple(re.findall(r'"([a-z-]+)"', m.group(1))) if m else ()
    if rules != RULES:
        errs.append(f"RULE_IDS: monitor.rs {rules}, decoder {RULES}")
    for e in errs:
        print(f"read-violation-record: {e}", file=sys.stderr)
    return 1 if errs else 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("elf", type=Path, nargs="?", help="the image the board is running")
    ap.add_argument("dump", type=Path, nargs="?", help="the record's bytes, dumped from the target")
    ap.add_argument("--addr-only", action="store_true", help="print ADDR LEN for savemem")
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument(
        "--check-layout",
        type=Path,
        metavar="MONITOR_RS",
        help="compare the decoder's layout constants and rule table with monitor.rs",
    )
    a = ap.parse_args()
    if a.self_test:
        return self_test()
    # The negative controls run on every path, so a decoder that drifted from
    # its own fixtures fails before it decodes a board's record.
    self_test(quiet=True)
    if a.check_layout:
        return check_layout(a.check_layout)
    if a.elf is None:
        ap.error("ELF required")
    helpers = _boot_report_helpers()
    if a.addr_only:
        addr, size = helpers.resolve_symbol(a.elf, SYMBOL)
        print(f"{addr:#x} {size}")
        return 0
    if a.dump is None:
        ap.error("dump required (or --addr-only)")
    hdr, slots = decode(a.dump.read_bytes())
    print(report(hdr, slots, a.elf, helpers))
    return 0


if __name__ == "__main__":
    sys.exit(main())
