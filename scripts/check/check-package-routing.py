#!/usr/bin/env python3
"""RFC-0094 D3 / phase-439 W0 — the routing diff, as a gate.

RFC-0094 D3 says two questions have been conflated at three sites:

    <build_type>   -> which DRIVER builds this package, if it is built here
    file presence  -> IS it built here

Today all three sites answer both questions with file presence:

    packages/cli/nros-cli-core/src/builder/cargo_root.rs   `Cargo.toml`      -> cargo member
    packages/cli/nros-cli-core/src/builder/cmake_root.rs   `CMakeLists.txt`  -> add_subdirectory
    packages/cli/nros-cli-core/src/cmd/build.rs            `CMakeLists.txt`  -> driver choice

W0 is a TEST WRITTEN BEFORE THE FEATURE. It pushes every tracked `package.xml`
through both rules and refuses any package that changes side for a reason the
design did not name. The acceptance is not "the diff is empty" — it is "the
diff is exactly the class RFC-0094 D3 predicted, and nothing else".

## The one class that legitimately changes side

A package carrying BOTH `CMakeLists.txt` and `Cargo.toml` and declaring a cmake
build type. cmake drives it; the `Cargo.toml` is an implementation detail
INSIDE that build (a Zephyr west leaf whose staticlib corrosion compiles, or a
backend crate whose C shim is a cmake target). `cargo_root.rs` pulls it into the
generated `[workspace] members` on file presence alone, which is wrong: cargo
cannot build a west leaf, and `examples/workspaces/rust` says so in the hand-
written `exclude` that the derivation replaced.

Measured on 2026-09-08: 21 such packages, all declaring `nros_cmake`. They leave
the cargo members list and stay cmake subdirectories. That is a FIX.

## The class that must NOT change side, and is the reason routing on the
## declaration ALONE was rejected

A package that declares a build type and has NEITHER build file. 64 of them —
bringup packages (launch + `system.toml`), platform/board descriptor packages,
and ROS interface packages whose build files `nros sync` generates. They are out
of both lists today and stay out: participation is file presence, so a
declaration with no file routes nowhere instead of hard-failing.

## What this gate asserts

For every tracked `package.xml`, today's routing and D3's routing AGREE, except
for dual-file packages declaring a cmake type, which leave the cargo member list.
Plus D3's own intersection rule: a PARTICIPATING package must carry the file its
declared driver needs (a package declaring `nros_cargo` with only a
`CMakeLists.txt` is a real defect that today is silently routed to cmake).

Deliberately NOT a baseline list. A list of 21 paths would go stale the first
time a Zephyr leaf is added and would have to be edited by every author who adds
one; the rule that generated the 21 does not. `check-c-array-pool-floors` keyed
on a guard EXISTING rather than on whether it could fire, and issue 1167 landed
through it — so this gate keys on the diff's SHAPE and carries a selftest that
plants each violation and demands a red.

## Scope, stated rather than assumed

The three sites route `discovered.packages` — one workspace's walk — and each
subtracts a workspace-specific `excluded` set (west and ESP-IDF entries, entries
for other boards) that no buildless reader can reconstruct. This gate is
therefore a per-PACKAGE model of the three predicates, over every tracked
`package.xml` in the repo rather than over one workspace's discovery. That is
strictly WIDER than any single build sees: a package this gate routes may never
be discovered at all. Widening is the right direction for a rule-drift gate —
a package outside every workspace still has a declared type, and it will be
inside one the day someone points a workspace at it.

The one site-specific subtraction that IS derivable is reproduced:
`cmake_root.rs` skips interface packages (issue 0862), and so does this.

## Four readers of one table, none of them a fourth copy

The `<build_type>` vocabulary already has three readers cross-checked by
`scripts/check-build-type-spelling.py` (RFC-0087 D2). This gate imports that
gate's table readers rather than restating the rows — CLAUDE.md's recurring
class is a rule that grew a second spelling instead of a shared helper, and the
rmw parity map is what two green tools disagreeing looks like.
"""

from __future__ import annotations

import importlib.util
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]

# The two sides a package can be routed to.
CARGO_MEMBER = "cargo-member"
CMAKE_SUBDIR = "cmake-subdir"


