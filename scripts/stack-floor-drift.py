#!/usr/bin/env python3
"""The stack-floor drift check: re-walk every `[[observed]]` row's image and
compare the walk with the row's recorded `static` (phase-557 W8).

An `[[observed]]` row in `config/stack-floors.toml` records the static walk
of one image, and everything sized from the row (the floor, a template's
stamped main stack) inherits that number. The walk moves with the tree: two
board commits moved the mps2 bare-metal talker's walk from 95,420 B to
100,660 B and nothing noticed, because its build still said "holds" (the
board's default stack is far above either number). This check builds each
row's image as a user would and holds the fresh walk to the row.

Rows:

- A template key (`<template>/<lang>/<rmw>@<board>`): a fresh `nros new` +
  `nros build` under `--out` (re-made every run, since `nros new` stamps the
  main stack from the table), outside the checkout: the user's road.
- A bringup key (`<bringup>@<image>`): `nros build <bringup>:<image>` of the
  in-tree example whose `examples/fixtures.toml` `[[workspace_fixture]]` row
  declares that bringup and image. Never a hand-written path.

Verdict, per row (`static_drift` in `scripts/nros-stack-floor.py`, the rule
the twin lane applies too): up by any amount FAILS, down by more than 2 %
FAILS, a smaller fall is a note. A row whose image cannot be built FAILS:
this check exists to notice, so it never skips silently.

Run: `just zephyr stack-floor-drift` (`source ./activate.sh` first; the
nucleo rows need the Zephyr SDK). `--self-test` needs neither, and runs
first on every run.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TABLE = ROOT / "config" / "stack-floors.toml"
FIXTURES = ROOT / "examples" / "fixtures.toml"


def _load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None and spec.loader is not None
    mod = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = mod  # its dataclasses look their module up there
    spec.loader.exec_module(mod)
    return mod


def _walker():
    return _load("nros_stack_floor", ROOT / "scripts" / "nros-stack-floor.py")


def _twin():
    return _load("stack_floor_twin", ROOT / "scripts" / "stack-floor-twin.py")


def bringup_rows(key: str, fixtures: list[dict]) -> list[dict]:
    """The `[[workspace_fixture]]` rows whose bringup and image make `key`."""
    return [f for f in fixtures
            if f.get("image") and f"{Path(f['bringup']).name}@{f['image']}" == key]


def pick_report(reports: list[Path], key: str, since: float = 0.0
                ) -> tuple[Path | None, str]:
    """The one report whose `image` is `key`, written at or after `since`,
    else (None, why). A report older than this run's build is another
    tree's walk, never this one's."""
    hits = []
    for r in reports:
        try:
            if r.stat().st_mtime >= since and json.loads(r.read_text()).get("image") == key:
                hits.append(r)
        except (OSError, ValueError):
            continue
    if len(hits) == 1:
        return hits[0], ""
    return None, (f"{len(hits)} stack-floor reports keyed {key} written by this run "
                  f"({len(reports)} examined)")


def _built(rc: int, report: Path | None, why: str, what: str, log: Path
           ) -> tuple[Path | None, str]:
    """A build that REFUSED still walked: the check writes its report before
    it refuses (a stack below the floor is the refusal this check predicts),
    so a fresh report is the answer and the refusal is noted."""
    if report is not None:
        return report, "" if rc == 0 else f"{what} exited {rc} after the walk ({log})"
    if rc:
        return None, f"{what} exited {rc} before any walk ({log})"
    return None, f"{why} ({log})"


def log_tail(log: Path, n: int = 60) -> list[str]:
    """The last `n` lines of a build log, for a row that could not be walked.
    The job runs in a container whose files nobody can read afterwards, so a
    failure that names only the log's path cannot be triaged from the job's
    output."""
    try:
        lines = log.read_text(errors="replace").splitlines()
    except OSError as e:
        return [f"(no log at {log}: {e.strerror})"]
    return lines[-n:]


def _reports(build_dir: Path) -> list[Path]:
    # A report sits at `<build>/.../nros/stack-floor/report.json` (cmake and
    # west roads) or `.../target/nros-stack-floor/report.json` (cargo road).
    # Scoped to the image's own build dir, never the checkout.
    found = build_dir.glob("**/report.json")  # walk-ok: untracked build output of one image
    return sorted(p for p in found if p.parent.name in ("stack-floor", "nros-stack-floor"))


def _env() -> dict:
    # The project's own `rust-toolchain.toml` picks the toolchain, as for a
    # user; an inherited RUSTUP_TOOLCHAIN would override it.
    env = dict(os.environ)
    env.pop("RUSTUP_TOOLCHAIN", None)
    return env


def build_template(nros: str, row: dict, out: Path, twin
                   ) -> tuple[Path | None, str, Path | None]:
    template, lang, rmw, board = twin.parse_key(row["image"])
    name = f"drift_{template}_{lang}".replace("-", "_")
    proj = out / name
    log = out / f"{name}.log"
    out.mkdir(parents=True, exist_ok=True)
    # FRESH every run: `nros new` stamps the template's main stack from the
    # table at creation, so a kept project would carry an old stamp.
    if proj.exists():
        shutil.rmtree(proj)
    with log.open("w") as lf:
        rc = subprocess.run([nros, "new", name, "--board", board, "--lang", lang,
                             "--rmw", rmw, "--template", template], cwd=out, stdout=lf,
                            stderr=subprocess.STDOUT, env=_env()).returncode
        if rc:
            return None, f"`nros new` exited {rc} ({log})", log
        start = time.time()
        rc = subprocess.run([nros, "build"], cwd=proj, stdout=lf, stderr=subprocess.STDOUT,
                            env=_env()).returncode
    report, why = pick_report(_reports(proj / "build"), row["image"], start)
    return *_built(rc, report, f"{why} under {proj / 'build'}", "`nros build`", log), log


def build_bringup(nros: str, row: dict, out: Path, fixtures: list[dict]
                  ) -> tuple[Path | None, str, Path | None]:
    hits = bringup_rows(row["image"], fixtures)
    if len(hits) != 1:
        return None, (f"{len(hits)} [[workspace_fixture]] rows in examples/fixtures.toml "
                      f"declare this bringup and image, not 1"), None
    fx = hits[0]
    ws = ROOT / fx["dir"]
    target = f"{Path(fx['bringup']).name}:{fx['image']}"
    out.mkdir(parents=True, exist_ok=True)
    log = out / f"drift_{fx['id']}.log"
    with log.open("w") as lf:
        start = time.time()
        rc = subprocess.run([nros, "build", target], cwd=ws, stdout=lf,
                            stderr=subprocess.STDOUT, env=_env()).returncode
    report, why = pick_report(_reports(ws / "build"), row["image"], start)
    return *_built(rc, report, f"{why} under {ws / 'build'}", f"`nros build {target}`",
                   log), log


def verdict(row: dict, report: Path, drift) -> tuple[str, list[str]]:
    role = json.loads(report.read_text()).get("roles", {}).get(row["role"])
    if role is None:
        return "fail", [f"  FAIL: {row['image']} {row['role']}: the build's report has no "
                        f"{row['role']} role ({report})"]
    if not role.get("measured"):
        return "fail", [f"  FAIL: {row['image']} {row['role']}: the build did not take the "
                        f"image as measured, so it is not keyed to this row ({report})"]
    return drift(row, int(role["depth"]))


def self_test() -> int:
    bad: list[str] = []

    def check(name: str, ok: bool) -> None:
        if not ok:
            bad.append(name)

    fx = [{"id": "a", "dir": "examples/x", "bringup": "src/qemu_bsp_talker_bringup",
           "image": "mps2-an385-baremetal"},
          {"id": "b", "dir": "examples/y", "bringup": "src/other_bringup", "image": "native"},
          {"id": "c", "dir": "examples/z", "bringup": "src/qemu_bsp_talker_bringup"}]
    check("bringup key: the row declaring bringup and image",
          [f["id"] for f in bringup_rows("qemu_bsp_talker_bringup@mps2-an385-baremetal", fx)]
          == ["a"])
    check("bringup key: a row without an image is no match",
          bringup_rows("qemu_bsp_talker_bringup@", fx) == [])

    import tempfile
    w = _walker()
    with tempfile.TemporaryDirectory() as td:
        d = Path(td)
        (d / "x" / "nros" / "stack-floor").mkdir(parents=True)
        (d / "y" / "target" / "nros-stack-floor").mkdir(parents=True)
        (d / "z" / "nros" / "stack-floor").mkdir(parents=True)
        r1 = d / "x" / "nros" / "stack-floor" / "report.json"
        r2 = d / "y" / "target" / "nros-stack-floor" / "report.json"
        r3 = d / "z" / "nros" / "stack-floor" / "report.json"
        r1.write_text(json.dumps({"image": "k1", "roles": {
            "OPEN": {"depth": 1000, "measured": True}}}))
        r2.write_text(json.dumps({"image": "k2", "roles": {
            "OPEN": {"depth": 1000, "measured": False}}}))
        r3.write_text(json.dumps({"image": "k1", "roles": {}}))
        (d / "other.json").write_text("{}")
        found = _reports(d)
        check("reports: both roads' report paths, nothing else", found == sorted([r1, r2, r3]))
        check("report: two keyed k1 is no pick", pick_report(found, "k1")[0] is None)
        check("report: the one keyed k2", pick_report(found, "k2")[0] == r2)
        check("report: one older than the run is not this run's walk",
              pick_report(found, "k2", r2.stat().st_mtime + 60)[0] is None)
        check("a refused build that walked answers with its report",
              _built(1, r2, "", "`nros build`", d / "l")[0] == r2)
        check("a build that failed before any walk is no answer",
              _built(1, None, "none", "`nros build`", d / "l")[0] is None)
        lg = d / "build.log"
        lg.write_text("".join(f"line {i}\n" for i in range(100)))
        check("a failed build prints its log's last lines",
              log_tail(lg) == [f"line {i}" for i in range(40, 100)])
        check("a missing log says so rather than raising",
              log_tail(d / "absent.log")[0].startswith("(no log at "))
        row = {"image": "k1", "role": "OPEN", "static": 1000}
        check("verdict: exact is ok", verdict(row, r1, w.static_drift)[0] == "ok")
        check("verdict: a walk above the row fails",
              verdict(dict(row, static=900), r1, w.static_drift)[0] == "fail")
        check("verdict: an image not taken as measured fails",
              verdict(dict(row, image="k2"), r2, w.static_drift)[0] == "fail")
        check("verdict: a report without the role fails",
              verdict(row, r3, w.static_drift)[0] == "fail")

    for b in bad:
        print(f"stack-floor-drift self-test: FAIL {b}", file=sys.stderr)
    print(f"stack-floor-drift self-test: {'FAIL' if bad else 'OK'}")
    return 1 if bad else 0


def main() -> int:
    ap = argparse.ArgumentParser(description=(__doc__ or "").split("\n\n")[0])
    ap.add_argument("--table", default=str(TABLE))
    ap.add_argument("--out", default=str(ROOT / "build" / "stack-floor-drift"),
                    help="where template projects are made, fresh each run "
                         "(default build/stack-floor-drift)")
    ap.add_argument("--only", action="append", help="a row's image key; repeat")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    rc = self_test()
    if args.self_test or rc:
        return rc

    w = _walker()
    twin = _twin()
    doc = w.load_table(Path(args.table))
    nros = shutil.which("nros")
    if not nros:
        raise SystemExit("stack-floor-drift: no `nros` on PATH; `source ./activate.sh` first")
    fixtures = w.load_toml(FIXTURES).get("workspace_fixture", [])
    rows = [r for r in doc.get("observed", []) if not args.only or r["image"] in args.only]
    if args.only and len(rows) != len(set(args.only)):
        raise SystemExit(f"stack-floor-drift: --only names a key with no row: {args.only}")
    failed = notes = 0
    for row in rows:
        print(f"== {row['image']} {row['role']} (recorded static {row['static']} B)", flush=True)
        start = time.time()
        if twin.parse_key(row["image"]):
            report, why, log = build_template(nros, row, Path(args.out), twin)
        else:
            report, why, log = build_bringup(nros, row, Path(args.out), fixtures)
        if report is None:
            failed += 1
            print(f"  FAIL: {row['image']}: the image could not be walked: {why}", flush=True)
            if log is not None:
                print(f"  -- last lines of {log} --", flush=True)
                print("\n".join(f"  | {line}" for line in log_tail(log)), flush=True)
            continue
        kind, lines = verdict(row, report, w.static_drift)
        if why:
            lines.append(f"  the build refused: {why}")
        failed += kind == "fail"
        notes += kind == "note"
        print("\n".join(lines + [f"  ({report}, {time.time() - start:.0f} s)"]), flush=True)
    print(f"stack-floor-drift: {len(rows)} row(s), {failed} failed, {notes} note(s)")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
