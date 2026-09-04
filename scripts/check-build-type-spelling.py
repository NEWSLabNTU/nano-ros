#!/usr/bin/env python3
"""RFC-0087 D2 / phase-420 W2 — `<build_type>` says how, and says whose.

A nano-ros workspace is a colcon workspace, so `<build_type>` is the one field
that says how a package is built. Counted on 2026-09-04 the tree spelled it
seven ways — `ament_cargo` 157, `ament_cmake` 125, `cmake` 75, `ament_nros` 5,
`nros_entry` 2, `nros_bringup` 1, `cargo` 1 — and three of those are
improvised nano-ros names that no colcon extension has ever registered.

RFC-0087 D2 replaces the improvisation with `nros_cargo` / `nros_cmake` and,
crucially, draws a CLASS BOUNDARY:

  * interface packages — `packages/interfaces/*` and a user's message
    packages — KEEP `ament_cmake`. They are genuinely ROS 2 packages and a
    ROS 2 node consumes their output; claiming otherwise is as wrong as the
    defect this RFC fixes, only pointing the other way.
  * entries, boards, RMW / platform / serdes providers and bringups take
    `nros_cargo` / `nros_cmake`. `ament_cargo` on a Cortex-R5 firmware entry
    says a stock `colcon build` can handle the package. It cannot, and the
    honest outcome is a refusal, not an attempt to install firmware into a
    prefix.
  * standalone examples with no ROS identity keep plain `cmake` / `cargo`.

**The boundary is the point of this gate.** A gate that checked only the
vocabulary would pass a freertos firmware entry declaring `ament_cargo` —
which is the exact defect, spelled in an allowed word.

## Ratchet

The tree is NOT migrated: that is phase-420 W3, kept a separate change on
purpose so a ~170-file mechanical sweep cannot hide a semantic one. So today's
violations are grandfathered in `scripts/build-type-spelling-baseline.json`,
which may only shrink — the same shape and the same reasoning as
`scripts/board-maintainer-baseline.json`: a gate that fails 300 rows on the day
it lands gets bypassed, and a bypassed gate is worse than a narrower one that
binds. A NEW package binds immediately, which is what stops W3 from being
outrun by packages written while it lands.

## Three readers of one table

`packages/cli/nros-cli-core/src/build_type.rs` and
`cmake/NanoRosPackageXml.cmake` both resolve old spellings to new, and this
gate needs the same vocabulary. Rather than a third copy, the gate PARSES both
tables and refuses a disagreement (S0 below). CLAUDE.md records why: the rmw
parity map and the vtable were two green tools disagreeing by 25 symbols
because neither read the other, and the fix was to cross-check, not to add a
third authority.

## Enumeration

Tracked `package.xml`, via `git ls-files`. `.nros-ignore` markers are NOT
honoured, deliberately: the two in this tree (repo root, `examples/templates/`)
prune a pkg-index walk for NAME UNIQUENESS across copy-out workspaces (issue
0621), not because those packages are unreal. A template a user copies out is
exactly a package whose build type has to be right on arrival.

Dependency-free: this host's Python is 3.10 (no `tomllib`), and no TOML is
parsed here anyway — `system.toml` is evidence by EXISTING, and the two tables
are regex-read from a `.rs` and a `.cmake` file.
"""

import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BASELINE = ROOT / "scripts/build-type-spelling-baseline.json"
RUST_TABLE = ROOT / "packages/cli/nros-cli-core/src/build_type.rs"
CMAKE_TABLE = ROOT / "cmake/NanoRosPackageXml.cmake"

# XML comments cannot contain `--`, so `([^-]|-[^-])*` matches their body
# exactly. Stripping them is not optional (issue 0516): a provider package.xml
# that DOCUMENTS a tag in a comment otherwise declares it, and a regex cannot
# tell the two apart. `nros_read_package_xml_body` in NanoRosPackageXml.cmake
# carries the identical pattern for the identical reason.
COMMENT_RE = re.compile(r"<!--([^-]|-[^-])*-->")
BUILD_TYPE_RE = re.compile(r"<build_type>([^<]*)</build_type>")

