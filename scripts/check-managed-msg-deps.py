#!/usr/bin/env python3
"""phase-517 W8 (RFC-0098 amendment, accepted 2026-10-26) — a message
`<depend>` is ONE edit for a Rust package, so no tracked manifest may hand-write
the path row it implies.

# The rule

`nros sync` (and `nros build`, before planning) writes one `# nros-managed`
row per `<depend>` / `<build_depend>` / `<exec_depend>` that resolves to a
message package:

    std_msgs = { path = "../../generated/std_msgs", default-features = false }  # nros-managed

An UNTAGGED path dependency into a `generated/` tree is the second spelling of
a fact `package.xml` already states. It is what made adding a message package
two edits, and it is what drifts: the tag is how sync tells its own rows from a
user's override, so an untagged row is one sync will never update or remove.

The row's target is the WORKSPACE's (or leaf's) `generated/<pkg>` — sync output,
gitignored. The committed core interface crates under
`packages/interfaces/generated/<edition>/nros-*` are a different thing (they
exist before any codegen and resolve from a bare clone) and are not this rule's
subject.

# What is exempt, and why

* A leaf whose `system.toml` deploys only to a board the registry
  (`packages/boards/board-support.toml`) derives as "Not supported": no lane
  builds it, so its manifest is parked as-is (ESP32 is dormant, issue 1525).
  DERIVED from the leaf's own board, never a path list.
* Rows a different generator owns, keyed on (manifest, crate) in `EXEMPT` with
  the reason as the value.

Run: python3 scripts/check-managed-msg-deps.py
"""

from __future__ import annotations

import os
import json
import re
import subprocess
import sys
from pathlib import Path, PurePosixPath

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
from exemptions import Exemptions  # noqa: E402
from git_hook_env import nros_clear_inherited_git_env  # noqa: E402

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11 — the repo's interpreter is 3.10
    import tomli as tomllib

REPO = Path(__file__).resolve().parents[1]
TAG = "# nros-managed"

_PX4 = (
    "px4_msgs is generated from the PX4-Autopilot .msg tree by `nros "
    "generate-px4-msgs`, not by `nros sync`; the package.xml says so and does "
    "not declare it, so sync never manages this row"
)
EXEMPT = {
    ("examples/px4/rust/companion/offboard-companion/Cargo.toml", "px4_msgs"): _PX4,
    ("examples/px4/rust/companion/px4-probe/Cargo.toml", "px4_msgs"): _PX4,
    ("examples/px4/rust/companion/px4-stub/Cargo.toml", "px4_msgs"): _PX4,
}
# One step away from each entry: the same manifest with a sync-generated crate,
# and the same crate in a manifest the reason does not name.
NEIGHBOURS = [
    ("examples/px4/rust/companion/px4-stub/Cargo.toml", "std_msgs"),
    ("examples/linux/rust/talker/Cargo.toml", "px4_msgs"),
]

DEP_HEADER = re.compile(
    r"^\[(?:target\.[^\]]+\.)?(?:dev-|build-)?dependencies(?P<sub>\.[^\]]+)?\]\s*(?:#.*)?$"
)
PATH_VALUE = re.compile(r'\bpath\s*=\s*"(?P<path>[^"]*)"')
KEY = re.compile(r'^\s*"?(?P<key>[A-Za-z0-9_-]+)"?\s*=')


def is_generated_target(manifest: str, path: str) -> bool:
    """`path` names `<something>/generated/<pkg>` (sync output) — not a
    committed core crate under `packages/interfaces/`."""
    parts = PurePosixPath(path).parts
    if len(parts) < 2 or parts[-2] != "generated":
        return False
    resolved = os.path.normpath(str(PurePosixPath(manifest).parent / path))
    return not resolved.startswith("packages/interfaces/")


def findings_in(manifest: str, text: str) -> "list[tuple[str, str, int]]":
    """(manifest, crate, line) for each untagged path row into `generated/`."""
    out = []
    section_crate = None  # the crate of a `[dependencies.<crate>]` table
    in_deps = False
    for n, line in enumerate(text.splitlines(), 1):
        s = line.strip()
        if s.startswith("["):
            m = DEP_HEADER.match(s)
            in_deps = bool(m)
            section_crate = m["sub"][1:].strip('"') if m and m["sub"] else None
            continue
        if not in_deps or s.startswith("#"):
            continue
        pv = PATH_VALUE.search(line)
        if not pv or not is_generated_target(manifest, pv["path"]):
            continue
        if section_crate is not None:
            out.append((manifest, section_crate, n))  # table form: cannot carry the tag
            continue
        if TAG in line:
            continue
        k = KEY.match(line)
        out.append((manifest, k["key"] if k else "?", n))
    return out


def _load(path: Path) -> dict:
    with open(path, "rb") as fh:
        return tomllib.load(fh)


