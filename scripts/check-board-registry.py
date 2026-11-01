#!/usr/bin/env python3
"""check-board-registry — RFC-0114 D12's one gate (phase-522 W3).

The board support registry (`packages/boards/board-support.toml` and the cell
table it names) is the only place a board fact is typed. This gate holds that:

  1. GENERATED VIEWS ARE CURRENT. `scripts/gen-board-support.py` regenerates
     every view into a temporary directory and each must equal the tracked
     file byte for byte (the cells, the probe tracks, issue 2169's ROWS, the
     book pages and the generated regions of SUPPORTED_MD and SUMMARY.md).
  2. THE REGISTRY HOLDS. Every predicate the five gates this one replaced
     enforced: the support-level predicates of `check-board-tiers.py`
     (RFC-0114 D10/D11 and, until phase-522 W4 retires them, the tiers), and
     the book run/flash sections of `check-book-board-run-sections.py`.
  3. NO BOARD FACT IS TYPED ELSEWHERE, where the file layout lets one be seen:
       - a `cell(...)` in matrix.rs outside the generated list
       - a literal track list in `probe-platforms.yml`'s matrix
       - a literal `PLATFORM_CHAPTER=` track table in run-bootstrap-probe.sh
       - a `("<chapter>.md", ...)` tuple in the 2169 test instead of the
         generated include
       - a `just probe <track>` recipe with no `[[probe]]` track, or a
         track with no recipe
       - a `[[probe]]` naming a board row or a `qemu_board` that does not exist
       - a QEMU-booting `probe-run` block in a chapter with no `[[probe]]`
         track carrying its `qemu_board` (the static half of issue 2169's test)
       - a fixture row whose `NROS_BOARD` names no board descriptor (fixture
         rows stay authored under RFC-0114's narrowing, so they are CHECKED)
       - a row typing `emulator` while an in-tree descriptor exists for its
         board: the emulator is the descriptor's `[board.run] emulator`
         (RFC-0118 D4, phase-528 W2), read by the generator; only a row with
         no in-tree descriptor (`external`) may type it

Known gaps, printed on every run (RFC-0114 evaluation step 2: no silent gap):
  - a fixture row's build arguments are not checked against the descriptor;
    the narrowing keeps them authored.
  - `interop::CELLS` rows stay authored and keep their own Rust gates
    (`matrix_fixture_coverage.rs` G1-G5), which read the compiled bindings.
  - a fixture locator port is not required to be unique: a talker and its
    listener share one router by design (48 locators over 96 rows).

The self-test runs on every call: each mutation the retired gates caught, and
each hand-typed fact above, is planted in a copy and must be caught by name.

Run:  python3 scripts/check-board-registry.py
"""

from __future__ import annotations

import importlib.util
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts/lib"))
import board_registry  # noqa: E402

MATRIX_RS = "packages/testing/nros-tests/src/matrix.rs"
WORKFLOW = ".github/workflows/probe-platforms.yml"
HARNESS = "scripts/probe/run-bootstrap-probe.sh"
BUILD_NEXT = "packages/cli/nros-cli-core/src/cmd/build_next.rs"
PROBE_JUST = "just/probe.just"
FIXTURES = "examples/fixtures.toml"
CHAPTERS = "book/src/getting-started"

KNOWN_GAPS = (
    "fixture build arguments are authored (RFC-0114 narrowing); only their board is checked",
    "interop::CELLS stays authored; matrix_fixture_coverage.rs G1-G5 hold it",
    "fixture locator ports may repeat: a talker and its listener share a router",
)

# Recipes in just/probe.just that are not tracks.
PROBE_NON_TRACKS = {"default", "bootstrap", "checkout", "installed", "platform",
                    "platform-asset", "zenoh", "bootstrap-arch", "bootstrap-fedora",
                    "bootstrap-zsh", "extract"}


def load(name: str, rel: str):
    spec = importlib.util.spec_from_file_location(name, ROOT / rel)
    mod = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(mod)
    return mod


# --------------------------------------------------------------------------
# hand-typed facts (rule 3); each takes text so the self-test can plant one

def stray_cells(matrix_rs: str) -> list[str]:
    out = []
    for n, line in enumerate(matrix_rs.splitlines(), 1):
        if re.match(r"\s*cell\(\s*[A-Z]\w*\s*,", line):
            out.append(f"{MATRIX_RS}:{n}: a `cell(...)` typed outside the generated list; "
                       "add a `[[cell]]` to packages/boards/board-support-cells.toml and "
                       "run scripts/gen-board-support.py")
    return out


