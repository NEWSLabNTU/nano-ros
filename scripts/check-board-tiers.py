#!/usr/bin/env python3
"""phase-320 W2/W3 — validate packages/boards/board-support.toml against evidence.

A support tier that is merely *asserted* drifts. This repo has the receipts:
`book/src/reference/supported-boards.md` claimed ARM FVP was "Tested" under a
legend defining Tested as "boots in CI", while the model is license-walled and
runs in no CI lane at all; and `matrix.rs` carried two FVP `Runtime` cells whose
tests skip on every host. Both are the shape of issue 0232's false green.

So the tier is declared once, in the registry, and CHECKED here against the
structures that actually decide it:

  matrix.rs                       -> does the platform have Runtime cells?
  examples/fixtures.toml          -> does it have fixture rows?
  .github/workflows/nightly.yml   -> is it in the nightly sweep?
  justfile rust-rtos-link-check   -> is it compiled by `just ci`?
  packages/boards/                -> does every board appear exactly once?

Dependency-free on purpose: CI hosts are not guaranteed a TOML library (this
repo's Python is 3.10, so no `tomllib`), and `scripts/build/fixtures-manifest.py`
already establishes regex parsing as the house pattern for exactly this reason.
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REGISTRY = ROOT / "packages/boards/board-support.toml"
MATRIX = ROOT / "packages/testing/nros-tests/src/matrix.rs"
FIXTURES = ROOT / "examples/fixtures.toml"
NIGHTLY = ROOT / ".github/workflows/nightly.yml"
JUSTFILE = ROOT / "justfile"
BOARDS_DIR = ROOT / "packages/boards"

VALID_TIERS = {"1", "2", "3", "scaffold", "infra"}


def parse_registry(text):
    """Minimal `[[board]]` reader — scalar strings, ints, and string arrays."""
    entries = []
    cur = None
    for raw in text.splitlines():
        line = raw.strip()
        if line.startswith("#") or not line:
            continue
        if line == "[[board]]":
            cur = {}
            entries.append(cur)
            continue
        if cur is None:
            continue
        m = re.match(r'^([a-z_]+)\s*=\s*(.+)$', line)
        if not m:
            continue
        key, val = m.group(1), m.group(2).strip()
        if val.startswith("["):
            items = re.findall(r'"([^"]*)"', val)
            cur[key] = items
        elif val.startswith('"'):
            cur[key] = val.strip('"')
        else:
            cur[key] = val.strip('"')
    return entries


def runtime_platforms(text):
    """Platforms with at least one `Runtime` cell."""
    out = set()
    # cell(...) spans lines; join then scan each invocation.
    for m in re.finditer(r'cell\(\s*(\w+)\s*,(.*?)\)\s*,\s*\n', text, re.S):
        plat, rest = m.group(1), m.group(2)
        if re.search(r'\bRuntime\b', rest):
            out.add(plat)
    return out


def platform_variants(text):
    """The `PlatformId` variants that actually exist.

    W3.a — a tier 1/2/3 board must name a REAL platform. Scaffold boards named
    none by definition, which was the honest encoding of "absent from
    PlatformId". phase-337 W7 then deleted every board in that state, so the
    scaffold tier is currently unpopulated — kept because the state is what
    stops the next unfinished board from reading as support.
    """
    m = re.search(r'pub const fn fixture_tokens\(self\).*?\n    \}', text, re.S)
    body = m.group(0) if m else ""
    return set(re.findall(r'PlatformId::(\w+)\s*=>', body))


def fixture_platforms(text):
    return set(re.findall(r'^platform\s*=\s*"([^"]+)"', text, re.M))


def nightly_tokens(text):
    """Modules the nightly per-platform job can run.

    Parse `runnable="..."`, NOT `all="..."`. The `all` set is computed from the
    lane at runtime (phase-318 W4.e made the sweep derive from `matrix::CELLS`
    so adding a platform there extends the sweep with no second edit), and the
    only literal `all="qemu freertos …"` left in the file is a COMMENT
    describing the old hand-written shape — which this parser matched on its
    first outing. `runnable` is the honest static bound.

    `zephyr` and `native` have their own homes in the tier (the 05:00 Zephyr
    cron and host-tests); the workflow says so and `lane-coverage` asserts it,
    so they count as covered.
    """
    m = re.search(r'^\s*runnable="([^"]+)"', text, re.M)
    toks = set(m.group(1).split()) if m else set()
    toks.update({"zephyr", "native"})
    return toks


def link_check_examples(text):
    body = re.search(r'^rust-rtos-link-check:.*?(?=\n^\w)', text, re.S | re.M)
    if not body:
        return set()
    return set(re.findall(r'cd (examples/[\w./-]+)', body.group(0)))


def main():
    reg = parse_registry(REGISTRY.read_text())
    matrix_txt = MATRIX.read_text()
    rt = runtime_platforms(matrix_txt)
    fx = fixture_platforms(FIXTURES.read_text())
    nl = nightly_tokens(NIGHTLY.read_text())
    variants = platform_variants(matrix_txt)
    lc = link_check_examples(JUSTFILE.read_text())

    errors = []

    # --- completeness, both directions (W3.a) ---------------------------------
    # A board is a directory git TRACKS CONTENT under — not merely a directory.
    #
    # Deleting a board crate does not delete the `target/` its last build left
    # behind: nothing under it is tracked, so git leaves the directory in place.
    # `nros-board-esp32` was dropped in 9211101cb and left 732 MB of residue, and
    # this gate reported it as "exists in packages/boards/ but absent from the
    # registry" — permanently red on `check-fast` for anyone who had ever built
    # that board, with a message pointing at the registry instead of at the
    # leftovers.
    #
    # Tracked-content is the right predicate rather than "has a Cargo.toml":
    # boards come in two shapes, crate boards (`Cargo.toml`) and declarative ones
    # (`nros-board.toml`, e.g. posix/zephyr), and a manifest-name check silently
    # dropped the declarative half.
    tracked = subprocess.run(
        ["git", "ls-files", "-z", "--", str(BOARDS_DIR)],
        cwd=ROOT, capture_output=True, text=True, check=True,
    ).stdout
    _rel = BOARDS_DIR.relative_to(ROOT)
    on_disk = set()
    for p in tracked.split("\0"):
        if not p:
            continue
        parts = Path(p).relative_to(_rel).parts
        # `len > 1` skips files that sit directly in packages/boards/ — notably
        # board-support.toml itself, which is the registry, not a board.
        if len(parts) > 1:
            on_disk.add(parts[0])
    # phase-337 W1.c — rows are keyed by (crate, matrix_platform), not by crate.
    #
    # One crate may serve several WITNESSES at different tiers: after W3 a single
    # `nuttx-qemu` crate carries arm (tier 1) and riscv (tier 2), and after W9
    # one `nros-board-zephyr` carries three. Tier is a per-row promise — "`just
    # ci` exercises it" is true of the arm witness and false of the riscv one —
    # so those cannot share a row, and a list-valued `matrix_platform` would
    # force them to. What must stay unique is the PAIR: two rows claiming the
    # same crate on the same platform are a genuine duplicate.
    #
    # Every crate on disk must still appear at least once; that completeness
    # check is the reason this registry exists.
    listed = [e.get("crate", "<no crate key>") for e in reg]
    keys = [(e.get("crate", "<no crate key>"), e.get("matrix_platform")) for e in reg]
    dupes = {k for k in keys if keys.count(k) > 1}
    for c, plat in sorted(dupes, key=lambda k: (k[0], k[1] or "")):
        errors.append(
            f"{c}: listed more than once for matrix_platform {plat!r}. "
            "A crate MAY appear on several rows, one per witness it serves "
            "(phase-337 W1.c), but not twice for the same one")
    missing = on_disk - set(listed)
    for c in sorted(missing):
        errors.append(
            f"{c}: exists in packages/boards/ but is absent from the registry. "
            "A board enumerated nowhere is the failure this gate exists to catch "
            "(four boards were in this state before phase-320)")
    extra = set(listed) - on_disk
    for c in sorted(extra):
        errors.append(f"{c}: in the registry but no such directory under packages/boards/")

    # phase-337 W2 — the OTHER direction of completeness, and the one the check
    # above structurally cannot do.
    #
    # Everything so far is keyed on board DIRECTORIES, which silently assumes a
    # board is a crate. RFC-0064's whole direction is that it is not: the Zephyr
    # Cortex-M witness is `cmake/zephyr/mps2-an385.conf` and owns no directory
    # under packages/boards/, so it could carry Runtime cells — a tier-1-or-2
    # promise — while being enumerated nowhere, which is the exact failure this
    # gate was written to catch. As boards keep becoming conf bundles (W9), the
    # directory check covers less and less of the rule it is enforcing.
    #
    # So assert it from the matrix side too: a platform that CI actually runs
    # must be somebody's declared promise.
    declared_platforms = {
        e.get("matrix_platform") for e in reg if e.get("matrix_platform")
    }
    for plat in sorted(rt - declared_platforms):
        errors.append(
            f"platform {plat} has Runtime cells but no registry row names it as a "
            "matrix_platform. A board that CI runs must carry a tier promise — "
            "and a board with no directory (a conf bundle) is invisible to the "
            "directory check above, which is why this one exists")

    # --- per-entry predicates -------------------------------------------------
    unowned = 0
    borrowed: list[tuple[str, str, str]] = []
    for e in reg:
        crate = e.get("crate", "<no crate key>")
        tier = str(e.get("tier", ""))
        if tier not in VALID_TIERS:
            errors.append(f"{crate}: tier {tier!r} is not one of {sorted(VALID_TIERS)}")
            continue
        if not e.get("maintainers"):
            unowned += 1
        if tier == "infra":
            continue

        plat = e.get("matrix_platform")
        if plat and plat not in variants:
            errors.append(
                f"{crate}: matrix_platform {plat!r} is not a PlatformId variant "
                f"({sorted(variants)})")
        has_runtime = plat in rt if plat else False

        if tier in ("1", "2"):
            if not plat:
                errors.append(f"{crate}: tier {tier} must name a matrix_platform")
            elif not has_runtime:
                errors.append(
                    f"{crate}: declared tier {tier} but platform {plat} has NO Runtime cell "
                    "in matrix.rs — tiers 1 and 2 promise asserted runtime coverage")
        if tier == "3" and has_runtime:
            # A platform's Runtime cells prove SOME board. When a DIFFERENT row
            # already claims that platform at tier 1/2, those cells are its
            # witness, and a build-only board sharing the token is not thereby
            # proven — so the contradiction this rule names does not exist.
            #
            # phase-372's `nros-board-s32z270-freertos` is the first of these:
            # a Cortex-R52 bundle that borrows `FreertosMps2` for the freertos
            # family lane "until a hardware witness exists", while the MPS2 board
            # owns that platform's cells. Read as a function platform -> tier,
            # the row looks like the FVP defect; read against the registry, it is
            # honest, and its `notes` already say so.
            #
            # Inferred from rows that already exist rather than a new key: a
            # `borrowed = true` flag would be a claim a board makes about itself,
            # and this is a fact about who owns the cells.
            owner = next(
                (o.get("crate") for o in reg
                 if o.get("crate") != crate
                 and o.get("matrix_platform") == plat
                 and str(o.get("tier", "")) in ("1", "2")),
                None,
            )
            if owner:
                borrowed.append((crate, plat, owner))
            else:
                errors.append(
                    f"{crate}: declared tier 3 (build-only) but platform {plat} HAS Runtime "
                    "cells and NO other row claims that platform at tier 1/2, so those cells "
                    "are this board's own witness. Either it is really tier 2, or those cells "
                    "overstate what the lane can do — the exact defect phase-320 W1.a fixed "
                    "for FVP")
        if tier == "scaffold":
            if has_runtime:
                errors.append(
                    f"{crate}: declared scaffold but platform {plat} has Runtime cells")
            if plat and any(t in fx for t in (plat,)):
                errors.append(f"{crate}: declared scaffold but has fixture rows")
        if tier == "1":
            tok = e.get("nightly_token")
            # `Linux` (was `Native` until phase-337 W8.b) is exempt because
            # `just ci` runs it DIRECTLY on every push — stronger than a nightly
            # build, which is what the token stands in for everywhere else.
            if plat != "Linux" and (not tok or tok not in nl):
                errors.append(
                    f"{crate}: declared tier 1 but no nightly lane covers it "
                    f"(nightly_token={tok!r}, workflow sweep={sorted(nl)}). Tier 1 promises a "
                    "regression fails before merge; without a lane nothing can fail")
            ex = e.get("link_check_example")
            if ex and ex not in lc:
                errors.append(
                    f"{crate}: link_check_example {ex!r} is not built by rust-rtos-link-check")

    for crate, plat, owner in sorted(borrowed):
        print(f"  tier-3 {crate} BORROWS platform {plat}; its Runtime cells are "
              f"{owner}'s witness, not this board's")
    print(f"board-support registry: {len(reg)} entries, {len(on_disk)} directories")
    print(f"  platforms with Runtime cells: {', '.join(sorted(rt)) or '(none)'}")
    print(f"  nightly sweep: {', '.join(sorted(nl)) or '(none)'}")
    if unowned:
        print(f"  note: {unowned} entries have no maintainer. Not enforced yet "
              "(phase-320 W3.b) — recording an owner is the point, inventing one is worse "
              "than leaving it blank.")

    if errors:
        print("\n[FAIL] board support tiers disagree with the evidence:", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        return 1
    print("Board support tiers match the evidence.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
