#!/usr/bin/env python3
"""issue 1581 — every embedded cargo IMAGE row bakes a locator its board READS.

A Rust image built from a `[[workspace_fixture]]` row with `image = "<id>"` gets
its connect locator from one of two carriers:

  * the bringup's `system.toml` — `[image.<id>] locator` over `[image_defaults]`
    over `[system]` (the overlay `nros_orchestration_ir::leaf_system` resolves
    and `nros::main!` bakes into the `DeployOverlay` / `NROS_BOOT_CONFIG`).
    A board applies it only if its crates SAY so — `BaseConfig::apply_overlay`
    or a read of `deploy.locator` — and the gate VERIFIES that too (issue
    1836: the NuttX board never did, and rule 1 counted the carrier anyway).
  * the row's `env = { NROS_LOCATOR = … }`, compiled in through
    `option_env!("NROS_LOCATOR")` — but only by a board crate that SAYS
    `option_env!("NROS_LOCATOR")`. On any other board the value reaches nothing.

When neither reaches the board, the image dials the board's compiled-in default
(`tcp/192.0.3.1:7447` on threadx-linux; the board's own gateway, `10.0.2.2`, on
the FreeRTOS QEMU boards since phase-484 W4b-2) while the test's router
listens on `alloc::port_of(...)`, and every cell fails `Executor::open:
ConnectionFailed` before any code under test runs. That is issue 1581: phase-383
W9 / W10.a deleted the hand-written entry packages whose
`[package.metadata.nros.deploy.<board>] locator` carried the value, and four
images silently fell back to the default — one of them while its row kept an
`NROS_LOCATOR` env that looked like a bake and was dead on that board.

THE RULES, per in-scope row (a cargo `workspace_fixture` row with an `image`,
not linux — the host reads its locator from the runtime env — and not a west or
cmake build, which carry `west_zenoh_locator` / `NROS_ENTRY_LOCATOR` and are
policed by `check-entry-locator-ssot`):

  1. at least one carrier the board reads states a locator;
  2. a row `NROS_LOCATOR` on a board that does not read it is REFUSED — dead
     config that reads as a bake is how 1581 hid;
  3. when both carriers state one they must agree;
  4. and they must not state the SAME one (phase-484 W4e): the board applies
     the image's locator OVER the default the env feeds (its deploy-overlay
     merge), so an equal row value is a second home that can only drift.
     State it once, in `system.toml`.

Scope (phase-484 W4e): a single-package cargo LEAF row — no `image`, a
`system.toml` beside its `Cargo.toml` declaring its one image — is an image row
too; `nros::main!` reads its `system.toml` exactly as a bringup's. Rows that
dial no router on purpose are listed in `NO_ROUTER` with why.

"Which boards read the env" is a CLAIM in `READS_ENV`, and the gate VERIFIES it
against the board crates' sources in both directions, so the table cannot drift
toward OK: a board listed as reading it whose crates never say
`option_env!("NROS_LOCATOR")` fails, and so does one listed as not reading it
whose crates do. A platform the table does not classify fails, so a new board
family is ruled on rather than waved through.

"Every board applies the image's locator" is verified the same way, one
direction: each family's crates must apply the deploy overlay
(`OVERLAY_APPLY`), unless `OVERLAY_ELSEWHERE` says how the image's locator
reaches that board instead. Rule 1 trusts that carrier, so it must not be a
claim nobody checks — it was, for NuttX, from phase-484 W4e until issue 1836:
seven fixture rows lost their env copy and every NuttX Rust image dialled the
empty locator.

This gate does not check the PORT against `alloc::port_of`: a manifest row
carries no workload, so the cell is not derivable here. The fixture resolvers
that know their cell assert that half
(`nros_tests::fixtures::baked_locator::assert_row_dials_port`).

Usage::

    check-image-locator-bake.py              # the gate
    check-image-locator-bake.py --audit      # print every in-scope row
    check-image-locator-bake.py --self-test  # negative controls only
"""

import os
import re
import subprocess
import sys

try:
    import tomllib
except ImportError:  # Python < 3.11
    import tomli as tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MANIFEST = "examples/fixtures.toml"
