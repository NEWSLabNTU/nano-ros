#!/usr/bin/env python3
"""`just check max-frame` -- no single stack frame in nano-ros's own code is
over the table's `max_frame` (issue 2815).

One `sub sp` larger than an MPU stack guard (32-64 B) jumps it, so the
overflow writes past the stack with no fault. The Nucleo C talker died that
way through 9 KiB frames, and an an536 Cyclone image through 64 KiB ones
(issue 2818). `max_frame` (4096 B) is therefore a CODE-QUALITY cap, not what a
guard catches; the stack-floor walk is the protection.

There is ONE scanner. The stack-floor walk records every linked frame over the
cap in each build's report (`big_frames`), and this gate reads those reports
over the fixture builds rather than reading frames a second way. It decides
three things the walk does not:

* **Ours.** A C/C++ symbol whose first name is `nros_*` or `zpico_*`, or an
  Itanium-mangled one in a namespace `nros*` or `rclcpp`. A Rust symbol whose
  defining crate (the first crate root in the mangled name, v0 or legacy) is a
  crate under `packages/`. That set is derived from the tracked manifests,
  never listed by hand. Third-party code (zenoh-pico, Cyclone's ddsc, Zephyr,
  libc) is out of scope by construction.
* **Allowed.** A `[[frame_allow]]` row of `config/stack-floors.toml`, matched
  as a substring of the symbol, with its reason.
* **Already known.** `config/max-frame-baseline.json`, which is shrink-only: one
  row per function with the largest size any build has. Keyed by FUNCTION, not
  by build, so a build the baseline never saw (a new fixture, a gate's own
  image) passes while it adds no frame and grows none; a new one, or one that
  grew past every build's size, fails. A row may be deleted or lowered by hand,
  which only tightens the gate. Adding or raising one goes through
  `--write-baseline`, which writes it from the reports; `--merge` keeps the rows
  this tree did not see and the larger size of those it did, so the baseline
  can be built one platform family at a time.

Only 32-bit ARM fixture images are read (thumbv*, armv7*, armv8r*); RISC-V
waits for the walk to read its frames. A report without `big_frames` predates
this gate and fails, never counted as clean, and so does finding no report at
all: a gate that read nothing would pass on any tree.

  scripts/check-max-frame.py [--write-baseline [--merge]] [--self-test]
"""
from __future__ import annotations

import importlib.util
import json
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts" / "lib"))

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11
    import tomli as tomllib  # type: ignore[no-redef]

BASELINE = ROOT / "config" / "max-frame-baseline.json"
TABLE = ROOT / "config" / "stack-floors.toml"
SCAN_DIRS = ("examples", "packages/testing", "tests")
ARM32 = re.compile(r"^(thumbv|armv7|armv8r)")

_C_OWNED = re.compile(r"^(nros_|zpico_)")
# Itanium: `_ZN` / `_ZNK` (nested, const), `_ZZN` (a local static), `_ZL`
# (internal linkage) and an optional `.constprop.0`-style suffix; the first
# source-name is <len><id>.
_CXX_FIRST = re.compile(r"^_Z(?:Z?NK?|L)?(\d+)")
# Rust v0: a crate root is `C`, an optional `s<base62>_` disambiguator, then
# <len><ident>. Legacy: `_ZN<len><crate>...17h<hash>E`.
_V0_CRATE = re.compile(r"C(?:s[0-9A-Za-z]*_)?(\d+)")


# What a rebuild may change in a symbol without changing the function: a v0 crate
# disambiguator (`Cs<base62>_`), a legacy hash (`17h<16 hex>E`) and LLVM's
# ThinLTO suffix. The baseline keys on the symbol with these removed, or every
# change of flags or toolchain would read as a new frame.
#
# Two more v0 parts move with code that is not the function (B34, run-matrix
# 45533259451, where 9 of 20 "new" frames were baseline rows under another
# spelling):
# - an impl block's index in its module (`Ms<n>_`, `Xs<n>_`): adding an impl
#   block above renames every function after it (`Executor`'s
#   declare_parameter_overridden went `Msl_` -> `Msn_` at the same 8,720 B).
#   One type cannot define a method twice across its inherent impls, nor
#   implement one trait twice, so the index tells nothing apart here;
# - a back-reference (`B<base62>_`), a BYTE offset into the symbol, which
#   moves when anything before its target changes length.
_V0_DISAMBIG = re.compile(r"Cs[0-9A-Za-z]*_")
_V0_IMPL_INDEX = re.compile(r"([MX])s[0-9A-Za-z]*_")
_V0_BACKREF = re.compile(r"B[0-9A-Za-z]*_")
_LEGACY_HASH = re.compile(r"17h[0-9a-f]{16}E$")
_LLVM_SUFFIX = re.compile(r"\.llvm\.\d+$")


