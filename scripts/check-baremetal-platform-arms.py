#!/usr/bin/env python3
"""issue 1512 — the bare-metal platform arms agree across the three files that
have to say the same thing.

Bare metal is the one platform whose `nros-platform/platform-*` feature is
BOARD-SPECIFIC (the board IS the port: its clock is that board's timer, its net
is that board's MAC, and there is no kernel in between). So instead of one name
appearing in each of `nros-platform`, `nros-c`, `nros-cpp` and the CMake ladder,
there are three, and the previous state of the tree is what happens when nobody
checks: `nros-c` and `nros-cpp` had NO bare-metal arm at all, and
`nros_feature_set()`'s PLATFORM ladder had no bare-metal branch, so a C leaf
fell into the `_cross` catch-all and asked cargo for `platform-baremetal` — a
feature no crate in the tree has.

Four rules, each in BOTH directions, because a partial set is how a fourth
spelling gets invented:

  A. the bare-metal platform features of `nros-platform` == the `platform-*`
     arms of `nros-c` that forward one == the arms of `nros-cpp` that forward a
     `nros-c` one;
  B. every `cmake/board/nano-ros-board-*-baremetal.cmake` has a row in
     `cmake/NanoRosBareMetalPlatform.cmake`'s map, and every row names a board
     overlay that exists;
  C. every map VALUE is one of the features from (A);
  D. `nros-c`'s bare-metal arms select `global-allocator` (RFC-0034 D6: the
     bare-metal row of its allocator table reads `on`) and
     `nros-log/platform-clock` (issue 1152: a linked port is exactly the
     condition under which `nros_platform_clock_ns` resolves).

WHAT IS DERIVED, and why it matters: the SET in (A) is not a list kept here. A
`platform-*` feature of `nros-platform` is bare-metal iff its body names a
`dep:nros-platform-<x>` other than the shared `-cffi`/`-api` crates — i.e. iff
it pulls a board's own Rust port. `platform-posix` and the four RTOS arms name
only `nros-platform-cffi` (their `nros_platform_*` symbols come from a C port),
so they classify out with no exclusion list. Add a fourth bare-metal board and
this gate asks for its three arms without being edited.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

NROS_PLATFORM = ROOT / "packages/platform/nros-platform/Cargo.toml"
NROS_C = ROOT / "packages/api/nros-c/Cargo.toml"
NROS_CPP = ROOT / "packages/api/nros-cpp/Cargo.toml"
MAP_CMAKE = ROOT / "cmake/NanoRosBareMetalPlatform.cmake"
BOARD_DIR = ROOT / "cmake/board"

# The platform-layer crates every port shares. A `platform-*` feature naming
# ONLY these is an RTOS/hosted arm whose C symbols come from a C port.
SHARED_PLATFORM_CRATES = {"nros-platform-cffi", "nros-platform-api"}


def feature_bodies(manifest: Path) -> dict[str, str]:
    """[`feature_bodies_from_text`] over a manifest on disk."""
    text = manifest.read_text()
    if "\n[features]" not in text:
        sys.exit(f"{manifest}: no [features] table")
    return feature_bodies_from_text(text)


def feature_bodies_from_text(text: str) -> dict[str, str]:
    """`{feature: body}` for every `<name> = [ … ]` in the `[features]` table.

    A loose scanner rather than a TOML parse, deliberately: these manifests carry
    heavy comments inside the arrays and the repo's other manifest checks read
    them the same way. Comments are stripped so a commented-out forward never
    counts as one.
    """
    start = text.find("\n[features]")
    if start < 0:
        return {}
    rest = text[start + len("\n[features]") :]
    end = re.search(r"^\[", rest, re.MULTILINE)
    if end:
        rest = rest[: end.start()]
    # Drop comments before matching, so `# platform-x = [...]` is not a feature.
    rest = "\n".join(line.split("#", 1)[0] for line in rest.splitlines())
    out: dict[str, str] = {}
    for m in re.finditer(
        r"^\s*([A-Za-z0-9_-]+)\s*=\s*(\[.*?\])", rest, re.MULTILINE | re.DOTALL
    ):
        out[m.group(1)] = m.group(2)
    return out


def baremetal_platform_features(bodies: dict[str, str]) -> set[str]:
    """`nros-platform`'s BOARD-SPECIFIC platform features — see the module docstring."""
    found = set()
    for name, body in bodies.items():
        if not name.startswith("platform-"):
            continue
        deps = set(re.findall(r"dep:(nros-platform-[A-Za-z0-9_-]+)", body))
        if deps - SHARED_PLATFORM_CRATES:
            found.add(name)
    return found