def parked_boards(repo: Path) -> "set[str]":
    """Every board NAME whose crate the registry derives as "Not supported"
    (RFC-0114 D10: nothing builds or boots it), read from the registry's
    generated status file."""
    status = json.loads((repo / "packages/boards/board-support-status.json").read_text())
    unsupported = {r["crate"] for r in status["rows"] if r["status"] == "Not supported"}
    names: "set[str]" = set()
    for desc in tracked(repo, "packages/boards/**/nros-board.toml"):
        d = repo / desc
        crate_of_dir = None
        cargo = d.parent / "Cargo.toml"
        if cargo.is_file():
            crate_of_dir = _load(cargo).get("package", {}).get("name")
        for b in _load(d).get("board", []):
            crate = b.get("board_crate") or crate_of_dir
            if crate in unsupported:
                names.update(b.get("names", []))
    return names


BOARD = re.compile(r'^\s*board\s*=\s*"(?P<b>[^"]+)"', re.M)


def parked_leaf(repo: Path, manifest: str, parked: "set[str]") -> bool:
    """The nearest `system.toml` at or above the manifest deploys ONLY to parked
    boards."""
    d = (repo / manifest).parent
    while True:
        st = d / "system.toml"
        if st.is_file():
            boards = set(BOARD.findall(st.read_text()))
            return bool(boards) and boards <= parked
        if d == repo or d.parent == d:
            return False
        d = d.parent


def tracked(repo: Path, pattern: str) -> "list[str]":
    out = subprocess.run(
        ["git", "-C", str(repo), "ls-files", "-z", "--", pattern],
        capture_output=True, text=True, check=True,
        env=nros_clear_inherited_git_env(dict(os.environ)),
    ).stdout
    return sorted(p for p in out.split("\0") if p)


def scan(repo: Path, exempt: Exemptions) -> "list[tuple[str, str, int]]":
    parked = parked_boards(repo)
    found = []
    for manifest in tracked(repo, "*Cargo.toml"):
        if manifest.startswith("packages/interfaces/"):
            continue
        text = (repo / manifest).read_text()
        if "generated/" not in text:
            continue
        rows = [r for r in findings_in(manifest, text) if not exempt.covers((r[0], r[1]))]
        if rows and parked_leaf(repo, manifest, parked):
            continue
        found += rows
    return found


def self_test() -> None:
    """NEGATIVE CONTROLS, on the normal path: a gate whose answer is "nothing
    found" says the same thing when its pattern is wrong."""
    m = "examples/workspaces/rust/src/talker_pkg/Cargo.toml"
    bad = '[dependencies]\nstd_msgs = { path = "../../generated/std_msgs", default-features = false }\n'
    assert findings_in(m, bad) == [(m, "std_msgs", 2)], findings_in(m, bad)
    good = bad.replace("}\n", "}  # nros-managed\n")
    assert findings_in(m, good) == [], "a tagged row is the rule's answer"
    # every dependency table, including target-scoped and the table form
    tgt = "[target.'cfg(unix)'.dev-dependencies]\nx = { path = \"generated/x\" }\n"
    assert findings_in("a/Cargo.toml", tgt) == [("a/Cargo.toml", "x", 2)]
    tbl = '[dependencies.x]\npath = "generated/x"\n'
    assert findings_in("a/Cargo.toml", tbl) == [("a/Cargo.toml", "x", 2)]
    # NOT the subject: the committed core crates, other tables, comments
    core = '[dependencies]\nnros-std-msgs = { path = "../../interfaces/generated/humble/nros-std-msgs" }\n'
    assert findings_in("packages/core/nros-node/Cargo.toml", core) == []
    assert findings_in(m, '[workspace]\nmembers = ["generated/x"]\n') == []
    assert findings_in(m, '[dependencies]\n# x = { path = "generated/x" }\n') == []
    # the registry derivation names the dormant board, and only it
    parked = parked_boards(REPO)
    assert "esp32-c3-baremetal" in parked and "linux" not in parked, parked
    problems = Exemptions(EXEMPT, what="row").check(NEIGHBOURS)
    assert not problems, problems


def main() -> int:
    self_test()
    exempt = Exemptions(EXEMPT, what="row")
    found = scan(REPO, exempt)
    stale = exempt.stale()
    if not found and not stale:
        print("check-managed-msg-deps: OK (every path row into generated/ is # nros-managed)")
        return 0
    if found:
        print(f"check-managed-msg-deps: {len(found)} untagged path row(s) into a "
              "generated/ tree:\n", file=sys.stderr)
        for manifest, crate, n in found:
            print(f"  {manifest}:{n}: {crate}", file=sys.stderr)
        print(
            "\nA message dependency is stated ONCE, in package.xml (RFC-0098, phase-517 W8).\n"
            "Delete the row and declare `<depend><pkg></depend>`, then run `nros sync`\n"
            "(or `nros ws managed-deps <dir>`): sync writes the row back, tagged\n"
            "`# nros-managed`. A row another generator owns goes in EXEMPT with its reason.",
            file=sys.stderr,
        )
    for k in stale:
        print(f"check-managed-msg-deps: stale exemption {k!r} matches no row", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