def stable_name(sym: str) -> str:
    """The baseline's key for a symbol: idempotent, so a stored key keys the
    same as the symbol it was made from."""
    sym = _LLVM_SUFFIX.sub("", sym)
    if sym.startswith("_R"):
        sym = _V0_DISAMBIG.sub("C", sym)
        sym = _V0_IMPL_INDEX.sub(r"\1", sym)
        return _V0_BACKREF.sub("B_", sym)
    return _LEGACY_HASH.sub("E", sym)


def image_key(r: dict) -> str:
    """Which build a frame came from, for a message: the image name and the
    report's directory (the rust, c, cpp and mixed workspaces build bringups of
    the same name for the same board)."""
    where = str(Path(r.get("_path", "?")).parent)
    return f"{r.get('image') or '?'} ({where})"


def owned_crates() -> set[str]:
    """Every crate under `packages/`, from the tracked manifests: the
    `[lib] name` if it sets one, else the package name with `-` as `_`."""
    out = subprocess.run(["git", "-C", str(ROOT), "ls-files", "packages/**/Cargo.toml",
                          "packages/*/Cargo.toml"], capture_output=True, text=True, check=True)
    crates = set()
    for rel in out.stdout.split():
        try:
            doc = tomllib.loads((ROOT / rel).read_text())
        except (OSError, ValueError):
            continue
        pkg = doc.get("package", {}).get("name")
        if not pkg:
            continue
        crates.add(pkg.replace("-", "_"))
        lib = doc.get("lib", {}).get("name")
        if lib:
            crates.add(lib)
    return crates


def _ident_at(sym: str, m: re.Match) -> str:
    n = int(m.group(1))
    return sym[m.end():m.end() + n]


def owner(sym: str, crates: set[str]) -> str | None:
    """Why `sym` is nano-ros's own code, or None when it is not."""
    if _C_OWNED.match(sym):
        return "C name"
    if sym.startswith("_R"):
        m = _V0_CRATE.search(sym)
        if m:
            crate = _ident_at(sym, m)
            return f"crate {crate}" if crate in crates else None
        return None
    m = _CXX_FIRST.match(sym)
    if m:
        first = _ident_at(sym, m)
        if first in crates:
            return f"crate {first}"
        if first == "rclcpp" or first.startswith("nros"):
            return f"C++ namespace {first}"
    return None


# Where each road writes its report: cmake and west under `<build>/nros/stack-floor/`,
# the cargo road under `<target dir>/nros-stack-floor/`.
_REPORT_DIRS = ("/nros/stack-floor", "/nros-stack-floor")


def zephyr_root() -> Path | None:
    """The checkout's Zephyr build root, from the one helper that names it: the
    west leaves build there, outside every directory SCAN_DIRS names."""
    out = subprocess.run(["bash", str(ROOT / "scripts" / "lib" / "zephyr-workspace.sh"),
                          "build-root"], capture_output=True, text=True, cwd=ROOT)
    root = Path(out.stdout.strip()) if out.returncode == 0 and out.stdout.strip() else None
    return root.parent.resolve() if root and root.parent.is_dir() else None


def find_reports() -> list[Path]:
    from repo_walk import walk
    tops = [ROOT / d for d in SCAN_DIRS]
    zroot = zephyr_root()
    if zroot:
        tops.append(zroot)
    found = set()
    for top in tops:
        if not top.is_dir():
            continue
        for dirpath, _dirs, files in walk(str(top)):
            d = dirpath.replace("\\", "/")
            if "report.json" in files and any(r in d for r in _REPORT_DIRS):
                found.add(Path(dirpath).resolve() / "report.json")
    return sorted(found)


# B34 -- which reports this run may judge. The check read every report on
# disk, and a self-hosted runner's workspace persists, so the tier-2 run that
# refused (run-matrix 45533259451) judged half its frames from builds tier 2
# never makes (another lane's fixtures, left from an earlier run). The scope
# is MEMBERSHIP, never mtime: an in-lane image that was already current is not
# relinked, so its report is older than the build and still ships. A report is
# RESIDUE only when it is proven to belong to rows outside the lane; anything
# unattributable, or attributed to an in-lane row, is checked (fail closed).
# The rows, their coordinates and roots come from the fixture manifest's own
# functions, the predicate the fixture build and the test resolver use.
STAMP = ROOT / os.environ.get("NROS_FIXTURE_STAMP", "target/nextest/.fixtures-built")
MANIFEST = ROOT / "scripts" / "build" / "fixtures-manifest.py"


