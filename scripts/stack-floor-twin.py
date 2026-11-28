#!/usr/bin/env python3
"""The stack-floor twin lane: re-measure every measured image on its twin.

A measured image's floor is `ceil8((static + allowance) x (100 + margin) / 100)`
(`scripts/nros-stack-floor.py`), and its allowance is a committed number:
`config/stack-floors.toml` `[[allowance]] bytes`, the twin's peak minus the
static walk at the time the row was measured. The static half moves with
every build. The allowance does not, so it goes stale, and it goes stale in
the UNSAFE direction: a frame on the walk shrinks, the walk falls, the row
stays, and the floor drops below the peak while every build still says
"holds". Nothing a build does can see that; only running the image can.

This lane runs the image. For every `[[allowance]]` and `[[observed]]` row
whose key is a `nros new` template key (`<template>/<lang>/<rmw>@<board>`) or
a bringup key (`<bringup>@<image>`, issue 2963), and whose board has a twin
(`[board.run] emulator = "renode:<machine>"` or `"qemu:<machine>"`; a QEMU
twin boots under `-icount`, so its runs agree too):

1. Make the project in `--out` (kept between runs, so a rerun is an
   incremental build). A template key: `nros new` the template. A bringup
   key: copy the files git tracks in the workspace its `[[workspace_fixture]]`
   row names (the drift check's lookup), so nothing in the checkout is
   edited. Then add `twin.conf` to the image's `os_config`: the
   thread analyzer, `CONFIG_INIT_STACKS`, and a 64 KiB main stack, so an
   image whose row is stale still runs (Renode does not enforce the MPU
   guard, so an overflow would be silent, issue 2359) and is measured
   rather than corrupted.
2. `nros build`. The build walks the image and writes what it measured to
   `nros/stack-floor/report.json` (`--report`): the static walk, the
   allowance it applied and the floor.
3. `nros run` in the twin for `--seconds`, and read the peak of the role's
   thread from the analyzer's rounds (the maximum; the twins are
   deterministic, so the rounds agree). The thread is the one the build names
   for the role (`--role-thread`, recorded in the report); OPEN's is `main`.
   A row's `twin_peers` (`["sntp=<port>", "dns=<port>"]`) are started on the
   host for the run (`scripts/lib/twin_peers.py`), and a peer that answers
   nothing fails the row: the run never reached the path it measures.
4. Verdict, per row. It FAILS when `peak > floor` (the floor does not hold
   the image), and when `bytes != peak - static` (the row is stale). A stale
   row fails in either direction, and the message says which direction and
   prints the corrected row: under-stating is the unsafe drift this lane
   exists for, and over-stating a stack is the cost the margin already pays.

`--measure KEY` runs a template key that has NO row yet and prints the
`[[observed]]` row it measured, for a person to add.

A row the lane cannot rebuild (a key that is not a template key, a board
with no Renode or QEMU twin) is reported as NOT RUN with the reason, and does not
fail the lane: a row measured on silicon is re-measured by hand.

Run: `just stack-floor-twin` (needs Renode, `nros setup --tool renode`, and
ROS's `rmw_zenohd`; `source ./activate.sh` first). `--self-test` needs
neither.
"""

from __future__ import annotations

import argparse
import functools
import importlib.util
import json
import re
import shutil
import subprocess
import sys
import time
import tempfile
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TABLE = ROOT / "config" / "stack-floors.toml"
FIXTURES = ROOT / "examples" / "fixtures.toml"
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import twin_peers  # noqa: E402
TWIN_CONF = "twin.conf"
TWIN_STACK = 65536
# The analyzer's line for one thread:
#   " main                : STACK: unused 4508 usage 11876 / 16384 (72 %); CPU: 44 %"
ANALYZER = re.compile(r"^\s*(?P<name>\S+)\s*:\s*STACK:\s*unused\s+\d+\s+usage\s+"
                      r"(?P<used>\d+)\s*/\s*(?P<size>\d+)")
# A string the image never prints: `nros run` runs until --timeout.
NEVER = "nros-stack-floor-twin: never printed"


@functools.cache
def _walker():
    """`scripts/nros-stack-floor.py`, whose table reader and floor formula
    this lane must agree with, so it imports them rather than restating."""
    spec = importlib.util.spec_from_file_location(
        "nros_stack_floor", ROOT / "scripts" / "nros-stack-floor.py")
    assert spec is not None and spec.loader is not None
    mod = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = mod  # its dataclasses look their module up there
    spec.loader.exec_module(mod)
    return mod


@functools.cache
def _drift():
    """`scripts/stack-floor-drift.py`, whose bringup-key lookup and build
    environment a bringup row is built with (issue 2963), so this lane and the
    drift check build the same image the same way."""
    spec = importlib.util.spec_from_file_location(
        "stack_floor_drift", ROOT / "scripts" / "stack-floor-drift.py")
    assert spec is not None and spec.loader is not None
    mod = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = mod
    spec.loader.exec_module(mod)
    return mod