def pair_tokens(toks: list[str]) -> list[tuple[str, str]]:
    """`[a, b, c, d]` -> `[(a, b), (c, d)]`; `ValueError` on an odd count.

    The CMake side reads the same flat list in pairs, so an odd count there is a
    silently truncated table — the one shape this parse must refuse rather than
    tolerate.
    """
    if len(toks) % 2:
        raise ValueError(
            f"odd token count ({len(toks)}) — the list is read in "
            f"<board> <feature> PAIRS"
        )
    return list(zip(toks[::2], toks[1::2], strict=True))


def parse_map() -> list[tuple[str, str]]:
    """The `<board token> <feature>` pairs from `_NROS_BAREMETAL_BOARD_FEATURES`."""
    text = MAP_CMAKE.read_text()
    m = re.search(
        r"set\(_NROS_BAREMETAL_BOARD_FEATURES\s*(.*?)CACHE INTERNAL", text, re.DOTALL
    )
    if not m:
        sys.exit(f"{MAP_CMAKE}: could not find the _NROS_BAREMETAL_BOARD_FEATURES set()")
    toks = [
        t
        for line in m.group(1).splitlines()
        for t in line.split("#", 1)[0].split()
    ]
    try:
        return pair_tokens(toks)
    except ValueError as e:
        sys.exit(f"{MAP_CMAKE}: _NROS_BAREMETAL_BOARD_FEATURES has an {e}")


def self_test(quiet: bool = False) -> int:
    """Negative controls — a gate whose rule never fires proves nothing.

    The three parts that could decay into comments are the DERIVATION (which
    `platform-*` features of `nros-platform` count as bare metal), the FORWARD
    detection in each language crate, and the MAP parse. Each case asserts the
    rule fires, then that the correct content silences it.
    """
    # 1. The derivation separates a board's own Rust port from the shared cffi
    #    crate. This is the whole reason the set needs no exclusion list, and it
    #    is the part a reader would most likely "simplify" into a name list.
    derived = baremetal_platform_features(
        {
            # a board port -> bare metal
            "platform-brd": '["dep:nros-platform-brd", "dep:nros-platform-cffi"]',
            # only the shared crates -> an RTOS/hosted arm
            "platform-rtos": '["dep:nros-platform-cffi"]',
            "platform-hosted": '["dep:nros-platform-cffi", "dep:nros-platform-api"]',
            # not a platform arm at all
            "global-allocator": '["nros-platform/global-allocator"]',
        }
    )
    assert derived == {"platform-brd"}, f"derivation picked {sorted(derived)}"

    # 2. A body that only MENTIONS the feature in a comment must not count as a
    #    forward — comments are stripped before matching.
    bodies = feature_bodies_from_text(
        "\n[features]\n"
        'platform-brd = [\n    "nros-platform/platform-brd",\n]\n'
        '# platform-ghost = ["nros-platform/platform-brd"]\n'
    )
    assert "platform-brd" in bodies, "a real arm must parse"
    assert "platform-ghost" not in bodies, "a commented-out arm must not parse"

    # 3. The map is read in PAIRS, so an odd token count is a hard error rather
    #    than a silently truncated table.
    try:
        pair_tokens(["a", "feat-a", "b"])
    except ValueError:
        pass
    else:  # pragma: no cover — the assertion below is the report
        raise AssertionError("an odd token count must be refused")
    assert pair_tokens(["a", "feat-a"]) == [("a", "feat-a")]

    if not quiet:
        print("check-baremetal-platform-arms: self-test OK (3 negative controls)")
    return 0


