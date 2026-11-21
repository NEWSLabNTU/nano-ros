#!/usr/bin/env python3
"""The stack-floor drift check: re-walk every `[[observed]]` and `[[drift]]`
row's image and compare the walk with the row's recorded `static` (phase-557 W8).

An `[[observed]]` row in `config/stack-floors.toml` records the static walk
of one image, and everything sized from the row (the floor, a template's
stamped main stack) inherits that number. The walk moves with the tree: two
board commits moved the mps2 bare-metal talker's walk from 95,420 B to
100,660 B and nothing noticed, because its build still said "holds" (the
board's default stack is far above either number). This check builds each
row's image as a user would and holds the fresh walk to the row.

A `[[drift]]` row records a walk and nothing else: no build reads it, so its
image stays UNMEASURED (the build warns, never refuses) and `nros new` stamps
nothing from it. The check holds its walk all the same, and accepts the
unmeasured report such an image produces. It exists for an image whose stack
is not sized from its walk: the FreeRTOS C tutorial ships 64 KiB and walks
about twice that, and raising the stack would charge every user's RAM to
satisfy a walk that over-counts.

Rows:

- A template key (`<template>/<lang>/<rmw>@<board>`): a fresh `nros new` +
  `nros build` under `--out` (re-made every run, since `nros new` stamps the
  main stack from the table), outside the checkout: the user's road.
- A bringup key (`<bringup>@<image>`): `nros build <bringup>:<image>` of the
  in-tree example whose `examples/fixtures.toml` `[[workspace_fixture]]` row
  declares that bringup and image. Never a hand-written path.

Verdict, per row (`static_drift` in `scripts/nros-stack-floor.py`, the rule
the twin lane applies too, issue 2400 Ruling (C)): a walk more than the
table's `drift_rise_pct` above the row's recorded max FAILS; a smaller rise,
and any fall, is a note. A row whose image cannot be built FAILS:
this check exists to notice, so it never skips silently.

Run: `just zephyr stack-floor-drift` (`source ./activate.sh` first; the
nucleo rows need the Zephyr SDK). `--self-test` needs neither, and runs
first on every run.
"""

from __future__ import annotations

import argparse
import functools
import importlib.util
import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TABLE = ROOT / "config" / "stack-floors.toml"
FIXTURES = ROOT / "examples" / "fixtures.toml"
INDEX = ROOT / "nros-sdk-index.toml"
# The west manifest each Zephyr line is pinned by (`<major>.<minor>` of the
# tree's VERSION file).
WEST_MANIFESTS = {"3.7": ROOT / "west.yml", "4.4": ROOT / "west-4.4.yml"}
sys.path.insert(0, str(ROOT / "scripts" / "lib"))


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
    # A row is measured the way a user builds it: the `dev` profile with
    # full debuginfo, cargo's default. The walk reads DWARF (the callee order
    # comes from inlined back-references), so the debuginfo level moves it
    # with no change to the code: measured on the mps2 bare-metal bringup,
    # 90,976 B with full debuginfo and 93,040 B with `line-tables-only`. The
    # build runs inside the checkout, so the checkout's own `.cargo/config.toml`
    # profile reaches it, and a user's copied-out project never sees that
    # file. Pinned here, so neither the checkout nor a CI job's environment
    # chooses what the row measures.
    env["CARGO_PROFILE_DEV_DEBUG"] = "true"
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


def verdict(row: dict, report: Path, drift, table: str = "observed"
            ) -> tuple[str, list[str]]:
    role = json.loads(report.read_text()).get("roles", {}).get(row["role"])
    if role is None:
        return "fail", [f"  FAIL: {row['image']} {row['role']}: the build's report has no "
                        f"{row['role']} role ({report})"]
    # A [[drift]] row is read by no build, so its image is unmeasured by design.
    if table == "observed" and not role.get("measured"):
        return "fail", [f"  FAIL: {row['image']} {row['role']}: the build did not take the "
                        f"image as measured, so it is not keyed to this row ({report})"]
    return drift(row, int(role["depth"]))


# ---------------------------------------------------------------- toolchain
#
# Issue 2688: one tree walked 4.8 KiB deeper on a dev host than in CI. The
# measured cause was the host's toolchain, not the code: a store GCC two
# builds older than the pin. A verdict from such a walk is about another
# toolchain, so it is neither a PASS nor a FAIL of this row. Before reporting
# one, the check compares what the image's build RESOLVED with what the tree
# pins, and a mismatch makes the row NOT VERIFIED (the skip ledger), naming
# both.

