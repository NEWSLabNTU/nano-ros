#!/usr/bin/env python3
"""A board's downstream west fragment pins what nano-ros builds the board with.

A board crate may ship `west-downstream.yml` next to its `nros-board.toml`: the
west projects a downstream consumer imports with one line instead of pinning
each by hand (`fvp-aemv8r-smp`: `zephyr-lang-rust`; `s32z270-rtu0-zephyr`:
`zephyr-lang-rust`, `hal_nxp`, `cmsis`). A fragment is only worth importing if
its revisions are nano-ros's, so for every tracked fragment this asserts:

  * `zephyr-lang-rust` equals the project of the same name in the west
    manifest of the board's Zephyr line (`west.yml` for 3.7);
  * every other project is a Zephyr module (`[zephyr_module.<name>]` in
    nros-sdk-index.toml), at the revision the Zephyr tree that manifest pins
    names for it (Zephyr's own `west.yml` at that revision). That is read from
    the provisioned workspace of the line, at the pinned commit (not the
    workspace's HEAD). A host with no provisioned workspace, or one that has
    not fetched the pinned commit, says NOT VERIFIED for that project; the
    module-fixtures job (`just zephyr build-module-fixtures`) verifies the same
    pair on every run, since it fetches through the fragment and refuses a
    fragment that differs from its Zephyr tree;
  * no other project (a fragment never pins `zephyr` or nano-ros: the consumer
    owns those two);

and for every west compile-check row that names `zephyr_modules`
(examples/fixtures.toml): its board has a fragment that declares each one,
since that is where the module-fixtures job reads the revision from.

Run:  python3 scripts/check-zephyr-west-downstream.py  (its self-test runs first)
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

try:
    import tomllib
except ImportError:  # Python < 3.11
    import tomli as tomllib  # type: ignore[no-redef]

import yaml

REPO = Path(__file__).resolve().parents[1]
INDEX = REPO / "nros-sdk-index.toml"
FIXTURES = REPO / "examples" / "fixtures.toml"
FRAGMENT = "west-downstream.yml"
RUST_MODULE = "zephyr-lang-rust"

sys.path.insert(0, str(REPO / "scripts" / "lib"))
from zephyr_board_lines import LINES, zephyr_boards  # noqa: E402
from git_history import HistoryUnknown, history_git  # noqa: E402


def projects(doc: dict) -> dict:
    """`{name: revision}` of a west manifest's `projects`."""
    out = {}
    for p in (doc.get("manifest") or {}).get("projects") or []:
        out[p.get("name")] = str(p.get("revision", ""))
    return out


def tracked_fragments() -> list[Path]:
    p = subprocess.run(["git", "-C", str(REPO), "ls-files", "--", f"packages/boards/**/{FRAGMENT}"],
                       capture_output=True, text=True, check=True)
    return [REPO / line for line in p.stdout.split()]


def workspace_for(line: str) -> Path | None:
    p = subprocess.run(["bash", str(REPO / "scripts/lib/zephyr-workspace.sh"), "--version", line, "resolve"],
                       capture_output=True, text=True)
    ws = p.stdout.strip()
    return Path(ws) if p.returncode == 0 and ws and (Path(ws) / "zephyr" / ".git").exists() else None


def zephyr_pins(line: str, zephyr_rev: str) -> dict | None:
    """`{module: revision}` of Zephyr's `west.yml` at `zephyr_rev`, from the
    line's provisioned workspace; None when that is not readable here."""
    ws = workspace_for(line)
    if ws is None:
        return None
    # One path at one commit, bounded and never lazily fetched: a workspace
    # that does not hold the pinned commit is NOT VERIFIED, not a failure.
    try:
        p = history_git(ws / "zephyr", "show", f"{zephyr_rev}:west.yml", check=False)
    except HistoryUnknown:
        return None
    if p.returncode != 0:
        return None
    return projects(yaml.safe_load(p.stdout))


def check_fragment(rel: str, frag: dict, line: str, manifest: dict, modules: set,
                   pins: dict | None) -> tuple[list[str], list[str]]:
    """(failures, notes) for one fragment against its line's manifest."""
    bad, notes = [], []
    ours = projects(manifest)
    for name, rev in projects(frag).items():
        if name == RUST_MODULE:
            want = ours.get(RUST_MODULE)
            if rev != want:
                bad.append(f"{rel}: {RUST_MODULE} at {rev!r}, {LINES[line]} pins {want!r}")
        elif name in modules:
            if pins is None:
                notes.append(f"{rel}: {name} NOT VERIFIED (no provisioned {line} workspace "
                             f"holding Zephyr {ours.get('zephyr')})")
            elif rev != pins.get(name):
                bad.append(f"{rel}: {name} at {rev!r}, Zephyr {ours.get('zephyr')} pins "
                           f"{pins.get(name)!r}")
        else:
            bad.append(f"{rel}: {name!r} is neither {RUST_MODULE} nor a Zephyr module "
                       f"(`[zephyr_module.*]`); the consumer pins zephyr and nano-ros itself")
    return bad, notes