def bringup_source(key: str, fixtures: list[dict]) -> tuple[dict | None, str]:
    """The `[[workspace_fixture]]` row that builds bringup key `key` (the drift
    check's lookup), or None and why."""
    hits = _drift().bringup_rows(key, fixtures)
    if len(hits) != 1:
        return None, (f"{len(hits)} [[workspace_fixture]] rows in examples/fixtures.toml "
                      "declare this bringup and image, not 1")
    return hits[0], ""


def image_board(system_toml: Path, image: str) -> str | None:
    """`[image.<image>] board` of a bringup's system.toml, read through the
    walker's loader (tomllib, or tomli on Python 3.10)."""
    try:
        doc = _walker().load_toml(system_toml)
    except (OSError, ValueError):
        return None
    b = doc.get("image", {}).get(image, {}).get("board")
    return b if isinstance(b, str) else None


def row_board(key: str, fixtures: list[dict], w) -> tuple[str | None, str]:
    """The board a row's image runs on: a template key states it, a bringup
    key's image declares it in its bringup's `system.toml`."""
    parsed = parse_key(key)
    if parsed:
        return parsed[3], ""
    shape = (w.parse_image_key(key) or ("",))[0]
    if shape != "bringup":
        return None, "not a template or bringup key, so the lane cannot make it"
    fx, why = bringup_source(key, fixtures)
    if fx is None:
        return None, why
    sysf = ROOT / fx["dir"] / fx["bringup"] / "system.toml"
    board = image_board(sysf, fx["image"]) if sysf.is_file() else None
    if board is None:
        return None, f"{sysf} declares no board for image `{fx['image']}`"
    return board, ""


def copy_tracked(src: str, dest: Path) -> int:
    """Copy the files git tracks under `src` (repo-relative) into `dest`,
    writing only those whose bytes differ, so a rerun is an incremental build.
    Returns how many it wrote. The copy is a project of its own under the
    lane's `--out`, so the lane never edits a tracked file."""
    p = subprocess.run(["git", "-C", str(ROOT), "ls-files", "-z", "--", src],
                       stdout=subprocess.PIPE, check=True)
    wrote = 0
    for f in filter(None, p.stdout.decode().split("\0")):
        data = (ROOT / f).read_bytes()
        d = dest / Path(f).relative_to(src)
        if not d.is_file() or d.read_bytes() != data:
            d.parent.mkdir(parents=True, exist_ok=True)
            d.write_bytes(data)
            wrote += 1
    return wrote


def stamped_stack(conf: Path) -> int | None:
    """The `CONFIG_MAIN_STACK_SIZE` a template's `stack.conf` states, if any."""
    if not conf.is_file():
        return None
    vals = [int(m.group(1)) for m in
            re.finditer(r"(?m)^CONFIG_MAIN_STACK_SIZE=([0-9]+)\s*$", conf.read_text())]
    return vals[-1] if vals else None


def twin_stack(stamped: int | None, board: int | None = None) -> int:
    """The twin's main stack: TWIN_STACK, or the stack the image gets anyway
    when that is larger: the template's stamp, else the board's own
    `prj.conf`. The build CHECKs a measured image's stack against its own
    walk's floor and refuses one below it, so lowering it to TWIN_STACK makes
    the twin refuse to build the image it is meant to measure. Issue 2790 hit
    that through the stamp; mps2-an385-zephyr hit it through the board
    (`nros new` stamps nothing there, because the board's 131,072 B is above
    the floor, and the twin's 64 KiB went below it). The twin never lowers the
    image's stack."""
    return max(TWIN_STACK, stamped or 0, board or 0)


def board_stack(board: str) -> int | None:
    """The `CONFIG_MAIN_STACK_SIZE` the board package's `prj.conf` states."""
    return stamped_stack(ROOT / "packages" / "boards" / f"nros-board-{board}" / "prj.conf")


def twin_conf(stack: int = TWIN_STACK, qemu: bool = False) -> str:
    # Zephyr turns QEMU's icount off for a networked image (`QEMU_ICOUNT`
    # `default y if !NETWORKING`); the twin turns it on, so Zephyr's own run
    # line boots under `-icount` and the analyzer reads the same peak every
    # run (phase-557 W10a).
    icount = ["# QEMU twin: Zephyr's run line adds -icount, so every run agrees.",
              "CONFIG_QEMU_ICOUNT=y"] if qemu else []
    return "\n".join([
        "# GENERATED by scripts/stack-floor-twin.py: the stack-floor twin lane.",
        "# The thread analyzer prints each thread's stack high-water mark every",
        "# 5 s; INIT_STACKS fills the stacks so the mark is measured, not",
        f"# guessed. A {stack} B main stack (at least {TWIN_STACK}, never below",
        "# the template's stamp) so an image whose row is stale still runs",
        "# (Renode does not enforce the MPU guard) and is measured.",
        "CONFIG_INIT_STACKS=y",
        "CONFIG_THREAD_NAME=y",
        "CONFIG_THREAD_STACK_INFO=y",
        "CONFIG_THREAD_ANALYZER=y",
        "CONFIG_THREAD_ANALYZER_USE_PRINTK=y",
        "CONFIG_THREAD_ANALYZER_AUTO=y",
        "CONFIG_THREAD_ANALYZER_AUTO_INTERVAL=5",
        "# The analyzer's own thread. Zephyr's 1024 B default overflowed on",
        "# qemu_cortex_a53 at its second round (a Data Abort that halted the",
        "# image, issue 2846), so it never reached the rounds the lane counts.",
        "CONFIG_THREAD_ANALYZER_AUTO_STACK_SIZE=4096",
        f"CONFIG_MAIN_STACK_SIZE={stack}",
        *icount,
        "",
    ])