def _load_spelling_gate():
    """Import `scripts/check-build-type-spelling.py` for its table readers.

    A hyphenated filename is not an importable module name, so this goes
    through importlib. The file's work is behind `if __name__ == "__main__"`,
    so importing it runs nothing.
    """
    path = REPO / "scripts/check-build-type-spelling.py"
    spec = importlib.util.spec_from_file_location("_nros_build_type_spelling", path)
    if spec is None or spec.loader is None:  # pragma: no cover - unreachable
        raise SystemExit(f"cannot load {path}")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


_SPELLING = _load_spelling_gate()

COMMENT_RE = _SPELLING.COMMENT_RE
BUILD_TYPE_RE = _SPELLING.BUILD_TYPE_RE

# A leaf declaring its own `[workspace]` is a workspace ROOT. Listing one as a
# member of another root is a hard cargo error, MEASURED on 1.9x against a
# synthetic two-manifest tree: `error: multiple workspace roots found in the
# same workspace`. So such a package cannot be a member at all, and its leaving
# the generated members list is a repair, not a loss.
OWN_WORKSPACE_RE = re.compile(r"^\s*\[workspace\]", re.M)
# `cargo_excluded_entry_dirs` reads this key, resolves the deploy target through
# the board catalog, and excludes the package when the driver is west. A package
# carrying it is therefore ALREADY out of the members list on the zephyr boards
# — D3 reaches the same answer from `<build_type>` instead of a Cargo.toml
# metadata round-trip through the catalog.
ENTRY_DEPLOY_RE = re.compile(
    r"\[package\.metadata\.nros\.entry\][^\[]*?^\s*deploy\s*=", re.M | re.S
)


def build_path_table(errors: list[str]) -> dict[str, str]:
    """`{raw spelling: "cargo" | "cmake"}`, cross-checked across both readers.

    Neither `build_type.rs` nor `NanoRosPackageXml.cmake` is the authority;
    their AGREEMENT is. Only the Rust table carries the build path, so that is
    where the path comes from — the cmake table is read for the cross-check,
    which is what stops this gate resolving a package differently from the
    cmake reader that will act on it.
    """
    rust = _SPELLING.rust_table((REPO / "packages/cli/nros-cli-core/src/build_type.rs").read_text())
    cm = _SPELLING.cmake_table((REPO / "cmake/NanoRosPackageXml.cmake").read_text())
    if rust is None or cm is None:
        errors.append(
            "cannot read the <build_type> table from build_type.rs / "
            "NanoRosPackageXml.cmake — RFC-0087 D2's vocabulary moved"
        )
        return {}
    errors += _SPELLING.compare_tables(rust, cm)
    return {raw: v[1] for raw, v in rust.items()}


# ---------------------------------------------------------------------------
# One package's facts
# ---------------------------------------------------------------------------
def is_interface_package(body: str, pkg_dir: Path) -> bool:
    """Mirror of `nros_cli_core::interface_package::is_interface_package`.

    An interface package has a `CMakeLists.txt` and must still not become a
    cmake subdirectory (issue 0862): its CMakeLists is verbatim upstream ROS,
    and `nros sync` routes it through codegen instead. Reproduced here because
    it is the one site-specific subtraction a buildless reader CAN make, and
    leaving it out would report 10 false side-changes.
    """
    return (
        "rosidl_interface_packages" in body
        or (pkg_dir / "msg").is_dir()
        or (pkg_dir / "srv").is_dir()
        or (pkg_dir / "action").is_dir()
    )


def read_package(rel: str, root: Path, paths: dict[str, str]) -> dict:
    """Everything the two routing rules need about one package.

    `paths` maps a raw `<build_type>` spelling to its build path. A spelling
    the table does not know (`ament_python`) resolves to `None` — "not ours to
    interpret", the same answer `build_type::canonical` gives.
    """
    pkg_dir = (root / rel).parent
    body = COMMENT_RE.sub("", (root / rel).read_text(encoding="utf-8", errors="replace"))
    declared = [b.strip() for b in BUILD_TYPE_RE.findall(body)]
    raw = declared[0] if declared else ""
    manifest = pkg_dir / "Cargo.toml"
    ct = manifest.read_text(encoding="utf-8", errors="replace") if manifest.is_file() else ""
    return {
        "rel": rel,
        "dir": pkg_dir,
        "raw": raw,
        "driver": paths.get(raw),
        "has_cargo": manifest.is_file(),
        "has_cmake": (pkg_dir / "CMakeLists.txt").is_file(),
        "interface": is_interface_package(body, pkg_dir),
        # Evidence for "fix or regression", read rather than asserted. Both are
        # reasons cargo membership is already impossible or already waived, so a
        # package carrying either LOSES nothing by leaving the members list.
        "own_workspace": OWN_WORKSPACE_RE.search(ct) is not None,
        "entry_deploy": ENTRY_DEPLOY_RE.search(ct) is not None,
    }