def main() -> int:
    if "--self-test" in sys.argv:
        return self_test()
    # Always, not only behind the flag: a negative control nobody runs decays
    # into a comment, and this rule's whole job is to fire.
    self_test(quiet=True)

    errors: list[str] = []

    plat = feature_bodies(NROS_PLATFORM)
    want = baremetal_platform_features(plat)
    if not want:
        sys.exit(
            f"{NROS_PLATFORM.relative_to(ROOT)}: derived ZERO bare-metal platform "
            "features — refusing to report OK over nothing. The derivation looks "
            "for a `platform-*` feature naming a `dep:nros-platform-<board>`."
        )

    # ---- A: nros-c ------------------------------------------------------
    c = feature_bodies(NROS_C)
    c_arms = {
        name
        for name, body in c.items()
        if name.startswith("platform-")
        and any(f'"nros-platform/{w}"' in body for w in want)
    }
    for missing in sorted(want - c_arms):
        errors.append(
            f"packages/api/nros-c/Cargo.toml: no `{missing}` arm. Bare metal has no "
            f"single platform feature, so nros-c names the BOARD's; add:\n"
            f"    {missing} = [\n"
            f'        "nros-log/platform-clock",\n'
            f'        "global-allocator",\n'
            f'        "nros-platform/{missing}",\n'
            f'        "nros-rmw-zenoh?/platform-bare-metal",\n'
            f"    ]"
        )
    for extra in sorted(c_arms - want):
        errors.append(
            f"packages/api/nros-c/Cargo.toml: `{extra}` forwards a bare-metal "
            f"`nros-platform` feature that is no longer one of "
            f"{sorted(want)} — drop the arm or fix the forward."
        )

    # ---- A: nros-cpp ----------------------------------------------------
    cpp = feature_bodies(NROS_CPP)
    cpp_arms = {
        name
        for name, body in cpp.items()
        if name.startswith("platform-")
        and any(f'"nros-c/{w}"' in body for w in want)
    }
    for missing in sorted(want - cpp_arms):
        errors.append(
            f"packages/api/nros-cpp/Cargo.toml: no `{missing}` arm. Pure forward, "
            f"like every other platform arm there:\n"
            f"    {missing} = [\n"
            f'        "nros-c/{missing}",\n'
            f"    ]"
        )
    for extra in sorted(cpp_arms - want):
        errors.append(
            f"packages/api/nros-cpp/Cargo.toml: `{extra}` forwards an nros-c "
            f"bare-metal arm that is no longer one of {sorted(want)}."
        )

    # ---- D: what each nros-c arm must select ----------------------------
    for arm in sorted(want & c_arms):
        body = c[arm]
        for required, why in (
            (
                "global-allocator",
                "RFC-0034 D6's bare-metal row reads `on`: the board owns the heap "
                "and the Rust `#[global_allocator]` routes to it",
            ),
            (
                "nros-log/platform-clock",
                "a linked port is exactly the condition under which "
                "`nros_platform_clock_ns` resolves (issue 1152)",
            ),
        ):
            if f'"{required}"' not in body:
                errors.append(
                    f"packages/api/nros-c/Cargo.toml: `{arm}` does not select "
                    f"`{required}` — {why}."
                )

    # ---- B + C: the CMake map ------------------------------------------
    rows = parse_map()
    mapped_boards = {b for b, _ in rows}
    overlays = {
        p.name[len("nano-ros-board-") : -len(".cmake")]
        for p in sorted(BOARD_DIR.glob("nano-ros-board-*-baremetal.cmake"))
    }
    if not overlays:
        sys.exit(
            f"{BOARD_DIR.relative_to(ROOT)}: found no `nano-ros-board-*-baremetal.cmake` "
            "overlays — refusing to report OK over nothing."
        )
    for missing in sorted(overlays - mapped_boards):
        errors.append(
            f"cmake/NanoRosBareMetalPlatform.cmake: no row for board "
            f"`{missing}` (cmake/board/nano-ros-board-{missing}.cmake exists). "
            f"Without it `nros_feature_set()` FATAL_ERRORs on that board, which "
            f"is better than the fall-through it replaced but still means no "
            f"C/C++ image can configure for it."
        )
    for extra in sorted(mapped_boards - overlays):
        errors.append(
            f"cmake/NanoRosBareMetalPlatform.cmake: row `{extra}` names no board "
            f"overlay — expected cmake/board/nano-ros-board-{extra}.cmake."
        )
    for board, feat in rows:
        if feat not in want:
            errors.append(
                f"cmake/NanoRosBareMetalPlatform.cmake: row `{board}` -> `{feat}`, "
                f"which is not a bare-metal platform feature of nros-platform "
                f"({sorted(want)})."
            )

    if errors:
        print("check-baremetal-platform-arms: FAILED (issue 1512)\n", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        return 1

    print(
        f"check-baremetal-platform-arms: OK — {len(want)} bare-metal platform "
        f"feature(s) ({', '.join(sorted(want))}) carried by nros-platform, nros-c "
        f"and nros-cpp; {len(rows)} board row(s) resolve."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