def parse_key(key: str) -> tuple[str, str, str, str] | None:
    """`<template>/<lang>/<rmw>@<board>` (key (c), `stack_floor_template_key`
    in nros-cli-core), else None."""
    m = re.fullmatch(r"([a-z][a-z0-9-]*)/([a-z]+)/([a-z]+)@([a-z0-9][a-z0-9-]*)", key)
    return (m.group(1), m.group(2), m.group(3), m.group(4)) if m else None


def project_name(template: str, lang: str) -> str:
    return f"twin_{template}_{lang}".replace("-", "_")


def renode_machine(board: str) -> str | None:
    """The board's Renode machine, from its descriptor's `[board.run]
    emulator = "renode:<machine>"`, else None."""
    t = twin_emulator(board)
    return t[1] if t and t[0] == "renode" else None


# The emulators a twin may run under: free and open source (RFC-0115
# Amendment 1). Arm FVP is not one, so an `fvp:` board has no twin here.
TWIN_EMULATORS = ("renode", "qemu")


def twin_emulator(board: str) -> tuple[str, str] | None:
    """`(emulator, machine)` from the board descriptor's `[board.run]
    emulator = "<emulator>:<machine>"`, when the emulator is a twin's, else
    None."""
    f = ROOT / "packages" / "boards" / f"nros-board-{board}" / "nros-board.toml"
    if not f.is_file():
        return None
    return emulator_of(f.read_text())


def twin_qemu_cpus(board: str) -> int:
    """How many CPUs the board's QEMU line runs (`[board.run.qemu] smp =
    "cpus=N"`), 1 when the descriptor states none."""
    f = ROOT / "packages" / "boards" / f"nros-board-{board}" / "nros-board.toml"
    return qemu_cpus_of(f.read_text()) if f.is_file() else 1


def qemu_cpus_of(descriptor: str) -> int:
    """`twin_qemu_cpus` over a descriptor's text."""
    section = ""
    for line in descriptor.splitlines():
        s = line.split("#", 1)[0].strip()
        if s.startswith("["):
            section = s
        elif section == "[board.run.qemu]":
            m = re.fullmatch(r'smp\s*=\s*"cpus=(\d+)[^"]*"', s)
            if m:
                return int(m.group(1))
    return 1


def emulator_of(descriptor: str) -> tuple[str, str] | None:
    """`twin_emulator` over a descriptor's text."""
    section = ""
    for line in descriptor.splitlines():
        s = line.split("#", 1)[0].strip()
        if s.startswith("["):
            section = s
        elif section == "[board.run]":
            m = re.fullmatch(r'emulator\s*=\s*"([a-z]+):([^"]+)"', s)
            if m and m.group(1) in TWIN_EMULATORS:
                return m.group(1), m.group(2)
    return None


def with_twin_conf(system_toml: str) -> str:
    """The `[image.*]` block that states `stack_floor_image` also names
    `twin.conf`, LAST, so its main stack wins over a `stack.conf` the
    template wrote (Zephyr merges fragments last-wins). Idempotent."""
    lines = system_toml.splitlines()
    out: list[str] = []
    block: list[str] = []

    def flush():
        if any(re.match(r"\s*stack_floor_image\s*=", b) for b in block):
            idx = next((i for i, b in enumerate(block) if re.match(r"\s*os_config\s*=", b)), None)
            if idx is None:
                at = next(i for i, b in enumerate(block) if re.match(r"\s*stack_floor_image\s*=", b))
                block.insert(at, f'os_config = ["{TWIN_CONF}"]')
            else:
                items = re.findall(r'"([^"]*)"', block[idx])
                if TWIN_CONF not in items:
                    items.append(TWIN_CONF)
                block[idx] = "os_config = [" + ", ".join(f'"{i}"' for i in items) + "]"
        out.extend(block)
        block.clear()

    for line in lines:
        if line.startswith("["):
            flush()
        block.append(line)
    flush()
    return "\n".join(out) + "\n"


def main_peaks(console: str, thread: str = "main") -> list[tuple[int, int]]:
    """(used, size) for every analyzer round of `thread`."""
    got = []
    for line in console.splitlines():
        m = ANALYZER.match(line)
        if m and m.group("name") == thread:
            got.append((int(m.group("used")), int(m.group("size"))))
    return got


@dataclass
class Verdict:
    ok: bool
    lines: list[str]