def self_test() -> None:
    manifest = {"manifest": {"projects": [{"name": "zephyr", "revision": "v3.7.2"},
                                          {"name": RUST_MODULE, "revision": "aaa"}]}}
    frag = {"manifest": {"projects": [{"name": RUST_MODULE, "revision": "aaa"},
                                      {"name": "hal_x", "revision": "h1"}]}}
    bad, notes = check_fragment("f", frag, "3.7", manifest, {"hal_x"}, {"hal_x": "h1"})
    assert not bad and not notes, (bad, notes)
    bad, _ = check_fragment("f", frag, "3.7", manifest, {"hal_x"}, {"hal_x": "h2"})
    assert len(bad) == 1 and "hal_x" in bad[0], bad
    manifest["manifest"]["projects"][1]["revision"] = "bbb"
    bad, _ = check_fragment("f", frag, "3.7", manifest, {"hal_x"}, {"hal_x": "h1"})
    assert len(bad) == 1 and RUST_MODULE in bad[0], bad
    bad, notes = check_fragment("f", frag, "3.7", manifest, {"hal_x"}, None)
    assert len(notes) == 1 and "NOT VERIFIED" in notes[0], notes
    bad, _ = check_fragment("f", {"manifest": {"projects": [{"name": "zephyr", "revision": "x"}]}},
                            "3.7", manifest, {"hal_x"}, {})
    assert len(bad) == 1 and "neither" in bad[0], bad
    sys.stdout.write("check-zephyr-west-downstream self-test: OK\n")


def main() -> int:
    self_test()
    modules = set(tomllib.loads(INDEX.read_text()).get("zephyr_module", {}))
    boards = {}  # descriptor dir -> (west_board, lines)
    for rel, wb, lines in zephyr_boards(REPO):
        boards[(REPO / rel).parent] = (wb, lines or [])
    bad, notes = [], []
    fragments = {}  # west_board -> {module names}
    pins_cache: dict = {}
    for path in tracked_fragments():
        rel = str(path.relative_to(REPO))
        wb, lines = boards.get(path.parent, ("", []))
        if not wb:
            bad.append(f"{rel}: no Zephyr board descriptor (nros-board.toml) next to it")
            continue
        frag = yaml.safe_load(path.read_text())
        fragments[wb] = set(projects(frag))
        for line in lines:
            if line not in LINES:
                bad.append(f"{rel}: board line {line!r} has no west manifest")
                continue
            manifest = yaml.safe_load((REPO / LINES[line]).read_text())
            zrev = projects(manifest).get("zephyr", "")
            if (line, zrev) not in pins_cache:
                pins_cache[(line, zrev)] = zephyr_pins(line, zrev)
            b, n = check_fragment(rel, frag, line, manifest, modules, pins_cache[(line, zrev)])
            bad += b
            notes += n
    rows = tomllib.loads(FIXTURES.read_text()).get("compile_check_fixture", [])
    for row in rows:
        mods = row.get("zephyr_modules") or []
        if not mods:
            continue
        wb = row.get("west_board", "")
        have = fragments.get(wb)
        if have is None:
            bad.append(f"examples/fixtures.toml row {row.get('id')!r}: board {wb!r} has no "
                       f"{FRAGMENT}, so the module-fixtures job has no revision for {mods}")
            continue
        for m in mods:
            if m not in have:
                bad.append(f"examples/fixtures.toml row {row.get('id')!r}: {m!r} is not in "
                           f"board {wb!r}'s {FRAGMENT}")
    for n in notes:
        sys.stdout.write(f"  note: {n}\n")
    if bad:
        sys.stderr.write("check-zephyr-west-downstream: FAIL\n\n")
        for b in bad:
            sys.stderr.write(f"  {b}\n")
        sys.stderr.write("\n  remedy: set the fragment's revision to the one named; hal_nxp/cmsis\n"
                         "  are the Zephyr tree's own pins (`git -C <ws>/zephyr show <rev>:west.yml`).\n")
        return 1
    sys.stdout.write(f"check-zephyr-west-downstream: OK -- {len(fragments)} fragment(s)\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