# ---------------------------------------------------------------------------
# The two rules
# ---------------------------------------------------------------------------
def route_today(pkg: dict) -> frozenset[str]:
    """What the three sites do now: file presence answers both questions."""
    sides = set()
    if pkg["has_cargo"]:
        sides.add(CARGO_MEMBER)
    if pkg["has_cmake"] and not pkg["interface"]:
        sides.add(CMAKE_SUBDIR)
    return frozenset(sides)


def route_proposed(pkg: dict) -> frozenset[str]:
    """RFC-0094 D3: the declaration picks the driver, files pick participation.

    An UNDECLARED package (or one declaring a build type this project does not
    define) falls back to file presence, which is today's answer. D3 changes
    what a declaration means; it does not invent one where none was written.
    """
    if not (pkg["has_cargo"] or pkg["has_cmake"]):
        return frozenset()
    driver = pkg["driver"]
    if driver is None:
        return route_today(pkg)
    sides = set()
    if driver == "cargo" and pkg["has_cargo"]:
        sides.add(CARGO_MEMBER)
    if driver == "cmake" and pkg["has_cmake"] and not pkg["interface"]:
        sides.add(CMAKE_SUBDIR)
    return frozenset(sides)


def missing_declared_file(pkg: dict) -> str | None:
    """D3's intersection rule, as a message or `None`.

    A package that PARTICIPATES (has at least one build file) must carry the
    file its declared driver needs. Today such a package is silently routed by
    whichever file it does have — the declaration is simply unread — which is
    RFC-0094's whole complaint, and W3's acceptance names this as the case that
    must become a loud error.
    """
    if not (pkg["has_cargo"] or pkg["has_cmake"]):
        return None
    if pkg["driver"] == "cargo" and not pkg["has_cargo"]:
        return "declares a cargo build type and carries no Cargo.toml"
    if pkg["driver"] == "cmake" and not pkg["has_cmake"]:
        return "declares a cmake build type and carries no CMakeLists.txt"
    return None


def classify(pkg: dict) -> tuple[str, str]:
    """`("agree" | "fix" | "violation", explanation)` for one package."""
    today, proposed = route_today(pkg), route_proposed(pkg)
    if today == proposed:
        return "agree", ""
    # The one class RFC-0094 D3 predicted and justified: a dual-file package
    # whose declared type is cmake leaves the cargo member list.
    if (
        pkg["has_cargo"]
        and pkg["has_cmake"]
        and pkg["driver"] == "cmake"
        and today == frozenset({CARGO_MEMBER, CMAKE_SUBDIR})
        and proposed == frozenset({CMAKE_SUBDIR})
    ):
        return "fix", "leaves the cargo members list; cmake drives it"
    return (
        "violation",
        f"{_sides(today)} -> {_sides(proposed)}, which RFC-0094 D3 does not name",
    )


def _sides(s: frozenset[str]) -> str:
    return "{" + ", ".join(sorted(s)) + "}" if s else "{}"


# ---------------------------------------------------------------------------
# Scanning
# ---------------------------------------------------------------------------
def tracked_package_xmls(root: Path) -> list[str]:
    """`git ls-files`, never a filesystem walk.

    A walk reaches build output, `third-party/`, and other agents' worktrees
    under `.claude/worktrees/` (issues 1157, 1166), and
    `check-no-tracked-file-find` rejects one outright — it measured 7m36s
    against 0.8s for the same paths.
    """
    out = subprocess.run(
        ["git", "ls-files", "-z", "--", "*package.xml"],
        cwd=root,
        capture_output=True,
        check=True,
    )
    return sorted(r for r in out.stdout.decode("utf-8", "replace").split("\0") if r)


def bucket(pkg: dict) -> str:
    if pkg["has_cargo"] and pkg["has_cmake"]:
        return "both"
    if pkg["has_cmake"]:
        return "cmake-only"
    if pkg["has_cargo"]:
        return "cargo-only"
    return "neither"