def verdict(row: dict, role: dict, peak: int, size: int, margin: int, floor_formula) -> Verdict:
    """Compare one row with one twin run. `role` is the build's report for
    the row's role; `peak`/`size` the analyzer's main thread."""
    key = row["image"]
    static = int(role["depth"])
    bytes_ = int(row["bytes"])
    floor = int(role["floor"])
    lines = [f"{key} {row['role']}: peak {peak} B, static {static} B, allowance {bytes_} B, "
             f"floor {floor} B"]
    if not role.get("measured") or int(role["allowance"]) != bytes_:
        return Verdict(False, lines + [
            f"  FAIL: the build applied allowance {role.get('allowance')} "
            f"(measured={role.get('measured')}), not this row's {bytes_} B: the image the "
            "lane built is not keyed to this row"])
    if peak >= size:
        return Verdict(False, lines + [
            f"  FAIL: the measured thread used its whole {size} B stack in the twin; the peak is "
            "not a measurement. Raise TWIN_STACK."])
    ok = True
    implied = peak - static
    if peak > floor:
        ok = False
        lines.append(f"  FAIL: the peak is {peak - floor} B ABOVE the floor; this image "
                     "overflows a stack sized to its floor")
    was = int(row.get("measured", {}).get("static", static))
    if was != static:
        lines.append(f"  note: the static walk moved {static - was:+d} B since the row was "
                     f"measured ({was} -> {static} B)")
    if implied > bytes_:
        ok = False
        lines.append(f"  FAIL: STALE, UNSAFE: peak - static = {implied} B, the row says "
                     f"{bytes_} B, so the allowance UNDER-states the indirect frames by "
                     f"{implied - bytes_} B and the floor is that much too low")
    elif implied < bytes_:
        ok = False
        lines.append(f"  FAIL: STALE, safe: peak - static = {implied} B, the row says "
                     f"{bytes_} B, so the floor over-states by {bytes_ - implied} B")
    if implied != bytes_:
        lines.append("  Re-measured row (config/stack-floors.toml, then --sync-json):")
        lines.append(f"    bytes = {implied}            # floor "
                     f"{floor_formula(static, implied, margin)} B")
        lines.append(f"    [allowance.measured] peak = {peak}, static = {static}")
    if ok:
        lines.append("  OK: the row is exact and the floor holds")
    return Verdict(ok, lines)


def verdict_observed(row: dict, role: dict, peak: int, size: int, drift) -> Verdict:
    """An `[[observed]]` row (issue 2402): under a walk that binds every
    indirect call, the static walk is the upper bound, and the run must stay
    under it. The row's peak is evidence, re-measured here. The build's walk
    is also held to the row's recorded `static` by `drift` (the walker's
    `static_drift`, phase-557 W8), so this lane and the nightly drift check
    cannot disagree about one row."""
    key = row["image"]
    static = int(role["depth"])
    floor = int(role["floor"])
    lines = [f"{key} {row['role']}: peak {peak} B, static {static} B (observed row), "
             f"floor {floor} B"]
    if not role.get("measured"):
        return Verdict(False, lines + [
            "  FAIL: the build did not take this image as measured: the image the lane "
            "built is not keyed to this row"])
    if peak >= size:
        return Verdict(False, lines + [
            f"  FAIL: the measured thread used its whole {size} B stack in the twin; the peak is "
            "not a measurement. Raise TWIN_STACK."])
    ok = True
    if peak > floor:
        ok = False
        lines.append(f"  FAIL: the peak is {peak - floor} B ABOVE the floor; this image "
                     "overflows a stack sized to its floor")
    if peak > static:
        ok = False
        lines.append(f"  FAIL: the run used {peak - static} B MORE than the static walk, which "
                     "binds every indirect call and is meant to bound it: the walk missed a "
                     "path (issue 2402)")
    if "static" in row:
        kind, dlines = drift(row, static)
        lines += dlines
        if kind == "fail":
            ok = False
    was = int(row.get("peak", peak))
    if was != peak:
        lines.append(f"  note: the peak moved {peak - was:+d} B since the row ({was} -> "
                     f"{peak} B); update `peak = {peak}` in config/stack-floors.toml")
    if ok:
        lines.append("  OK: the static walk covers the run")
    return Verdict(ok, lines)


def measured_row(row: dict, role: dict, peak: int, size: int,
                 peaks: list[tuple[int, int]]) -> Verdict:
    """`--measure`: a key with no row yet. Print the `[[observed]]` row the
    run measured, for a person to add, and fail only when the run is not a
    measurement."""
    key = row["image"]
    rounds = sorted({u for u, _ in peaks})
    lines = [f"{key} {row['role']}: peak {peak} B over {len(peaks)} round(s), "
             f"static {role['depth']} B"]
    if peak >= size:
        return Verdict(False, lines + [
            f"  FAIL: the measured thread used its whole {size} B stack in the twin; the peak is "
            "not a measurement. Raise TWIN_STACK."])
    if len(rounds) > 1:
        lines.append(f"  note: the rounds disagree ({rounds}); the row takes the largest")
    lines += ["  measured row (add to config/stack-floors.toml):",
              "  [[observed]]",
              f'  image = "{key}"',
              f'  rmw = "{row["rmw"]}"',
              f'  role = "{row["role"]}"',
              f"  peak = {peak}",
              f"  static = {role['depth']}"]
    return Verdict(True, lines)