# A CMake verb that BUILDS or REGISTERS an image. `nros_generate_interfaces` and
# `nros_find_interfaces` are deliberately absent: a message package calls those,
# and treating them as ownership evidence would classify every user interface
# package as firmware — the wrong direction the RFC warns about.
OWNED_CMAKE_RE = re.compile(
    r"\b(nano_ros_add_executable|nano_ros_auto_add_library|nano_ros_add_node"
    r"|nano_ros_entry|nros_components_register_node|nano_ros_node_register"
    r"|nano_ros_use_board)\s*\("
)
OWNED_CARGO_RE = re.compile(r"^\s*(nros[-\w]*\s*=|\[package\.metadata\.nros)", re.M)

RULES = {
    # The vocabulary itself.
    "unknown-spelling": "declares a <build_type> this project does not define",
    "duplicate-build-type": "declares more than one <build_type>",
    # The class boundary (RFC-0087 D2).
    "owned-declares-ament": "nano-ros-owned package claims an ament build type",
    "owned-declares-nothing": "nano-ros-owned package declares no build type",
    "interface-declares-nros": "interface package claims a nano-ros build type",
}


# ---------------------------------------------------------------------------
# S0 — the vocabulary, read from the readers rather than restated
# ---------------------------------------------------------------------------
def rust_table(text):
    """`("raw", "canonical", BuildPath::X, retired)` rows from build_type.rs."""
    body = re.search(r"const TABLE:[^=]*=\s*&\[(.*?)\n\];", text, re.S)
    if not body:
        return None
    rows = {}
    for raw, canon, path, retired in re.findall(
        r'\(\s*"([a-z_]+)",\s*"([a-z_]+)",\s*BuildPath::(\w+),\s*(true|false)\s*\)',
        body.group(1),
    ):
        rows[raw] = (canon, path.lower(), retired == "true")
    return rows


def cmake_table(text):
    """The same rows from the `set(_NROS_BUILD_TYPE_*)` lists."""
    m = re.search(r"set\(_NROS_BUILD_TYPE_MAP(.*?)CACHE INTERNAL", text, re.S)
    r = re.search(r"set\(_NROS_BUILD_TYPE_RETIRED(.*?)CACHE INTERNAL", text, re.S)
    if not m or not r:
        return None
    retired = set(re.findall(r'"([a-z_]+)"', r.group(1)))
    return {
        raw: (canon, None, raw in retired)
        for raw, canon in re.findall(r'"([a-z_]+)=([a-z_]+)"', m.group(1))
    }


def compare_tables(rust, cm):
    """Disagreements between the two reader tables, as messages."""
    out = []
    for raw in sorted(set(rust) | set(cm)):
        a, b = rust.get(raw), cm.get(raw)
        if a is None or b is None:
            out.append(
                f"`{raw}` is known to "
                f"{rel(RUST_TABLE) if a else rel(CMAKE_TABLE)} and not to the "
                "other — the two readers would resolve the same package.xml "
                "differently"
            )
            continue
        if (a[0], a[2]) != (b[0], b[2]):
            out.append(
                f"`{raw}`: {rel(RUST_TABLE)} says (canonical={a[0]}, "
                f"retired={a[2]}), {rel(CMAKE_TABLE)} says (canonical={b[0]}, "
                f"retired={b[2]})"
            )
    return out