# ---------------------------------------------------------------------------
# Negative control, on the normal path
# ---------------------------------------------------------------------------
def self_test(quiet: bool = False) -> int:
    """Plant each violation the gate exists to catch and demand a red.

    On the normal path, not behind a flag — `check-gate-selftests` requires it,
    and issue 1167 is what a gate keyed on a guard EXISTING rather than on
    whether it can fire costs.

    Synthetic packages, not the tree: a control driven by the very tree it
    checks passes the day the tree changes, for the wrong reason.
    """

    def p(**kw):
        base = {
            "rel": "src/x/package.xml",
            "dir": Path("/nonexistent"),
            "raw": "",
            "driver": None,
            "has_cargo": False,
            "has_cmake": False,
            "interface": False,
            "own_workspace": False,
            "entry_deploy": False,
        }
        base.update(kw)
        return base

    # 1. declared cargo + has Cargo.toml -> unchanged, both rules agree
    a = p(raw="nros_cargo", driver="cargo", has_cargo=True)
    assert route_today(a) == frozenset({CARGO_MEMBER}), "cargo leaf left the cargo side"
    assert classify(a)[0] == "agree", "a plain cargo leaf must not move"
    assert missing_declared_file(a) is None

    # 2. declared cmake + has BOTH -> the named fix, and ONLY under a cmake
    #    declaration. This is the class the whole work item is about.
    b = p(raw="nros_cmake", driver="cmake", has_cargo=True, has_cmake=True)
    assert route_today(b) == frozenset({CARGO_MEMBER, CMAKE_SUBDIR})
    assert route_proposed(b) == frozenset({CMAKE_SUBDIR})
    assert classify(b)[0] == "fix", "the dual-file cmake leaf is D3's named fix"

    # 3. declared cargo + has NEITHER -> routed nowhere by both rules. The 64.
    #    Routing on the declaration ALONE would hard-fail these, which is why
    #    participation stays file presence.
    c = p(raw="nros_cargo", driver="cargo")
    assert route_today(c) == frozenset() and route_proposed(c) == frozenset()
    assert classify(c)[0] == "agree", "a declaration with no build file routes nowhere"
    assert missing_declared_file(c) is None, "a non-participant cannot fail the intersection"

    # 4. declared nothing -> falls back to file presence, so it cannot move.
    d = p(has_cargo=True, has_cmake=True)
    assert classify(d)[0] == "agree", "an undeclared package must not move"

    # 5. THE RED: a participant whose declared driver has no file. Today it is
    #    silently routed by the file it does have; W3 makes it loud.
    e = p(raw="nros_cargo", driver="cargo", has_cmake=True)
    assert missing_declared_file(e), "a cargo declaration over a cmake-only dir must fire"
    assert classify(e)[0] == "violation", "and it must also read as a side change"
    f = p(raw="nros_cmake", driver="cmake", has_cargo=True)
    assert missing_declared_file(f), "a cmake declaration over a cargo-only dir must fire"

    # 6. THE OTHER RED: a dual-file package declaring CARGO leaves the cmake
    #    subdir list. Not a class D3 named, so it must not be waved through as
    #    "a dual-file package" — the fix arm keys on the DECLARATION.
    g = p(raw="nros_cargo", driver="cargo", has_cargo=True, has_cmake=True)
    assert route_proposed(g) == frozenset({CARGO_MEMBER})
    assert classify(g)[0] == "violation", "only a CMAKE declaration is the named fix"

    # 7. An interface package is off the cmake side under BOTH rules (0862).
    h = p(raw="ament_cmake", driver="cmake", has_cmake=True, interface=True)
    assert route_today(h) == frozenset() and route_proposed(h) == frozenset()
    assert classify(h)[0] == "agree"

    if not quiet:
        print("selftest: 7 cases, each direction pinned")
    return 0