def _store_tool(path: str) -> tuple[str, str] | None:
    """`(<tool>, <version>)` of a path inside the SDK store
    (`<store>/<tool>/<version>/...`, RFC-0095 D2), else None."""
    parts = Path(path).parts
    for i in range(len(parts) - 2):
        if parts[i] == "sdk" and re.fullmatch(r"[0-9][0-9A-Za-z.+-]*", parts[i + 2]):
            return parts[i + 1], parts[i + 2]
    return None


def index_versions(index: dict, tool: str) -> set[str]:
    """The versions `nros-sdk-index.toml` pins for a store directory: its own
    `[tool.<tool>]` and its per-line siblings (`[tool.<tool>-1-0-1]`)."""
    return {str(t["version"]) for name, t in index.get("tool", {}).items()
            if (name == tool or name.startswith(tool + "-")) and "version" in t}


def manifest_revision(manifest: Path) -> str | None:
    """The `revision` a west manifest pins for the `zephyr` project."""
    if not manifest.is_file():
        return None
    in_zephyr = False
    for line in manifest.read_text().splitlines():
        s = line.strip()
        if s.startswith("- name:"):
            in_zephyr = s.split(":", 1)[1].strip() == "zephyr"
        elif in_zephyr and s.startswith("revision:"):
            return s.split(":", 1)[1].strip().strip("\"'")
    return None


def toolchain_problems(cache: dict[str, str], index: dict, manifests: dict[str, str | None],
                       zephyr: tuple[str, str | None] | None) -> tuple[list[str], list[str]]:
    """`(identity, problems)` for one image's build, from its CMakeCache facts.

    `identity` names what the build resolved; `problems` the facts that differ
    from the tree's pins. `zephyr` is `(line, tag)` of the build's
    ZEPHYR_BASE (the tag its HEAD carries, or None), when it has one."""
    identity: list[str] = []
    problems: list[str] = []
    seen: set[tuple[str, str]] = set()
    for var in ("CMAKE_C_COMPILER", "ZEPHYR_SDK_INSTALL_DIR"):
        path = cache.get(var)
        if not path:
            continue
        st = _store_tool(path)
        if st is None:
            identity.append(f"{var}={path} (outside the SDK store)")
            problems.append(f"{var} is {path}, outside the SDK store, so no pin speaks for it")
            continue
        # The compiler inside a Zephyr SDK and the SDK dir name one fact.
        if st in seen:
            continue
        seen.add(st)
        tool, ver = st
        identity.append(f"{tool} {ver}")
        pins = index_versions(index, tool)
        if pins and ver not in pins:
            problems.append(f"{tool} {ver} is what the build resolved, and "
                            f"nros-sdk-index.toml pins {' or '.join(sorted(pins))}")
    if zephyr is not None:
        line, tag = zephyr
        want = manifests.get(line)
        identity.append(f"zephyr {tag or 'untagged'}")
        if want and tag != want:
            problems.append(f"the Zephyr {line} tree is at {tag or 'an untagged commit'}, and "
                            f"its west manifest pins {want} (`west update`)")
    return identity, problems


def read_cache(report: Path) -> dict[str, str]:
    """The CMakeCache.txt nearest above `report`, as `{name: value}`; empty
    when no directory above it holds one (a cargo-road build). A bringup's
    fixture builds outside `--out`, so the search is bounded by the
    filesystem, never by `--out`."""
    d = report.parent
    while True:
        c = d / "CMakeCache.txt"
        if c.is_file():
            out = {"": str(c)}
            for line in c.read_text(errors="replace").splitlines():
                m = re.match(r"^([A-Za-z_][A-Za-z0-9_]*):[A-Z]+=(.*)$", line)
                if m:
                    out[m.group(1)] = m.group(2)
            # A toolchain file sets CMAKE_C_COMPILER as a normal variable, so
            # the cache need not hold it. The compiler the configure USED is
            # recorded in CMakeFiles/<cmake version>/CMakeCCompiler.cmake.
            for f in sorted(d.glob("CMakeFiles/*/CMakeCCompiler.cmake")):
                m = re.search(r'^set\(CMAKE_C_COMPILER "([^"]+)"\)', f.read_text(), re.M)
                if m:
                    out["CMAKE_C_COMPILER"] = m.group(1)
            return out
        if d.parent == d:
            return {}
        d = d.parent


