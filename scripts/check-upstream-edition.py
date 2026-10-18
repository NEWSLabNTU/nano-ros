#!/usr/bin/env python3
"""The committed upstream-edition snapshots agree with the pins that name them.

`scripts/gen-upstream-edition.py` records a ROS 2 edition's API (Jazzy today)
from pinned upstream sources, because that edition cannot be installed on the
box, and commits the result so the parity gates (`rmw-api-parity`,
`rmw-abi-shape`, `api-parity`, all `--edition all`) run on the fast line with
no install anywhere. This is the staleness half of that bargain — the role
`check-abi-bindings` plays for committed bindgen output — narrowed to what is
checkable OFFLINE:

  * every snapshot names, in its header, the commit `pins.toml` pins for the
    repo it was read from — a pin moved without regenerating, or a snapshot
    regenerated from a different tree, is red;
  * every recorded client-library surface (`api-surface/<edition>/*.json`)
    carries the same refs in its provenance;
  * the recorded contract is COVERED by the recorded signatures. Without that
    `rmw-abi-shape` silently skips a contract symbol it has no signature for —
    the comparison loops over signatures, not over the contract;
  * the Humble type surface the delta is taken against exists.

Re-deriving the snapshots needs the network and a ROS box, so it is not a gate:
`scripts/gen-upstream-edition.py --edition <ed> --src tmp/<ed>-src all`, then
`git diff --exit-code docs/reference/editions docs/reference/api-surface`.

Usage:
    scripts/check-upstream-edition.py --check
    scripts/check-upstream-edition.py --self-test
"""

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile

try:
    import tomllib  # Python 3.11+
except ModuleNotFoundError:  # the ros:humble image is 3.10
    import tomli as tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EDITIONS_DIR = os.path.join(ROOT, "docs", "reference", "editions")
SURFACE_DIR = os.path.join(ROOT, "docs", "reference", "api-surface")

# snapshot file -> the pins.toml repo whose commit its header must name.
SNAPSHOTS = {
    "rmw-implementation-contract.txt": "rmw_implementation",
    "rmw-implementation-signatures.txt": "rmw",
    "rmw-type-surface.txt": "rmw",
}
# recorded surface -> {provenance key: pins.toml repo}
SURFACES = {
    "rclc.json": {"rclc_ref": "rclc"},
    "rclcpp.json": {},
    "rclrs.json": {"ref": "ros2_rust"},
}
PIN_LINE = re.compile(r"^# pin (\S+): (\S+) @ ([0-9a-f]{40})")


def body(path):
    return [l.rstrip("\n") for l in open(path, encoding="utf-8")
            if l.strip() and not l.startswith("#")]


def check_edition(ed, edir, sdir):
    problems = []
    pins_path = os.path.join(edir, "pins.toml")
    with open(pins_path, "rb") as fh:
        pins = tomllib.load(fh)
    if pins.get("edition") != ed:
        problems.append(f"{ed}/pins.toml: edition = {pins.get('edition')!r}")
    repos = pins.get("repo", {})
    for snap, repo in SNAPSHOTS.items():
        path = os.path.join(edir, snap)
        if not os.path.exists(path):
            problems.append(f"{ed}/{snap}: missing (gen-upstream-edition.py ... rmw)")
            continue
        named = {}
        for line in open(path, encoding="utf-8"):
            m = PIN_LINE.match(line)
            if m:
                named[m.group(1)] = (m.group(2), m.group(3))
        want = repos.get(repo)
        if want is None:
            problems.append(f"{ed}/pins.toml: no [repo.{repo}], which {snap} is read from")
        elif repo not in named:
            problems.append(f"{ed}/{snap}: header names no pin for {repo}")
        elif named[repo] != (want["url"], want["commit"]):
            problems.append(f"{ed}/{snap}: read {repo} @ {named[repo][1]}, pins.toml says "
                            f"{want['commit']} — regenerate the snapshot or fix the pin")
    for surf, keys in SURFACES.items():
        path = os.path.join(sdir, surf)
        if not os.path.exists(path):
            problems.append(f"api-surface/{ed}/{surf}: missing (gen-upstream-edition.py ... surfaces)")
            continue
        prov = json.load(open(path, encoding="utf-8")).get("provenance", {})
        distro = prov.get("distro") or prov.get("ros_distro")
        if distro != ed:
            problems.append(f"api-surface/{ed}/{surf}: provenance distro {distro!r}")
        if surf != "rclrs.json" and prov.get("pins") != f"docs/reference/editions/{ed}/pins.toml":
            problems.append(f"api-surface/{ed}/{surf}: provenance names no pins file")
        for key, repo in keys.items():
            want = repos.get(repo, {}).get("commit")
            if prov.get(key) != want:
                problems.append(f"api-surface/{ed}/{surf}: {key} = {prov.get(key)!r}, "
                                f"pins.toml {repo} = {want!r}")
    contract = os.path.join(edir, "rmw-implementation-contract.txt")
    sigs = os.path.join(edir, "rmw-implementation-signatures.txt")
    if os.path.exists(contract) and os.path.exists(sigs):
        have = {l.split("\t", 1)[0] for l in body(sigs)}
        for sym in body(contract):
            if sym not in have:
                problems.append(f"{ed}: contract symbol {sym} has no recorded signature — "
                                "rmw-abi-shape would skip it silently")
    return problems


