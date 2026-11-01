#!/usr/bin/env python3
"""The board support registry's predicates — RFC-0114 D10/D11 (phase-522).

A support level that is merely *asserted* drifts. This repo has the receipts:
`book/src/reference/supported-boards.md` once claimed ARM FVP was "Tested"
under a legend defining Tested as "boots in CI", while the model ran in no CI
lane at all; and `matrix.rs` carried two FVP `Runtime` cells whose tests
skipped on every host. Both are the shape of issue 0232's false green.

So the evidence is declared once, in `packages/boards/board-support.toml`
(`runs`, `cadence`, `last_run`, `breadth`, ...), and CHECKED here against the
structures that actually decide it:

  the generated matrix::CELLS     -> does the platform have Runtime cells?
  examples/fixtures.toml          -> does a fixture build the board?
  .github/workflows/nightly.yml   -> does the nightly sweep run its module?
  justfile rust-rtos-link-check   -> is a per-merge Rust board link-checked?
  packages/boards/                -> does every board appear exactly once?

phase-375 W1 adds the one input that is not a structure but a person: a claim
needs someone who made it. RFC-0114 D10 keys that on the derived status word:
a Verified row needs 2 named maintainers, a Boots or Builds row 1, with the
rows that predate the rule grandfathered by
scripts/board-maintainer-baseline.json, a list that only shrinks.

The shape and evidence rules of the primitives live in
`scripts/lib/board_registry.py` (the same module derives the status word for
every view); this script adds the cross-file predicates. It is a library of
`check-board-registry.py`, the gate (phase-522 W3), and still runs alone.

The tier numbers and `execution_class` this script checked from phase-320 to
phase-522 W3 retired in phase-522 W4.

Run:
  python3 scripts/check-board-tiers.py                 # the predicates
  python3 scripts/check-board-tiers.py --print-status  # status word per row
      [--today YYYY-MM-DD]                             # ... on another day
  python3 scripts/check-board-tiers.py --print-cadence # platform/cadence/runs
  python3 scripts/check-board-tiers.py --write-baseline
"""

import datetime
import json
import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
import board_registry  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
REGISTRY = ROOT / "packages/boards/board-support.toml"
BASELINE = ROOT / "scripts/board-maintainer-baseline.json"
MATRIX = ROOT / "packages/testing/nros-tests/src/matrix.rs"
# The cell list matrix.rs includes, generated from the registry (phase-522 W2).
MATRIX_CELLS = ROOT / "packages/testing/nros-tests/src/matrix_cells.rs"
FIXTURES = ROOT / "examples/fixtures.toml"
PROBE_TRACKS = ROOT / "scripts/probe/probe-tracks.json"
NIGHTLY = ROOT / ".github/workflows/nightly.yml"
JUSTFILE = ROOT / "justfile"
BOARDS_DIR = ROOT / "packages/boards"

# RFC-0114 D10 — maintainers per derived level. More owners for a stronger
# claim, and the weakest claim still has exactly one person to ask (Rust's
# target-tier policy is the model). A Not supported or Infrastructure row
# claims nothing, so it owes nobody.
LEVELS = ("verified", "boots", "builds")          # strongest first
MAINTAINER_MIN = {"verified": 2, "boots": 1, "builds": 1}


def level_of(word):
    """The maintainer level a status word claims, or None."""
    for prefix, level in (("Verified", "verified"), ("Boots", "boots"),
                          ("Builds", "builds")):
        if word.startswith(prefix):
            return level
    return None


def level_supported_by(count):
    """The strongest level `count` maintainers can carry, or None."""
    for level in LEVELS:
        if count >= MAINTAINER_MIN[level]:
            return level
    return None


def row_key(entry):
    """Baseline key: the (crate, platform) PAIR the registry is keyed by.

    Not the crate alone — phase-337 W1.c made one crate able to hold several
    rows (`nros-board-zephyr` is native_sim and the FVP), so a crate-keyed
    exemption would grandfather a witness nobody looked at.
    """
    return board_registry.row_name(entry)


