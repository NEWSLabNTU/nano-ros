#!/usr/bin/env python3
"""
Phase 112.F migration: rename Cargo.toml `[package] name = "rustapp"` to a
descriptive name, and pin `[lib] name = "rustapp"` so the staticlib output
stays at `librustapp.a` for zephyr-lang-rust's hard-coded linker
expectation.

Idempotent: skips files where `[package] name` is already not "rustapp".
"""

import re
import sys
from pathlib import Path


def migrate(path: Path) -> bool:
    text = path.read_text()
    parts = path.parts

    # Derive package name from path: examples/zephyr/rust/<rmw>/<usecase>/Cargo.toml
    try:
        ix = parts.index('rust')
        rmw = parts[ix + 1]
        usecase = parts[ix + 2].replace('-', '_')
        new_name = f'nros_zephyr_{rmw}_{usecase}'
    except (ValueError, IndexError):
        print(f"can't derive name: {path}", file=sys.stderr)
        return False

    # 1. Rewrite [package] name
    new_text = re.sub(
        r'(?m)^# Name must be "rustapp" for zephyr-lang-rust integration\nname = "rustapp"',
        '# Phase 112.F: descriptive package name; staticlib still outputs\n'
        '# `librustapp.a` via [lib] name to satisfy zephyr-lang-rust.\n'
        f'name = "{new_name}"',
        text)
    if new_text == text:
        # Fallback — try just the bare assignment
        new_text = re.sub(
            r'(?m)^name = "rustapp"',
            f'name = "{new_name}"',
            text, count=1)
    if new_text == text:
        return False

    # 2. Add `name = "rustapp"` to [lib] block
    new_text = re.sub(
        r'(?m)^\[lib\]\ncrate-type = \["staticlib"\]',
        '[lib]\nname = "rustapp"\ncrate-type = ["staticlib"]',
        new_text)

    path.write_text(new_text)
    print(f"migrated: {path} -> [package] name = {new_name}")
    return True


def main():
    if len(sys.argv) < 2:
        print("usage: migrate-rustapp.py <Cargo.toml> [...]", file=sys.stderr)
        return 1
    for arg in sys.argv[1:]:
        migrate(Path(arg))
    return 0


if __name__ == '__main__':
    sys.exit(main())