def load_vocabulary(errors):
    """The allowed set, cross-checked across both readers.

    Neither file is the authority — agreement is. A table that only this gate
    could read would be a third spelling of the rule, and a rule with three
    spellings is the class CLAUDE.md keeps paying for.
    """
    rust = rust_table(RUST_TABLE.read_text())
    cm = cmake_table(CMAKE_TABLE.read_text())
    if rust is None:
        errors.append(f"cannot find the build-type TABLE in {rel(RUST_TABLE)}")
    if cm is None:
        errors.append(f"cannot find _NROS_BUILD_TYPE_MAP in {rel(CMAKE_TABLE)}")
    if rust is None or cm is None:
        return set(), set()
    errors += compare_tables(rust, cm)
    retired = {raw for raw, v in rust.items() if v[2]}
    return set(rust) - retired, retired


# ---------------------------------------------------------------------------
# Classification — from evidence in the package directory, never a hand list
# ---------------------------------------------------------------------------
def read_evidence(pkg_xml, pkg_dir, rel_path):
    """Everything the rules need about one package. Comments already stripped."""
    body = COMMENT_RE.sub("", pkg_xml.read_text(encoding="utf-8", errors="replace"))
    declared = [b.strip() for b in BUILD_TYPE_RE.findall(body)]

    nano_ros = re.search(r"<nano_ros\s([^>]*)>", body)
    deploy = ""
    if nano_ros:
        d = re.search(r'deploy="([^"]*)"', nano_ros.group(1))
        deploy = d.group(1) if d else ""

    cargo = pkg_dir / "Cargo.toml"
    cmakelists = pkg_dir / "CMakeLists.txt"
    ev = set()
    if "nano_ros_provides" in body:
        ev.add("provides")
    if (pkg_dir / "system.toml").is_file():
        ev.add("system.toml")
    if nano_ros or "<nano_ros_uses" in body:
        ev.add("uses")
    if cargo.is_file() and OWNED_CARGO_RE.search(
        cargo.read_text(encoding="utf-8", errors="replace")
    ):
        ev.add("cargo-nros-dep")
    if cmakelists.is_file() and OWNED_CMAKE_RE.search(
        cmakelists.read_text(encoding="utf-8", errors="replace")
    ):
        ev.add("cmake-nano-ros-verb")

    iface = set()
    if rel_path.startswith("packages/interfaces/"):
        iface.add("packages/interfaces")
    if "rosidl_interface_packages" in body:
        iface.add("member_of_group")
    if "rosidl_default_generators" in body:
        iface.add("rosidl-generators")
    if any((pkg_dir / d).is_dir() for d in ("msg", "srv", "action")):
        iface.add("idl-dir")

    return {
        "declared": declared,
        "deploy": deploy,
        "owned_evidence": ev,
        "interface_evidence": iface,
        # A package pinned to a platform, a provider, or a bringup: nothing
        # else can build it, whatever else it also ships.
        "platform_committed": bool(
            {"provides", "system.toml"} & ev or deploy not in ("", "native")
        ),
    }


def classify(ev, rel_path):
    """`interface` | `owned` | `ambiguous` | `unclassified`.

    `ambiguous` is not indecision — it is the honest answer for a package that
    is BOTH a message package and a nano-ros application and is not pinned to
    a platform (`examples/native/c/custom-msg` is one: it declares
    `<member_of_group>rosidl_interface_packages</member_of_group>` AND
    `<nano_ros deploy="native"/>`). Which side of D2's boundary such a package
    belongs on is a semantic judgement, and W3 is where semantic judgements
    are made. The gate names them rather than silently picking, and still
    holds them to the vocabulary.

    A package that IS platform-committed is never ambiguous, whatever messages
    it also ships — that is the firmware case the gate exists for.
    """
    if rel_path.startswith("packages/interfaces/"):
        return "interface"
    owned = bool(ev["owned_evidence"])
    iface = bool(ev["interface_evidence"])
    if owned and iface:
        return "owned" if ev["platform_committed"] else "ambiguous"
    if owned:
        return "owned"
    if iface:
        return "interface"
    return "unclassified"


