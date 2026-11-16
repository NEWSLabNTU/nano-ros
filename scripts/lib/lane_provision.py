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
  lane_provision.py plan <lane> [--ci]
                                the lane's provisioning in run order, one step a
                                line: `run<TAB><provisioner>`, `job<TAB><claim>`
                                (CI: a job step provides it), `host<TAB><claim>
                                <TAB><remedy>`
  lane_provision.py remedy <capability>
  lane_provision.py --check     every capability has a provisioner or a host
                                remedy (gate `lane-claims-provisioned`)
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


PROVISIONERS = ROOT / "scripts" / "lib" / "claim-provision.sh"


def claim_entries(decl):
    """`[provision.claim]`: capability -> {remedy, provisioner?, ci?}."""
    return decl.get("provision", {}).get("claim", {})


def plan(decl, lane_name, ci=False):
    """What provisioning a lane means, in run order: ("run", provisioner) for
    a claim nano-ros installs, ("host", claim, remedy) for one it can only
    name. Under `ci`, a `ci = false` provisioner is a job step's, so it is
    ("job", claim)."""
    _, claims = lane(decl, lane_name)
    entries = claim_entries(decl)
    order = decl.get("provision", {}).get("order", [])
    missing = [c for c in claims if c not in entries]
    if missing:
        raise ProvisionError(f"lane `{lane_name}` claims {', '.join(missing)}, which "
                             "[provision.claim] does not say how to provide")
    runs = {entries[c]["provisioner"]: c for c in claims if entries[c].get("provisioner")}
    out = []
    for prov in order:
        if prov in runs:
            c = runs.pop(prov)
            if ci and entries[c].get("ci") is False:
                out.append(("job", c))
            else:
                out.append(("run", prov))
    if runs:
        raise ProvisionError(f"provisioner(s) {', '.join(sorted(runs))} not in [provision] order")
    out += [("host", c, entries[c]["remedy"]) for c in claims if not entries[c].get("provisioner")]
    return out


def check(decl, implemented):
    """Every capability says how it is provided; every provisioner is in the
    order and implemented; every lane claims only listed capabilities."""
    caps = set(decl.get("capabilities", {}))
    entries = claim_entries(decl)
    order = decl.get("provision", {}).get("order", [])
    out = [f"capability `{c}` has no [provision.claim] entry (a provisioner or a host remedy)"
           for c in sorted(caps - set(entries))]
    out += [f"[provision.claim] `{c}` names no capability" for c in sorted(set(entries) - caps)]
    for c, e in sorted(entries.items()):
        if not e.get("remedy"):
            out.append(f"[provision.claim] `{c}` has no remedy")
        prov = e.get("provisioner")
        if prov and prov not in order:
            out.append(f"[provision.claim] `{c}`: provisioner `{prov}` is not in [provision] order")
        if prov and prov not in implemented:
            out.append(f"[provision.claim] `{c}`: no claim_{prov.replace('-', '_')} in "
                       "scripts/lib/claim-provision.sh")
    for name, l in sorted(decl.get("lane", {}).items()):
        out += [f"lane `{name}` claims `{c}`, which [capabilities] does not list"
                for c in l.get("claims", []) if c not in caps]
    return out


# Who runs the provisioners (phase-565 W2): a worktree runs the `dev` lane's,
# a CI job its own lane's. Each is a (file, text it must contain, what breaks).
WIRING = (
    ("justfile", "scripts/lib/claim-provision.sh dev",
     "`just setup-worktree` no longer provisions the dev lane's claims from the table"),
    ("scripts/ci/provision-gate-lane.sh", 'scripts/lib/claim-provision.sh "$lane" --ci',
     "CI's provisioner no longer runs the shared claim provisioners"),
)


def wiring_problems(root=ROOT):
    out = []
    for f, needle, why in WIRING:
        try:
            text = (root / f).read_text()
        except OSError:
            text = ""
        if needle not in text:
            out.append(f"{f}: {why} (expected `{needle}`)")
    return out


def implemented_provisioners(path=PROVISIONERS):
    import re
    try:
        text = path.read_text()
    except OSError:
        return set()
    return {m.replace("_", "-") for m in re.findall(r"^claim_([a-z_]+)\(\)", text, re.M)}


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
    full = {
        "capabilities": {"cli": "", "submodules": "", "cmake": ""},
        "lane": {"dev": {"tiers": ["fast"], "claims": ["cmake", "cli", "submodules"]}},
        "provision": {
            "order": ["submodules", "cli"],
            "claim": {
                "cli": {"provisioner": "cli", "ci": False, "remedy": "just setup-cli"},
                "submodules": {"provisioner": "submodules", "remedy": "just setup-worktree"},
                "cmake": {"remedy": "install cmake"},
            },
            "submodule": [{"path": "a", "tiers": ["fast"], "reads": "g"}],
        },
    }
    case("a lane's plan: provisioners in [provision] order, then host tools",
         plan(full, "dev") == [("run", "submodules"), ("run", "cli"), ("host", "cmake", "install cmake")])
    case("under --ci a `ci = false` provisioner is the job's",
         plan(full, "dev", ci=True)[1] == ("job", "cli"))
    case("a complete table checks clean", check(full, {"cli", "submodules"}) == [])
    # Negative control (phase-565 W2's): a claim added to `dev` with no
    # provisioner and no named host tool.
    import copy
    broken = copy.deepcopy(full)
    broken["capabilities"]["colcon"] = ""
    broken["lane"]["dev"]["claims"].append("colcon")
    case("negative control: a claim with neither a provisioner nor a host remedy is refused",
         any("`colcon` has no [provision.claim] entry" in p for p in check(broken, {"cli", "submodules"})))
    try:
        plan(broken, "dev")
        case("negative control: planning such a lane refuses", False)
    except ProvisionError:
        case("negative control: planning such a lane refuses", True)
    case("negative control: a provisioner with no implementation is refused",
         any("no claim_cli" in p for p in check(full, {"submodules"})))
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
        if argv[:1] == ["plan"] and len(argv) in (2, 3) and argv[2:] in ([], ["--ci"]):
            for step in plan(load(), argv[1], ci=argv[2:] == ["--ci"]):
                print("\t".join(step))
            return 0
        if argv[:1] == ["remedy"] and len(argv) == 2:
            e = claim_entries(load()).get(argv[1])
            if not e:
                raise ProvisionError(f"no [provision.claim] entry for `{argv[1]}`")
            print(e["remedy"])
            return 0
        if argv == ["--check"]:
            import contextlib
            import io
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                rc = self_test()
            if rc:
                sys.stderr.write(buf.getvalue())
                return 1
            problems = check(load(), implemented_provisioners()) + wiring_problems()
            if problems:
                print("lane-claims-provisioned: FAILED (phase-565 W2)", file=sys.stderr)
                for p in problems:
                    print("  " + p, file=sys.stderr)
                return 1
            print("lane-claims-provisioned: OK (every capability has a provisioner or a host remedy)")
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
