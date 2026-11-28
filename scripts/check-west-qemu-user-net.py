#!/usr/bin/env python3
"""A west-road QEMU board states how its NIC reaches the host — issue 3014.

On the west road `nros run` boots Zephyr's own run line (`west build -t
run`), and that line's network comes from one Kconfig choice:
`NET_QEMU_USER` gives `-nic user,model=...` (QEMU's user network: no host
setup, the host at 10.0.2.2), while `NET_QEMU_ETHERNET`, Zephyr's default on
boards such as qemu_cortex_a53, gives `-nic tap,...,ifname=zeth`, which needs
root to create and open. qemu-cortex-a53 left the choice to Zephyr: its
`prj.conf` stated the user network's addresses while its run line was the TAP
one, so `nros run` failed before the guest printed a line on any host without
root. The test harness never saw it, because it boots the descriptor's line
and puts the NIC on the user network itself.

WHAT IT REFUSES: a board package (`packages/boards/*/nros-board.toml`) that
is on the west road (`[board.zephyr] west_board`), runs under QEMU
(`[board.run] emulator = "qemu:..."`), and turns networking on in its
`prj.conf` (`CONFIG_NETWORKING=y`), but states neither choice:

  * `CONFIG_NET_QEMU_USER=y` — the default every such board should take;
  * `CONFIG_NET_QEMU_ETHERNET=y` — a board-level TAP opt-in, for a board
    whose network really is a TAP. It must be stated, never inherited.

An image that wants a TAP on a user-network board opts in with its own
`os_config` fragment; that is the image's fact and this gate does not read
it.

Absolute, no baseline.

Usage::

    check-west-qemu-user-net.py [--selftest]
"""

from __future__ import annotations

import re
import sys
import tempfile
try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11
    import tomli as tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CHOICES = ("CONFIG_NET_QEMU_USER", "CONFIG_NET_QEMU_ETHERNET")


def conf_set(text: str, symbol: str) -> bool:
    """Whether `symbol=y` is an assignment line of a Kconfig fragment."""
    return re.search(rf"(?m)^\s*{symbol}=y\s*$", text) is not None


def west_qemu_entries(doc: dict) -> list[tuple[str, str]]:
    """`(west_board, emulator)` for each board a descriptor defines that is on
    the west road under QEMU. A descriptor holds one `[board]` table or a
    `[[board]]` list."""
    boards = doc.get("board", {})
    out = []
    for b in boards if isinstance(boards, list) else [boards]:
        if not isinstance(b, dict):
            continue
        zephyr = b.get("zephyr", {})
        west_board = zephyr.get("west_board") if isinstance(zephyr, dict) else None
        run = b.get("run", {})
        emulator = str(run.get("emulator", "")) if isinstance(run, dict) else ""
        if west_board and emulator.startswith("qemu:"):
            out.append((west_board, emulator))
    return out


def board_problems(board_dir: Path) -> tuple[bool, list[str]]:
    """Whether one board package is a west-road QEMU board with networking,
    and what is wrong with it."""
    desc = board_dir / "nros-board.toml"
    try:
        doc = tomllib.loads(desc.read_text())
    except (OSError, tomllib.TOMLDecodeError):
        return False, []
    entries = west_qemu_entries(doc)
    if not entries:
        return False, []
    conf = board_dir / "prj.conf"
    text = conf.read_text() if conf.is_file() else ""
    if not conf_set(text, "CONFIG_NETWORKING"):
        return False, []
    if any(conf_set(text, c) for c in CHOICES):
        return True, []
    try:
        rel = conf.relative_to(ROOT)
    except ValueError:
        rel = conf
    west_board, emulator = entries[0]
    return True, [
        f"{rel}: `{west_board}` runs under `{emulator}` on the west road with "
        "CONFIG_NETWORKING=y and states no QEMU network choice, so Zephyr's "
        "default decides its run line (on qemu_cortex_a53 a TAP, `ifname=zeth`, "
        "which needs root). State CONFIG_NET_QEMU_USER=y (QEMU's user network, "
        "the host at 10.0.2.2), or CONFIG_NET_QEMU_ETHERNET=y for a board whose "
        "network really is a TAP."
    ]