def prepare_template(nros: str, row: dict, out: Path, lf
                     ) -> tuple[Path, Path, str] | Verdict:
    """`nros new` the template into `out` (once; kept, so a rerun is
    incremental). Returns (project, system.toml, board)."""
    template, lang, rmw, board = parse_key(row["image"])  # type: ignore[misc]
    proj = out / project_name(template, lang)
    if not proj.exists():
        rc = subprocess.run([nros, "new", proj.name, "--board", board, "--lang", lang,
                             "--rmw", rmw, "--template", template],
                            cwd=out, stdout=lf, stderr=subprocess.STDOUT).returncode
        if rc:
            return Verdict(False, [f"{row['image']}: FAIL: `nros new` exited {rc}"])
    systems = [p for p in proj.glob("src/*/system.toml")
               if "stack_floor_image" in p.read_text()]
    if len(systems) != 1:
        return Verdict(False, [f"{row['image']}: FAIL: {proj} holds {len(systems)} "
                               "system.toml stating stack_floor_image, not 1"])
    return proj, systems[0], board


def prepare_bringup(row: dict, out: Path, fixtures: list[dict]
                    ) -> tuple[Path, Path, str, str] | Verdict:
    """A bringup key (issue 2963): copy the workspace its fixture row names
    into `out` (tracked files only) and build the row's image there, so the
    twin's conf goes into a copy and never into the checkout. Returns
    (project, system.toml, board, `<bringup>:<image>`)."""
    fx, why = bringup_source(row["image"], fixtures)
    if fx is None:
        return Verdict(False, [f"{row['image']}: FAIL: {why}"])
    proj = out / f"twin_{fx['image']}"
    copy_tracked(fx["dir"], proj)
    sysf = proj / fx["bringup"] / "system.toml"
    board = image_board(sysf, fx["image"])
    if board is None:
        return Verdict(False, [f"{row['image']}: FAIL: {sysf} declares no board for "
                               f"image `{fx['image']}`"])
    return proj, sysf, board, f"{Path(fx['bringup']).name}:{fx['image']}"


def run_row(nros: str, row: dict, out: Path, seconds: int, rounds: int, w,
            drift, measure: bool = False, fixtures: list[dict] | None = None) -> Verdict:
    out.mkdir(parents=True, exist_ok=True)
    bringup = parse_key(row["image"]) is None
    log = out / f"{re.sub(r'[^A-Za-z0-9_.-]', '_', row['image'])}.log"
    env = _drift()._env() if bringup else None
    with log.open("w") as lf:
        if bringup:
            prep = prepare_bringup(row, out, fixtures or [])
            if isinstance(prep, Verdict):
                return prep
            proj, sysf, board, target = prep
        else:
            prep = prepare_template(nros, row, out, lf)
            if isinstance(prep, Verdict):
                return Verdict(False, [f"{prep.lines[0]} ({log})"])
            (proj, sysf, board), target = prep, None
        text = sysf.read_text()
        if f'stack_floor_image = "{row["image"]}"' not in text:
            return Verdict(False, [f"{row['image']}: FAIL: {sysf} is not keyed to this row"])
        stack = twin_stack(stamped_stack(sysf.parent / "stack.conf"), board_stack(board))
        emu = twin_emulator(board)
        # Issue 3015 -- icount only on a single-CPU QEMU twin. With two
        # vCPUs and QEMU's user network, no fixed `-icount shift` gave
        # qemu_cortex_a53 runs that agree, or even a session that holds:
        # shift=4 (Zephyr's) lost the session over slirp, and shift=0 and 1
        # each gave one run with SNTP answers and one without (measured on
        # host 139, 2026-11-28). An SMP twin runs without icount, as Zephyr
        # itself does under NETWORKING, and its row keeps the highest peak.
        icount = bool(emu and emu[0] == "qemu") and twin_qemu_cpus(board) == 1
        (sysf.parent / TWIN_CONF).write_text(twin_conf(stack, qemu=icount))
        sysf.write_text(with_twin_conf(text))
        tgt = [target] if target else []

        rc = subprocess.run([nros, "build", *tgt], cwd=proj, stdout=lf,
                            stderr=subprocess.STDOUT, env=env).returncode
        if rc:
            return Verdict(False, [f"{row['image']}: FAIL: `nros build` exited {rc} ({log})"])
        # The build walks only when the image relinks, so a table edit alone
        # leaves an old report. Re-run the build's own generated walk: same
        # command, current table, and a warm llc cache makes it seconds.
        # The report and its walk are where the build says (issue 2795): its
        # plan states the road's directory, `report_path` the file in it.
        rdir, why = w.plan_report_dir(nros, target, proj, lf, env=env)
        if rdir is None:
            return Verdict(False, [f"{row['image']}: FAIL: {why} ({log})"])
        check = rdir / "check.cmake"
        if not check.is_file():
            return Verdict(False, [f"{row['image']}: FAIL: the build generated no "
                                   f"stack-floor walk at {check}"])
        subprocess.run(["cmake", "-P", str(check)], cwd=rdir, stdout=lf,
                       stderr=subprocess.STDOUT)
        rpath = w.report_path(rdir, row["image"])
        if not rpath.is_file():
            return Verdict(False, [f"{row['image']}: FAIL: the build wrote no stack-floor "
                                   f"report at {rpath}"])
        report = json.loads(rpath.read_text())
        role = report["roles"].get(row["role"])
        if role is None:
            return Verdict(False, [f"{row['image']}: FAIL: the build's report has no "
                                   f"{row['role']} role (refused: {report.get('refused')})"])
        # The thread whose analyzer line is this role's (issue 2963): the
        # build states it (`--role-thread`), OPEN's is `main`.
        thread = role.get("thread") or ("main" if row["role"] == "OPEN" else None)
        if thread is None:
            return Verdict(False, [f"{row['image']}: FAIL: the build names no thread for "
                                   f"role {row['role']} (`--role-thread`), so the lane cannot "
                                   "tell which analyzer line is its"])

        try:
            peers = twin_peers.Peers(row.get("twin_peers"))
        except ValueError as e:
            return Verdict(False, [f"{row['image']}: FAIL: {e}"])
        start = time.time()
        with peers:
            subprocess.run([nros, "run", *tgt, "--expect", NEVER, "--timeout", str(seconds)],
                           cwd=proj, stdout=lf, stderr=subprocess.STDOUT, env=env)
    consoles = [c for c in proj.glob("build/nros/run/*/console.log")
                if c.stat().st_mtime >= start]
    if len(consoles) != 1:
        return Verdict(False, [f"{row['image']}: FAIL: the twin run left {len(consoles)} fresh "
                               f"console logs, not 1 ({log})"])
    quiet = [k for k, n in peers.answered.items() if n == 0]
    if quiet:
        return Verdict(False, [f"{row['image']}: FAIL: peer(s) {', '.join(quiet)} answered "
                               "nothing, so the run never reached the path the row measures "
                               f"({consoles[0]}, {log})"])
    peaks = main_peaks(consoles[0].read_text(errors="replace"), thread)
    if len(peaks) < rounds:
        return Verdict(False, [f"{row['image']}: FAIL: {len(peaks)} analyzer round(s) of "
                               f"thread `{thread}` in {seconds} s, fewer than {rounds} "
                               f"({consoles[0]}, {log})"])
    peak = max(u for u, _ in peaks)
    size = peaks[-1][1]
    if measure:
        v = measured_row(row, role, peak, size, peaks)
    elif "bytes" not in row:
        v = verdict_observed(row, role, peak, size, drift)
    else:
        v = verdict(row, role, peak, size, int(report_margin(report, row)), w.floor_formula)
    if peers.answered:
        v.lines.insert(1, "  peers answered: " + ", ".join(
            f"{k} {n}" for k, n in peers.answered.items()))
    return v