def literal_workflow_tracks(yml: str) -> list[str]:
    if re.search(r"^\s*platform:\s*\n(\s*- [\w-]+\s*\n)+", yml, re.M):
        return [f"{WORKFLOW}: the chapter matrix lists tracks by hand; it must read "
                "`${{ fromJSON(needs.asset.outputs.tracks) }}`, the registry's `[[probe]]` "
                "tracks with `weekly = true`"]
    if "fromJSON(needs.asset.outputs.tracks)" not in yml:
        return [f"{WORKFLOW}: the chapter matrix does not read the generated track list"]
    return []


def literal_harness_tracks(sh: str) -> list[str]:
    if re.search(r"^\s+[a-z0-9-]+\)\s+PLATFORM_CHAPTER=", sh, re.M):
        return [f"{HARNESS}: a `<track>) PLATFORM_CHAPTER=…` table is typed by hand; tracks "
                "are `[[probe]]` rows of the registry, read through probe-track.py"]
    if "probe-track.py" not in sh:
        return [f"{HARNESS}: does not resolve its track through scripts/probe/probe-track.py"]
    return []


def literal_boot_rows(rs: str) -> list[str]:
    m = re.search(r"fn every_probed_chapter_boot_line_is_the_printed_run_command\(\)"
                  r"(.*?)\n    \}\n", rs, re.S)
    if not m:
        return [f"{BUILD_NEXT}: issue 2169's test "
                "`every_probed_chapter_boot_line_is_the_printed_run_command` is gone"]
    body = m.group(1)
    errs = []
    if 'include!("probe_boot_rows.rs")' not in rs or "PROBE_BOOT_ROWS" not in body:
        errs.append(f"{BUILD_NEXT}: issue 2169's ROWS must be `PROBE_BOOT_ROWS`, the "
                    "generated `include!(\"probe_boot_rows.rs\")`")
    if re.search(r'\(\s*"[\w-]+\.md"\s*,\s*"[\w-]*"\s*,\s*"[\w-]+"\s*\)', body):
        errs.append(f"{BUILD_NEXT}: a `(\"<chapter>.md\", variant, board)` tuple is typed in "
                    "issue 2169's test; give the `[[probe]]` track a `qemu_board` instead")
    return errs


def probe_recipe_drift(just_text: str, tracks: list[str]) -> list[str]:
    recipes = set(re.findall(r"^([a-z0-9-]+)(?:\s[^:\n]*)?:(?!=)", just_text, re.M))
    recipes -= PROBE_NON_TRACKS
    have = set(tracks) - {"linux"}  # the default `installed` recipe replays linux
    errs = [f"{PROBE_JUST}: recipe `{r}` names no `[[probe]]` track in the registry"
            for r in sorted(recipes - have)]
    errs += [f"{PROBE_JUST}: `[[probe]]` track `{t}` has no `just probe {t}` recipe"
             for t in sorted(have - recipes)]
    return errs


def probe_reference_errors(probes: list[dict], rows: list[dict],
                           descriptor_names: set[str]) -> list[str]:
    keys = {board_registry.row_name(r) for r in rows}
    errs = []
    for p in probes:
        if p.get("row") and p["row"] not in keys:
            errs.append(f"[[probe]] `{p['track']}` names board row `{p['row']}`, which is "
                        "not a registry row")
        if p.get("qemu_board") and p["qemu_board"] not in descriptor_names:
            errs.append(f"[[probe]] `{p['track']}` names qemu_board `{p['qemu_board']}`, "
                        "which no nros-board.toml descriptor names")
    return errs


def qemu_blocks(chapter_text: str) -> list[str]:
    """Variants of the chapter's `probe-run` blocks that boot QEMU."""
    out = []
    for m in re.finditer(r"^```([^\n]*)\n(.*?)^```", chapter_text, re.M | re.S):
        info, body = m.group(1), m.group(2)
        if "probe-run" not in info:
            continue
        if "qemu-system" not in body and "QEMU_SYSTEM_" not in body:
            continue
        v = re.search(r"\bvariant=(\S+)", info)
        out.append(v.group(1) if v else "")
    return out