def zephyr_of(cache: dict[str, str]) -> tuple[str, str | None] | None:
    """`(line, tag)` of the build's ZEPHYR_BASE, or None without one."""
    base = cache.get("ZEPHYR_BASE")
    if not base or not Path(base, "VERSION").is_file():
        return None
    v = dict(re.findall(r"^(VERSION_MAJOR|VERSION_MINOR)\s*=\s*(\d+)",
                        Path(base, "VERSION").read_text(), re.M))
    line = f"{v.get('VERSION_MAJOR', '?')}.{v.get('VERSION_MINOR', '?')}"
    import git_hook_env
    env = git_hook_env.nros_clear_inherited_git_env(dict(os.environ))
    r = subprocess.run(["git", "-C", base, "describe", "--tags", "--exact-match", "HEAD"],
                       capture_output=True, text=True, env=env)
    return line, (r.stdout.strip() or None) if r.returncode == 0 else None


def table_rows(doc: dict, only: list[str] | None) -> list[tuple[str, dict]]:
    """Every row the check walks, with the table it came from."""
    return [(t, r) for t in ("observed", "drift") for r in doc.get(t, [])
            if not only or r["image"] in only]


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
        sd = functools.partial(w.static_drift, rise_pct=2)
        row = {"image": "k1", "role": "OPEN", "static": 1000}
        check("verdict: exact is ok", verdict(row, r1, sd)[0] == "ok")
        check("verdict: a walk above the row fails",
              verdict(dict(row, static=900), r1, sd)[0] == "fail")
        check("verdict: an image not taken as measured fails",
              verdict(dict(row, image="k2"), r2, sd)[0] == "fail")
        check("verdict: a report without the role fails",
              verdict(row, r3, sd)[0] == "fail")
        drow = {"image": "k2", "role": "OPEN", "static": 1000}
        check("verdict: a drift row takes an unmeasured walk, exact is ok",
              verdict(drow, r2, sd, "drift")[0] == "ok")
        check("verdict: a drift row whose walk rose fails",
              verdict(dict(drow, static=900), r2, sd, "drift")[0] == "fail")
        check("verdict: a drift row whose walk fell is a note, never a failure",
              verdict(dict(drow, static=1100), r2, sd, "drift")[0] == "note")
        check("rows: observed then drift, each tagged, --only across both",
              [(t, r["image"]) for t, r in table_rows(
                  {"observed": [{"image": "a"}], "drift": [{"image": "b"}]}, None)]
              == [("observed", "a"), ("drift", "b")]
              and [r["image"] for _, r in table_rows(
                  {"observed": [{"image": "a"}], "drift": [{"image": "b"}]}, ["b"])] == ["b"])

    # Issue 2688: a walk under another toolchain than the pins is NOT VERIFIED.
    idx = {"tool": {"arm-none-eabi-gcc": {"version": "13.2-nros5"},
                    "zephyr-sdk": {"version": "0.16.9"},
                    "zephyr-sdk-1-0-1": {"version": "1.0.1"}}}
    man = {"3.7": "v3.7.2", "4.4": "v4.4.0"}
    gcc = "/h/.nros/sdk/arm-none-eabi-gcc/{}/bin/arm-none-eabi-gcc"
    sdk = "/h/.nros/sdk/zephyr-sdk/{0}/zephyr-sdk-{0}"
    _, p = toolchain_problems({"CMAKE_C_COMPILER": gcc.format("13.2-nros5")}, idx, man, None)
    check("toolchain: the pinned store GCC is verified", p == [])
    _, p = toolchain_problems({"CMAKE_C_COMPILER": gcc.format("13.2-nros1")}, idx, man, None)
    check("toolchain: an older store GCC is NOT VERIFIED, naming both versions",
          len(p) == 1 and "13.2-nros1" in p[0] and "13.2-nros5" in p[0])
    _, p = toolchain_problems({"ZEPHYR_SDK_INSTALL_DIR": sdk.format("0.16.8")}, idx, man, None)
    check("toolchain: a Zephyr SDK no index entry pins is NOT VERIFIED",
          len(p) == 1 and "0.16.8" in p[0])
    _, p = toolchain_problems({"ZEPHYR_SDK_INSTALL_DIR": sdk.format("1.0.1")}, idx, man, None)
    check("toolchain: a per-line SDK entry counts as a pin", p == [])
    _, p = toolchain_problems({"CMAKE_C_COMPILER": "/usr/bin/arm-none-eabi-gcc"}, idx, man, None)
    check("toolchain: a compiler outside the store is NOT VERIFIED", len(p) == 1)
    _, p = toolchain_problems({}, idx, man, ("3.7", "v3.7.0"))
    check("toolchain: a Zephyr tree behind its manifest is NOT VERIFIED",
          len(p) == 1 and "v3.7.0" in p[0] and "v3.7.2" in p[0])
    _, p = toolchain_problems({}, idx, man, ("3.7", "v3.7.2"))
    check("toolchain: a Zephyr tree at its manifest's tag is verified", p == [])
    ident, p = toolchain_problems({}, idx, man, None)
    check("toolchain: a cargo-only build has nothing to compare", ident == [] and p == [])
    check("toolchain: store paths parse to (tool, version)",
          _store_tool(gcc.format("13.2-nros5")) == ("arm-none-eabi-gcc", "13.2-nros5")
          and _store_tool("/usr/bin/gcc") is None)
    with tempfile.TemporaryDirectory() as d:
        m = Path(d, "west.yml")
        m.write_text("manifest:\n  projects:\n    - name: other\n      revision: x\n"
                     "    - name: zephyr\n      remote: z\n      revision: v3.7.2\n")
        check("toolchain: the manifest's zephyr revision is read, not another project's",
              manifest_revision(m) == "v3.7.2" and manifest_revision(Path(d, "no.yml")) is None)
        b = Path(d, "b")
        (b / "x" / "nros" / "stack-floor").mkdir(parents=True)
        (b / "x" / "CMakeCache.txt").write_text("CMAKE_C_COMPILER:FILEPATH=/g\nFOO:STRING=1\n")
        tc = b / "y" / "CMakeFiles" / "3.22.1"
        tc.mkdir(parents=True)
        (b / "y" / "CMakeCache.txt").write_text("CMAKE_TOOLCHAIN_FILE:FILEPATH=/t.cmake\n")
        (tc / "CMakeCCompiler.cmake").write_text('set(CMAKE_C_COMPILER "/s/gcc")\n')
        check("toolchain: a toolchain-file build's compiler comes from CMakeCCompiler.cmake",
              read_cache(b / "y" / "r.json").get("CMAKE_C_COMPILER") == "/s/gcc")
        rep_ = b / "x" / "nros" / "stack-floor" / "report.json"
        check("toolchain: the nearest CMakeCache above the report is read",
              read_cache(rep_).get("CMAKE_C_COMPILER") == "/g"
              and read_cache(Path(d, "elsewhere", "r.json")) == {})
    _, p = toolchain_problems({"CMAKE_C_COMPILER": sdk.format("0.16.8") + "/bin/gcc",
                               "ZEPHYR_SDK_INSTALL_DIR": sdk.format("0.16.8")}, idx, man, None)
    check("toolchain: an SDK's compiler and the SDK dir are one fact, reported once",
          len(p) == 1)

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
    drift = functools.partial(w.static_drift, rise_pct=w.drift_rise_pct(doc))
    nros = shutil.which("nros")
    if not nros:
        raise SystemExit("stack-floor-drift: no `nros` on PATH; `source ./activate.sh` first")
    fixtures = w.load_toml(FIXTURES).get("workspace_fixture", [])
    rows = table_rows(doc, args.only)
    if args.only and len(rows) != len(set(args.only)):
        raise SystemExit(f"stack-floor-drift: --only names a key with no row: {args.only}")
    index = w.load_toml(INDEX)
    manifests = {line: manifest_revision(m) for line, m in WEST_MANIFESTS.items()}
    failed = notes = unverified = 0
    for table, row in rows:
        print(f"== {row['image']} {row['role']} (recorded static {row['static']} B, "
              f"[[{table}]])", flush=True)
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
        kind, lines = verdict(row, report, drift, table)
        if why:
            lines.append(f"  the build refused: {why}")
        cache = read_cache(report)
        identity, problems = toolchain_problems(cache, index, manifests, zephyr_of(cache))
        if identity:
            lines.append("  toolchain: " + ", ".join(identity))
        elif cache:
            lines.append(f"  toolchain: {cache['']} names no C compiler or Zephyr tree: "
                         "not checked")
        else:
            lines.append("  toolchain: no CMakeCache.txt above the report (a cargo-road "
                         "build): not checked")
        if problems:
            # Neither verdict is about this tree's toolchain (issue 2688).
            lines.append(f"  NOT VERIFIED: the walk above ran under another toolchain than the "
                         f"tree pins, so it says nothing about this row ({kind} withheld):")
            lines += [f"    - {p}" for p in problems]
            import check_skip
            failed += check_skip.unverified(f"stack-floor-drift {row['image']}",
                                            "; ".join(problems))
            unverified += 1
            kind = "unverified"
        failed += kind == "fail"
        notes += kind == "note"
        print("\n".join(lines + [f"  ({report}, {time.time() - start:.0f} s)"]), flush=True)
    print(f"stack-floor-drift: {len(rows)} row(s), {failed} failed, {notes} note(s), "
          f"{unverified} not verified (another toolchain than the pins)")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
