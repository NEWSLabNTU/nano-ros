#!/usr/bin/env python3
"""phase-347 W2 — the RMW descriptors agree with the live lowering, row for row.

RFC-0071 moves each backend's build facts out of a central `match` and into an
`nros-rmw.toml` the backend ships. W2 adds the files and CHANGES NOTHING ELSE:
`resolve_rmw()` and the generated `nros_rmw_dispatch()` remain the live path.

That makes a descriptor a SECOND derivation of a fact the tree already computes,
and a second derivation is exactly what this repo keeps paying for — the fixture
`row_coord`, the group key, the sizes-header mirror. This gate is what makes it
one derivation instead: while both exist, they must agree, and W3 may only
delete the old one once this has been green.

It reads the generated `cmake/NanoRosRmwDispatch.cmake` rather than shelling
into cargo, so it stays in `check-fast`'s buildless budget. That file is itself
generated from `resolve_rmw()` and gated for staleness by
`rmw_cmake_dispatch_is_current`, so agreeing with it IS agreeing with the
resolver.

`uorb` is EXEMPT and the exemption is the finding, not a convenience: it is a
first-class `NANO_ROS_RMW` value in `NanoRosFeatureSet.cmake` and in
`nros-cpp/CMakeLists.txt`, and `nros_rmw_dispatch` FATAL_ERRORs on it. Three
lists, two of which accept it. Its descriptor exists so W3 can retire all three
together; until then there is no dispatch row to compare against, and inventing
one would hide the bug this phase exists to fix.
"""

import glob
import os
import re
import sys

try:
    import tomllib  # 3.11+
except ModuleNotFoundError:  # 3.10 backport, same spelling as the sibling gates
    import tomli as tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DISPATCH = os.path.join(ROOT, "cmake/NanoRosRmwDispatch.cmake")

# Not in the dispatch by construction — see the module docstring.
EXEMPT = {"uorb"}


def descriptors():
    """Every `nros-rmw.toml` under packages/rmw, keyed by canonical name."""
    out = {}
    for path in sorted(glob.glob(os.path.join(ROOT, "packages/rmw/*/*/nros-rmw.toml"))):
        with open(path, "rb") as fh:
            data = tomllib.load(fh)
        rmw = data.get("rmw")
        if not rmw:
            sys.exit(f"{path}: no [rmw] table")
        names = rmw.get("names") or []
        if not names:
            sys.exit(f"{path}: [rmw].names is empty — nothing could resolve to it")
        out[names[0]] = (path, data)
    return out


def dispatch_rows():
    """Parse the generated cmake dispatch into {rmw: {field: value}}.

    Deliberately a parse of the GENERATED file rather than a re-implementation
    of `resolve_rmw()`: a re-implementation would be a third derivation, which
    is the defect this gate exists to prevent, one level up.
    """
    text = open(DISPATCH).read()
    rows = {}
    # `if(rmw STREQUAL "zenoh")` … `set(NROS_RMW_X "v" PARENT_SCOPE)` …
    blocks = re.split(r'(?:els)?if\(rmw STREQUAL "([a-z0-9-]+)"\)', text)
    # blocks[0] is the preamble; then (name, body) pairs
    for i in range(1, len(blocks) - 1, 2):
        name, body = blocks[i], blocks[i + 1]
        fields = {}
        # Values come BOTH quoted (`"rmw-zenoh-cffi"`, and `""` for empty) and
        # bare (`ON` / `OFF`). Matching only the quoted form silently produced
        # "no NROS_RMW_NEEDS_CXX_LINKER" for every backend — a parser gap that
        # would have read as a descriptor error.
        for m in re.finditer(
            r'set\((NROS_RMW_[A-Z_]+)\s+(?:"([^"]*)"|([^\s)]+))\s+PARENT_SCOPE\)', body
        ):
            fields[m.group(1)] = m.group(2) if m.group(2) is not None else m.group(3)
        rows[name] = fields
    if not rows:
        sys.exit(
            f"{DISPATCH}: parsed no backend rows — the generated shape changed, "
            "so this gate is checking nothing. Fix the parser before trusting it."
        )
    return rows


def main():
    desc = descriptors()
    rows = dispatch_rows()
    problems = []

    # --- every dispatch row has a descriptor ------------------------------
    for name in sorted(rows):
        if name not in desc:
            problems.append(
                f"`{name}` is in {os.path.relpath(DISPATCH, ROOT)} but ships no "
                f"nros-rmw.toml — the descriptor set is narrower than the lowering "
                f"it is supposed to replace"
            )

    # --- every descriptor agrees, field for field -------------------------
    for name, (path, data) in sorted(desc.items()):
        rel = os.path.relpath(path, ROOT)
        if name in EXEMPT:
            if name in rows:
                problems.append(
                    f"`{name}` is exempt in this gate but NOW HAS a dispatch row. "
                    f"The exemption exists because it had none; delete it from "
                    f"EXEMPT in {os.path.basename(__file__)} and let it be checked."
                )
            continue
        if name not in rows:
            problems.append(f"{rel}: `{name}` has no row in the dispatch")
            continue

        row = rows[name]
        link = data["rmw"].get("link", {})
        want = {
            "NROS_RMW_UMBRELLA_CFFI_FEATURE": data["rmw"].get("cffi_feature", ""),
            "NROS_RMW_RLIB_DEP": link.get("rlib_dep", ""),
            "NROS_RMW_EXTRA_LINK_LIBS": ";".join(link.get("extra_link_libs", [])),
            "NROS_RMW_NEEDS_CXX_LINKER": "ON" if link.get("needs_cxx_linker") else "OFF",
        }
        for field, expected in want.items():
            actual = row.get(field)
            if actual is None:
                problems.append(f"{rel}: dispatch row `{name}` has no {field}")
            elif actual != expected:
                problems.append(
                    f"{rel}: {field} disagrees for `{name}`\n"
                    f"        descriptor: {expected!r}\n"
                    f"        dispatch:   {actual!r}"
                )

    if problems:
        sys.stderr.write("check-rmw-descriptors: FAILED\n")
        for p in problems:
            sys.stderr.write(f"  {p}\n")
        sys.stderr.write(
            "\n  The descriptor and the live lowering are two derivations of one\n"
            "  fact. While both exist they must agree — that is the whole point of\n"
            "  phase-347 W2, and W3 may only delete the lowering once this is green.\n"
        )
        return 1

    checked = len(desc) - len(EXEMPT & set(desc))
    print(
        f"rmw descriptors: OK ({len(desc)} descriptor(s), {checked} checked against "
        f"{len(rows)} dispatch row(s); exempt: {', '.join(sorted(EXEMPT & set(desc))) or 'none'})"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