ENV_READ = re.compile(r'option_env!\(\s*"NROS_LOCATOR"\s*\)')
# issue 1836 — what "this board applies the image's `system.toml` locator" looks
# like in source: the shared merge, or a direct read of the overlay's field.
OVERLAY_APPLY = re.compile(r'\.apply_overlay\(|\b(?:deploy|overlay)\.locator\b')

# platform token -> (reads NROS_LOCATOR at build time?, board crates that decide it)
READS_ENV = {
    "freertos": (
        False,
        ["packages/boards/nros-board-freertos", "packages/boards/nros-board-mps2-an385-freertos"],
    ),
    "threadx-linux": (
        False,
        ["packages/boards/nros-board-threadx-linux", "packages/boards/nros-board-threadx"],
    ),
    # issue 1836 — the board overlay owns the connect default (the family
    # driver `nros-board-nuttx` receives the merged pair and reads no env).
    "nuttx": (True, ["packages/boards/nros-board-nuttx-qemu"]),
    "nuttx-riscv": (True, ["packages/boards/nros-board-nuttx-qemu"]),
    "esp32": (True, ["packages/boards/nros-board-esp32-qemu"]),
    # phase-484 W4e — reached once leaf rows came into scope.
    "baremetal": (True, ["packages/boards/nros-board-mps2-an385"]),
    "threadx-riscv64": (
        False,
        ["packages/boards/nros-board-threadx-qemu-riscv64", "packages/boards/nros-board-threadx"],
    ),
    # Zephyr's Rust lane bakes the Kconfig locator through `nros`'s own
    # `option_env!` (`Context::baked`, fed by `nros_zephyr_build`), and a test
    # may override it at run time (`-testargs --nros-locator`).
    "zephyr": (True, ["packages/api/nros"]),
}


# Families whose image locator reaches the board by a road other than the
# `DeployOverlay`, with how. Every other family's crates must apply the overlay.
OVERLAY_ELSEWHERE = {
    "zephyr": (
        "the image's locator is a Kconfig value the entry's build renders from "
        "system.toml, read through `nros::Context::baked`, not a DeployOverlay"
    ),
}


# Rows that dial NO router on purpose, by `dir`, with why. Rule 1 does not
# apply to them; nothing else is relaxed.
NO_ROUTER = {
    "packages/testing/nros-tests/bins/rtic-run-plan-e2e": (
        "baremetal_run_plan_runtime.rs runs it with no zenohd and asserts the board "
        "reaches Executor::open and reports ConnectionFailed"
    ),
    "packages/testing/nros-tests/bins/qemu-baremetal-main-e2e": (
        "same test, the non-RTIC board — no router by design"
    ),
}


def in_scope(row):
    if ("image" not in row and not row.get("_leaf")) or row.get("platform") == "linux":
        return False
    if row.get("builder") in ("west", "cmake"):
        return False
    return not any(k in row for k in ("cmake_defs", "build_subdir", "west_build_name"))


def leaf_system_toml(row, root):
    """phase-484 W4e — a single-package cargo LEAF row (no `image`, no
    `bringup`): its own `system.toml` beside its `Cargo.toml`, or None."""
    if "image" in row or "bringup" in row or "dir" not in row:
        return None
    d = os.path.join(root, row["dir"])
    path = os.path.join(d, "system.toml")
    if os.path.isfile(path) and os.path.isfile(os.path.join(d, "Cargo.toml")):
        return path
    return None


def image_locator(row, root):
    """The locator the bringup states for this row's image, or None."""
    if row.get("_leaf"):
        # A leaf's `system.toml` declares its ONE image, which `nros::main!`
        # bakes exactly as it does a bringup's (`leaf_system`'s overlay).
        path = leaf_system_toml(row, root)
        with open(path, "rb") as f:
            doc = tomllib.load(f)
        images = sorted(doc.get("image", {}))
        if len(images) != 1:
            return None, path, f"a leaf declares {len(images)} images; name one with `image`"
        image_id = images[0]
        row["image"] = image_id
    else:
        path = os.path.join(root, row["dir"], row.get("bringup", "src/demo_bringup"), "system.toml")
        with open(path, "rb") as f:
            doc = tomllib.load(f)
        image_id = row["image"].split(":")[-1]
    image = doc.get("image", {}).get(image_id)
    if image is None:
        return None, path, f"`[image.{image_id}]` is not declared"
    for table in (image, doc.get("image_defaults", {}), doc.get("system", {})):
        if isinstance(table.get("locator"), str):
            return table["locator"], path, None
    return None, path, None


