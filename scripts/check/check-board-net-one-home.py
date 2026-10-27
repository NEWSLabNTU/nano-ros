#!/usr/bin/env python3
"""check-board-net-one-home — a migrated board's network identity has one home.

RFC-0103 D1/D9/D10 / phase-484 W4b, phase-497 W2. A board's fallback IP /
netmask / gateway / MAC is its network's plan (`[board.net] network`), or the
platform default, and every copy is generated from it: the crate's
`Config::default()` (`BOARD_NET_*`), its cargo-road C `NROS_APP_CONFIG`, and
its cmake-road one (`nros board net`). An IMAGE's address is the image's
(`[image.<id>.net.<if>]`, applied over that default through `NROS_IMAGE_NET`),
and the board-level `ip` / `mac` keys that used to restate one image's identity
are refused when the descriptor is read (D10g). This is a RATCHET over the boards
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
    "rv-virt-threadx": [
        "packages/boards/nros-board-threadx-qemu-riscv64/src/config.rs",
        "packages/boards/nros-board-common/src/threadx_qemu_riscv64_build.rs",
        "cmake/board/nano-ros-board-rv-virt-threadx.cmake",
        # phase-497 W2 — the C board's `cfg_ip` / `cfg_mac` statics held a
        # third copy (10.0.2.40, :56) until `nros_threadx_set_config` ran.
        "packages/boards/nros-board-threadx-qemu-riscv64/c/board_threadx_qemu_riscv64.c",
    ],
    # The FreeRTOS family (mps2-an385, mps3-an536, s32z270, freertos-posix
    # state no `[board.net]`: they ARE the platform default). The generic
    # crate's presets derive from `BaseConfig::default()`, the cargo road's C
    # emitter resolves the overlay's descriptor, and the cmake template asks
    # `nros_board_net_c`. `freertos_build.rs` is not listed: its tests assert
    # the emitted text, which is a literal on purpose.
    # phase-484 W4b-2 (RFC-0103 D10) — the QEMU boards on QEMU's own network.
    # `mps2-an385` (bare-metal) is NOT listed: its `docker` profile still
    # carries a second network as literals until phase-497 makes it a
    # `[network.docker]` row.
    "esp32-qemu": [
        "packages/boards/nros-board-esp32-qemu/src/config.rs",
        "packages/boards/nros-board-esp32-qemu/src/board_entry.rs",
    ],
    "nuttx-qemu": [
        "packages/boards/nros-board-nuttx-qemu/src/config.rs",
        "packages/boards/nros-board-nuttx-qemu/src/entry_212n.rs",
    ],
    "freertos": [
        "packages/boards/nros-board-freertos/src/config.rs",
        "cmake/templates/freertos_app_config.c.in",
        "cmake/NanoRosBoardNet.cmake",
    ],
}

# A literal 4-octet IP or 6-byte MAC array assigned to an identity field.
LITERAL = re.compile(
    r"""\.?(?:cfg_)?(?:ip|mac|gateway|netmask)(?:\[\d+\])?\s*[:=]\s*[\[{]\s*(?:0x[0-9a-fA-F]+|\d+)\s*,\s*(?:0x[0-9a-fA-F]+|\d+)\s*,"""
)


def self_test() -> None:
    for hit in [
        "ip: [192, 0, 3, 10],",
        ".ip      = { 10, 0, 2, 40 },",
        ".mac = {0x52, 0x54, 0x00",
        '"        .gateway = {192, 0, 3, 1},"',
        "static uint8_t cfg_ip[4]      = {10, 0, 2, 40};",
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