def uncovered_qemu_blocks(chapters: dict[str, str], probes: list[dict]) -> list[str]:
    covered = {(Path(p["chapter"]).name, p.get("variant", "")) for p in probes
               if p.get("qemu_board")}
    errs = []
    for name, text in sorted(chapters.items()):
        for v in qemu_blocks(text):
            if (name, v) not in covered:
                errs.append(f"{CHAPTERS}/{name}: a `probe-run` block (variant={v!r}) boots "
                            "QEMU and no `[[probe]]` track carries its `qemu_board`, so "
                            "issue 2169's test cannot compare it to the printed command")
    return errs


def board_key_errors(rows: list[dict], descriptor_names: set[str]) -> list[str]:
    """`board_keys` (phase-522 W5) must name descriptors, once each."""
    errs, owner = [], {}
    for r in rows:
        for k in r.get("board_keys") or []:
            if k not in descriptor_names:
                errs.append(f"{board_registry.row_name(r)}: board_keys names `{k}`, which no "
                            "nros-board.toml descriptor names")
            if k in owner:
                errs.append(f"board key `{k}` is claimed by {owner[k]} and "
                            f"{board_registry.row_name(r)}; one board is one row")
            owner[k] = board_registry.row_name(r)
    return errs


def fixture_board_errors(fixtures: str, descriptor_names: set[str]) -> list[str]:
    errs = []
    for b in sorted(set(re.findall(r'NROS_BOARD\s*=\s*"([^"]+)"', fixtures))):
        if b not in descriptor_names:
            errs.append(f"{FIXTURES}: NROS_BOARD = {b!r} names no board descriptor "
                        "(packages/boards/*/nros-board.toml `names`)")
    return errs


def descriptor_names(root: Path) -> set[str]:
    names = set()
    # Tracked descriptors at any depth: a conf bundle sits under its family
    # crate (`nros-board-zephyr/boards/fvp-aemv8r-smp/`, phase-337 W9.a).
    tracked = subprocess.run(["git", "ls-files", "-z", "--", "packages/boards/**nros-board.toml"],
                             cwd=root, capture_output=True, text=True, check=True).stdout
    for rel in sorted(p for p in tracked.split("\0") if p):
        f = root / rel
        doc = board_registry.load(f)
        boards = doc.get("board", [])
        boards = boards if isinstance(boards, list) else [boards]
        for b in boards:
            names.update(b.get("names") or [])
            if b.get("name"):
                names.add(b["name"])
    return names


# --------------------------------------------------------------------------

def generated_errors(gen) -> list[str]:
    reg = gen.Registry(ROOT)
    produced = gen.outputs(reg)
    errs = []
    with tempfile.TemporaryDirectory() as d:
        for rel, text in produced.items():
            (Path(d) / rel).parent.mkdir(parents=True, exist_ok=True)
            (Path(d) / rel).write_text(text)
        for rel in produced:
            mine = (Path(d) / rel).read_bytes()
            tracked = (ROOT / rel).read_bytes() if (ROOT / rel).exists() else None
            if mine != tracked:
                errs.append(f"{rel}: differs from a fresh generation; edit the registry, "
                            "not the view, and run `python3 scripts/gen-board-support.py`")
    for rel in gen.stale_board_pages(reg, produced):
        errs.append(f"{rel}: an evidence page no registry row produces; regenerate")
    return errs


