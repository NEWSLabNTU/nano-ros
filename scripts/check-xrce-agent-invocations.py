#!/usr/bin/env python3
"""phase-519 W4 — no doc or example may start the XRCE agent by hand.

The agent has one door now, `just xrce agent <locator>` over
`scripts/dev/xrce-agent.sh` (`nros_xrce_agent_exec`). It takes the locator in
nano-ros's own spelling, the string a `system.toml` domain states
(`udp/127.0.0.1:8888`), and starts the agent `just xrce setup` provisioned.

A hand-typed `MicroXRCEAgent udp4 -p 8888` is wrong in two ways the door is not:

  * it RESTATES the transport and port the image's domain owns (RFC-0109 U8),
    so a README copied from a neighbour's keeps the neighbour's port, and the
    device's session open then times out with nothing naming the port;
  * it runs whatever `MicroXRCEAgent` is first on PATH, which is not the agent
    built against the sourced ROS's Fast-DDS (issue 0741's skew).

This is the agent's sibling of `check-zenohd-flag-invocations` (issue 0654),
and it exists for the same reason: the class regenerates every time someone
copies a neighbouring example's run block.

Exempt: history (`/archived/`, `docs/issues/`, `docs/roadmap/` — a record of
what was run is correct as written), `third-party/`, `tmp/`, the test harness
(`packages/testing/`, which spawns its own agent with its own argv and
isolation profile, issue 1009), the door itself and this gate.
"""
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# `MicroXRCEAgent <transport>`: the agent's own transport words. Not a bare
# `MicroXRCEAgent`, which prose legitimately names.
TRANSPORTS = "udp4|udp6|tcp4|tcp6|serial|multiserial|pseudoterminal|canfd"
PAT = re.compile(rf"(?<![\w.-])MicroXRCEAgent(?:\.real)?\s+(?:{TRANSPORTS})\b")


def self_test() -> None:
    """Negative controls on the normal path (phase-472 W9)."""
    for hit in ("MicroXRCEAgent udp4 -p 8888 &",
                "//! MicroXRCEAgent udp4 -p 8888",
                "run `MicroXRCEAgent serial -D /dev/ttyACM0 -b 115200`",
                "build/xrce-agent/MicroXRCEAgent tcp4 -p 2019"):
        assert PAT.search(hit), f"self-test: missed {hit!r}"
    for miss in ("just xrce agent udp/127.0.0.1:8888",
                 "the Micro-XRCE-DDS Agent (`MicroXRCEAgent`) on the host",
                 "Start `zenohd` / `MicroXRCEAgent` on the host loopback",
                 "MicroXRCEAgent udpX -p 1",
                 "my_MicroXRCEAgent udp4 -p 1"):
        assert not PAT.search(miss), f"self-test: flagged {miss!r}"


def is_exempt(path: str) -> bool:
    if path.startswith(("third-party/", "tmp/", "docs/issues/", "docs/roadmap/",
                        "packages/testing/")):
        return True
    if "/archived/" in path:
        return True
    return path in {
        "scripts/dev/xrce-agent.sh",
        "scripts/check-xrce-agent-invocations.py",
    }


def main() -> int:
    self_test()
    listing = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files"],
        capture_output=True, text=True, check=True,
    ).stdout.split()

    hits: list[tuple[str, int, str]] = []
    for rel in listing:
        if is_exempt(rel):
            continue
        try:
            text = (ROOT / rel).read_text(encoding="utf-8")
        except (UnicodeDecodeError, FileNotFoundError, IsADirectoryError):
            continue
        if "MicroXRCEAgent" not in text:
            continue
        for n, line in enumerate(text.splitlines(), 1):
            if PAT.search(line):
                hits.append((rel, n, line.strip()[:110]))

    if hits:
        print("[FAIL] the XRCE agent started by hand (phase-519 W4):")
        for rel, n, line in hits:
            print(f"         {rel}:{n}")
            print(f"           {line}")
        print()
        print("       Start it through the one door, with the locator the")
        print("       image's domain states:")
        print("         just xrce agent udp/127.0.0.1:8888")
        print("         just xrce agent <leaf-dir>[:<image>]   # that image's locator")
        print("       Shell: `nros_xrce_agent_exec <locator>` (scripts/dev/xrce-agent.sh).")
        print("       A hand-typed command restates the port the domain owns and")
        print("       runs whatever agent is on PATH, not the ROS-paired one.")
        return 1

    print(f"check-xrce-agent-invocations: OK ({len(listing)} tracked file(s); "
          "no hand-typed agent command outside history and the test harness)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
