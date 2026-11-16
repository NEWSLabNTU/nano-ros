#!/usr/bin/env python3
"""What a gate lane's claims provision: the ONE reader of that table.

phase-565 W1 (RFC-0131 D3). A lane's claims live in `.config/gate-lanes.toml`
`[lane.<name>]`, and what the `submodules` claim provisions lives beside them,
as `[[provision.submodule]]` rows:

    [[provision.submodule]]
    path  = "third-party/dds/cyclonedds"
    tiers = ["build"]                 # the gate tiers that read it
    reads = "test-targets, ..."       # which gates, so a row says why it exists

Until W1 those rows were a second file, `.config/worktree-provisioning.txt`,
which `setup-worktree`, the lane preflight and CI's provisioner each parsed
with their own `grep`/`sed`. A worktree's provisioning and a lane's claims
could then disagree with nothing to say so. Every reader now goes through
this file, and `check-lane-provisioning-single-source` refuses a second one.

Usage:
  lane_provision.py submodules [--tiers <tier>...]
                                print one path per line: every row, or the rows
                                whose `tiers` meet the given ones
  lane_provision.py lane <name> print `tiers: ...` and `claims: ...` for a lane
  lane_provision.py --self-test
"""

import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11 (jammy)
    import tomli as tomllib

ROOT = Path(__file__).resolve().parents[2]
LANES = ROOT / ".config" / "gate-lanes.toml"


class ProvisionError(Exception):
    pass


def load(path=LANES):
    with open(path, "rb") as f:
        return tomllib.load(f)


def submodule_rows(decl):
    """The `[[provision.submodule]]` rows, each checked for its three keys.
    A malformed row is an error, never a skipped row: a row dropped here is a
    gate's dependency nobody provisions."""
    rows = decl.get("provision", {}).get("submodule")
    if not rows:
        raise ProvisionError("no [[provision.submodule]] rows in .config/gate-lanes.toml")
    out = []
    for i, r in enumerate(rows):
        missing = [k for k in ("path", "tiers", "reads") if not r.get(k)]
        if missing:
            raise ProvisionError(f"[[provision.submodule]] row {i + 1} lacks {', '.join(missing)}")
        if not isinstance(r["tiers"], list):
            raise ProvisionError(f"[[provision.submodule]] {r['path']}: `tiers` is not a list")
        out.append(r)
    return out


def submodules(decl, tiers=None):
    """Paths, in table order: all rows, or those whose tiers meet `tiers`."""
    rows = submodule_rows(decl)
    if tiers:
        rows = [r for r in rows if set(r["tiers"]) & set(tiers)]
    return [r["path"] for r in rows]


def lane(decl, name):
    d = decl.get("lane", {}).get(name)
    if d is None:
        raise ProvisionError(f"no [lane.{name}] in .config/gate-lanes.toml")
    return d.get("tiers", []), d.get("claims", [])


def self_test():
    bad = 0

    def case(name, ok):
        nonlocal bad
        print(f"  {'ok  ' if ok else 'FAIL'} {name}")
        bad += not ok

    decl = {
        "lane": {"fast": {"tiers": ["fast"], "claims": ["submodules"]}},
        "provision": {"submodule": [
            {"path": "a", "tiers": ["fast"], "reads": "g1"},
            {"path": "b", "tiers": ["build"], "reads": "g2"},
            {"path": "c", "tiers": ["fast", "build"], "reads": "g3"},
        ]},
    }
    case("every row, in table order", submodules(decl) == ["a", "b", "c"])
    case("a tier keeps the rows that name it", submodules(decl, ["fast"]) == ["a", "c"])
    case("two tiers keep the union", submodules(decl, ["fast", "build"]) == ["a", "b", "c"])
    case("a lane's tiers and claims", lane(decl, "fast") == (["fast"], ["submodules"]))
    # Negative controls: a row the readers would silently drop is an error.
    for desc, broken in (
        ("a row with no reason", {"path": "d", "tiers": ["fast"]}),
        ("a row with no tiers", {"path": "d", "reads": "g"}),
    ):
        try:
            submodules({"provision": {"submodule": [broken]}})
            case(f"negative control: {desc} is refused", False)
        except ProvisionError:
            case(f"negative control: {desc} is refused", True)
    try:
        submodules({})
        case("negative control: no table at all is refused", False)
    except ProvisionError:
        case("negative control: no table at all is refused", True)
    if bad:
        print("lane_provision self-test: FAILED", file=sys.stderr)
        return 1
    return 0


def main(argv):
    if argv == ["--self-test"]:
        return self_test()
    try:
        if argv[:1] == ["submodules"]:
            tiers = None
            if argv[1:2] == ["--tiers"]:
                tiers = argv[2:]
            elif len(argv) > 1:
                raise ProvisionError(f"unexpected argument {argv[1]!r}")
            for p in submodules(load(), tiers):
                print(p)
            return 0
        if argv[:1] == ["lane"] and len(argv) == 2:
            tiers, claims = lane(load(), argv[1])
            print("tiers:", " ".join(tiers))
            print("claims:", " ".join(claims))
            return 0
    except ProvisionError as e:
        print(f"lane_provision: {e}", file=sys.stderr)
        return 1
    print(__doc__.split("Usage:")[1].split("\n\n")[0], file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