def runtime_platforms(text):
    """Platforms with at least one `Runtime` cell."""
    out = set()
    for m in re.finditer(r'cell\(\s*(\w+)\s*,(.*?)\)\s*,\s*\n', text, re.S):
        if re.search(r'\bRuntime\b', m.group(2)):
            out.add(m.group(1))
    return out


def build_platforms(text):
    """Platforms with at least one `Runtime` or `BuildOnly` cell."""
    out = set()
    for m in re.finditer(r'cell\(\s*(\w+)\s*,(.*?)\)\s*,\s*\n', text, re.S):
        if re.search(r'\b(Runtime|BuildOnly)\b', m.group(2)):
            out.add(m.group(1))
    return out


def platform_variants(text):
    """The `PlatformId` variants that actually exist."""
    m = re.search(r'pub const fn fixture_tokens\(self\).*?\n    \}', text, re.S)
    body = m.group(0) if m else ""
    return set(re.findall(r'PlatformId::(\w+)\s*=>', body))


def fixture_boards(text):
    """Board names a fixture row builds with (`NROS_BOARD = "..."`)."""
    return set(re.findall(r'NROS_BOARD\s*=\s*"([^"]+)"', text))


def probe_tracks():
    """The weekly probe tracks — generated from the registry since phase-522
    W2, so read from the generated list rather than the workflow."""
    tracks = json.loads(PROBE_TRACKS.read_text())["tracks"]
    return {t["track"] for t in tracks if t["weekly"]}


def nightly_tokens(text):
    """Modules the nightly per-platform job can run.

    Parse `runnable="..."`, NOT `all="..."`: the `all` set is computed from the
    lane at runtime, and the only literal `all="…"` left in the file is a
    COMMENT. `zephyr` and `linux` have their own homes (the 05:00 Zephyr cron
    and host-tests); the workflow says so and `lane-coverage` asserts it.
    """
    m = re.search(r'^\s*runnable="([^"]+)"', text, re.M)
    toks = set(m.group(1).split()) if m else set()
    toks.update({"zephyr", "linux"})
    return toks


def link_check_examples(text):
    body = re.search(r'^rust-rtos-link-check:.*?(?=\n^\w)', text, re.S | re.M)
    if not body:
        return set()
    return set(re.findall(r'cd (examples/[\w./-]+)', body.group(0)))


def row_built(row, built_plats, fx_boards):
    plat = row.get("matrix_platform")
    return bool(plat and plat in built_plats) or \
        board_registry.board_name(row) in fx_boards


def levels(doc, matrix_txt, fixtures_txt):
    """{row key: maintainer level or None} for every row, on no clock."""
    built_plats = build_platforms(matrix_txt)
    fx = fixture_boards(fixtures_txt)
    stale = doc.get("manual_stale_days", board_registry.DEFAULT_STALE_DAYS)
    return {row_key(r): level_of(board_registry.status_word(
        r, row_built(r, built_plats, fx), stale_days=stale)) for r in doc.get("board", [])}


TODAY = None  # `--today YYYY-MM-DD` (RFC-0114 evaluation step 5)


def print_status(doc, matrix_txt, fixtures_txt):
    """`crate@platform<TAB>status word<TAB>breadth`, one row per registry row."""
    built_plats = build_platforms(matrix_txt)
    fx = fixture_boards(fixtures_txt)
    stale = doc.get("manual_stale_days", board_registry.DEFAULT_STALE_DAYS)
    for row in doc.get("board", []):
        word = board_registry.status_word(
            row, row_built(row, built_plats, fx), stale_days=stale,
            today=TODAY or datetime.date.today())
        print(f"{row_key(row)}\t{word}\t{row.get('breadth', '-')}")
    return 0