def self_test(gen) -> None:
    def fails(errs, what):
        assert errs, f"self-test: {what} was not caught"

    fails(stray_cells("    cell(Linux, Rust, Zenoh, Pubsub, Example, Runtime),\n"),
          "a cell(...) typed in matrix.rs")
    assert not stray_cells("const fn cell(p: PlatformId) -> Cell {\n"), \
        "the `cell` constructor itself is not a cell"
    fails(literal_workflow_tracks("      matrix:\n        platform:\n          - baremetal\n"),
          "a probe matrix entry typed in the workflow")
    assert not literal_workflow_tracks(
        "        platform: ${{ fromJSON(needs.asset.outputs.tracks) }}\n")
    fails(literal_harness_tracks('    baremetal)       PLATFORM_CHAPTER=book/x.md ;;\n'),
          "a track typed in the harness's case table")
    test = ("    include!(\"probe_boot_rows.rs\");\n"
            "    fn every_probed_chapter_boot_line_is_the_printed_run_command() {\n"
            "        const ROWS: &[(&str, &str, &str)] = PROBE_BOOT_ROWS;\n{}    }}\n")
    assert not literal_boot_rows(test.replace("{}", "").replace("}}", "}"))
    fails(literal_boot_rows(test.replace("{}", '        ("bare-metal.md", "", "x"),\n')
                            .replace("}}", "}")),
          "a ROWS tuple typed in issue 2169's test")
    fails(probe_recipe_drift("baremetal: (platform \"baremetal\")\n", ["linux"]),
          "a probe recipe with no track")
    fails(probe_recipe_drift("", ["linux", "baremetal"]), "a track with no recipe")
    fails(probe_reference_errors([{"track": "t", "row": "nope@X"}], [], set()),
          "a probe naming a missing row")
    fails(probe_reference_errors([{"track": "t", "qemu_board": "nope"}], [], {"b"}),
          "a probe naming a missing board")
    chapter = "```bash probe=30 probe-run=10 variant=arm\nqemu-system-arm -kernel x\n```\n"
    fails(uncovered_qemu_blocks({"n.md": chapter}, []), "a QEMU run block with no track")
    assert not uncovered_qemu_blocks(
        {"n.md": chapter},
        [{"track": "t", "chapter": "getting-started/n.md", "variant": "arm",
          "qemu_board": "b"}])
    fails(fixture_board_errors('NROS_BOARD = "ghost"', {"real"}),
          "a fixture row naming an undeclared board")
    fails(board_key_errors([{"crate": "a", "board_keys": ["ghost"]}], {"real"}),
          "a board key naming no descriptor")
    fails(board_key_errors([{"crate": "a", "board_keys": ["k"]},
                            {"crate": "b", "board_keys": ["k"]}], {"k"}),
          "one board key claimed by two rows")
    # The retired gates' own negative controls run where they live:
    # check-board-tiers (tiers, maintainers, migration), board_registry (D10/D11,
    # round-trip, the stale-day degradation), check-book-board-run-sections
    # (run/flash blocks) and gen-board-support (a hand-edited view is stale).
    gen.self_test()


def main() -> int:
    gen = load("gen_board_support", "scripts/gen-board-support.py")
    tiers = load("check_board_tiers", "scripts/check-board-tiers.py")
    runs = load("check_book_board_run_sections", "scripts/check-book-board-run-sections.py")
    self_test(gen)
    # The rendered book applies D10's clock through this preprocessor; its
    # negative controls (the FVP row on both sides of its stale date) run here.
    load("mdbook_board_status", "scripts/mdbook-board-status.py").self_test()

    errs = generated_errors(gen)
    reg = gen.Registry(ROOT)
    names = descriptor_names(ROOT)
    errs += stray_cells((ROOT / MATRIX_RS).read_text())
    errs += literal_workflow_tracks((ROOT / WORKFLOW).read_text())
    errs += literal_harness_tracks((ROOT / HARNESS).read_text())
    errs += literal_boot_rows((ROOT / BUILD_NEXT).read_text())
    errs += probe_recipe_drift((ROOT / PROBE_JUST).read_text(),
                               [p["track"] for p in reg.probes])
    errs += probe_reference_errors(reg.probes, reg.rows, names)
    chapters = {p.name: p.read_text() for p in sorted((ROOT / CHAPTERS).glob("*.md"))}
    errs += uncovered_qemu_blocks(chapters, reg.probes)
    errs += fixture_board_errors((ROOT / FIXTURES).read_text(), names)
    errs += board_key_errors(reg.rows, names)
    errs += board_registry.typed_emulator_errors(reg.raw_doc.get("board", []), reg.index)

    # The absorbed gates run their own self-tests on entry and print their own
    # verdicts; their exit status is part of this one.
    sys.argv = [sys.argv[0]]
    rc_tiers = tiers.main()
    rc_runs = runs.main()

    print("check-board-registry: known gaps (not checked here):")
    for gap in KNOWN_GAPS:
        print(f"  - {gap}")
    if errs:
        print("\n[FAIL] board facts disagree with the registry:", file=sys.stderr)
        for e in errs:
            print(f"  - {e}", file=sys.stderr)
    if errs or rc_tiers or rc_runs:
        return 1
    print(f"check-board-registry: OK ({len(reg.rows)} rows, {len(reg.cells)} cells, "
          f"{len(reg.probes)} probe tracks; registry data {reg.digest})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