def violations(rel_path, ev, cls, allowed):
    """Every rule this package breaks, as `(rule, declared)` pairs."""
    out = []
    declared = ev["declared"]
    if len(declared) > 1:
        out.append(("duplicate-build-type", ",".join(declared)))
    value = declared[0] if declared else ""

    if declared and value not in allowed:
        out.append(("unknown-spelling", value))

    if cls == "owned":
        if not declared:
            # catkin_pkg's `Package.get_build_type()` returns 'catkin' when the
            # export is absent (verified against the installed catkin_pkg), so
            # declaring nothing is not neutral: every ROS-side reader supplies
            # an ament-family default. Same false claim as `ament_cmake`, made
            # by omission.
            out.append(("owned-declares-nothing", ""))
        elif value.startswith("ament_"):
            out.append(("owned-declares-ament", value))
    elif cls == "interface" and value.startswith("nros_"):
        out.append(("interface-declares-nros", value))
    return out


# ---------------------------------------------------------------------------
# Scanning
# ---------------------------------------------------------------------------
def tracked_package_xmls():
    """Tracked package.xml, plus untracked ones git does not ignore.

    `--others --exclude-standard` matters for the RATCHET specifically: a
    brand-new package is exactly what the gate has to bind immediately, and
    with `--cached` alone it stays invisible until someone remembers to
    `git add` it — which is after the point where the author wanted to know.
    Ignored paths (build dirs, `generated/`, `third-party/`) stay out, and the
    tree currently has zero untracked package.xml, so this widens what the gate
    can see without widening what it reports today.
    """
    out = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "-z", "--cached", "--others",
         "--exclude-standard", "--", "*package.xml"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    return sorted(set(p for p in out.split("\0") if p))


def walk_package_xmls(root):
    return sorted(
        str(p.relative_to(root)) for p in Path(root).rglob("package.xml") if p.is_file()
    )


def scan(root, paths, allowed):
    """`[(rel_path, cls, [(rule, declared), …])]`, one row per package."""
    rows = []
    for rel_path in paths:
        pkg_xml = Path(root) / rel_path
        ev = read_evidence(pkg_xml, pkg_xml.parent, rel_path)
        cls = classify(ev, rel_path)
        rows.append((rel_path, cls, violations(rel_path, ev, cls, allowed)))
    return rows


def rel(p):
    return str(Path(p).relative_to(ROOT))


# ---------------------------------------------------------------------------
# Baseline — the ratchet
# ---------------------------------------------------------------------------
def load_baseline(errors):
    """The grandfather list, keyed `<path>@<rule>` -> the declared spelling.

    Keyed by RULE and not by path alone because one package can break two
    rules at once (a bringup declaring `ament_nros` breaks the vocabulary AND
    the boundary), and grandfathering one must not silently cover the other.
    The VALUE is the spelling that was grandfathered, so a package that swaps
    one violating spelling for a different one is a new violation rather than
    a covered one — the same ratchet direction as
    `board-maintainer-baseline.json` refusing an exemption granted at tier 3
    to cover a promotion to tier 2.
    """
    if not BASELINE.exists():
        errors.append(
            f"missing baseline {rel(BASELINE)} — regenerate with "
            "--write-baseline. A missing baseline is a FAILURE, not an empty "
            "exemption set: read as empty it would fail ~300 unmigrated "
            "packages at once, with a message about spelling rather than "
            "about the missing file, and invite the bypass the ratchet exists "
            "to avoid"
        )
        return {}
    try:
        return json.loads(BASELINE.read_text())
    except (OSError, ValueError) as exc:
        errors.append(f"cannot read baseline {rel(BASELINE)}: {exc}")
        return {}


def write_baseline(rows):
    out = {
        f"{path}@{rule}": declared
        for path, _cls, vs in rows
        for rule, declared in vs
    }
    BASELINE.write_text(json.dumps(dict(sorted(out.items())), indent=2) + "\n")
    print(f"wrote {rel(BASELINE)}: {len(out)} violation(s) grandfathered")
    return 0


