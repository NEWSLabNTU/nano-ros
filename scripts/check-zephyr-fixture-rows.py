#!/usr/bin/env python3
"""phase-350 W1 — the zephyr west leaves and their manifest rows must agree.

`examples/fixtures.toml` now carries a `builder = "west"` row for every zephyr
west leaf (issue 0535), but `scripts/build/zephyr-fixture-leaves.sh` still
DERIVES its own matrix from `fixture-matrix.sh`. Until the emitter reads the
rows (the next step), those are two spellings of one matrix — exactly the shape
this phase exists to remove — so this gate makes them unable to drift.

It compares the leaf sets on `(board, lang, role, rmw)`, in BOTH directions,
after normalising for the one axis that is legitimately host-dependent.

## The host-dependence, and why it is not just ignored

`zephyr-fixture-leaves.sh` gates cyclonedds on `idlc` being available:

    fixture_rmws=(zenoh xrce)
    if idlc present; then fixture_rmws+=(cyclonedds); fi

So a host without idlc emits 36 role leaves and one with it emits 54, while the
manifest always carries 54. Asserting set equality outright would fail on the
smaller host — a gate that is red for a reason the developer cannot act on gets
disabled, which is worse than no gate.

Comparing only `emitted ⊆ manifest` would be host-safe but blind in the
direction that matters most: a DELETED leaf would leave its row behind forever,
which is how `fixture-inventory.py` rotted (issue 0538).

So: restrict BOTH sides to the RMWs this host actually emitted, then require set
equality. A missing leaf and an orphan row are both caught, on every host, and
the cyclonedds rows are simply out of scope where cyclonedds is.
"""
import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]

FIELDS = ("kind id target board lang lang_tag role rmw src src_dir build_name build_dir log "
          "xrce_agent_port zenoh_locator cyclone_domain conf_files extra_cmake_defs sig "
          "sig_file best_effort eff_pristine").split()

ROLES = ("talker", "listener", "service-server", "service-client",
         "action-server", "action-client")


def emitted_leaves():
    """Every leaf the emitter would build. Read-only: it runs no build tool."""
    proc = subprocess.run(
        [
            "bash", str(REPO / "scripts/build/zephyr-fixture-leaves.sh"),
            "--emit", "records",
            "--include-workspace-entry",
        ],
        capture_output=True, text=True, cwd=REPO,
    )
    if proc.returncode != 0:
        print(f"zephyr-fixture-rows: emitter failed (rc={proc.returncode}):\n{proc.stderr}",
              file=sys.stderr)
        sys.exit(2)
    out = set()
    for line in proc.stdout.splitlines():
        if not line.strip():
            continue
        r = dict(zip(FIELDS, line.split("\t")))
        # The 12 `entry` leaves are WORKSPACE cells and get
        # `[[workspace_fixture]]` rows, whose shape (workspace root + bringup +
        # entry) this comparison does not model. Out of scope here, tracked by
        # phase-350 W1.
        if r["role"] == "entry":
            continue
        out.add((r["board"], r["lang"], r["role"], r["rmw"]))
    return out


def manifest_rows():
    proc = subprocess.run(
        [sys.executable, str(REPO / "scripts/build/fixtures-manifest.py"), "coords"],
        capture_output=True, text=True, cwd=REPO,
    )
    proc.check_returncode()
    # `coords` does not carry board/role, so read the manifest directly for the
    # west rows. One parser, the same file.
    try:
        import tomllib
    except ModuleNotFoundError:
        import tomli as tomllib
    with open(REPO / "examples/fixtures.toml", "rb") as f:
        manifest = tomllib.load(f)
    out = set()
    for row in manifest.get("fixture", []):
        if row.get("builder") != "west":
            continue
        d = row["dir"].rstrip("/")
        role = row.get("west_role") or Path(d).name
        rmw = row.get("rmw") or ("default" if role == "logging-smoke" else "zenoh")
        out.add((row["board"], row["lang"], role, rmw))
    return out


def main():
    emitted = emitted_leaves()
    rows = manifest_rows()

    # Normalise the one host-dependent axis (see module docstring).
    emitted_rmws = {rmw for _, _, _, rmw in emitted}
    rows_in_scope = {k for k in rows if k[3] in emitted_rmws}

    missing_rows = sorted(emitted - rows_in_scope)     # leaf built, no row
    orphan_rows = sorted(rows_in_scope - emitted)      # row with no leaf

    if not missing_rows and not orphan_rows:
        print(f"zephyr-fixture-rows: OK ({len(emitted)} leaf/leaves emitted, "
              f"{len(rows)} west row(s), rmws={sorted(emitted_rmws)})")
        return 0

    if missing_rows:
        print(f"zephyr-fixture-rows: {len(missing_rows)} leaf/leaves the emitter builds "
              f"with NO `builder = \"west\"` row in examples/fixtures.toml:", file=sys.stderr)
        for board, lang, role, rmw in missing_rows:
            print(f"  {board} {lang}/{role}/{rmw}", file=sys.stderr)
    if orphan_rows:
        print(f"\nzephyr-fixture-rows: {len(orphan_rows)} west row(s) no leaf produces "
              f"(delete the row, or restore the leaf):", file=sys.stderr)
        for board, lang, role, rmw in orphan_rows:
            print(f"  {board} {lang}/{role}/{rmw}", file=sys.stderr)
    print("\nThe manifest and scripts/build/zephyr-fixture-leaves.sh describe one matrix. "
          "See docs/issues/0535-west-fixtures-have-no-coordinate.md.", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