def print_cadence(doc):
    """`platform<TAB>cadence<TAB>runs`, one line per row that runs.

    phase-395 W19's ONE derivation, re-keyed from tiers to cadence by
    phase-522 W4: `ci_lane`'s tier-1 pool is the set of `cadence = "merge"`
    platforms, and its test reads this rather than a second list.
    """
    for e in sorted(doc.get("board", []), key=lambda x: x.get("matrix_platform") or ""):
        plat = e.get("matrix_platform")
        if not plat or e.get("infra") or e.get("runs") in (None, "none"):
            continue
        print(f"{plat}\t{e.get('cadence')}\t{e['runs']}")
    return 0


def load_baseline(errors):
    """The grandfather list: rows that predate the maintainer rule.

    A committed file, so growing it is a reviewable diff rather than a silent
    default. A missing baseline is a FAILURE, not an empty exemption set:
    treating it as empty would fail every unowned row at once, the cliff the
    ratchet exists to avoid, with a message about maintainers rather than
    about the missing file.
    """
    if not BASELINE.exists():
        errors.append(f"missing maintainer baseline {BASELINE.relative_to(ROOT)} — "
                      "regenerate with --write-baseline")
        return {}
    try:
        return json.loads(BASELINE.read_text())
    except (OSError, ValueError) as exc:
        errors.append(f"cannot read maintainer baseline: {exc}")
        return {}


def check_maintainers(entry, level, baseline, errors, exempt, improved):
    """Enforce MAINTAINER_MIN against the grandfather baseline."""
    need = MAINTAINER_MIN.get(level)
    if need is None:
        return
    have = len(entry.get("maintainers") or [])
    key = row_key(entry)
    granted = baseline.get(key)
    if have >= need:
        if granted is not None:
            improved.append((key, level, have))
        return
    # An exemption is granted at a LEVEL and does not travel upward: a row
    # baselined at Builds that later reads Verified makes a stronger claim than
    # the one anybody grandfathered. Weaker is fine — a demotion must never be
    # blocked by the rule that asks for demotions.
    if granted in LEVELS and LEVELS.index(level) >= LEVELS.index(granted):
        exempt.append((key, level, have, granted))
        return
    supported = level_supported_by(have)
    if granted is not None:
        errors.append(
            f"{key}: baselined at {granted!r} but now reads {level!r} — a stronger "
            f"claim is a NEW one, so it needs its own owners ({have} named, "
            f"{level} needs {need})")
        return
    errors.append(
        f"{key}: reads {level!r} with {have} maintainer(s); {level} needs {need}. "
        + (f"This row supports {supported!r}." if supported else
           "With no maintainer named, no claim holds.")
        + " Recording an owner is the point; inventing one is worse than leaving "
          "the row at the level it can carry")


def write_baseline(doc, lv):
    out = {}
    for e in doc.get("board", []):
        level = lv[row_key(e)]
        need = MAINTAINER_MIN.get(level)
        if need is not None and len(e.get("maintainers") or []) < need:
            out[row_key(e)] = level
    BASELINE.write_text(json.dumps(dict(sorted(out.items())), indent=2) + "\n")
    print(f"wrote {BASELINE.relative_to(ROOT)}: {len(out)} row(s) grandfathered")
    return 0