def _manifest():
    spec = importlib.util.spec_from_file_location("fixtures_manifest", MANIFEST)
    assert spec is not None and spec.loader is not None
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def lane_coords(stamp_text: str | None) -> tuple[set | None, str]:
    """The coordinates the last fixture build covered, from its stamp, or None
    (scope nothing, check every report) with the reason. `lane=all`, or a
    stamp with no `lane=` (pre-0393), means everything."""
    if stamp_text is None:
        return None, (f"no fixture-build stamp at {STAMP.relative_to(ROOT) if STAMP.is_relative_to(ROOT) else STAMP}, "
                      "so no lane is known: every report is checked")
    lane = next((l[5:].strip() for l in stamp_text.splitlines() if l.startswith("lane=")),
                "all")
    coords = {tuple(l[6:].strip().split(",")) for l in stamp_text.splitlines()
              if l.startswith("coord=")}
    if lane == "all" or not coords:
        return None, f"lane `{lane}` names every coordinate: every report is checked"
    return coords, f"lane `{lane}`, {len(coords)} coordinate(s)"


def row_roots(fm, manifest: Path) -> list[tuple[str, dict]]:
    """(root, row) for every buildable row: a cargo/cmake row's artifact root
    (`row_artifact_root`), a west row's build dir under the Zephyr build root,
    spelled as `report_where` spells it (`<zephyr-builds>/<line>/<name>`, the
    line left as `*`)."""
    out = []
    for e in fm.load(str(manifest)) + fm.load_workspace_fixtures(str(manifest)):
        if e.get("skip_build"):
            continue
        if fm.row_builder(e) == "west" or e.get("west_build_name"):
            out.append((f"<zephyr-builds>/*/{fm.west_build_name(e)}", e))
        elif fm.row_artifact_root(e):
            out.append((fm.row_artifact_root(e), e))
    return out


def is_residue(where: str, roots: list[tuple[str, dict]], coords: set, in_coords) -> bool:
    """Whether the report at `where` (as `report_where` spells it) belongs only
    to rows outside `coords`. The longest root containing it decides; no root,
    or any in-lane row at that root, keeps it checked."""
    best, rows = -1, []
    parts = where.split("/")
    for root, e in roots:
        rp = root.split("/")
        if len(rp) > len(parts):
            continue
        if all(a == "*" or a == b for a, b in zip(rp, parts)):
            if len(rp) > best:
                best, rows = len(rp), [e]
            elif len(rp) == best:
                rows.append(e)
    return bool(rows) and not any(in_coords(e, coords) for e in rows)


def scope(reports: list[dict], coords: set | None, roots, in_coords
          ) -> tuple[list[dict], list[dict]]:
    """(the reports this run judges, the residue it skips)."""
    if coords is None:
        return reports, []
    keep, skip = [], []
    for r in reports:
        (skip if is_residue(r.get("_path", ""), roots, coords, in_coords) else keep).append(r)
    return keep, skip


def offenders(reports: list[dict], crates: set[str], allow: list[dict]
              ) -> tuple[dict[str, tuple[int, str]], list[str]]:
    """(the nros-owned frames over the cap that no allow row accepts, keyed by
    the function's stable name: (the largest size any build has, one build that
    has it); problems with the reports)."""
    seen: dict[str, tuple[int, str]] = {}
    problems = []
    for r in reports:
        image = image_key(r)
        if "big_frames" not in r:
            problems.append(f"{r.get('_path', image)}: the report has no `big_frames`, so "
                            "it predates this gate; rebuild the fixture")
            continue
        # B34 -- one image holds each function once, so two DIFFERENT symbols
        # of one image under one key are two functions the key would merge.
        # Refuse, naming both, rather than let one hide the other's frame.
        names: dict[str, set[str]] = {}
        for fn, n in r["big_frames"]:
            if owner(fn, crates) is None:
                continue
            if any(a["match"] in fn for a in allow):
                continue
            key = stable_name(fn)
            names.setdefault(key, set()).add(_LLVM_SUFFIX.sub("", fn))
            if int(n) > seen.get(key, (0, ""))[0]:
                seen[key] = (int(n), image)
        for key, raw in sorted(names.items()):
            if len(raw) > 1:
                problems.append(f"{image}: {len(raw)} different functions share the baseline "
                                f"key {key}: {', '.join(sorted(raw))}. The key would merge "
                                "them; make `stable_name` tell them apart")
    return seen, problems