def apply_baseline(rows, baseline):
    """Split the scan into (errors, exempt, improved) against the baseline."""
    errors, exempt, seen = [], [], set()
    for path, _cls, vs in rows:
        for rule, declared in vs:
            key = f"{path}@{rule}"
            seen.add(key)
            if key not in baseline:
                errors.append(
                    f"{path}: {RULES[rule]} (<build_type>{declared or '(absent)'}"
                    f"</build_type>) [{rule}]"
                )
            elif baseline[key] != declared:
                errors.append(
                    f"{path}: grandfathered as `{baseline[key]}` but now declares "
                    f"`{declared}` — a different violation is not the one that was "
                    f"granted; fix it, do not re-baseline it [{rule}]"
                )
            else:
                exempt.append(key)
    improved = sorted(set(baseline) - seen)
    return errors, exempt, improved


# ---------------------------------------------------------------------------
# Negative controls
# ---------------------------------------------------------------------------
def _pkg(root, rel_path, build_type=None, extra="", files=()):
    d = Path(root) / rel_path
    d.mkdir(parents=True, exist_ok=True)
    bt = f"<build_type>{build_type}</build_type>" if build_type is not None else ""
    (d / "package.xml").write_text(
        f'<package format="3"><name>p</name>\n'
        f"  <export>{bt}{extra}</export>\n</package>\n"
    )
    for name, text in files:
        (d / name).write_text(text)
    return f"{rel_path}/package.xml"


