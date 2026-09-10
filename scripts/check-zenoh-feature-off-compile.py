#!/usr/bin/env python3
"""zenoh-pico's portable sources must COMPILE in the feature set nano-ros ships.

Issue 1021. zenoh-pico 1.8.0 did not compile with `Z_FEATURE_MATCHING=0`:
`_z_write_filter_clear` called `_z_write_filter_ctx_remove_callbacks`, which is
declared and defined only under `#if Z_FEATURE_MATCHING`. The Zephyr lane passes
exactly that define (`zephyr/cmake/nros_rmw_zenoh.cmake`), so every Zephyr zenoh
image broke the moment phase-415 moved the patch line to 1.8.0 — and nothing
short of a Zephyr build could see it, because every other lane compiles the
library with MATCHING=1 and the unguarded call resolves there.

The fix is carried on the fork's `nano-ros` line (`0343ad1b`), and upstream
guarded the same call in `07c84ebc` (#1225), which is in 1.10.0. What was
missing is anything on OUR side that would notice the next such break before a
Zephyr build does. This is that: a host `-fsyntax-only` pass over every TU the
shared manifest compiles unconditionally, in each configuration the Zephyr lane
really uses. Measured 2026-09-11: it fails on the tree just before `0343ad1b`
with the issue's exact diagnostic, and passes at the pin.

WHAT IT COMPILES

  * TUs: the manifest's `always` groups (`zpico-sys/zenoh-sources.txt`, read
    with `check-zenoh-source-manifest`'s own parser). Per-platform `system/*`
    trees are out of scope — they need the platform's headers, and a feature
    guard in PORTABLE code is the class this gate is about. The host platform
    header set (`ZENOH_LINUX`) stands in for the target's; no portable TU
    branches on it for anything but the platform typedefs.
  * Configurations: DERIVED from the Zephyr lane, never restated. Every literal
    `Z_FEATURE_<X>=<n>` in a `zephyr_compile_definitions(...)` call is read; a
    knob set to more than one value (`Z_FEATURE_TX_SPLIT_LOCK`, behind a Kconfig
    `if`) yields one configuration per value. Every other knob keeps
    `config.h`'s `#ifndef` default, which is what the Zephyr build gets too.
    `Z_FEATURE_MATCHING=0` is also REQUIRED to be covered, so moving the Zephyr
    line off it cannot quietly retire this issue's coverage.
  * Flags: `-Werror=implicit-function-declaration`, because GCC before 14 only
    warns on it — a gate that passes on the implicit declaration it exists to
    catch is worse than none. The self-test proves the compiler in use refuses.

WHAT IT DOES NOT COMPILE, measured and deliberately left out

  Both UDP links off with scouting ON fails in `session/scout.c` (`UDP_SCHEMA`
  undeclared) at the pin AND in upstream 1.10.0. No nano-ros build reaches it:
  the cargo lane ties `Z_FEATURE_SCOUTING` to `multicast_transport_flag()`
  (off, issue 0682), and the Zephyr lane keeps UDP on. Recorded in issue 1021.

Exit 78 = cannot run here (no zenoh-pico source, or no C compiler); the just
recipe turns that into a `nros_check_skip` entry instead of an OK.

Run:  python3 scripts/check-zenoh-feature-off-compile.py [--self-test]
"""

from __future__ import annotations

import importlib.util
import itertools
import os
import re
import shutil
import subprocess
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CMAKE = ROOT / "zephyr/cmake/nros_rmw_zenoh.cmake"
SKIP = 78

# Issue 1021's axis. Must be a subset of at least one compiled configuration.
REQUIRED_AXIS = {"Z_FEATURE_MATCHING": "0"}

FLAGS = [
    "-std=gnu11",
    "-fsyntax-only",
    "-Werror=implicit-function-declaration",
    "-DZENOH_LINUX",
]

_CALL = re.compile(r"zephyr_compile_definitions\s*\(([^)]*)\)")
_DEF = re.compile(r"\bZ_FEATURE_([A-Z0-9_]+)=([0-9]+)\b")