def report_margin(report: dict, row: dict) -> int:
    return int(report["roles"][row["role"]]["margin_pct"])


def self_test() -> int:
    w = _walker()
    bad = []

    def check(name, cond):
        if not cond:
            bad.append(name)

    check("key: template key parses",
          parse_key("action-server/rust/zenoh@nucleo-f767zi-zephyr")
          == ("action-server", "rust", "zenoh", "nucleo-f767zi-zephyr"))
    check("key: a bringup key is not a template key", parse_key("fc_bringup@mps2-an385") is None)
    check("project name is an identifier",
          project_name("action-server", "rust") == "twin_action_server_rust")
    check("the nucleo board has a Renode twin",
          renode_machine("nucleo-f767zi-zephyr") == "nucleo_f767zi")
    check("a board with no descriptor has none", renode_machine("no-such-board") is None)
    # phase-557 W10a -- QEMU is a twin, FVP is not (RFC-0115 Amendment 1)
    check("the twin never lowers a board's own main stack (mps2-an385-zephyr)",
          board_stack("mps2-an385-zephyr") == 131072
          and twin_stack(None, board_stack("mps2-an385-zephyr")) == 131072)
    check("the mps2-an385 Zephyr board has a QEMU twin",
          twin_emulator("mps2-an385-zephyr") == ("qemu", "mps2-an385"))
    check("a QEMU board has no Renode machine", renode_machine("mps2-an385-zephyr") is None)
    fvp = ROOT / "packages/boards/nros-board-zephyr/boards/fvp-aemv8r-smp/nros-board.toml"
    check("an FVP board has no twin (its real descriptor)",
          fvp.is_file() and emulator_of(fvp.read_text()) is None
          and emulator_of('[board.run]\nemulator = "qemu:x"\n') == ("qemu", "x"))
    check("an SMP QEMU board runs its twin without icount (qemu-cortex-a53: 2 CPUs)",
          twin_qemu_cpus("qemu-cortex-a53") == 2 and twin_qemu_cpus("mps2-an385-zephyr") == 1
          and qemu_cpus_of('[board.run.qemu]\nsmp = "cpus=4"\n') == 4
          and qemu_cpus_of('[board.run]\nsmp = "cpus=4"\n') == 1)
    check("a QEMU twin's conf turns icount on, a Renode twin's does not",
          "CONFIG_QEMU_ICOUNT=y" in twin_conf(qemu=True)
          and "QEMU_ICOUNT" not in twin_conf())
    mr = measured_row({"image": "talker/rust/zenoh@b", "role": "OPEN", "rmw": "zenoh"},
                      {"depth": 60000}, 30000, 65536, [(29000, 65536), (30000, 65536)])
    check("--measure prints the row it measured, with the largest round",
          mr.ok and "  peak = 30000" in mr.lines and "  static = 60000" in mr.lines
          and any("rounds disagree" in x for x in mr.lines))
    check("--measure refuses a run that filled its stack",
          not measured_row({"image": "k", "role": "OPEN", "rmw": "z"}, {"depth": 1},
                           65536, 65536, [(65536, 65536)]).ok)

    sysx = ('[system]\nname = "x"\n\n[image.zephyr]\nboard = "b"\n'
            'stack_floor_image = "talker/c/zenoh@b"\n')
    once = with_twin_conf(sysx)
    check("os_config added before the key",
          'os_config = ["twin.conf"]\nstack_floor_image' in once)
    check("adding twice is idempotent", with_twin_conf(once) == once)
    raised = sysx.replace("stack_floor_image", 'os_config = ["stack.conf"]\nstack_floor_image')
    check("the twin's main stack is never below the template's stamp (issue 2790)",
          twin_stack(78152) == 78152 and twin_stack(32768) == TWIN_STACK
          and twin_stack(None) == TWIN_STACK
          and twin_stack(None, 131072) == 131072 and twin_stack(None, 16384) == TWIN_STACK
          and "CONFIG_MAIN_STACK_SIZE=78152" in twin_conf(twin_stack(78152)))
    with tempfile.TemporaryDirectory() as d:
        c = Path(d, "stack.conf")
        c.write_text("# x\nCONFIG_MAIN_STACK_SIZE=75536\nCONFIG_MAIN_STACK_SIZE=78152\n")
        check("the stamp is the conf's last CONFIG_MAIN_STACK_SIZE",
              stamped_stack(c) == 78152 and stamped_stack(Path(d, "none.conf")) is None)
    check("twin.conf goes LAST, after stack.conf",
          'os_config = ["stack.conf", "twin.conf"]' in with_twin_conf(raised))
    other = '[image.other]\nos_config = ["a.conf"]\n\n' + sysx
    check("only the keyed image gets it", 'os_config = ["a.conf"]\n' in with_twin_conf(other))

    console = "\n".join([
        " thread_analyzer     : STACK: unused 808 usage 216 / 1024 (21 %); CPU: 10 %",
        " main                : STACK: unused 4508 usage 11876 / 16384 (72 %); CPU: 44 %",
        "Publishing: 'Hello World: 1'",
        " main                : STACK: unused 4400 usage 11984 / 16384 (73 %); CPU: 40 %",
        " mainline            : STACK: unused 1 usage 9 / 10 (90 %)",
    ])
    check("main's rounds, and only main's",
          main_peaks(console) == [(11876, 16384), (11984, 16384)])

    row = {"image": "talker/c/zenoh@b", "role": "OPEN", "bytes": -4416,
           "measured": {"peak": 11696, "static": 16112}}
    role = {"depth": 16112, "floor": 12872, "allowance": -4416, "measured": True,
            "margin_pct": 10}
    ff = w.floor_formula
    v = verdict(row, role, 11696, 65536, 10, ff)
    check("exact row: ok", v.ok and any("OK:" in x for x in v.lines))
    v = verdict(row, role, 11876, 65536, 10, ff)
    check("peak above peak-at-measure: STALE, UNSAFE, fails",
          not v.ok and any("UNSAFE" in x and "180 B" in x for x in v.lines))
    check("stale row prints the corrected bytes and its floor",
          any("bytes = -4236" in x and "13064 B" in x for x in v.lines))
    v = verdict(row, role, 11000, 65536, 10, ff)
    check("peak below: STALE, safe, fails", not v.ok and any("safe:" in x for x in v.lines))
    v = verdict(row, role, 13000, 65536, 10, ff)
    check("peak above the floor fails as an overflow",
          not v.ok and any("ABOVE the floor" in x for x in v.lines))
    v = verdict(row, dict(role, allowance=0, measured=False), 11696, 65536, 10, ff)
    check("a build that did not apply the row fails", not v.ok)
    v = verdict(row, role, 65536, 65536, 10, ff)
    check("a full stack is not a measurement", not v.ok)
    v = verdict(row, dict(role, depth=16000), 11584, 65536, 10, ff)
    check("a moved walk is noted", any("moved -112 B" in x for x in v.lines))

    # issue 2402 — an observed row: the walk must cover the run.
    orow = {"image": "talker/rust/zenoh@b", "role": "OPEN", "peak": 30000, "static": 50000}
    orole = {"depth": 50000, "floor": 55000, "measured": True}
    sd = functools.partial(w.static_drift, rise_pct=2)
    check("observed: a walk that covers the run is OK",
          verdict_observed(orow, orole, 30000, 65536, sd).ok)
    check("observed: a run above the static walk FAILS",
          not verdict_observed(dict(orow, static=29000), dict(orole, depth=29000), 30000,
                               65536, sd).ok)
    check("observed: an image the build did not take as measured FAILS",
          not verdict_observed(orow, dict(orole, measured=False), 30000, 65536, sd).ok)
    check("observed: a full-stack run is no measurement",
          not verdict_observed(orow, orole, 65536, 65536, sd).ok)
    # phase-557 W8: the build's walk is held to the row's `static` too.
    v = verdict_observed(orow, dict(orole, depth=51008), 30000, 65536, sd)
    check("observed: a walk that ROSE more than drift_rise_pct past the row FAILS",
          not v.ok and any("ROSE" in x for x in v.lines))
    v = verdict_observed(orow, dict(orole, depth=50008), 30000, 65536, sd)
    check("observed: a walk that rose within drift_rise_pct is a note",
          v.ok and any("note:" in x for x in v.lines))
    v = verdict_observed(orow, dict(orole, depth=48000), 30000, 65536, sd)
    check("observed: a walk that fell 4 % below the row's static is a note, never a failure",
          v.ok and any("fell" in x for x in v.lines))
    v = verdict_observed(orow, dict(orole, depth=49500), 30000, 65536, sd)
    check("observed: a walk 1 % below the row's static is a note",
          v.ok and any("note:" in x for x in v.lines))

    # issue 2963 -- a bringup key is a row the lane can make; its board is its
    # image's, read from the bringup the fixture row names.
    fixtures = w.load_toml(FIXTURES).get("workspace_fixture", [])
    check("a bringup row's board is its image's (the SNTP re-sync row)",
          row_board("pubsub_bringup@zephyr_cortex_m_talker_clock_sntp_resync", fixtures, w)
          == ("mps2-an385-zephyr", ""))
    pb = row_board("talker_entry/talker@mps2-an385-zephyr", fixtures, w)
    check("a program key is NOT RUN, with a reason", pb[0] is None and "cannot make" in pb[1])
    nb = row_board("no_such_bringup@no_image", fixtures, w)
    check("a bringup key no fixture row builds is NOT RUN, naming the count",
          nb[0] is None and "0 [[workspace_fixture]] rows" in nb[1])
    bad += [f"twin_peers: {b}" for b in twin_peers.self_test()]

    for b in bad:
        print(f"stack-floor-twin self-test: FAIL {b}", file=sys.stderr)
    print(f"stack-floor-twin self-test: {'FAIL' if bad else 'OK'}")
    return 1 if bad else 0