def compare(seen: dict[str, tuple[int, str]], baseline: dict[str, int],
            cap: int) -> list[str]:
    fails = []
    for fn, (n, image) in sorted(seen.items()):
        was = baseline.get(fn)
        if was is None:
            fails.append(f"{image}: {fn}: a {n} B frame, over the {cap} B cap (new)")
        elif n > was:
            fails.append(f"{image}: {fn}: a {n} B frame, grown from {was} B (over the "
                         f"{cap} B cap)")
    return fails


def report_where(p: Path, zroot: Path | None) -> str:
    """A report's place, spelled the same on every host: under the Zephyr build
    root (which may sit outside the checkout) relative to that root, else
    relative to the checkout. It names a build in messages and `seen_in`."""
    if zroot and p.is_relative_to(zroot):
        return f"<zephyr-builds>/{p.relative_to(zroot)}"
    return str(p.relative_to(ROOT)) if p.is_relative_to(ROOT) else str(p)


def load_reports(paths: list[Path], zroot: Path | None = None) -> list[dict]:
    out = []
    for p in paths:
        try:
            doc = json.loads(p.read_text())
        except (OSError, ValueError):
            continue
        if not ARM32.match(doc.get("target") or ""):
            continue
        doc["_path"] = report_where(p, zroot)
        out.append(doc)
    return out


def read_baseline() -> dict[str, int]:
    if not BASELINE.is_file():
        return {}
    doc = json.loads(BASELINE.read_text())
    out: dict[str, int] = {}
    for r in doc.get("frames", []):
        # Rows written under an older key keep counting (stable_name is
        # idempotent); two that now share a key keep the larger size.
        k = stable_name(r["function"])
        out[k] = max(out.get(k, 0), int(r["bytes"]))
    return out


def write_baseline(seen: dict[str, tuple[int, str]], merge: bool = False) -> None:
    """Write the baseline from `seen`. With `merge`, rows this tree did not see
    are kept and a row it did see keeps the larger of the two sizes, so a
    baseline can be built one platform family at a time without one family
    lowering a size another needs."""
    rows = dict(seen)
    if merge:
        old = json.loads(BASELINE.read_text()).get("frames", []) if BASELINE.is_file() else []
        for r in old:
            fn, n = stable_name(r["function"]), int(r["bytes"])
            if n >= rows.get(fn, (0, ""))[0]:
                rows[fn] = (n, r.get("seen_in", ""))
    doc = {"schema": 2,
           "about": "issue 2815: the nros-owned stack frames over `max_frame` that exist "
                    "today, one row per function with the largest size any 32-bit ARM "
                    "fixture build has (`seen_in` names one such build), shrink-only. A "
                    "row may be DELETED or LOWERED by hand when its frame shrinks (that "
                    "only tightens the gate); a row is added or raised only by "
                    "`scripts/check-max-frame.py --write-baseline`.",
           "frames": [{"function": f, "bytes": n, "seen_in": w}
                      for f, (n, w) in sorted(rows.items())]}
    BASELINE.write_text(json.dumps(doc, indent=2) + "\n")


