#!/usr/bin/env python3
"""Every Rust cross target DECLARED in the tree has a row in config/rust-targets.txt.

Issue 0833. The list itself removes the installer-vs-doctor divergence; this
gate removes the next one up. A board lands a new triple in its
`nros-board.toml`, a toolchain file sets `Rust_CARGO_TARGET`, a leaf pins
`[build] target` — and nothing connects any of those to the provisioning
recipe. `armv8r-none-eabihf` was declared by two boards and two fixture rows
while `just doctor` never looked for it, so the doctor passed on a host where
`just freertos build-fixtures` could not get past cmake configure.

Three declaring producers, because fixing one and leaving the others running is
this repo's most-repeated bug shape (CLAUDE.md, "fix the CLASS"):

  * packages/boards/*/nros-board.toml   `[target.<triple>]`
  * cmake/toolchain/*.cmake             `set(Rust_CARGO_TARGET "<triple>" ...)`
  * **/.cargo/config.toml               `[build] target = "<triple>"`

The list is allowed to be a SUPERSET — it carries targets no board declares yet
(armv7r for the Orin SPE board). The gate is one-directional on purpose:
over-provisioning costs a download, under-provisioning costs a red build whose
error is four layers from its cause.
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LIST = ROOT / "config" / "rust-targets.txt"


def declared_rows():
    """(target, source) for every declaration, via git ls-files (no traversal)."""
    rows = []

    def tracked(*globs):
        out = subprocess.run(
            ["git", "-C", str(ROOT), "ls-files", "-z", *globs],
            capture_output=True, text=True, check=True,
        ).stdout
        return [p for p in out.split("\0") if p]

    for rel in tracked("packages/boards/*/nros-board.toml"):
        text = (ROOT / rel).read_text(encoding="utf-8", errors="replace")
        for m in re.finditer(r"^\[target\.([^\]]+)\]", text, re.M):
            rows.append((m.group(1), rel))

    for rel in tracked("cmake/toolchain/*.cmake"):
        text = (ROOT / rel).read_text(encoding="utf-8", errors="replace")
        for m in re.finditer(r'set\(\s*Rust_CARGO_TARGET\s+"([^"]+)"', text):
            rows.append((m.group(1), rel))

    for rel in tracked("*.cargo/config.toml", ".cargo/config.toml"):
        text = (ROOT / rel).read_text(encoding="utf-8", errors="replace")
        # Only [build] target, not [target.<triple>.<key>] override tables.
        in_build = False
        for line in text.splitlines():
            s = line.strip()
            if s.startswith("["):
                in_build = s == "[build]"
                continue
            if not in_build:
                continue
            m = re.match(r'target\s*=\s*"([^"]+)"', s)
            if m:
                # A custom target is declared by its JSON path; the triple is
                # the stem, which is what `rustup`/`-Zbuild-std` name.
                rows.append((re.sub(r"\.json$", "", m.group(1)), rel))
    return rows


def listed():
    if not LIST.is_file():
        print(f"MISSING: {LIST.relative_to(ROOT)}", file=sys.stderr)
        sys.exit(2)
    out = {}
    for raw in LIST.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split()
        if len(parts) != 2 or parts[1] not in ("rustup", "build-std"):
            print(
                f"config/rust-targets.txt: bad row {raw!r}\n"
                "  expected: <triple> <rustup|build-std>",
                file=sys.stderr,
            )
            sys.exit(2)
        out[parts[0]] = parts[1]
    return out


def main():
    known = listed()
    missing = {}
    for target, source in declared_rows():
        if target not in known:
            missing.setdefault(target, set()).add(source)

    if missing:
        print("Rust cross targets declared in-tree with no row in "
              "config/rust-targets.txt:\n", file=sys.stderr)
        for target in sorted(missing):
            print(f"  {target}", file=sys.stderr)
            for src in sorted(missing[target]):
                print(f"      declared by {src}", file=sys.stderr)
        print(
            "\nAdd a row to config/rust-targets.txt. Column 2:\n"
            "  rustup     — `rustup target list` knows it (prebuilt rust-std).\n"
            "               `just workspace rust-targets` will install it and\n"
            "               `just doctor` will verify it; both read that file.\n"
            "  build-std  — Tier 3 / custom JSON. Nothing to install; the row\n"
            "               exists so this gate can tell deliberate from forgotten.\n"
            "\nWithout the row nothing provisions the target, and the failure\n"
            "surfaces as a cmake CONFIGURE error from corrosion (Issue 0833).",
            file=sys.stderr,
        )
        return 1

    print(f"check-rust-targets-covered: OK "
          f"({len(known)} listed, {len(set(t for t, _ in declared_rows()))} declared)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
