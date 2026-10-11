#!/usr/bin/env python3
"""check-cargo-locked — the `--locked` PATH shim stays retired, and the
recipes that build a closed-graph root pass `--locked` themselves.

Phase-485 W4 (RFC-0104 D9). Issues 0359/0378 injected `--locked` everywhere
through `scripts/bin/cargo` and an environment variable. Every enforcement leak
after that (0384, 0386, 0676, 0873, 1307, 1356, 1358) was a new way of
invoking cargo the shim did not see. The replacement is a SET comparison after
resolution, which no invocation path can bypass:

  * a leaf lock is seeded from the registry pin set and checked by `nros
    build` after it resolves (`orchestration::drift`);
  * each locked root's lock is held to the pin set by
    `check-cargo-registry-pins`, whoever rewrote it.

What this gate holds, so the old mechanism cannot creep back half-alive:

  1. no tracked `scripts/bin/cargo`;
  2. the retired variable is named nowhere outside `docs/` (a reader, a
     setter or an escape-hatch prefix would each be dead code that reads as
     live);
  3. the recipes that build a closed-graph root carry `--locked`: the CLI and
     the launch resolver (`just setup-cli`, `just setup-launch-resolve`), and
     every repo-root nextest run (`nros_cargo_nextest_args`).

Buildless, ~0.1 s.
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RETIRED = "NROS_" + "CARGO_FLAGS"  # spelled split so this file is not a hit

LOCKED_SITES = [
    ("justfile", re.compile(r'cargo build --release --locked --manifest-path "\$root/packages/cli/Cargo\.toml"')),
    ("justfile", re.compile(r'cargo build --release --locked --manifest-path "\$crate/Cargo\.toml"')),
    ("scripts/build/cargo.sh", re.compile(r'nros_cargo_nextest_args\(\) \{[^}]*echo "--locked"', re.S)),
]


def tracked() -> list[str]:
    return subprocess.run(
        ["git", "-C", str(ROOT), "ls-files"], capture_output=True, text=True, check=True
    ).stdout.splitlines()


def main() -> int:
    files = tracked()
    bad = []
    if "scripts/bin/cargo" in files:
        bad.append("scripts/bin/cargo is tracked again — the --locked PATH shim is retired "
                   "(phase-485 W4); the drift check replaces it.")
    hits = []
    for rel in files:
        if rel.startswith("docs/") or rel.startswith("third-party/"):
            continue
        p = ROOT / rel
        try:
            text = p.read_text(errors="ignore")
        except (IsADirectoryError, FileNotFoundError):
            continue
        if RETIRED in text:
            hits.append(rel)
    if hits:
        bad.append(f"{RETIRED} is retired (phase-485 W4) but named in:\n    "
                   + "\n    ".join(hits)
                   + "\n  Drop the reader/setter; a closed-root build passes `--locked` itself.")
    for rel, pat in LOCKED_SITES:
        if not pat.search((ROOT / rel).read_text()):
            bad.append(f"{rel}: a closed-graph root build lost its `--locked` ({pat.pattern})")
    if bad:
        print("check-cargo-locked FAILED:\n  " + "\n  ".join(bad), file=sys.stderr)
        return 1
    print(f"check-cargo-locked: shim retired; {len(LOCKED_SITES)} closed-root builds pass --locked")
    return 0


if __name__ == "__main__":
    sys.exit(main())