def self_test(quiet: bool = False) -> int:
    crates = {"nros_node", "nros_rmw_cffi"}
    cases = [
        ("_RNvMsh_NtNtCsjR1TXmGj4Tx_9nros_node8executor4spinNtB5_8Executor4pass", True),
        ("_RINvNtNtCs1mC7cpYT4Tw_17compiler_builtins5float3cmp3cmpC3f16EB6_", False),
        ("_ZN9nros_node8executor4pass17h0123456789abcdefE", True),
        ("_ZN19nros_rmw_cyclonedds21service_send_responseEPK13rmw_service_tx15rmw_byte_span_t",
         True),
        ("_ZN19nros_rmw_cycloneddsL24service_take_request_lenEPK13rmw_service_t.constprop.0",
         True),
        ("_ZN6rclcpp4Node17create_wall_timerEv", True),
        ("nros_cpp_publish_raw", True),
        ("zpico_spin_once", True),
        ("dds_write", False),
        ("_z_session_open", False),
        ("_ZN4core3fmt5write17h0123456789abcdefE", False),
    ]
    fails = [s for s, want in cases if (owner(s, crates) is not None) != want]
    reports = [{"image": "a@b", "target": "thumbv7m-none-eabi", "big_frames": [
        ["nros_big", 9000], ["dds_big", 70000], ["nros_bind_subscription_x", 16384]]}]
    allow = [{"match": "bind_subscription", "reason": "user M"}]
    seen, probs = offenders(reports, crates, allow)
    k = "nros_big"
    if {f: n for f, (n, _) in seen.items()} != {k: 9000} or probs:
        fails.append("offenders: ours, not third-party, not allowed")
    if not compare(seen, {}, 4096):
        fails.append("a new frame over the cap fails")
    if compare(seen, {k: 9000}, 4096):
        fails.append("a frame at its baseline passes")
    if compare(seen, {k: 9500}, 4096):
        fails.append("a frame that shrank passes")
    if not compare(seen, {k: 8000}, 4096):
        fails.append("a frame that grew fails")
    a = "_RNvMsh_NtNtCsjR1TXmGj4Tx_9nros_node8executor4spinNtB5_8Executor4pass"
    b = "_RNvMsh_NtNtCs4xKnRAC4kOP_9nros_node8executor4spinNtB5_8Executor4pass.llvm.123"
    if stable_name(a) != stable_name(b):
        fails.append("a crate disambiguator or LLVM suffix does not change the key")
    # B34 -- an impl block's index and a back-reference move with unrelated
    # code; the function does not.
    if stable_name("_RNvMsl_NtNtC9nros_node8executor4spinNtB5_8Executor28declare_parameter_overridden") \
            != stable_name("_RNvMsn_NtNtC9nros_node8executor4spinNtB6_8Executor28declare_parameter_overridden"):
        fails.append("an impl index or a back-reference does not change the key")
    if stable_name("_RNvMs_NtNtC4nros4spinNtB4_8Executor4pass") == \
            stable_name("_RNvMs_NtNtC4nros4spinNtB4_8Executor5spin2"):
        fails.append("two different functions keep different keys")
    # 243's controls: normalising never hides a NEW frame, and never merges two
    # functions of one image.
    fresh = "_RNvMs3_NtNtC9nros_node8executor4spinNtB5_8Executor20brand_new_big_function"
    s_new, _ = offenders([{"image": "n@b", "big_frames": [[fresh, 6000]]}], crates, allow)
    if not compare(s_new, {stable_name(a): 9000}, 4096):
        fails.append("a NEW frame under a fresh name still fails after normalising")
    clash = [{"image": "c@b", "big_frames": [
        ["_RNvMs1_NtC9nros_node4spinNtB2_4Exec3run", 5000],
        ["_RNvMs2_NtC9nros_node4spinNtB3_4Exec3run", 6000]]}]
    _, probs = offenders(clash, crates, allow)
    if not any("different functions share the baseline key" in p for p in probs):
        fails.append("two functions of one image under one key are refused, by name")
    # B34 scoping, both directions. Roots and the lane are given here; the
    # manifest's own `row_in_coords` is the predicate.
    fm = _manifest()
    roots = [("packages/testing/x/target-freertos", {"platform": "freertos", "lang": "rust"}),
             ("<zephyr-builds>/*/build-lc", {"platform": "zephyr-cortex-m", "lang": "c"}),
             ("examples/shared/target", {"platform": "freertos", "lang": "rust"}),
             ("examples/shared/target", {"platform": "nuttx", "lang": "rust"})]
    lane = {("nuttx", "rust", "zenoh"), ("zephyr-cortex-m", "c", "zenoh")}
    big = [[fresh, 6000]]
    stale_out = {"image": "o@b", "_path": "packages/testing/x/target-freertos/nros-stack-floor/report.json",
                 "big_frames": big}
    old_in = {"image": "i@b", "_path": "<zephyr-builds>/3.7/build-lc/nros/stack-floor/report.json",
              "big_frames": big, "_mtime": 0}
    unknown = {"image": "u@b", "_path": "somewhere/else/nros-stack-floor/report.json",
               "big_frames": big}
    shared = {"image": "s@b", "_path": "examples/shared/target/nros-stack-floor/report.json",
              "big_frames": big}
    keep, skip = scope([stale_out, old_in, unknown, shared], lane, roots, fm.row_in_coords)
    if stale_out not in skip:
        fails.append("a report of an out-of-lane row is residue, skipped")
    if old_in not in keep or not compare(offenders([old_in], crates, allow)[0], {}, 4096):
        fails.append("an in-lane report is checked whatever its age, and fails over the cap")
    if unknown not in keep:
        fails.append("an unattributable report is checked (fail closed)")
    if shared not in keep:
        fails.append("a root shared with an in-lane row is checked")
    if scope([stale_out], None, roots, fm.row_in_coords)[0] != [stale_out]:
        fails.append("with no lane known, every report is checked")
    if lane_coords(None)[0] is not None or lane_coords("lane=all\n")[0] is not None \
            or lane_coords("lane=tier2\ncoord=a,b,c\n")[0] != {("a", "b", "c")}:
        fails.append("the stamp's lane is read: missing or `all` scopes nothing")
    k2 = stable_name(a)
    if stable_name(k2) != k2:
        fails.append("the key is idempotent, so a stored key reads as itself")
    if stable_name("_ZN9nros_node4pass17h0123456789abcdefE") != \
            stable_name("_ZN9nros_node4pass17hfedcba9876543210E"):
        fails.append("a legacy hash does not change the key")
    two = [{"image": "x@b", "_path": p, "big_frames": [["nros_big", n]]}
           for p, n in (("examples/workspaces/c/build/nros/stack-floor/x/report.json", 9000),
                        ("examples/workspaces/cpp/build/nros/stack-floor/x/report.json", 9200))]
    got = offenders(two, crates, allow)[0]
    if got.get("nros_big", (0, ""))[0] != 9200 or "cpp" not in got["nros_big"][1]:
        fails.append("one row per function, at the largest size any build has")
    _, probs = offenders([{"image": "old@b"}], crates, allow)
    if not probs:
        fails.append("a report without big_frames is a problem, never clean")
    for f in fails:
        print(f"check-max-frame self-test FAIL: {f}", file=sys.stderr)
    if not quiet or fails:
        print(f"check-max-frame self-test: {len(cases) + 20} checks, {len(fails)} failed")
    return 1 if fails else 0