def self_test(quiet=False):
    """Every rule is fired, then silenced by its intended escape.

    A gate nobody has watched fail is a gate that has not been shown to have a
    rule at all, and the two rules here are exactly the ones that LOOK
    satisfied by a green vocabulary check.
    """
    allowed = {"nros_cargo", "nros_cmake", "ament_cargo", "ament_cmake", "cmake", "cargo"}

    with tempfile.TemporaryDirectory() as tmp:
        firmware = _pkg(
            tmp,
            "src/firmware",
            "ament_cargo",
            extra='<nano_ros deploy="freertos" board="mps2-an385-freertos"/>',
        )
        msgs = _pkg(tmp, "src/robot_msgs", "nros_cmake", extra="")
        (Path(tmp) / "src/robot_msgs/msg").mkdir()
        provider = _pkg(
            tmp,
            "src/my_rmw",
            "ament_cmake",
            extra='<nano_ros_provides kind="rmw" name="mine"/>',
        )
        silent = _pkg(
            tmp,
            "src/board_pkg",
            None,
            extra='<nano_ros_provides kind="board" name="b"/>',
        )
        retired = _pkg(tmp, "src/bringup", "nros_bringup")
        (Path(tmp) / "src/bringup/system.toml").write_text("")
        standalone = _pkg(tmp, "src/plain_tool", "cmake")
        good = _pkg(
            tmp,
            "src/good_entry",
            "nros_cargo",
            extra='<nano_ros deploy="freertos"/>',
        )
        twice = _pkg(tmp, "src/two", "cmake")
        p = Path(tmp) / "src/two/package.xml"
        p.write_text(p.read_text().replace(
            "<build_type>cmake</build_type>",
            "<build_type>cmake</build_type><build_type>ament_cmake</build_type>"))

        rows = scan(tmp, walk_package_xmls(tmp), allowed)
        by_path = {r[0]: r for r in rows}

        def fired(path):
            return {rule for rule, _ in by_path[path][2]}

        # R1 — the boundary, the direction the whole gate exists for. A green
        # VOCABULARY check passes this file: `ament_cargo` is a perfectly
        # allowed spelling. It is the CLASS that makes it a lie.
        assert by_path[firmware][1] == "owned", by_path[firmware]
        assert "owned-declares-ament" in fired(firmware), fired(firmware)
        # R1 for a provider and (R1') for one that declares nothing at all.
        assert "owned-declares-ament" in fired(provider), fired(provider)
        assert "owned-declares-nothing" in fired(silent), fired(silent)

        # R2 — the other direction. A message package that claims to be
        # nano-ros-built is equally wrong: a ROS 2 node consumes its output.
        assert by_path[msgs][1] == "interface", by_path[msgs]
        assert "interface-declares-nros" in fired(msgs), fired(msgs)

        # R3 — vocabulary. A retired spelling fires even though its CLASS is
        # right (a bringup IS nano-ros-owned and `nros_bringup` is not ament).
        assert fired(retired) == {"unknown-spelling"}, fired(retired)

        # R4 — two <build_type> elements. catkin_pkg raises InvalidPackage on
        # the second, so a file with two is unbuildable by every ROS reader.
        assert "duplicate-build-type" in fired(twice), fired(twice)

        # The escapes. Each is the FIX, not an exemption.
        assert not fired(good), f"a correct owned entry must pass: {fired(good)}"
        assert not fired(standalone), (
            "a standalone example with no ROS identity keeps plain `cmake` "
            f"(RFC-0087 D2): {fired(standalone)}"
        )
        assert by_path[standalone][1] == "unclassified"

        # Interface evidence must not be OUTRANKED by an interface verb: a
        # message package calling `nros_generate_interfaces` is still a
        # message package. This is the misclassification that would invert R2.
        gen = _pkg(tmp, "src/gen_msgs", "ament_cmake",
                   files=[("CMakeLists.txt", "nros_generate_interfaces(gen_msgs)\n")])
        (Path(tmp) / "src/gen_msgs/msg").mkdir()
        rows2 = {r[0]: r for r in scan(tmp, [gen], allowed)}
        assert rows2[gen][1] == "interface", rows2[gen]

        # ...while a verb that builds an IMAGE is ownership evidence even with
        # no export tag at all. 148 `ament_cargo` packages have no
        # `<nano_ros>` element; without this the gate would see none of them.
        entry = _pkg(tmp, "src/plain_entry", "ament_cmake",
                     files=[("CMakeLists.txt", "nano_ros_add_executable(x)\n")])
        rows3 = {r[0]: r for r in scan(tmp, [entry], allowed)}
        assert rows3[entry][1] == "owned"
        assert "owned-declares-ament" in {r for r, _ in rows3[entry][2]}

        # A comment is not a declaration (issue 0516).
        cm = Path(tmp) / "src/commented"
        cm.mkdir(parents=True)
        (cm / "package.xml").write_text(
            '<package format="3"><name>p</name><export>\n'
            "  <!-- <nano_ros_provides kind=\"rmw\" name=\"x\"/> is provision -->\n"
            "  <build_type>ament_cmake</build_type>\n</export></package>\n"
        )
        rows4 = {r[0]: r for r in scan(tmp, ["src/commented/package.xml"], allowed)}
        assert rows4["src/commented/package.xml"][1] == "unclassified", (
            "a documented provision tag must not classify the package as a "
            "provider (issue 0516)"
        )

        # The ratchet, in both directions.
        base = {f"{firmware}@owned-declares-ament": "ament_cargo"}
        errs, exempt, improved = apply_baseline([by_path[firmware]], base)
        assert not errs and len(exempt) == 1, (errs, exempt)
        # A DIFFERENT violating spelling is not the one that was granted.
        errs, _, _ = apply_baseline(
            [(firmware, "owned", [("owned-declares-ament", "ament_cmake")])], base)
        assert errs and "not the one that was granted" in errs[0], errs
        # Fixing it is reported, never failed — a ratchet that punishes the
        # good deed gets bypassed too (board-maintainer-baseline's rule).
        errs, _, improved = apply_baseline([(firmware, "owned", [])], base)
        assert not errs and improved == [f"{firmware}@owned-declares-ament"]

    # S0 — the cross-check itself. Both live tables must be PARSEABLE (a regex
    # that silently matches nothing would make the comparison vacuously green,
    # which is the failure mode of every "two green tools" story in CLAUDE.md);
    # whether they AGREE is main's verdict, reported rather than asserted, so a
    # real divergence prints a diagnosis instead of a traceback.
    real_rust = rust_table(RUST_TABLE.read_text())
    real_cmake = cmake_table(CMAKE_TABLE.read_text())
    assert real_rust and real_cmake, "both reader tables must be parseable"
    assert len(real_rust) >= 6 and len(real_cmake) >= 6, (real_rust, real_cmake)
    for case, rust, cm in [
        ("a row only one reader has",
         {"a": ("a", None, False)}, {}),
        ("a row the two resolve differently",
         {"a": ("nros_cargo", None, False)}, {"a": ("nros_cmake", None, False)}),
        ("a row one reader thinks is retired",
         {"a": ("a", None, True)}, {"a": ("a", None, False)}),
    ]:
        assert compare_tables(rust, cm), f"{case} must be reported"
    assert {r for r, v in real_rust.items() if v[2]} == {
        "ament_nros", "nros_entry", "nros_bringup"
    }, "the retired set is RFC-0087 D2's, not whatever the table happens to say"

    # A missing baseline must FAIL rather than read as an empty exemption set.
    global BASELINE
    real, BASELINE = BASELINE, ROOT / "scripts/does-not-exist.json"
    try:
        errs = []
        assert load_baseline(errs) == {} and errs, (
            "a missing baseline must be an error, not a silent empty set")
    finally:
        BASELINE = real

    if not quiet:
        print("check-build-type-spelling self-test: OK")
    return 0


