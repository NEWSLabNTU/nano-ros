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

  EVERY platform-* MUST select a malloc provider (`global-allocator`).
                   phase-359 W10 stage C made this hold for `platform-posix`
                   too: it used to be exempt because libstd supplied the
                   allocator, and no C/C++ image links libstd any more.
  EVERY platform-* MUST NOT select a panic provider, and `nros_feature_set()`
                   must not emit one either. phase-366 moved that decision to
                   the IMAGE: `nros::main!(panic = …)` for a Rust entry,
                   `nano_ros_entry(… PANIC …)` for a C/C++ one, which reaches
                   the staticlib through `corrosion_set_features()`.

                   The table cannot answer it. "Does anything else already
                   supply the handler?" is a property of the entry and its link
                   step, not of the RTOS — the same platform is `own` for a
                   Zephyr-hosted C/C++ image (the `zephyr` crate has it) and
                   `platform` for a bare one.

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

# phase-472 W3 — ONE comment stripper. This gate used to test `MALLOC in body`
# over the raw row, comments included, so the issue-0617 row itself — whose
# comment is the prose ABOUT the provider — could lose its `"global-allocator"`
# element to a `# "global-allocator",` and still pass.
sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
import comments  # noqa: E402

MALLOC = "global-allocator"
# phase-366 R1/R2 — the entry-selected provider. `panic-spin` is deleted.
PANIC = ("panic-platform", "panic-halt")
# phase-359 W10 stage C — there are no std-backed rows. `platform-posix` was
# the last (`STD_BACKED`), after ThreadX (issue 1763) and FreeRTOS (issue 1778)
# left the std tier; the API crates have no `std` feature any more, so a
# `nros_feature_set()` arm naming one is a configure that cargo refuses.


def selects(body: str, feature: str) -> bool:
    """Does this feature-list BODY name `feature` as an element (own or `dep/`)?

    A quoted element, not a substring: `"no-global-allocator"` is not it."""
    return re.search(r'"(?:[A-Za-z0-9_-]+\??/)?' + re.escape(feature) + r'"', body) is not None


def features(manifest, text: str | None = None) -> dict[str, str]:
    """Map `platform-*` feature name -> its body, COMMENTS BLANKED.

    Hand-parsed rather than via `tomllib` so a row's shape stays as written.
    The body's comments are where the reasons live, and they are exactly what
    must not count as a selection (phase-472 W3), so the rows are read from the
    stripped text; the file itself still carries the prose for a reader.
    """
    text = manifest.read_text() if text is None else text
    text = comments.strip_comments(text, "toml")
    out: dict[str, str] = {}
    for m in re.finditer(r"^(platform-[a-z0-9-]+) = \[", text, re.M):
        name = m.group(1)
        # Scan to the matching close bracket at column 0. Comments are blanked
        # now, but a body may still hold `]` inside a quoted element.
        end = text.index("\n]", m.end())
        out[name] = text[m.end() : end]
    return out


def self_test() -> None:
    """Negative controls on the NORMAL path (phase-472 W9)."""
    comments.self_test()
    rows = features(None, (
        'platform-a = [\n    # "global-allocator",  commented out\n    "x/platform-a",\n]\n'
        'platform-b = [\n    "global-allocator",  # the provider\n]\n'
        'platform-c = [\n    "nros-c/platform-c",\n    # "panic-halt" is the image\'s call\n]\n'
    ))
    assert set(rows) == {"platform-a", "platform-b", "platform-c"}, rows
    assert not selects(rows["platform-a"], MALLOC), "a commented-out provider is not selected"
    assert selects(rows["platform-b"], MALLOC)
    assert not any(selects(rows["platform-c"], p) for p in PANIC), "prose naming a panic provider"
    assert '"nros-c/platform-c"' in rows["platform-c"]
    assert not selects('"no-global-allocator"', MALLOC)
    assert selects('"nros-platform?/global-allocator"', MALLOC)
    assert cmake_feature_lists("# list(APPEND _feats platform-x panic-halt)\n") == []
    assert cmake_feature_lists("list(APPEND _feats platform-x panic-halt)\n") == [
        ["platform-x", "panic-halt"]
    ]


def cmake_feature_lists(text: str) -> list[list[str]]:
    """Each `list(APPEND _feats …)` in CODE, as its word list."""
    code = comments.strip_comments(text, "cmake")
    return [m.group(1).split() for m in re.finditer(r"list\(APPEND _feats ([^)]*)\)", code)]


def main() -> int:
    self_test()
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
        has_malloc = selects(body, MALLOC)
        has_panic = any(selects(body, p) for p in PANIC)
        # phase-366 — no platform row may choose the ending, std-backed or not.
        if has_panic:
            print(
                f"ERROR: nros-c/{name} selects a panic provider. The ending is the "
                "IMAGE's call (RFC-0077): `nros::main!(panic = …)` or "
                "`nano_ros_entry(… PANIC …)`, never the platform.",
                file=sys.stderr,
            )
            fail = 1
        if not has_malloc:
            print(
                f"ERROR: nros-c/{name} selects no `{MALLOC}`.",
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
        for feats in cmake_feature_lists(fs.read_text()):
            plat = next((f for f in feats if f.startswith("platform-")), None)
            if plat is None:
                continue  # a partial append; the platform arrives in another
            if "std" in feats:
                # phase-359 W10 stage C — `nros-c`/`nros-cpp` have no `std`.
                # It was a SECOND runtime beside the port's (issue 1763): the
                # Rust heap glibc's, `panic-platform` standing down, `PANIC
                # halt` refused (issue 1742). Every arm emits `alloc` now.
                print(
                    f"ERROR: nros_feature_set emits {plat} with `std` "
                    f"({' '.join(feats)}), a feature nros-c/nros-cpp no longer "
                    f"have. The port supplies the runtime (`{MALLOC}` + the "
                    "image's panic ending): emit `alloc`, plus `env`/`log` for "
                    "a hosted target.",
                    file=sys.stderr,
                )
                fail = 1
                continue
            if any(p in feats for p in PANIC):
                print(
                    f"ERROR: nros_feature_set emits {plat} with a panic provider "
                    f"({' '.join(feats)}). phase-366 R2 removed those rows — the "
                    "ending comes from `nano_ros_entry(… PANIC …)`, which applies "
                    "it with corrosion_set_features() after the entry is declared.",
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
            "  that supplies them (phase-361 W8.d), on a host target too\n"
            "  (phase-359 W10 stage C) — no image root links libstd.",
            file=sys.stderr,
        )
        return 1

    print(
        f"platform provider features: OK ({len(rows)} row(s), all port-supplied)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
