"""The FreeRTOS family config directory, and a reader that follows its includes.

Issue 2687 — a board's `FreeRTOSConfig.h`, `lwipopts.h` and `arch/cc.h` state
the board's facts and include the family's by NAME
(`#include "nros_freertos_family_config.h"`). The compiler finds those through
the family directory on the include path, after the board's own config dir.
So two things a script can get wrong:

- a COMPILE probe of a FreeRTOS config TU needs `-I <family dir>` after the
  board's `-I`, or the include fails;
- a TEXT reader of a board's `FreeRTOSConfig.h` sees only the board's facts,
  and reads every family setting as absent.

The directory comes from ONE place, `FREERTOS_FAMILY_CONFIG_DIR` in
`nros-build-paths` (the cargo road's resolver), read here rather than spelled
a second time. The CMake road's twin is `nros_freertos_family_config_dir()`.

Usage:
    python3 scripts/lib/freertos_config.py family-dir   # absolute path, for -I
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
_RESOLVER = "packages/tooling/nros-build-paths/src/lib.rs"
_CONST = re.compile(r'pub const FREERTOS_FAMILY_CONFIG_DIR: &str = "([^"]+)";')


def family_config_dir(root: Path = ROOT) -> Path:
    """The family config dir under `root`, from the Rust resolver's constant."""
    text = (ROOT / _RESOLVER).read_text(encoding="utf-8")
    m = _CONST.search(text)
    if m is None:
        raise RuntimeError(f"{_RESOLVER}: no FREERTOS_FAMILY_CONFIG_DIR constant")
    return Path(root) / m.group(1)


def read_config(path: Path, root: Path = ROOT, _depth: int = 0) -> str | None:
    """The text of a FreeRTOS config header with its quoted includes inlined.

    A quoted include is looked up as the compiler would on a board build:
    beside the including file, then in the family dir. `#include <...>` and
    conditional compilation are left alone; that is the shape these headers
    use. Returns None when `path` does not exist.
    """
    path = Path(path)
    if not path.is_file():
        return None
    if _depth > 8:  # a cycle must not hang a gate
        return ""
    out: list[str] = []
    search = [path.parent, family_config_dir(root)]
    for line in path.read_text(encoding="utf-8").splitlines():
        m = re.match(r'\s*#\s*include\s+"([^"]+)"', line)
        if m:
            for d in search:
                cand = d / m.group(1)
                if cand.is_file():
                    out.append(read_config(cand, root, _depth + 1) or "")
                    break
            continue
        out.append(line)
    return "\n".join(out) + "\n"


def main(argv: list[str]) -> int:
    if argv[1:] == ["family-dir"]:
        print(family_config_dir())
        return 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