def check_all(editions_dir=EDITIONS_DIR, surface_dir=SURFACE_DIR):
    problems = []
    if not os.path.exists(os.path.join(editions_dir, "humble", "rmw-type-surface.txt")):
        problems.append("humble/rmw-type-surface.txt: missing "
                        "(gen-upstream-edition.py --edition humble type-surface)")
    eds = sorted(d for d in os.listdir(editions_dir)
                 if os.path.exists(os.path.join(editions_dir, d, "pins.toml")))
    for ed in eds:
        problems += check_edition(ed, os.path.join(editions_dir, ed),
                                  os.path.join(surface_dir, ed))
    return eds, problems


def self_test():
    """Mutate a copy of the real tree one way at a time; each must go red."""
    import shutil
    bad = []
    eds, problems = check_all()
    if problems:
        bad.append(f"the real tree is not clean, so the mutations prove nothing: {problems[:3]}")
    with tempfile.TemporaryDirectory() as td:
        def fresh():
            e = os.path.join(td, "editions")
            s = os.path.join(td, "surface")
            for p in (e, s):
                shutil.rmtree(p, ignore_errors=True)
            shutil.copytree(EDITIONS_DIR, e)
            shutil.copytree(SURFACE_DIR, s)
            return e, s

        def edit(path, old, new):
            text = open(path, encoding="utf-8").read()
            assert old in text, (path, old)
            open(path, "w", encoding="utf-8").write(text.replace(old, new, 1))

        for ed in eds:
            cases = []
            pins = tomllib.load(open(os.path.join(EDITIONS_DIR, ed, "pins.toml"), "rb"))["repo"]
            cases.append(("a moved pin", lambda e, s: edit(
                os.path.join(e, ed, "pins.toml"), pins["rmw"]["commit"], "0" * 40)))
            cases.append(("a contract symbol with no signature", lambda e, s: open(
                os.path.join(e, ed, "rmw-implementation-contract.txt"), "a").write(
                "rmw_planted_symbol\n")))
            cases.append(("a surface from another rclc", lambda e, s: edit(
                os.path.join(s, ed, "rclc.json"), pins["rclc"]["commit"], "f" * 40)))
            cases.append(("a deleted snapshot", lambda e, s: os.remove(
                os.path.join(e, ed, "rmw-type-surface.txt"))))
            for label, mutate in cases:
                e, s = fresh()
                mutate(e, s)
                _eds, probs = check_all(e, s)
                if not probs:
                    bad.append(f"{ed}: accepted {label}")
    # The generator's own negative controls ride along: the extractor that
    # wrote these snapshots is checked by the gate that trusts them.
    gen = subprocess.run([sys.executable, os.path.join(ROOT, "scripts", "gen-upstream-edition.py"),
                          "--self-test"], capture_output=True, text=True)
    if gen.returncode:
        bad.append("gen-upstream-edition.py --self-test: " + (gen.stderr or gen.stdout).strip())
    if bad:
        for b in bad:
            sys.stderr.write("check-upstream-edition --self-test: " + b + "\n")
        return 2
    print(f"check-upstream-edition --self-test: OK ({4 * len(eds)} mutation(s) over {eds})")
    return 0


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--self-test", action="store_true")
    a = ap.parse_args(argv)
    rc = self_test()
    if a.self_test or rc:
        return rc
    eds, problems = check_all()
    if problems:
        sys.stderr.write(f"check-upstream-edition: {len(problems)} problem(s):\n")
        for p in problems:
            sys.stderr.write(f"  {p}\n")
        sys.stderr.write(
            "\n  The committed snapshots must be what the pins say. Regenerate in the\n"
            "  ROS 2 distrobox:  scripts/gen-upstream-edition.py --edition <ed> --src\n"
            "  tmp/<ed>-src all   — and commit the snapshots with the pin move.\n")
        return 1 if a.check else 0
    print(f"check-upstream-edition: OK — {', '.join(eds)} snapshot(s) match their pins")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