def self_test(quiet=False):
    """Negative controls — a gate whose rule never fires proves nothing."""
    def run(entry, level, baseline):
        errors, exempt, improved = [], [], []
        check_maintainers(entry, level, baseline, errors, exempt, improved)
        return errors, exempt, improved

    new = {"crate": "new-board", "matrix_platform": "Linux", "maintainers": []}
    errors, _, _ = run(new, "verified", {})
    assert errors, "a NEW Verified row with no maintainer must fail"
    errors, _, _ = run({**new, "maintainers": ["a", "b"]}, "verified", {})
    assert not errors, "two maintainers must satisfy Verified"
    errors, _, _ = run({**new, "maintainers": ["a"]}, "verified", {})
    assert errors and "supports 'boots'" in errors[0], \
        "one maintainer must be refused at Verified AND told what it supports"
    errors, exempt, _ = run(new, "verified", {"new-board@Linux": "verified"})
    assert not errors and len(exempt) == 1, "a baselined row must be exempt"
    errors, _, _ = run(new, "verified", {"new-board@Linux": "builds"})
    assert errors and "stronger" in errors[0], "an exemption must not travel upward"
    errors, exempt, _ = run(new, "builds", {"new-board@Linux": "verified"})
    assert not errors and exempt, "a demotion must stay exempt"
    errors, _, _ = run(new, None, {})
    assert not errors, "Not supported and Infrastructure owe nobody"
    errors, _, improved = run({**new, "maintainers": ["a", "b"]}, "verified",
                              {"new-board@Linux": "verified"})
    assert not errors and improved, "an improved row is reported, not failed"
    assert level_of("Verified on host (merge)") == "verified"
    assert level_of("Boots, manual (2026-09-06)") == "boots"
    assert level_of("Builds (last booted 2026-09-06)") == "builds"
    assert level_of("Not supported") is None and level_of("Infrastructure") is None
    assert row_key({"crate": "n", "matrix_platform": "A"}) != \
        row_key({"crate": "n", "matrix_platform": "B"}), "rows are keyed by the pair"
    global BASELINE
    real, BASELINE = BASELINE, ROOT / "scripts/does-not-exist.json"
    try:
        errors = []
        assert load_baseline(errors) == {} and errors, \
            "a missing baseline must be an error, not a silent empty set"
    finally:
        BASELINE = real
    if not quiet:
        print("check-board-tiers self-test: OK")
    return 0


def tracked_board_dirs():
    """Directories under packages/boards/ that git TRACKS CONTENT under.

    Deleting a board crate does not delete the `target/` its last build left
    behind, so "a directory exists" reported 732 MB of `nros-board-esp32`
    residue as an unregistered board for anyone who had built it. Tracked
    content is the predicate, and it covers both board shapes (crates and
    `nros-board.toml` descriptor directories).
    """
    tracked = subprocess.run(["git", "ls-files", "-z", "--", str(BOARDS_DIR)],
                             cwd=ROOT, capture_output=True, text=True, check=True).stdout
    rel = BOARDS_DIR.relative_to(ROOT)
    out = set()
    for p in tracked.split("\0"):
        parts = Path(p).relative_to(rel).parts if p else ()
        if len(parts) > 1:      # skips the registry itself
            out.add(parts[0])
    return out


