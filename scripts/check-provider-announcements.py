#!/usr/bin/env python3
"""phase-348 W2 — discovery and resolution claim the same names.

A provider says what it IS twice: its `package.xml` announces
`<nano_ros_provides kind=… name=…/>` so a source-time scan can FIND it, and its
descriptor (`nros-rmw.toml`, `nros-board.toml`) declares the names a consumer
RESOLVES against. Nothing structural keeps the two equal, so this compares
them.

  A1  a package.xml sitting beside a descriptor announces provisions of that
      kind — otherwise it is invisible to the scan while looking migrated;
  A2  its provision names equal the descriptor's declared names EXACTLY and in
      order, canonical first, since names[0] is what error messages list.

**One gate for every family, not one per family.** This started as S5 inside
`check-rmw-descriptors.py`, covering rmw alone. Extending the rule to boards by
adding a second copy next to the board descriptors is exactly the
second-spelling antipattern this repo keeps paying for (see the Zephyr
unset-variable guard, #282 → #326), so S5 moved here instead and
`check-rmw-descriptors.py` kept only what is rmw-SPECIFIC (S1–S4). Adding a
`platform` family later means one row in FAMILIES, not another script.

A package.xml with no descriptor beside it is NOT checked: the migration
proceeds one provider at a time, and an unmigrated provider is simply not
discoverable while its existing build path keeps working.

Buildless — TOML plus a regex, no cmake, no cargo.
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

# kind -> (descriptor glob, how to pull the declared names out of it).
#
# The two families disagree in shape and that is not an accident: an rmw
# descriptor is ONE backend (`[rmw]`), while a board descriptor is an ARRAY
# (`[[board]]`) because one package can ship several boards — nros-board-nuttx-qemu
# declares both the ARM and RISC-V variants, disambiguated by `target_contains`.
# So the board reader flattens entries in declaration order.
FAMILIES = {
    "rmw": (
        "packages/rmw/*/*/nros-rmw.toml",
        lambda d: list(d.get("rmw", {}).get("names", [])),
    ),
    "board": (
        "packages/boards/*/nros-board.toml",
        lambda d: [n for b in d.get("board", []) for n in b.get("names", [])],
    ),
    # phase-349 W1. Platform descriptors live under `config/`, not
    # `packages/platform/` — a fact that cost a wrong "the family does not
    # exist" claim in phase-348 W2 (corrected there). `names` is top-level,
    # not in a table, and defaults to the directory name for a file that
    # declares none.
    "platform": (
        "config/*/nros-platform.toml",
        lambda d: list(d.get("names", [])),
    ),
}

PROVIDES_RE = r'<nano_ros_provides\s+kind="{kind}"\s+name="([^"]+)"\s*/?>'
COMMENT_RE = re.compile(r"<!--([^-]|-[^-])*-->")


def declared_provisions(path, kind):
    """Provision names of one kind, in file order, comments stripped.

    The strip is not optional (issue 0516): a provider's package.xml documents
    the provision tag in a comment, and a regex cannot tell that from a
    declaration. Without it a commented-out example counts as a claimed name.
    """
    with open(path, encoding="utf-8") as fh:
        body = COMMENT_RE.sub("", fh.read())
    return re.findall(PROVIDES_RE.format(kind=kind), body)


def main():
    problems = []
    checked = 0
    announced = 0

    for kind, (pattern, extract) in sorted(FAMILIES.items()):
        paths = sorted(glob.glob(os.path.join(ROOT, pattern)))
        if not paths:
            # A family whose descriptors all vanished would otherwise make this
            # gate quietly vacuous for that family.
            problems.append(
                f"family {kind!r}: no descriptor matched {pattern!r} — refusing to "
                f"pass on an empty set"
            )
            continue

        for desc_path in paths:
            desc_rel = os.path.relpath(desc_path, ROOT)
            pkg_xml = os.path.join(os.path.dirname(desc_path), "package.xml")
            if not os.path.exists(pkg_xml):
                continue  # not migrated yet; not discoverable, still builds
            checked += 1
            rel = os.path.relpath(pkg_xml, ROOT)

            with open(desc_path, "rb") as fh:
                try:
                    data = tomllib.load(fh)
                except Exception as e:  # noqa: BLE001 — report, do not raise
                    problems.append(f"{desc_rel}: not valid TOML: {e}")
                    continue
            names = extract(data)
            if not names:
                problems.append(
                    f"{desc_rel}: declares no names — nothing could resolve to it"
                )
                continue

            found = declared_provisions(pkg_xml, kind)
            announced += len(found)
            if not found:
                problems.append(
                    f'{rel}: sits beside a {kind} descriptor but announces no '
                    f'<nano_ros_provides kind="{kind}"/> — it would be invisible '
                    f"to the phase-348 scan"
                )
            elif found != names:
                problems.append(
                    f"{rel}: provides {found} but {desc_rel} declares {names} — "
                    f"discovery and resolution must claim the same names, "
                    f"canonical first"
                )

    if problems:
        sys.stderr.write("check-provider-announcements: FAILED\n")
        for p in problems:
            sys.stderr.write(f"  {p}\n")
        return 1

    print(
        f"provider announcements: OK ({checked} migrated provider(s) across "
        f"{len(FAMILIES)} famil(ies), {announced} name(s) announced, all matching "
        f"their descriptor)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
