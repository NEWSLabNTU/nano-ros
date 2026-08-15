#!/usr/bin/env python3
"""Issue 0617 — every RTOS `platform-*` feature must SUPPLY malloc and panic.

A `no_std` final artifact needs exactly one `#[global_allocator]` and exactly
one `#[panic_handler]`. In this tree the `platform-*` feature is what names the
port that supplies them: phase-361 W8.d states it as "a `platform-*` feature
selects the PROVIDER of malloc + panic for this platform ... and NOTHING else".

`nros-c/platform-nuttx` did not, and the omission was invisible for as long as
NuttX images linked `std` — libstd supplies both lang items, so nothing missed
them. The drop-std campaign (phase-359/361) removed that cover and NuttX became
the only RTOS row selecting neither, which surfaced four crates away as

    error: `#[panic_handler]` function required, but not found

while compiling `nros-c` for `armv7a-nuttx-eabihf`, taking out the entire NuttX
fixture family. A HOST build cannot detect this: it gets both items from `std`.

THE RULE

  platform-posix   MUST NOT select a provider — libstd supplies both, and a
                   second `#[global_allocator]` is a duplicate lang item.
  every other      MUST select a malloc provider (`global-allocator`) and a
  platform-*       panic provider (`panic-spin` or `panic-halt`).

`nros-cpp` delegates (`platform-X = ["nros-c/platform-X"]`), so `nros-c` is the
single source of truth and this gate checks the delegation holds rather than
duplicating the table.

WHY A GATE AND NOT A COMMENT

The manifest already carried the rule in prose — "ports that own the unified
kernel heap enable `global-allocator` + `panic-spin`" — directly above the row
that did not. Prose adjacent to the exception is the shape this repo keeps
finding (issues 0196, 0442, 0574): the invariant is written down, and the site
that breaks it is the one nobody re-read.
"""

import re
import sys
from pathlib import Path

MALLOC = "global-allocator"
PANIC = ("panic-spin", "panic-halt")
# `std` supplies both lang items, so these must NOT name a provider.
STD_BACKED = {"platform-posix"}


def features(manifest: Path) -> dict[str, str]:
    """Map `platform-*` feature name -> its raw body.

    Hand-parsed rather than via `tomllib` so the body keeps its comments: the
    reason a row looks the way it does is in them, and a diagnostic that can
    quote the row is worth more than one that reports a bare name.
    """
    text = manifest.read_text()
    out: dict[str, str] = {}
    for m in re.finditer(r"^(platform-[a-z0-9-]+) = \[", text, re.M):
        name = m.group(1)
        # Scan to the matching close bracket at column 0 — a body contains `]`
        # inside comments (`#[panic_handler]`), which is exactly what a lazy
        # `\[(.*?)\]` gets wrong, and it got this author first.
        end = text.index("\n]", m.end())
        out[name] = text[m.end() : end]
    return out


def main() -> int:
    core = Path("packages/api/nros-c/Cargo.toml")
    cpp = Path("packages/api/nros-cpp/Cargo.toml")
    if not core.is_file():
        print(f"check-platform-provider-features: {core} missing", file=sys.stderr)
        return 2

    fail = 0
    rows = features(core)
    if not rows:
        print("check-platform-provider-features: no platform-* rows found — "
              "the table moved and this gate is now blind", file=sys.stderr)
        return 2

    for name, body in sorted(rows.items()):
        has_malloc = MALLOC in body
        has_panic = any(p in body for p in PANIC)
        if name in STD_BACKED:
            if has_malloc or has_panic:
                print(
                    f"ERROR: nros-c/{name} names a malloc/panic provider, but `std` "
                    "already supplies both — that is a duplicate lang item.",
                    file=sys.stderr,
                )
                fail = 1
            continue
        missing = []
        if not has_malloc:
            missing.append(f"`{MALLOC}`")
        if not has_panic:
            missing.append("`panic-spin` or `panic-halt`")
        if missing:
            print(
                f"ERROR: nros-c/{name} selects no {' and no '.join(missing)}.",
                file=sys.stderr,
            )
            fail = 1

    # ---------------------------------------------------------------- CMake
    # The SECOND provider table. `nros_feature_set()` emits features for the
    # C/C++ dep-sites, entirely independently of the Cargo manifests above, and
    # the same invariant applies: a row either rides `std` (which supplies both
    # lang items) or names a panic provider itself. It is consistent today —
    # this asserts it stays that way, because a table nobody checks is how the
    # Cargo-side NuttX row lost both providers and nobody noticed until a
    # `no_std` link failed four crates away (issue 0617).
    fs = Path("cmake/NanoRosFeatureSet.cmake")
    if fs.is_file():
        # Each arm appends a flat list: `list(APPEND _feats std platform-posix)`.
        for m in re.finditer(r"list\(APPEND _feats ([^)]*)\)", fs.read_text()):
            feats = m.group(1).split()
            plat = next((f for f in feats if f.startswith("platform-")), None)
            if plat is None:
                continue  # a partial append; the platform arrives in another
            if "std" in feats:
                continue  # std supplies panic + allocator
            if not any(p in feats for p in PANIC):
                print(
                    f"ERROR: nros_feature_set emits {plat} with neither `std` nor a "
                    f"panic provider: {' '.join(feats)}",
                    file=sys.stderr,
                )
                fail = 1

    # nros-cpp must delegate, not restate. A second copy of the table is a
    # second thing to forget to update.
    if cpp.is_file():
        for name, body in sorted(features(cpp).items()):
            if f'"nros-c/{name}"' not in body:
                print(
                    f"ERROR: nros-cpp/{name} does not forward to nros-c/{name}. "
                    "The provider table has ONE home; delegate to it.",
                    file=sys.stderr,
                )
                fail = 1

    if fail:
        print(
            "\n  A no_std final artifact needs exactly one #[global_allocator] and\n"
            "  one #[panic_handler]. The `platform-*` feature is what names the port\n"
            "  that supplies them (phase-361 W8.d). A host build cannot catch this —\n"
            "  it gets both from `std` (issue 0617).",
            file=sys.stderr,
        )
        return 1

    print(
        f"platform provider features: OK ({len(rows)} row(s); "
        f"{len(STD_BACKED)} std-backed, {len(rows) - len(STD_BACKED)} port-supplied)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
