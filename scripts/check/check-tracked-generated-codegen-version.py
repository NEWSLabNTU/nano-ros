#!/usr/bin/env python3
"""The tracked, pre-generated message crates were emitted by THIS codegen.

`packages/interfaces/generated/<distro>/*` is committed because core crates
need those messages before any codegen runs. Nothing compared it with its
emitter, so it drifted for weeks: emitted at codegen version 18 while the
emitter wrote 21, with old design ids in its rustdoc (issue 2554).

Buildless rule: every tracked generated crate's `NROS_EMITTED_CODEGEN_VERSION`
equals the emitter's `NROS_CODEGEN_VERSION`. That is enough, because
`check-codegen-version-surface` already refuses an emitter surface change
that does not bump the version. So a crate at the current version was emitted
by today's templates. When this fails, regenerate inside the pinned Humble:
`just generate-interfaces` (the ament inputs come from the ROS install; see
issue 2554 for the image used).

Run: python3 scripts/check/check-tracked-generated-codegen-version.py
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
VERSION_RS = ROOT / "packages" / "core" / "nros-core" / "src" / "codegen_version.rs"
GENERATED = "packages/interfaces/generated"
CURRENT = re.compile(r"pub const NROS_CODEGEN_VERSION\s*:\s*u32\s*=\s*(\d+)")
EMITTED = re.compile(r"pub const NROS_EMITTED_CODEGEN_VERSION\s*:\s*u32\s*=\s*(\d+)")


def problems(current: int, crates: dict[str, str]) -> list[str]:
    """`crates`: path of a tracked generated `lib.rs` -> its text."""
    out = []
    for path, text in sorted(crates.items()):
        m = EMITTED.search(text)
        if not m:
            out.append(f"{path}: no NROS_EMITTED_CODEGEN_VERSION (not emitted by `nros dev codegen`?)")
        elif int(m.group(1)) != current:
            out.append(f"{path}: emitted at codegen version {m.group(1)}, the emitter writes {current}")
    return out


def self_test() -> bool:
    """Negative control on the normal path: a stale crate and a crate with no
    stamp are both refused; a current one passes."""
    stamp = "pub const NROS_EMITTED_CODEGEN_VERSION: u32 = {};\n"
    got = problems(21, {"a/lib.rs": stamp.format(21), "b/lib.rs": stamp.format(18), "c/lib.rs": "// none\n"})
    ok = len(got) == 2 and got[0].startswith("b/lib.rs") and got[1].startswith("c/lib.rs")
    if not ok:
        print(f"check-tracked-generated-codegen-version: SELF-TEST FAILED: {got}", file=sys.stderr)
    return ok


def main() -> int:
    if not self_test():
        return 1
    m = CURRENT.search(VERSION_RS.read_text())
    if not m:
        print(f"check-tracked-generated-codegen-version: no NROS_CODEGEN_VERSION in "
              f"{VERSION_RS.relative_to(ROOT)}", file=sys.stderr)
        return 1
    current = int(m.group(1))
    listed = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "-z", "--", f"{GENERATED}/*/*/src/lib.rs"],
        capture_output=True, text=True, check=True,
    ).stdout.split("\0")
    crates = {p: (ROOT / p).read_text(errors="replace") for p in listed if p}
    if not crates:
        print(f"check-tracked-generated-codegen-version: no tracked crate under {GENERATED}; "
              "the listing broke, refusing a vacuous pass", file=sys.stderr)
        return 1
    bad = problems(current, crates)
    if bad:
        print("check-tracked-generated-codegen-version: FAIL -- the tracked message crates "
              "are not what today's codegen emits:\n  " + "\n  ".join(bad)
              + "\n  Regenerate them: ./scripts/bootstrap.sh (contributors: just generate-interfaces)",
              file=sys.stderr)
        return 1
    print(f"check-tracked-generated-codegen-version: OK -- {len(crates)} tracked crate(s) "
          f"at codegen version {current}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