def main() -> int:
    ap = argparse.ArgumentParser(description=(__doc__ or "").split("\n\n")[0])
    ap.add_argument("--table", default=str(TABLE))
    ap.add_argument("--out", default=str(ROOT / "build" / "stack-floor-twin"),
                    help="where the twin projects are made and kept (default build/stack-floor-twin)")
    ap.add_argument("--only", action="append", help="a row's image key; repeat")
    ap.add_argument("--measure", action="append", metavar="KEY",
                    help="a template key with NO row yet: run it and print the "
                         "[[observed]] row it measured; repeat")
    ap.add_argument("--seconds", type=int, default=75,
                    help="how long the twin runs (default 75: ~14 analyzer rounds)")
    ap.add_argument("--rounds", type=int, default=6,
                    help="the fewest analyzer rounds of the main thread that count")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    # The negative controls run first on every run: a lane whose verdict
    # logic is broken must not report a row exact.
    rc = self_test()
    if args.self_test or rc:
        return rc

    w = _walker()
    doc = w.load_table(Path(args.table))
    nros = shutil.which("nros")
    if not nros:
        raise SystemExit("stack-floor-twin: no `nros` on PATH; `source ./activate.sh` first")
    if args.measure:
        known = {r["image"] for r in doc.get("allowance", []) + doc.get("observed", [])}
        rows = []
        for k in args.measure:
            pk = parse_key(k)
            if pk is None:
                raise SystemExit(f"stack-floor-twin: --measure takes a template key: {k}")
            if k in known:
                raise SystemExit(f"stack-floor-twin: {k} already has a row; use --only")
            rows.append({"image": k, "role": "OPEN", "rmw": pk[2]})
    else:
        # An `unwalked` row is a host image (issue 2834): no twin runs it.
        rows = [r for r in doc.get("allowance", []) + doc.get("observed", [])
                if (not args.only or r["image"] in args.only) and "unwalked" not in r]
    if args.only and len(rows) != len(set(args.only)):
        raise SystemExit(f"stack-floor-twin: --only names a key with no row: {args.only}")
    fixtures = w.load_toml(FIXTURES).get("workspace_fixture", [])
    failed = 0
    ran = 0
    for row in rows:
        board, why = row_board(row["image"], fixtures, w)
        if board is None:
            print(f"{row['image']}: NOT RUN: {why}")
            continue
        if twin_emulator(board) is None:
            print(f"{row['image']}: NOT RUN: board {board} has no Renode or QEMU twin")
            continue
        print(f"== {row['image']} {row['role']}", flush=True)
        v = run_row(nros, row, Path(args.out), args.seconds, args.rounds, w,
                    functools.partial(w.static_drift, rise_pct=w.drift_rise_pct(doc)),
                    measure=bool(args.measure), fixtures=fixtures)
        ran += 1
        failed += not v.ok
        print("\n".join(v.lines), flush=True)
    print(f"stack-floor-twin: {ran} row(s) run, {failed} failed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