def crates_say(crates, root, pattern):
    # `git ls-files` rather than a walk: every caller passes ROOT (see
    # `verify_table`'s two call sites), so every path here is a tracked in-tree
    # crate source. A walk would also read generated or stray `.rs` under
    # `src/`, which is a different question from "what does this crate SAY",
    # and `check-no-tracked-file-find` refuses it for that reason.
    for crate in crates:
        listed = subprocess.run(
            ["git", "-C", root, "ls-files", "-z", "--", f"{crate}/src/*.rs"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
        for rel in listed.split("\0"):
            if not rel:
                continue
            with open(os.path.join(root, rel), encoding="utf-8") as f:
                if pattern.search(f.read()):
                    return True
    return False


def verify_table(table, root):
    problems = []
    for platform, (claim, crates) in sorted(table.items()):
        missing = [c for c in crates if not os.path.isdir(os.path.join(root, c, "src"))]
        if missing:
            problems.append(f"READS_ENV[{platform!r}] names crates with no src/: {missing}")
            continue
        actual = crates_say(crates, root, ENV_READ)
        if actual != claim:
            problems.append(
                f"READS_ENV[{platform!r}] claims reads-NROS_LOCATOR={claim}, but {crates} "
                f"{'DO' if actual else 'do NOT'} say option_env!(\"NROS_LOCATOR\") — fix the "
                f"table, it is what rule 2 trusts"
            )
    return problems


def verify_overlay(table, root):
    """issue 1836 — each family's crates apply the image's deploy overlay."""
    problems = []
    for platform, (_, crates) in sorted(table.items()):
        if platform in OVERLAY_ELSEWHERE:
            continue
        if not crates_say(crates, root, OVERLAY_APPLY):
            problems.append(
                f"`{platform}`: {crates} never apply the deploy overlay (no `apply_overlay(` "
                f"and no `deploy.locator`), so an image's `system.toml` locator reaches no "
                f"session and rule 1 is trusting a carrier that is dead (issue 1836) — merge "
                f"it with `BaseConfig::apply_overlay`, or name the road in OVERLAY_ELSEWHERE"
            )
    return problems


def check_rows(rows, table, locate):
    """Apply rules 1-3. `locate(row) -> (locator|None, path, error|None)`."""
    problems, audit = [], []
    for row in rows:
        if not in_scope(row):
            continue
        rid, platform = row.get("id", "?"), row.get("platform")
        if platform not in table:
            problems.append(
                f"{rid}: platform `{platform}` is not classified in READS_ENV — rule on whether "
                f"its board reads `NROS_LOCATOR` before adding a cargo image row for it"
            )
            continue
        reads_env = table[platform][0]
        stated, path, err = locate(row)
        if err:
            problems.append(f"{rid}: {os.path.relpath(path, ROOT)}: {err}")
            continue
        env = (row.get("env") or {}).get("NROS_LOCATOR")
        rid = rid if rid != "?" else row.get("dir", "?")
        audit.append(f"{rid:48} {platform:14} image={stated} env={env}")
        if env is not None and env == stated:
            # Rule 4 (phase-484 W4e) — one fact, one home. `nros::main!` bakes
            # the image's locator over the board default the env feeds
            # (`BootConfig::over_board_defaults`), so an equal row value is a
            # second home that can only drift.
            problems.append(
                f"{rid}: `env.NROS_LOCATOR = {env!r}` restates `[image.{row['image']}] locator` "
                f"in {os.path.relpath(path, ROOT)} — drop it from the row"
            )
            continue
        if env is not None and not reads_env:
            problems.append(
                f"{rid}: `env.NROS_LOCATOR = {env!r}` reaches nothing — the `{platform}` board "
                f"reads no `NROS_LOCATOR`. State it as `[image.{row['image']}] locator` in "
                f"{os.path.relpath(path, ROOT)} and drop it from the row"
            )
            continue
        if stated is None and env is None:
            if row.get("dir") in NO_ROUTER:
                continue
            problems.append(
                f"{rid}: the image states no locator and the row carries none, so it bakes the "
                f"`{platform}` board DEFAULT and dials a router no test starts (issue 1581). "
                f"Add `locator = \"tcp/<host>:<alloc::port_of(...)>\"` to `[image.{row['image']}]` "
                f"in {os.path.relpath(path, ROOT)}"
            )
            continue
        if stated is not None and env is not None and stated != env:
            problems.append(
                f"{rid}: two carriers disagree — `[image.{row['image']}] locator = {stated!r}` "
                f"vs row `NROS_LOCATOR = {env!r}`. Keep one"
            )
    return problems, audit


def self_test():
    table = {"brd": (False, []), "envbrd": (True, [])}

    def run(row, stated):
        return check_rows([row], table, lambda r: (stated, "x/system.toml", None))[0]

    base = {"id": "r", "dir": "d", "image": "i", "platform": "brd", "lang": "rust"}
    cases = [
        ("no carrier (the 1581 shape)", base, None, 1),
        ("stated in the image", base, "tcp/a:1", 0),
        ("dead env on a board that ignores it", dict(base, env={"NROS_LOCATOR": "tcp/a:1"}), None, 1),
        ("dead env even beside a stated image", dict(base, env={"NROS_LOCATOR": "tcp/a:1"}), "tcp/a:1", 1),
        ("env on a board that reads it", dict(base, platform="envbrd", env={"NROS_LOCATOR": "tcp/a:1"}), None, 0),
        ("carriers disagree", dict(base, platform="envbrd", env={"NROS_LOCATOR": "tcp/a:1"}), "tcp/a:2", 1),
        ("carriers restate one fact", dict(base, platform="envbrd", env={"NROS_LOCATOR": "tcp/a:1"}), "tcp/a:1", 1),
        ("no router, on purpose", dict(base, dir=next(iter(NO_ROUTER))), None, 0),
        ("unclassified platform", dict(base, platform="newos"), "tcp/a:1", 1),
        ("cmake row out of scope", dict(base, cmake_defs={}), None, 0),
        ("linux out of scope", dict(base, platform="linux"), None, 0),
    ]
    bad = []
    for name, row, stated, want in cases:
        got = len(run(row, stated))
        if got != want:
            bad.append(f"self-test '{name}': {got} problem(s), expected {want}")
    # The table verifier must catch a claim the sources contradict.
    if not verify_table({"freertos": (True, READS_ENV["freertos"][1])}, ROOT):
        bad.append("self-test: a false READS_ENV claim (freertos reads env) was not caught")
    # ...and a family whose crates never apply the overlay (the 1836 shape: the
    # NuttX family driver alone, which receives the merged pair).
    if not verify_overlay({"nuttx": (False, ["packages/boards/nros-board-nuttx"])}, ROOT):
        bad.append("self-test: a board that never applies the deploy overlay was not caught")
    return bad


def main(argv):
    bad = self_test()
    if bad or "--self-test" in argv:
        for b in bad:
            print(f"FAIL: check-image-locator-bake {b}", file=sys.stderr)
        if not bad:
            print("check-image-locator-bake: self-test OK")
        return 1 if bad else 0

    with open(os.path.join(ROOT, MANIFEST), "rb") as f:
        manifest = tomllib.load(f)
    rows = [r for v in manifest.values() if isinstance(v, list) for r in v if isinstance(r, dict)]
    for r in rows:
        if leaf_system_toml(r, ROOT) and r.get("lang", "rust") == "rust":
            r["_leaf"] = True

    problems = verify_table(READS_ENV, ROOT) + verify_overlay(READS_ENV, ROOT)
    row_problems, audit = check_rows(rows, READS_ENV, lambda r: image_locator(r, ROOT))
    problems += row_problems
    if "--audit" in argv:
        print("\n".join(audit))
    if problems:
        print("check-image-locator-bake: FAIL", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        return 1
    print(
        f"check-image-locator-bake: OK ({len(audit)} embedded cargo image rows each bake a "
        f"locator their board reads; READS_ENV verified against {len(READS_ENV)} board families)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