def main(argv: list[str]) -> int:
    if "--self-test" in argv:
        return self_test()
    # Always, not only behind the flag: a negative control nobody runs decays
    # into a comment.
    if self_test(quiet=True):
        return 1
    table = tomllib.loads(TABLE.read_text())
    cap = int(table["max_frame"])
    allow = table.get("frame_allow", [])
    paths = find_reports()
    reports = load_reports(paths, zephyr_root())
    stamp_text = STAMP.read_text() if STAMP.is_file() else None
    coords, why = lane_coords(stamp_text)
    fm = _manifest()
    reports, residue = scope(reports, coords, row_roots(fm, ROOT / fm.DEFAULT_MANIFEST),
                             fm.row_in_coords)
    print(f"check-max-frame: {why}; {len(reports)} report(s) judged, {len(residue)} "
          "skipped as residue of rows outside the lane")
    for r in residue:
        print(f"  skipped (residue): {r.get('_path')}")
    if not reports:
        print("check-max-frame: FAIL -- no 32-bit ARM fixture build has a stack-floor "
              f"report under {', '.join(SCAN_DIRS)} ({len(paths)} report(s) in all). Build "
              "the fixtures first (`just build-test-fixtures`); a gate that read nothing "
              "would pass on any tree.", file=sys.stderr)
        return 1
    seen, problems = offenders(reports, owned_crates(), allow)
    if "--write-baseline" in argv:
        if problems:
            print("check-max-frame: not writing the baseline:\n  " + "\n  ".join(problems),
                  file=sys.stderr)
            return 1
        write_baseline(seen, merge="--merge" in argv)
        print(f"check-max-frame: wrote {BASELINE.relative_to(ROOT)}: {len(seen)} frame(s) "
              f"over {cap} B from {len(reports)} image report(s)")
        return 0
    fails = problems + compare(seen, read_baseline(), cap)
    if fails:
        print(f"check-max-frame: FAIL -- {len(fails)} frame(s) in nano-ros's own code over "
              f"the {cap} B cap (config/stack-floors.toml `max_frame`):\n  "
              + "\n  ".join(fails)
              + "\nShrink the frame (move a large local off the stack), or, for a frame "
              "whose size is not ours, add a reasoned [[frame_allow]] row. A frame that "
              "shrank needs no change here.", file=sys.stderr)
        return 1
    print(f"check-max-frame: OK -- {len(reports)} 32-bit ARM image report(s), every "
          f"nros-owned frame over {cap} B is in the shrink-only baseline")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
