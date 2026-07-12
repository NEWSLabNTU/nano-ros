#!/usr/bin/env python3
"""Phase-287 W6 — migrate embedded canonical role leaves to the native-identical
ament shape (option A, 2026-07-12 maintainer decision).

For each canonical role leaf (talker / listener / service-* / action-*) of an
embedded platform dir, this makes the leaf a byte-identical copy of its native
counterpart except for `package.xml`:

  * CMakeLists.txt  <- examples/native/<lang>/<role>/CMakeLists.txt  (verbatim)
  * src/            <- native src/ (the portable main.c / main.cpp — the
                       NROS_APP_MAIN_REGISTER() + NROS_ENTRY_LOCATOR /
                       NROS_ENTRY_DOMAIN_ID seam), replacing the old
                       component-class sources
  * package.xml     <- native package.xml with the leaf's own <name> kept and
                       the <nano_ros .../> line swapped for the platform tuple

Usage:
  migrate-embedded-example-native-shape.py <platform-dir> <deploy> <board> [rmw]

  e.g. migrate-embedded-example-native-shape.py \
      examples/qemu-arm-freertos freertos mps2-an385-freertos zenoh
"""

import re
import shutil
import subprocess
import sys
from pathlib import Path

ROLES = [
    "talker",
    "listener",
    "service-server",
    "service-client",
    "action-server",
    "action-client",
]


def migrate_leaf(native: Path, leaf: Path, deploy: str, board: str, rmw: str) -> None:
    # 1. CMakeLists.txt — byte-identical with the native counterpart.
    shutil.copyfile(native / "CMakeLists.txt", leaf / "CMakeLists.txt")

    # 2. src/ — replace the component-class sources with the portable main.
    old_sources = list((leaf / "src").glob("*"))
    for p in old_sources:
        subprocess.run(["git", "rm", "-q", "--ignore-unmatch", str(p)], check=True)
        if p.exists():
            p.unlink()
    (leaf / "src").mkdir(exist_ok=True)
    for p in sorted((native / "src").glob("*")):
        shutil.copyfile(p, leaf / "src" / p.name)

    # 3. package.xml — native's, with the leaf's own <name> + the deploy tuple.
    own_name = re.search(r"<name>([^<]+)</name>", (leaf / "package.xml").read_text())
    text = (native / "package.xml").read_text()
    if own_name:
        text = re.sub(r"<name>[^<]+</name>", f"<name>{own_name.group(1)}</name>", text, count=1)
    tuple_line = f'<nano_ros deploy="{deploy}" board="{board}" rmw="{rmw}"/>'
    text, n = re.subn(r"<nano_ros[^>]*/>", tuple_line, text, count=1)
    if n != 1:
        raise SystemExit(f"{native}/package.xml: no <nano_ros .../> line to swap")
    (leaf / "package.xml").write_text(text)


def main() -> None:
    if len(sys.argv) < 4:
        raise SystemExit(__doc__)
    platform_dir = Path(sys.argv[1])
    deploy, board = sys.argv[2], sys.argv[3]
    rmw = sys.argv[4] if len(sys.argv) > 4 else "zenoh"

    migrated = 0
    for lang in ("c", "cpp"):
        for role in ROLES:
            leaf = platform_dir / lang / role
            if not leaf.is_dir():
                continue
            native = Path("examples/native") / lang / role
            if not (native / "CMakeLists.txt").is_file():
                raise SystemExit(f"no native counterpart for {leaf}")
            migrate_leaf(native, leaf, deploy, board, rmw)
            migrated += 1
            print(f"migrated {leaf}")
    print(f"{migrated} leaves -> native-identical shape ({deploy}/{board}/{rmw})")


if __name__ == "__main__":
    main()