# ---------------------------------------------------------------------------
def print_listing(rows):
    for path, cls, vs in rows:
        marks = ",".join(r for r, _ in vs) or "-"
        print(f"{cls}\t{marks}\t{path}")
    return 0


def main():
    argv = sys.argv[1:]
    if "--self-test" in argv:
        return self_test()
    # Always, not only behind the flag: a negative control nobody runs decays
    # into a comment, and this rule's whole job is to fire.
    self_test(quiet=True)

    errors = []
    allowed, _retired = load_vocabulary(errors)
    if errors:
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        print("\n[FAIL] the build-type readers disagree", file=sys.stderr)
        return 1

    scan_root = None
    if "--scan-root" in argv:
        scan_root = argv[argv.index("--scan-root") + 1]
    root = scan_root or ROOT
    paths = walk_package_xmls(root) if scan_root else tracked_package_xmls()
    rows = scan(root, paths, allowed)

    if "--list" in argv:
        return print_listing(rows)
    if "--write-baseline" in argv:
        if scan_root:
            print("--write-baseline works on the repo, not --scan-root",
                  file=sys.stderr)
            return 2
        return write_baseline(rows)

    baseline = {} if scan_root else load_baseline(errors)
    errs, exempt, improved = apply_baseline(rows, baseline)
    errors += errs

    counts = {}
    for _p, cls, _v in rows:
        counts[cls] = counts.get(cls, 0) + 1
    print(
        f"{len(rows)} package.xml: "
        + ", ".join(f"{n} {c}" for c, n in sorted(counts.items()))
    )
    if exempt:
        print(
            f"  {len(exempt)} violation(s) grandfathered by {rel(BASELINE)} — "
            "phase-420 W3 empties it; a NEW package binds immediately"
        )
    for key in improved:
        path, _, rule = key.rpartition("@")
        if not (Path(root) / path).is_file():
            errors.append(
                f"baseline names {key}, whose package.xml does not exist — a "
                "stale exemption is how a rule gets reclaimed by accident; "
                "drop it (--write-baseline)"
            )
        else:
            print(
                f"  {path} no longer breaks `{rule}` — drop it from the "
                "baseline (--write-baseline); the list only shrinks"
            )

    if errors:
        print("\n[FAIL] <build_type> spelling / class boundary (RFC-0087 D2):",
              file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        return 1
    print("Every <build_type> is an allowed spelling on the right side of D2.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
