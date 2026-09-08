#!/usr/bin/env python3
"""RFC-0094 D3 / phase-439 W0+W3 — the routing diff, as a gate.

RFC-0094 D3 says two questions have been conflated at three sites:

    <build_type>   -> which DRIVER builds this package, if it is built here
    file presence  -> IS it built here

Until phase-439 W3 all three sites answered both questions with file presence:

    packages/cli/nros-cli-core/src/builder/cargo_root.rs   `Cargo.toml`      -> cargo member
    packages/cli/nros-cli-core/src/builder/cmake_root.rs   `CMakeLists.txt`  -> add_subdirectory
    packages/cli/nros-cli-core/src/cmd/build.rs            `CMakeLists.txt`  -> driver choice

All three now route through `nros_cli_core::routing`, which reads the
declaration. `SITE_CONTRACT` below is what keeps that true.

W0 was a TEST WRITTEN BEFORE THE FEATURE: it pushed every tracked `package.xml`
through both rules and refused any package that changed side for a reason the
design did not name. Measured 2026-09-08, that diff was **21 packages**, all of
one class.

**W3 landed the feature, so the diff is now EMPTY and this gate's job changed.**
The three sites read `<build_type>` through one helper,
`nros_cli_core::routing`, and the file-presence rule survives here only as the
`--report` column that says what moved and why. What the gate ASSERTS now is
three things, in increasing order of how easily they rot:

1. **The intersection rule, live over the real tree.** A PARTICIPATING package
   must carry the file its declared driver needs. Zero offenders today; the
   selftest plants one in each direction.
2. **The sites still read the declaration.** Three greps, because a python model
   of Rust code is worth nothing unless something ties it to the Rust. Delete
   the `routing::route` call from any of the three, or the `check_declarations`
   call that makes a misdeclaration loud, and this reds.
3. **The historical diff is still exactly D3's named class.** A package that
   would move for a reason the design did not name is still refused — which is
   what stops someone "fixing" a routing surprise by giving one package a
   declaration that contradicts its files.

## The one class that legitimately changes side

A package carrying BOTH `CMakeLists.txt` and `Cargo.toml` and declaring a cmake
build type. cmake drives it; the `Cargo.toml` is an implementation detail
INSIDE that build (a Zephyr west leaf whose staticlib corrosion compiles, or a
backend crate whose C shim is a cmake target). `cargo_root.rs` pulls it into the
generated `[workspace] members` on file presence alone, which is wrong: cargo
cannot build a west leaf, and `examples/workspaces/rust` says so in the hand-
written `exclude` that the derivation replaced.

Measured on 2026-09-08: 21 such packages, all declaring `nros_cmake`. They left
the cargo members list and stayed cmake subdirectories when W3 landed. That is a
FIX, and the one with observable consequences is
`examples/workspaces/mixed/src/rust_heartbeat_pkg` — a cmake-driven Rust node
carrying its own `[workspace]`, which made the generated cargo root for `mixed`
unreadable by cargo.

## The class that must NOT change side, and is the reason routing on the
## declaration ALONE was rejected

A package that declares a build type and has NEITHER build file. 64 of them —
bringup packages (launch + `system.toml`), platform/board descriptor packages,
and ROS interface packages whose build files `nros sync` generates. They are out
of both lists today and stay out: participation is file presence, so a
declaration with no file routes nowhere instead of hard-failing.

## What "the diff is empty" means, precisely

There are three rules in play and only two of them are code:

    route_file_presence   the PRE-W3 rule. Not implemented anywhere any more;
                          kept so `--report` can show what W3 moved.
    route_declaration     RFC-0094 D3. This IS what the three sites do, which
                          is what assertion 2 above ties down.

W3's acceptance is `route_declaration == what the code does`, and a python
function cannot prove that about Rust. So the proof is split: the SHAPE check
here says the sites call the one helper, and the helper's own unit tests
(`nros_cli_core::routing`, twelve of them, both directions) say what it does.
Neither half alone is worth anything; a gate claiming otherwise would be the
"guard that exists but cannot fire" issue 1167 is about.

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
def route_file_presence(pkg: dict) -> frozenset[str]:
    """The PRE-W3 rule: file presence answered both questions.

    Nothing implements this any more. It survives so `--report` can name what
    W3 moved, and so the "changes side for an unnamed reason" refusal keeps
    working on packages added after W3.
    """
    sides = set()
    if pkg["has_cargo"]:
        sides.add(CARGO_MEMBER)
    if pkg["has_cmake"] and not pkg["interface"]:
        sides.add(CMAKE_SUBDIR)
    return frozenset(sides)


def route_declaration(pkg: dict) -> frozenset[str]:
    """RFC-0094 D3: the declaration picks the driver, files pick participation.

    This is what `nros_cli_core::routing::route` does, and `SITE_CONTRACT`
    below is what says the three sites still call it.

    An UNDECLARED package (or one declaring a build type this project does not
    define) falls back to file presence. D3 changes what a declaration means;
    it does not invent one where none was written, and `ament_python` is not
    ours to interpret.
    """
    if not (pkg["has_cargo"] or pkg["has_cmake"]):
        return frozenset()
    driver = pkg["driver"]
    if driver is None:
        return route_file_presence(pkg)
    sides = set()
    if driver == "cargo" and pkg["has_cargo"]:
        sides.add(CARGO_MEMBER)
    if driver == "cmake" and pkg["has_cmake"] and not pkg["interface"]:
        sides.add(CMAKE_SUBDIR)
    return frozenset(sides)


# ---------------------------------------------------------------------------
# What ties this python model to the Rust that actually routes (phase-439 W3)
# ---------------------------------------------------------------------------
# One helper owns D3's rule, and the three sites call it. Written as a contract
# rather than checked by re-implementing the Rust, because the second
# implementation is the failure mode: CLAUDE.md's rmw parity map is two green
# tools disagreeing by 25 symbols, each confident it had read the other.
#
# `must_call` is the routing predicate; `why` is what the site would silently
# get wrong without it. `cmd/build.rs` additionally has to make a
# misdeclaration LOUD, which is the acceptance sentence of the work item.
SITE_CONTRACT = {
    "packages/cli/nros-cli-core/src/builder/cargo_root.rs": [
        ("routing::route(", "the [workspace] members list would key on Cargo.toml presence"),
        ("routing::check_declarations(", "a misdeclared participant would drop out silently"),
    ],
    "packages/cli/nros-cli-core/src/builder/cmake_root.rs": [
        ("routing::route(", "add_subdirectory() would key on CMakeLists.txt presence"),
        ("routing::check_declarations(", "a misdeclared participant would drop out silently"),
    ],
    "packages/cli/nros-cli-core/src/cmd/build.rs": [
        ("routing::route(", "the cargo-vs-cmake driver choice would key on file presence"),
        ("routing::check_declarations(", "W3's acceptance: the error must be loud and name it"),
    ],
    # The rule itself has exactly one home. If this file stops resolving the
    # build-type table, the three call sites above are calling something else.
    "packages/cli/nros-cli-core/src/routing.rs": [
        ("build_type::{BuildPath, canonical}", "the one reader of the <build_type> vocabulary"),
        ("pub fn route(", "the predicate the three sites share"),
    ],
}


# The other half of the contract, and the half that was MEASURED to be needed.
#
# A `must_call` needle alone is defeated by reverting the routing predicate
# while leaving any other call to the helper in the file — demonstrated on this
# tree: putting `pkg.dir.join("Cargo.toml").is_file()` back as the member test
# in `cargo_root.rs` left that file's OTHER `routing::route(p)` call (the
# `exclude` derivation) in place, and this gate stayed GREEN while
# `a_dual_file_package_declaring_cmake_is_not_a_member` went red in 0.14 s. A
# gate that cannot fail its own mutation is issue 1167.
#
# So in the two EMITTERS, where the retired predicate lived, a build-file probe
# must say why it is not a routing decision. Both probes are listed rather than
# one, because the defect was symmetric.
#
# `cmd/build.rs` is deliberately NOT in this table: it holds five legitimate
# probes answering other questions (does a hand-written entry package exist,
# which ancestor holds a manifest, where is the west application), and
# annotating those would be noise that trains a reader to ignore the marker.
# Its retired shape is caught by the exact-line ban below instead.
FILE_PROBES = ('join("Cargo.toml").is_file()', 'join("CMakeLists.txt").is_file()')
PROBE_EXEMPT = "nros-routing-exempt:"
# How far above a probe the marker may sit. Three, so a two-line justification
# reads as prose above the code rather than a trailing comment rustfmt owns.
PROBE_EXEMPT_LOOKBEHIND = 3
PROBE_BANNED_IN = (
    "packages/cli/nros-cli-core/src/builder/cargo_root.rs",
    "packages/cli/nros-cli-core/src/builder/cmake_root.rs",
)

# The exact pre-W3 spelling at the third site. A rename defeats this, and that
# is STATED rather than hidden: the primary evidence for what the three sites
# do is `nros_cli_core`'s own unit tests. This table is the drift tripwire.
BANNED_LINES = {
    "packages/cli/nros-cli-core/src/cmd/build.rs": [
        (
            '.filter(|p| p.dir.join("CMakeLists.txt").is_file())',
            "the cargo-vs-cmake driver choice, back on file presence",
        ),
    ],
}


def _code_lines(body: str) -> list[str]:
    """Each line with its `//` comment stripped.

    The retired predicates are QUOTED in the comments that explain why they
    went, so a scanner that read comments would refuse the explanation of its
    own rule. The exemption marker is read from the RAW text instead.
    """
    return [line.split("//", 1)[0] for line in body.splitlines()]


def check_sites(read: "callable[[str], str | None]") -> list[str]:
    """Assertion 2: the Rust still routes on the declaration.

    `read` is injected so the selftest can run this against synthetic sources —
    a control driven by the tree it checks passes the day the tree changes, for
    the wrong reason.
    """
    out: list[str] = []
    for rel, needles in SITE_CONTRACT.items():
        body = read(rel)
        if body is None:
            out.append(f"{rel}: gone — RFC-0094 D3's routing has no home there any more")
            continue
        for needle, why in needles:
            if needle not in body:
                out.append(f"{rel}: no `{needle}` — {why}")

        raw = body.splitlines()
        code = _code_lines(body)
        if rel in PROBE_BANNED_IN:
            for i, line in enumerate(code):
                if not any(probe in line for probe in FILE_PROBES):
                    continue
                window = raw[max(0, i - PROBE_EXEMPT_LOOKBEHIND) : i + 1]
                if any(PROBE_EXEMPT in w for w in window):
                    continue
                out.append(
                    f"{rel}:{i + 1}: a build-file probe in an emitter that routes "
                    f"on the DECLARATION (RFC-0094 D3). If it is not a routing "
                    f"decision, say so with `{PROBE_EXEMPT} <reason>` within "
                    f"{PROBE_EXEMPT_LOOKBEHIND} lines above it."
                )
        for banned, why in BANNED_LINES.get(rel, []):
            for i, line in enumerate(code):
                if banned in line:
                    out.append(f"{rel}:{i + 1}: `{banned}` is the pre-W3 predicate — {why}")
    return out


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
    """`("agree" | "fix" | "violation", explanation)` for one package.

    "fix" means "W3 MOVED this one" — it is history now, reported and not
    failed. "violation" is a package whose declaration and files disagree in a
    way RFC-0094 D3 never named, and it still fails.
    """
    today, proposed = route_file_presence(pkg), route_declaration(pkg)
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
    assert route_declaration(a) == frozenset({CARGO_MEMBER}), "cargo leaf left the cargo side"
    assert classify(a)[0] == "agree", "a plain cargo leaf must not move"
    assert missing_declared_file(a) is None

    # 2. declared cmake + has BOTH -> the named fix, and ONLY under a cmake
    #    declaration. This is the class the whole work item is about.
    b = p(raw="nros_cmake", driver="cmake", has_cargo=True, has_cmake=True)
    assert route_file_presence(b) == frozenset({CARGO_MEMBER, CMAKE_SUBDIR})
    assert route_declaration(b) == frozenset({CMAKE_SUBDIR})
    assert classify(b)[0] == "fix", "the dual-file cmake leaf is D3's named fix"

    # 3. declared cargo + has NEITHER -> routed nowhere by both rules. The 64.
    #    Routing on the declaration ALONE would hard-fail these, which is why
    #    participation stays file presence.
    c = p(raw="nros_cargo", driver="cargo")
    assert route_file_presence(c) == frozenset() and route_declaration(c) == frozenset()
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
    assert route_declaration(g) == frozenset({CARGO_MEMBER})
    assert classify(g)[0] == "violation", "only a CMAKE declaration is the named fix"

    # 7. An interface package is off the cmake side under BOTH rules (0862).
    h = p(raw="ament_cmake", driver="cmake", has_cmake=True, interface=True)
    assert route_file_presence(h) == frozenset() and route_declaration(h) == frozenset()
    assert classify(h)[0] == "agree"

    # 8. phase-439 W3 — the SHAPE check must be able to go red. A model of the
    #    Rust that cannot notice the Rust changing is the whole failure mode
    #    this gate is guarding against one layer down.
    full = {rel: "\n".join(n for n, _ in needles) for rel, needles in SITE_CONTRACT.items()}
    assert not check_sites(full.get), "the contract must be satisfiable"

    # 8a. The probe ban, in each emitter and for each retired probe. This is
    #     the mutation the needle check alone could not catch: reverting the
    #     routing predicate to a file probe while another call to the helper
    #     survives elsewhere in the file.
    for rel in PROBE_BANNED_IN:
        for probe in FILE_PROBES:
            reverted = dict(full)
            reverted[rel] += f"\n        if !pkg.dir.{probe} {{ continue; }}"
            errs = check_sites(reverted.get)
            assert errs, f"reverting {rel} to `{probe}` must red the gate"
            assert all(rel in e for e in errs), errs
            # And the exemption must ACTUALLY exempt, or the rule is a ban and
            # the marker is decoration.
            exempted = dict(full)
            exempted[rel] += (
                f"\n        // {PROBE_EXEMPT} measured, not routing"
                f"\n        if !pkg.dir.{probe} {{ continue; }}"
            )
            assert not check_sites(exempted.get), f"{rel}: the marker must exempt"
        # A marker further above than the lookbehind must NOT exempt, or the
        # window is unbounded and one marker licenses the whole file.
        far = dict(full)
        far[rel] += (
            f"\n        // {PROBE_EXEMPT} too far away"
            + "\n        //" * PROBE_EXEMPT_LOOKBEHIND
            + f'\n        if !pkg.dir.{FILE_PROBES[0]} {{ continue; }}'
        )
        assert check_sites(far.get), f"{rel}: an out-of-window marker must not exempt"

    # 8b. `cmd/build.rs`'s retired predicate, by its exact pre-W3 spelling.
    for rel, banned in BANNED_LINES.items():
        for line, _ in banned:
            back = dict(full)
            back[rel] += f"\n        {line}"
            assert check_sites(back.get), f"{rel}: `{line}` must red the gate"
            # In a COMMENT it is the explanation of the rule, not the rule
            # being broken — the two emitters' own doc comments quote it.
            quoted = dict(full)
            quoted[rel] += f"\n        // was `{line}`, which answered both questions"
            assert not check_sites(quoted.get), f"{rel}: a quoted predicate is prose"
    for rel, needles in SITE_CONTRACT.items():
        for needle, _ in needles:
            broken = dict(full)
            broken[rel] = broken[rel].replace(needle, "/* deleted */")
            errs = check_sites(broken.get)
            assert errs, f"deleting `{needle}` from {rel} must red the gate"
            assert any(rel in e and needle in e for e in errs), errs
        # And a site that vanishes entirely is reported, not skipped — a
        # renamed file must not read as compliance.
        gone = {k: v for k, v in full.items() if k != rel}
        assert any(rel in e for e in check_sites(gone.get)), f"{rel} vanishing must red"

    if not quiet:
        n = sum(len(v) for v in SITE_CONTRACT.values()) + len(SITE_CONTRACT)
        n += len(PROBE_BANNED_IN) * (2 * len(FILE_PROBES) + 1)
        n += sum(2 * len(v) for v in BANNED_LINES.values())
        print(f"selftest: 7 routing cases + {n} site-contract mutations, each direction pinned")
    return 0


# ---------------------------------------------------------------------------
def main() -> int:
    argv = sys.argv[1:]
    if "--self-test" in argv:
        return self_test()
    self_test(quiet=True)

    # Assertion 2 — the Rust still routes on the declaration. FIRST, because a
    # site that stopped reading `<build_type>` makes every number below a
    # description of a rule nothing implements.
    def _read(rel: str) -> str | None:
        f = REPO / rel
        return f.read_text(encoding="utf-8", errors="replace") if f.is_file() else None

    drift = check_sites(_read)
    if drift:
        print(
            "[FAIL] RFC-0094 D3's routing is no longer read where it is applied:",
            file=sys.stderr,
        )
        for d in drift:
            print(f"  - {d}", file=sys.stderr)
        print(
            "\n  The rule has ONE home, `nros_cli_core::routing`, and three\n"
            "  callers. A site that re-derives the driver from file presence is\n"
            "  issue 1207 coming back — see RFC-0094 D3.",
            file=sys.stderr,
        )
        return 1

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
    # Count what was MEASURED, never what is expected. A summary that says
    # "0 misdeclare" while the block below names one is the shape of a green
    # tool disagreeing with itself, which is the failure this gate family
    # exists to stop (CLAUDE.md, the rmw parity map).
    print(
        f"RFC-0094 D3: routing reads <build_type> at all {len(SITE_CONTRACT) - 1} "
        f"sites; {len(violations)} package(s) route in a way D3 does not name."
    )
    print(
        f"  (phase-439 W3 moved {len(fixes)} package(s) off file-presence routing, "
        "all of them the dual-file cmake leaves D3 names.)"
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