def scan(boards: Path) -> tuple[int, list[str]]:
    seen = 0
    problems: list[str] = []
    for desc in sorted(boards.glob("*/nros-board.toml")):
        in_scope, p = board_problems(desc.parent)
        seen += 1 if in_scope else 0
        problems.extend(p)
    return seen, problems


def selftest() -> int:
    bad = 0

    def check(name: str, ok: bool) -> None:
        nonlocal bad
        print(f"  {'ok  ' if ok else 'FAIL'} {name}")
        bad += 0 if ok else 1

    def board(root: Path, name: str, toml: str, conf: str | None) -> Path:
        d = root / "packages" / "boards" / name
        d.mkdir(parents=True)
        (d / "nros-board.toml").write_text(toml)
        if conf is not None:
            (d / "prj.conf").write_text(conf)
        return d

    west_qemu = ('[board.zephyr]\nwest_board = "qemu_cortex_a53"\n'
                 '[board.run]\nemulator = "qemu:virt"\n')
    with tempfile.TemporaryDirectory() as t:
        r = Path(t)
        unstated = board(r, "a", west_qemu, "CONFIG_NETWORKING=y\n")
        check("an unstated choice on a networked west-road QEMU board is refused",
              len(board_problems(unstated)[1]) == 1)
        user = board(r, "b", west_qemu, "CONFIG_NETWORKING=y\nCONFIG_NET_QEMU_USER=y\n")
        check("NET_QEMU_USER passes", board_problems(user) == (True, []))
        tap = board(r, "c", west_qemu, "CONFIG_NETWORKING=y\nCONFIG_NET_QEMU_ETHERNET=y\n")
        check("a stated TAP opt-in passes", board_problems(tap) == (True, []))
        commented = board(r, "d", west_qemu,
                          "CONFIG_NETWORKING=y\n# CONFIG_NET_QEMU_USER=y\n")
        check("a commented-out choice is no choice", len(board_problems(commented)[1]) == 1)
        no_net = board(r, "e", west_qemu, "CONFIG_PRINTK=y\n")
        check("a board without networking is out of scope", board_problems(no_net) == (False, []))
        renode = board(r, "f", '[board.zephyr]\nwest_board = "nucleo_f767zi"\n'
                       '[board.run]\nemulator = "renode:nucleo_f767zi"\n',
                       "CONFIG_NETWORKING=y\n")
        check("a Renode board is out of scope", board_problems(renode) == (False, []))
        not_west = board(r, "g", '[board.run]\nemulator = "qemu:mps2-an385"\n',
                         "CONFIG_NETWORKING=y\n")
        check("a board off the west road is out of scope",
              board_problems(not_west) == (False, []))
        listed = board(r, "h", '[[board]]\nname = "x"\n[board.zephyr]\n'
                       'west_board = "qemu_cortex_a53"\n[board.run]\nemulator = "qemu:virt"\n',
                       "CONFIG_NETWORKING=y\n")
        check("a `[[board]]` list descriptor is read too",
              len(board_problems(listed)[1]) == 1)
    print(f"check-west-qemu-user-net self-test: {'OK' if bad == 0 else 'FAILED'}")
    return 1 if bad else 0


def main(argv: list[str]) -> int:
    if "--selftest" in argv:
        return selftest()
    if selftest() != 0:
        return 1
    seen, problems = scan(ROOT / "packages" / "boards")
    if problems:
        print(f"check-west-qemu-user-net: {len(problems)} board(s) leave the QEMU "
              "network to Zephyr's default:")
        for p in problems:
            print(f"  {p}")
        return 1
    if seen == 0:
        print("check-west-qemu-user-net: no west-road QEMU board found; the scan "
              "reached nothing (packages/boards moved?)")
        return 1
    print(f"check-west-qemu-user-net: OK ({seen} west-road QEMU board(s), each "
          "states its network)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