def _manifest_module():
    path = ROOT / "scripts" / "check-zenoh-source-manifest.py"
    spec = importlib.util.spec_from_file_location("zenoh_source_manifest", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


MANIFEST_MOD = _manifest_module()
SRC_ROOT = ROOT / MANIFEST_MOD.TREES["zenoh_pico"]
INCLUDE = SRC_ROOT.parent / "include"
# A FILE, not a directory: an initialised-but-empty submodule dir is the state
# that makes a directory probe lie.
PROBE = INCLUDE / "zenoh-pico" / "config.h"


def strip_cmake_comments(text: str) -> str:
    return "\n".join(line.split("#", 1)[0] for line in text.splitlines())


def derive_configs(cmake_text: str) -> list[dict[str, str]]:
    """Every Z_FEATURE value the Zephyr lane can pass, as configurations."""
    seen: dict[str, set[str]] = {}
    for call in _CALL.finditer(strip_cmake_comments(cmake_text)):
        for name, value in _DEF.findall(call.group(1)):
            seen.setdefault(f"Z_FEATURE_{name}", set()).add(value)
    if not seen:
        return []
    keys = sorted(seen)
    return [
        dict(zip(keys, combo))
        for combo in itertools.product(*(sorted(seen[k]) for k in keys))
    ]


def with_required_axis(configs: list[dict[str, str]]) -> list[dict[str, str]]:
    if any(all(c.get(k) == v for k, v in REQUIRED_AXIS.items()) for c in configs):
        return configs
    return configs + [dict(REQUIRED_AXIS)]


def portable_tus() -> list[Path]:
    groups, rows = MANIFEST_MOD.parse_manifest(
        MANIFEST_MOD.MANIFEST.read_text(encoding="utf-8")
    )
    out: set[Path] = set()
    for kind, group, tree, rel in rows:
        if groups.get(group) != MANIFEST_MOD.UNCONDITIONAL or tree != "zenoh_pico":
            continue
        p = SRC_ROOT / rel
        # The manifest's own `dir` rule ("every .c under <path>, recursively"),
        # expanded the way both lanes expand it. The tree is the vendored
        # submodule, whose files are in no index this repo's `git ls-files` reads.
        out.update(p.rglob("*.c") if kind == "dir" else [p])  # walk-ok: vendored submodule tree, not repo-tracked files; the manifest's `dir` rule is recursive
    return sorted(out)


def compiler() -> str | None:
    for cc in (os.environ.get("CC"), "gcc", "cc"):
        if cc and shutil.which(cc):
            return cc
    return None


def compile_tu(cc: str, tu: Path, config: dict[str, str]) -> tuple[bool, str]:
    cmd = [cc, *FLAGS, *(f"-D{k}={v}" for k, v in sorted(config.items()))]
    cmd += ["-I", str(INCLUDE), str(tu)]
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
    except (OSError, subprocess.SubprocessError) as exc:
        return False, f"could not run {cc}: {exc}"
    return r.returncode == 0, r.stderr


def first_errors(stderr: str, n: int = 3) -> str:
    lines = [l for l in stderr.splitlines() if "error" in l]
    return "\n".join(f"        {l}" for l in lines[:n]) or "        (no error line)"


# ---------------------------------------------------------------------------


def self_test(cc: str | None) -> list[str]:
    bad: list[str] = []

    # Derivation: comments ignored, a multi-valued knob fans out, and the
    # required axis is added only when nothing covers it.
    cm = (
        "zephyr_compile_definitions(Z_FEATURE_INTEREST=1 Z_FEATURE_MATCHING=0)\n"
        "if(X)\n  zephyr_compile_definitions(Z_FEATURE_TX_SPLIT_LOCK=1)\n"
        "else()\n  zephyr_compile_definitions(Z_FEATURE_TX_SPLIT_LOCK=0)\nendif()\n"
        "# zephyr_compile_definitions(Z_FEATURE_BATCHING=1)\n"
        "zephyr_compile_definitions(ZENOH_ZEPHYR)\n"
    )
    got = derive_configs(cm)
    if len(got) != 2 or any(c.get("Z_FEATURE_MATCHING") != "0" for c in got):
        bad.append(f"derive_configs fanned out wrong: {got}")
    if any("Z_FEATURE_BATCHING" in c for c in got):
        bad.append("derive_configs read a commented-out definition")
    if with_required_axis(got) != got:
        bad.append("the required axis was added although a configuration covers it")
    if with_required_axis([{"Z_FEATURE_MATCHING": "1"}])[-1] != REQUIRED_AXIS:
        bad.append("the required axis was not added when nothing covers it")
    if derive_configs("zephyr_compile_definitions(ZENOH_ZEPHYR)\n"):
        bad.append("derive_configs invented a configuration from no Z_FEATURE define")

    if cc is None:
        return bad

    # The flags must make an implicit declaration an ERROR on this compiler.
    with tempfile.TemporaryDirectory() as d:
        undecl = Path(d) / "undecl.c"
        undecl.write_text("void f(void) { g(); }\n")
        decl = Path(d) / "decl.c"
        decl.write_text("int g(void);\nvoid f(void) { g(); }\n")
        if compile_tu(cc, undecl, {})[0]:
            bad.append(f"{cc} accepted an implicit declaration under {FLAGS}")
        ok, err = compile_tu(cc, decl, {})
        if not ok:
            bad.append(f"{cc} rejected a well-formed TU:\n{err}")

    if not PROBE.is_file():
        return bad

    # The real mutation: undo 0343ad1b and require the issue's diagnostic.
    target = SRC_ROOT / "net" / "filtering.c"
    guarded = re.compile(
        r"#if Z_FEATURE_MATCHING == 1\n"
        r"([ \t]*_z_write_filter_ctx_remove_callbacks\([^\n]*\n)"
        r"#endif\n"
    )
    text = target.read_text(encoding="utf-8") if target.is_file() else ""
    mutated, n = guarded.subn(r"\1", text)
    if n != 1:
        bad.append(
            f"{target.relative_to(ROOT)}: the guarded call issue 1021 is about was not "
            f"found exactly once ({n}), so the negative control below cannot run.\n"
            "      If upstream restructured it, point the mutation at the new shape — "
            "never drop it."
        )
        return bad
    with tempfile.TemporaryDirectory() as d:
        m = Path(d) / "filtering.c"
        m.write_text(mutated, encoding="utf-8")
        ok, err = compile_tu(cc, m, REQUIRED_AXIS)
        if ok or "_z_write_filter_ctx_remove_callbacks" not in err:
            bad.append(
                "negative control: filtering.c with 0343ad1b's guard removed was NOT "
                f"rejected under {REQUIRED_AXIS} — this gate would miss issue 1021.\n"
                + first_errors(err)
            )
    return bad


def main(argv: list[str]) -> int:
    cc = compiler()
    problems = self_test(cc)
    if problems:
        print("check-zenoh-feature-off-compile: SELF-TEST FAILED", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        return 1
    if "--self-test" in argv:
        print("check-zenoh-feature-off-compile: self-test OK")
        return 0

    if cc is None:
        print("no C compiler (gcc/cc, or $CC) on PATH")
        return SKIP
    if not PROBE.is_file():
        print(
            "zenoh-pico submodule not checked out "
            "(git submodule update --init packages/rmw/zenoh/zpico-sys/zenoh-pico)"
        )
        return SKIP

    configs = with_required_axis(derive_configs(CMAKE.read_text(encoding="utf-8")))
    if configs == [REQUIRED_AXIS]:
        print(
            f"check-zenoh-feature-off-compile: read NO Z_FEATURE definition from "
            f"{CMAKE.relative_to(ROOT)} — the reader has drifted from the lane.",
            file=sys.stderr,
        )
        return 1
    tus = portable_tus()
    if not tus:
        print("check-zenoh-feature-off-compile: the manifest selected no TU", file=sys.stderr)
        return 1

    jobs = [(tu, cfg) for cfg in configs for tu in tus]
    workers = max(1, min(4, os.cpu_count() or 1))
    with ThreadPoolExecutor(max_workers=workers) as pool:
        results = list(pool.map(lambda j: compile_tu(cc, *j), jobs))

    failed = [(tu, cfg, err) for (tu, cfg), (ok, err) in zip(jobs, results) if not ok]
    for tu, cfg, err in failed:
        flags = " ".join(f"-D{k}={v}" for k, v in sorted(cfg.items()))
        print(f"FAIL {tu.relative_to(ROOT)}\n      with {flags}\n{first_errors(err)}")
    summary = (
        f"{len(tus)} portable TU(s) x {len(configs)} configuration(s) from "
        f"{CMAKE.relative_to(ROOT)}"
    )
    if failed:
        print(
            f"check-zenoh-feature-off-compile: {len(failed)} compile(s) FAILED over "
            f"{summary}.\n  A feature-guard break in zenoh-pico is fixed on the fork's "
            "patch line (issue 1021), never by turning the feature back on here."
        )
        return 1
    print(f"check-zenoh-feature-off-compile: OK — {summary}.")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
