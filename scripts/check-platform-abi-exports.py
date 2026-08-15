#!/usr/bin/env python3
"""phase-364 W4 — every platform ABI symbol the header DECLARES is EXPORTED by
the Rust-port macro.

## Why this exists

A platform provides the ABI in one of two ways:

  * a hand-written `platform.c` (posix, freertos, threadx, zephyr, esp-idf,
    nuttx), or
  * a Rust `impl` of the `nros-platform-api` traits, whose symbols are emitted
    by `nros_platform_cffi::nros_platform_export!` (mps2-an385, stm32f4,
    esp32-qemu).

Adding a symbol means touching both, and nothing enforced it. phase-359 W10
added `nros_platform_task_storage_{size,align}` to the header and to all five C
ports and NOT to the macro, so the three Rust ports did not export them and a
caller linking one got an undefined symbol. phase-364 W2 and W3 would each have
repeated it.

RFC-0076 C5 proposes generating the macro's list from the header. This gate is
the half that makes the drift *unrepresentable* — the generation is a
refactoring, the check is the guarantee, and the check is what a reviewer can
trust. Written so it FAILS on the tree that motivated it: the nine symbols above
were its first output.

## What it compares

  * DECLARED — function declarations in `<nros/platform.h>`, i.e. the ABI's own
    statement of what a port provides.
  * EXPORTED — `pub extern "C" fn nros_platform_*` inside
    `nros-platform-cffi`'s `nros_platform_export*!` macros.

A declared symbol with no export is an error: a Rust port cannot satisfy the
ABI. The converse (exported, not declared) is reported but not fatal — the
macros also emit net/timer/board symbols that live in sibling headers.
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HEADER = ROOT / "packages/platform/nros-platform-api/include/nros/platform.h"
EXPORTS = ROOT / "packages/platform/nros-platform-cffi/src/lib.rs"

# A C function declaration: `<ret> [*]nros_platform_<name>(`. Anchored at the
# start of a line so a mention inside a comment or a call does not count.
DECL_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*\s+\**\s*(nros_platform_[a-z0-9_]+)\s*\(", re.M)
EXPORT_RE = re.compile(r'pub extern "C" fn\s+(nros_platform_[a-z0-9_]+)')

# Declared here, but deliberately NOT dispatched through the Rust trait macro.
# Each entry needs a reason; an empty allowlist is the goal.
ALLOWED_UNEXPORTED: set[str] = set(
    # None today. Add with a comment naming why a Rust port cannot provide it.
)


def main() -> int:
    declared = set(DECL_RE.findall(HEADER.read_text()))
    exported = set(EXPORT_RE.findall(EXPORTS.read_text()))

    missing = sorted(declared - exported - ALLOWED_UNEXPORTED)
    extra = sorted(exported - declared)

    print(f"platform ABI: {len(declared)} declared, {len(exported)} exported by the macro")

    if extra:
        # Not fatal: the macro also emits the net / timer / board surfaces,
        # which are declared in sibling headers this gate does not read.
        print(f"  ({len(extra)} exported symbol(s) not declared in platform.h — "
              f"sibling headers, not checked here)")

    if missing:
        print(f"\n[FAIL] {len(missing)} symbol(s) declared in <nros/platform.h> "
              f"but NOT exported by `nros_platform_export!`:", file=sys.stderr)
        for name in missing:
            print(f"    {name}", file=sys.stderr)
        print(
            "\n  A Rust platform port (mps2-an385, stm32f4, esp32-qemu) emits its\n"
            "  symbols through that macro, so a caller linking one of them gets an\n"
            "  undefined symbol for each name above. Add the symbol to\n"
            "  `nros_platform_export!` in nros-platform-cffi/src/lib.rs — with a\n"
            "  defaulted trait method in nros-platform-api if ports should be able\n"
            "  to opt out — or, if a Rust port genuinely cannot provide it, add it\n"
            "  to ALLOWED_UNEXPORTED here with the reason.",
            file=sys.stderr,
        )
        return 1

    print("platform ABI exports: OK (every declared symbol is exported)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
