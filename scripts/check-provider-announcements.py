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
      order, canonical first, since names[0] is what error messages list;
  A3  for a family whose descriptors carry no names at all, the announced names
      are UNIQUE across the family — see "Two shapes of family" below.

**Two shapes of family, and A2 exists only for one of them.** `rmw`, `board` and
`platform` predate RFC-0087 D4: their descriptors repeat the provider's names, so
there are two spellings of one fact and A2 is what keeps them equal. D4 removed
`names` from NEW descriptors — a descriptor now carries only what no convention
can produce, and the announcement is the ONLY place a name is written. For such a
family A2 has nothing to compare against and cannot be written honestly, so the
row declares `extract=None` and the check becomes A1 + A3: the descriptor must
still be announced (or it is invisible to the scan), and since the announcement
is the sole spelling, the one thing that can still go wrong is two providers
claiming one name.

Do NOT "fix" a nameless family back into an A2 comparison by adding `names` to
its descriptor: that re-creates the second spelling D4 deleted, and A2 would then
be checking the descriptor against a copy of itself — the cannot-fail gate
`check-rmw-descriptors` retired its own S-checks over (see its docstring).
Equally, do not delete A2 from the three families that DO have `names`; their
duplication is real and unchecked otherwise.

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

# kind -> (descriptor glob, how to pull the declared names out of it — or None
# for a family whose descriptors carry no names, see A3 above).
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
    # phase-421 W4 / RFC-0088 D6. The first family born after RFC-0087 D4, so
    # its descriptor has NO `names` key and never will: `nros-serdes.toml`
    # carries `impl` and `format_id`, the two facts no convention can derive,
    # and the `<nano_ros_provides>` announcement is the name. `None` selects
    # A1 + A3 instead of A1 + A2 (see the module docstring).
    #
    # Everything serdes-SPECIFIC — descriptor well-formedness, the `format_id`
    # discriminant, and the descriptor a package announcing `serdes` must have —
    # lives in `scripts/check-serdes-descriptors.py`, the same split that keeps
    # this gate one gate for every family rather than one per family.
    "serdes": ("packages/*/*/nros-serdes.toml", None),
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
    claimed = {}  # (kind, name) -> package.xml that announced it (A3)

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
            names = None if extract is None else extract(data)
            if extract is not None and not names:
                problems.append(
                    f"{desc_rel}: declares no names — nothing could resolve to it"
                )
                continue

            found = declared_provisions(pkg_xml, kind)
            announced += len(found)
            if not found:
                # A1 — true for both shapes of family.
                problems.append(
                    f'{rel}: sits beside a {kind} descriptor but announces no '
                    f'<nano_ros_provides kind="{kind}"/> — it would be invisible '
                    f"to the phase-348 scan"
                )
            elif names is None:
                # A3 — a nameless-descriptor family (RFC-0087 D4). There is no
                # second spelling to compare against, so the only failure left
                # is two providers answering to one name.
                for n in found:
                    prior = claimed.get((kind, n))
                    if prior is not None and prior != rel:
                        problems.append(
                            f"{rel}: announces {kind} name {n!r}, already announced "
                            f"by {prior} — a name resolves to one provider, and "
                            f"for this family the announcement is the ONLY place "
                            f"it is written"
                        )
                    claimed[(kind, n)] = rel
            elif found != names:
                # A2 — a family whose descriptor repeats the names.
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

    nameless = sum(1 for _, e in FAMILIES.values() if e is None)
    print(
        f"provider announcements: OK ({checked} migrated provider(s) across "
        f"{len(FAMILIES)} famil(ies), {announced} name(s) announced; "
        f"{len(FAMILIES) - nameless} famil(ies) matched name-for-name against "
        f"their descriptor, {nameless} nameless-descriptor famil(ies) checked "
        f"for announcement + uniqueness)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
