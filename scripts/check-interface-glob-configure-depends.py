#!/usr/bin/env python3
"""phase-363 (W2's class) — an interface glob must carry CONFIGURE_DEPENDS.

`file(GLOB)` runs at CONFIGURE time only. A glob over `msg/*.msg` (or `.srv`,
`.action`) therefore captures the interface set ONCE, and adding a message
afterwards leaves the build generating the OLD set until something unrelated
forces a reconfigure. The generated sources are a function of that list, so a
stale list is a museum artifact — the exact defect phase-363 W2 fixed in
`cmake/NanoRosGenerateInterfaces.cmake`.

WHY A GATE: W2 fixed the file it was looking at. The Zephyr module carries a COPY
of the same function, and it kept the bug for four more days until the phase's
standing re-sweep found it. Every one of phase-363's re-sweeps found the class
surviving in a sibling file; this makes the third occurrence impossible rather
than merely unlikely.

WHY ONLY INTERFACE EXTENSIONS: the tree has other unflagged globs, and they are
deliberately out of scope. Globs over vendored ThreadX/CycloneDDS sources only
gain files on a submodule bump, which reconfigures anyway; globs over an SDK
store are one-shot discovery, not a build input set. `.msg`/`.srv`/`.action` are
USER content that changes with nothing else moving, which is what makes a stale
capture reachable. A gate that flagged all 39 would be noise, and noise is how a
gate gets bypassed.
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# `file(GLOB …)` / `file(GLOB_RECURSE …)` up to the closing paren, one line.
GLOB = re.compile(r"file\s*\(\s*GLOB(?:_RECURSE)?\b([^)]*)\)", re.IGNORECASE)
INTERFACE_EXT = re.compile(r"\*\.(msg|srv|action)\b")


def main() -> int:
    listing = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "--", "*.cmake", "*CMakeLists.txt"],
        capture_output=True, text=True, check=True,
    ).stdout.split()

    offenders: list[tuple[str, int, str]] = []
    checked = 0
    for rel in listing:
        if rel.startswith("third-party/"):
            continue
        p = ROOT / rel
        try:
            text = p.read_text(encoding="utf-8")
        except (UnicodeDecodeError, FileNotFoundError):
            continue
        for n, line in enumerate(text.splitlines(), 1):
            for m in GLOB.finditer(line):
                body = m.group(1)
                if not INTERFACE_EXT.search(body):
                    continue
                checked += 1
                if "CONFIGURE_DEPENDS" not in body.upper():
                    offenders.append((rel, n, line.strip()[:100]))

    if offenders:
        print("[FAIL] interface glob without CONFIGURE_DEPENDS (phase-363 / W2):")
        for rel, n, line in offenders:
            print(f"         {rel}:{n}")
            print(f"           {line}")
        print()
        print("       `file(GLOB)` captures the interface set at CONFIGURE time,")
        print("       so a NEWLY ADDED .msg is invisible until an unrelated")
        print("       reconfigure — and the generated sources are a function of")
        print("       that list, so the build ships the old set.")
        print()
        print("       Add CONFIGURE_DEPENDS:")
        print('         file(GLOB _local_msg CONFIGURE_DEPENDS "…/msg/*.msg")')
        return 1

    print(f"check-interface-glob-configure-depends: OK ({checked} interface glob(s) "
          "across the tree, all CONFIGURE_DEPENDS)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
