#!/usr/bin/env python3
"""check-board-net-one-home — a migrated board's network identity has one home.

RFC-0103 D1/D9 / phase-484 W4b. A board's fallback IP / netmask / gateway /
MAC is stated once, in its descriptor's `[board.net]` (or inherited from the
platform default), and every copy is generated from it: the crate's
`Config::default()` (`BOARD_NET_*`), its cargo-road C `NROS_APP_CONFIG`, and
its cmake-road one (`nros board net`). This is a RATCHET over the boards
already migrated (MIGRATED below): in their files, a literal IP / MAC octet
array — `ip: [192, 0, 3, 10]`, `.ip = {10, 0, 2, 40}`, `.mac = {0x52, …}` —
is a second home coming back. A board joins MIGRATED when its copies go.

Buildless, ~0.1 s; self-tests its pattern on each run.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

# board -> the files that used to carry a copy.
MIGRATED = {
    "threadx-linux": [
        "packages/boards/nros-board-threadx-linux/src/config.rs",
        "packages/boards/nros-board-threadx-linux/build.rs",
        "cmake/board/nano-ros-board-threadx-linux.cmake",
    ],
    "threadx-qemu-riscv64": [
        "packages/boards/nros-board-threadx-qemu-riscv64/src/config.rs",
        "packages/boards/nros-board-common/src/threadx_qemu_riscv64_build.rs",
        "cmake/board/nano-ros-board-rv-virt-threadx.cmake",
    ],
}

# A literal 4-octet IP or 6-byte MAC array assigned to an identity field.
LITERAL = re.compile(
    r"""\.?(?:ip|mac|gateway|netmask)\s*[:=]\s*[\[{]\s*(?:0x[0-9a-fA-F]+|\d+)\s*,\s*(?:0x[0-9a-fA-F]+|\d+)\s*,"""
)


def self_test() -> None:
    for hit in [
        "ip: [192, 0, 3, 10],",
        ".ip      = { 10, 0, 2, 40 },",
        ".mac = {0x52, 0x54, 0x00",
        '"        .gateway = {192, 0, 3, 1},"',
    ]:
        assert LITERAL.search(hit), hit
    for miss in ["ip: BOARD_NET_IP,", ".network = ${_nros_board_net_c},", "// ip: [ ... ] used to be here"]:
        assert not LITERAL.search(miss) or miss.lstrip().startswith("//"), miss


def main() -> int:
    self_test()
    bad = []
    for board, files in MIGRATED.items():
        for rel in files:
            for n, line in enumerate((ROOT / rel).read_text().splitlines(), 1):
                if line.lstrip().startswith(("//", "#", "*", "///")):
                    continue
                if LITERAL.search(line):
                    bad.append(f"  {rel}:{n}: [{board}] {line.strip()[:100]}")
    if bad:
        print("check-board-net-one-home: FAIL — a migrated board states its network "
              "identity outside [board.net]:", file=sys.stderr)
        print("\n".join(bad), file=sys.stderr)
        print("  Read BOARD_NET_* (Rust), ResolvedNet::c_network_initializer (build.rs) or "
              "`nros board net <board> --format c-network` (cmake).", file=sys.stderr)
        return 1
    n = sum(len(f) for f in MIGRATED.values())
    print(f"check-board-net-one-home: OK — {len(MIGRATED)} board(s), {n} file(s), no literal identity")
    return 0


if __name__ == "__main__":
    sys.exit(main())