# ---------------------------------------------------------------------------
def main() -> int:
    argv = sys.argv[1:]
    if "--self-test" in argv:
        return self_test()
    self_test(quiet=True)

    errors: list[str] = []
    paths = build_path_table(errors)
    if errors:
        print("[FAIL] the <build_type> readers disagree:", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        return 1

    rows = [read_package(rel, REPO, paths) for rel in tracked_package_xmls(REPO)]

    buckets = {"both": 0, "cmake-only": 0, "cargo-only": 0, "neither": 0}
    fixes: list[tuple[dict, str]] = []
    violations: list[tuple[dict, str]] = []
    for pkg in rows:
        buckets[bucket(pkg)] += 1
        verdict, why = classify(pkg)
        # The intersection failure is the ROOT cause when both fire: "declares
        # cargo and has no Cargo.toml" tells the author what to change, where
        # "{cmake-subdir} -> {}" only tells them what happened.
        miss = missing_declared_file(pkg)
        if miss:
            violations.append((pkg, miss))
        elif verdict == "fix":
            fixes.append((pkg, why))
        elif verdict == "violation":
            violations.append((pkg, why))

    if "--report" in argv:
        return report(rows, buckets, fixes)

    print(
        f"{len(rows)} tracked package.xml: "
        + ", ".join(f"{n} {k}" for k, n in buckets.items())
    )
    print(
        f"RFC-0094 D3 routing diff: {len(fixes)} package(s) change side, "
        "all of them the dual-file cmake leaves D3 names as a fix."
    )
    if violations:
        print(
            "\n[FAIL] a package changes routing side for a reason RFC-0094 D3 "
            "does not name:",
            file=sys.stderr,
        )
        for pkg, why in violations:
            print(
                f"  - {pkg['rel']}\n"
                f"      <build_type>{pkg['raw'] or '(none)'}</build_type>, "
                f"Cargo.toml={'yes' if pkg['has_cargo'] else 'no'}, "
                f"CMakeLists.txt={'yes' if pkg['has_cmake'] else 'no'}"
                f"{', interface package' if pkg['interface'] else ''}\n"
                f"      {why}",
                file=sys.stderr,
            )
        print(
            "\n  Either the declaration is wrong for the package, or D3's rule "
            "needs a\n  class it does not have. Both are decisions; neither is "
            "a tuning knob on\n  this gate. Run with --report for the full "
            "table.",
            file=sys.stderr,
        )
        return 1
    return 0


def report(rows: list[dict], buckets: dict[str, int], fixes) -> int:
    """The human table — W0's deliverable, not the gate's verdict."""
    print(f"# RFC-0094 D3 routing diff — {len(rows)} tracked package.xml\n")
    print("## File-presence buckets\n")
    for k, n in buckets.items():
        print(f"  {k:12s} {n:4d}")

    print("\n## Declared <build_type>, per bucket\n")
    per: dict[tuple[str, str], int] = {}
    for pkg in rows:
        per[(bucket(pkg), pkg["raw"] or "(none)")] = (
            per.get((bucket(pkg), pkg["raw"] or "(none)"), 0) + 1
        )
    for b in ("both", "cmake-only", "cargo-only", "neither"):
        items = sorted((t, n) for (bb, t), n in per.items() if bb == b)
        print(f"  {b:12s} " + ", ".join(f"{t}={n}" for t, n in items))

    print(f"\n## Packages that CHANGE SIDE ({len(fixes)})\n")
    print(
        "  `own [workspace]` — cargo REFUSES to list it as a member of another\n"
        "                     root (measured: `multiple workspace roots found`),\n"
        "                     so leaving the members list repairs a broken root.\n"
        "  `entry deploy`    — `cargo_excluded_entry_dirs` already excludes it on\n"
        "                     a west board; D3 reaches the same answer from the\n"
        "                     declaration instead of Cargo.toml metadata.\n"
        "  neither           — a package that WOULD lose real membership. Read it\n"
        "                     before calling the change a fix.\n"
    )
    for pkg, why in sorted(fixes, key=lambda x: x[0]["rel"]):
        ev = [
            n
            for n, v in (("own [workspace]", pkg["own_workspace"]),
                         ("entry deploy", pkg["entry_deploy"]))
            if v
        ]
        print(f"  {pkg['raw']:12s} {pkg['rel']}")
        print(f"      {why}; {' + '.join(ev) if ev else 'NEITHER — read this one'}")

    print("\n## Declared a type, carries NEITHER build file (routed nowhere, both rules)\n")
    none_file = [p for p in rows if not p["has_cargo"] and not p["has_cmake"]]
    per_t: dict[str, list[str]] = {}
    for pkg in none_file:
        per_t.setdefault(pkg["raw"] or "(none)", []).append(pkg["rel"])
    for t in sorted(per_t):
        print(f"  {t} ({len(per_t[t])}):")
        for rel in sorted(per_t[t]):
            print(f"      {rel}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