def main():
    global TODAY
    if "--self-test" in sys.argv:
        return self_test()
    # Always, not only behind the flag: a negative control nobody runs decays
    # into a comment, and this rule's whole job is to fire.
    self_test(quiet=True)
    board_registry.self_test()
    if "--today" in sys.argv:
        TODAY = datetime.date.fromisoformat(sys.argv[sys.argv.index("--today") + 1])
    # RFC-0118 D4 (phase-528 W2) — `emulator` comes from each row's descriptor.
    doc = board_registry.with_descriptor_facts(board_registry.load(REGISTRY),
                                               board_registry.descriptor_index(ROOT))
    reg = doc.get("board", [])
    matrix_txt = MATRIX.read_text() + MATRIX_CELLS.read_text()
    fixtures_txt = FIXTURES.read_text()
    if "--print-status" in sys.argv:
        return print_status(doc, matrix_txt, fixtures_txt)
    if "--print-cadence" in sys.argv:
        return print_cadence(doc)
    lv = levels(doc, matrix_txt, fixtures_txt)
    if "--write-baseline" in sys.argv:
        return write_baseline(doc, lv)

    rt = runtime_platforms(matrix_txt)
    nl = nightly_tokens(NIGHTLY.read_text())
    variants = platform_variants(matrix_txt)
    lc = link_check_examples(JUSTFILE.read_text())
    errors = list(board_registry.round_trip_errors(doc))

    stale = doc.get("manual_stale_days")
    if not isinstance(stale, int) or isinstance(stale, bool) or stale <= 0:
        errors.append(f"top-level `manual_stale_days` must be a positive integer "
                      f"(got {stale!r})")

    # --- completeness, both directions ----------------------------------------
    on_disk = tracked_board_dirs()
    keys = [row_key(e) for e in reg]
    for k in sorted({k for k in keys if keys.count(k) > 1}):
        errors.append(f"{k}: listed more than once. A crate MAY appear on several "
                      "rows, one per witness it serves, but not twice for one")
    listed = {e.get("crate") for e in reg if not e.get("external")}
    for c in sorted(on_disk - listed):
        errors.append(f"{c}: exists in packages/boards/ but is absent from the registry")
    for c in sorted(listed - on_disk):
        errors.append(f"{c}: in the registry but no such directory under packages/boards/ "
                      "(a package that lives elsewhere says so with `external`)")
    # A platform CI runs must be somebody's declared board: a conf bundle with no
    # directory of its own is invisible to the directory check above.
    declared = {e.get("matrix_platform") for e in reg if e.get("matrix_platform")}
    for plat in sorted(rt - declared):
        errors.append(f"platform {plat} has Runtime cells but no registry row names it")

    # --- per-row predicates ---------------------------------------------------
    tracks = probe_tracks()
    baseline = load_baseline(errors)
    exempt, improved = [], []
    for e in reg:
        errors += board_registry.shape_errors(e, variants)
        errors += board_registry.evidence_errors(e, runtime=rt, nightly=nl,
                                                 probe_tracks=tracks)
        plat = e.get("matrix_platform")
        if plat and plat not in variants:
            errors.append(f"{row_key(e)}: matrix_platform {plat!r} is not a PlatformId "
                          f"variant ({sorted(variants)})")
        if e.get("cadence") == "merge":
            # Per merge means a regression fails before it lands; without a
            # nightly module covering it (or `just ci` running it directly, the
            # Linux host) nothing can fail.
            tok = e.get("nightly_token")
            if plat != "Linux" and (not tok or tok not in nl):
                errors.append(f"{row_key(e)}: cadence = \"merge\" but no nightly module "
                              f"covers it (nightly_token={tok!r}, sweep={sorted(nl)})")
            ex = e.get("link_check_example")
            if ex and ex not in lc:
                errors.append(f"{row_key(e)}: link_check_example {ex!r} is not built by "
                              "rust-rtos-link-check")
        check_maintainers(e, lv[row_key(e)], baseline, errors, exempt, improved)

    for key in sorted(set(baseline) - set(keys)):
        errors.append(f"maintainer baseline names {key}, which is not a registry row — "
                      "drop it (--write-baseline)")
    for key, value in sorted(baseline.items()):
        if value not in LEVELS:
            errors.append(f"maintainer baseline grants {key} the level {value!r}; levels "
                          f"are {', '.join(LEVELS)}")

    unowned = sum(1 for e in reg if not e.get("maintainers"))
    print(f"board-support registry: {len(reg)} rows, {len(on_disk)} directories")
    print(f"  platforms with Runtime cells: {', '.join(sorted(rt)) or '(none)'}")
    print(f"  maintainer rule: Verified needs {MAINTAINER_MIN['verified']}, Boots or "
          f"Builds {MAINTAINER_MIN['builds']} ({unowned} of {len(reg)} rows have none)")
    if exempt:
        print(f"  {len(exempt)} row(s) grandfathered by {BASELINE.relative_to(ROOT)}")
    for key, level, have in sorted(improved):
        print(f"  {key} now has {have} maintainer(s) and meets {level} — drop it from "
              "the baseline (--write-baseline); the list only shrinks")
    if errors:
        print("\n[FAIL] the board registry disagrees with the evidence:", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        return 1
    print("Board support levels match the evidence.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
