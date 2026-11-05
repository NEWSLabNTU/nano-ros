#!/usr/bin/env python3
"""A node package's manifest names no selection crate and no board crate —
RFC-0121 D1/D7, phase-531 W2.

THE RULE. RFC-0103 D1 puts every selection (board, RMW, network) in the
project layer, which RFC-0121 D2 places in a bringup's `system.toml`. A node
package is a library of nodes: its `Cargo.toml` names no board, no RMW and no
selection crate, because the image's generated entry is what names the board
(RFC-0121 D6). Two dependency spellings carry a selection into a node package:

  * a `<pkg>_nros_selection` crate — the generated selection package of the
    standalone shape (RFC-0098 D6, `leaf_system::selection_crate_name`), or
    a `<entry>_nros_derived` crate — an OWNED entry's adapter (RFC-0071 D13,
    `entry_names::adapter_crate_name`), which a node package never needs;
  * an `nros-board-*` crate — the board itself, by hand.

Either, in any dependency table of the manifest (`[dependencies]`,
`[dev-dependencies]`, `[build-dependencies]` and their `[target.*]` scopes),
is a violation.

SCOPE. A NODE package is a directory under `examples/` or `packages/testing/`
holding a tracked `Cargo.toml` with a `[package]` and either a tracked
`package.xml` or a tracked `system.toml` (the standalone shape, which is a node
package and its bringup in one directory). Not in scope, because naming the
board is their job:

  * an ENTRY package (`*_entry`, or `entry` in the name as a word) — the
    platform seam, generated under `build/` in the end state (RFC-0121 D6) and
    still hand-written for `nros new entry` until phase-531 W7 step 8;
  * a BOARD, RUNNER or COMMAND package (`<nano_ros_provides kind=...>` in its
    `package.xml`, RFC-0087/RFC-0119).

THE RATCHET. Phase-531 W1 measured the violations on 2026-11-05; they are
listed in `.config/node-package-selection-free-baseline.txt` as
`<manifest>  <crate>` rows and the list may only SHRINK: an unlisted violation
fails, and a listed row that no longer violates fails until it is deleted.
`--write-baseline` refuses to add a row. Phase-531 W6 (with phase-535 for the
paired examples) empties it.

  check-node-package-selection-free.py [--self-test | --list | --write-baseline]
"""

import os
import re
import subprocess
import sys

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11
    import tomli as tomllib  # type: ignore

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BASELINE = os.path.join(ROOT, ".config", "node-package-selection-free-baseline.txt")
SCOPES = ("examples/", "packages/testing/")
DEP_TABLES = ("dependencies", "dev-dependencies", "build-dependencies")
ENTRY = re.compile(r"(^|[_-])entry([_-]|$)")
PROVIDER = re.compile(r'<nano_ros_provides\b[^>]*\bkind\s*=\s*"(board|runner|command)"')
HEADER = """\
# check-node-package-selection-free (RFC-0121 D7, phase-531 W2) — node-package
# manifests that still name a selection crate (`<pkg>_nros_selection`) or a
# board crate (`nros-board-*`), as `<manifest>  <crate>` rows. Generated from
# phase-531 W1's measurement (2026-11-05).
#
# A RATCHET that may only SHRINK: an unlisted violation fails, and a listed row
# that no longer violates fails until it is deleted. Phase-531 W6 and phase-535
# empty it. Regenerate (shrink only):
#   python3 scripts/check-node-package-selection-free.py --write-baseline
"""


def tracked_files():
    out = subprocess.run(["git", "-C", ROOT, "ls-files", "-z", *SCOPES],
                         capture_output=True, check=True).stdout
    return {p.decode() for p in out.split(b"\0") if p}


def offending(name):
    """Is the dependency KEY `name` a selection or board crate?"""
    n = name.replace("-", "_")
    return (n.endswith("_nros_selection") or n.endswith("_nros_derived")
            or n.startswith("nros_board_"))


def manifest_names(doc):
    """Every dependency key (and `package =` rename target) in every table."""
    names = set()
    scopes = [doc] + [t for t in (doc.get("target") or {}).values() if isinstance(t, dict)]
    for scope in scopes:
        for table in DEP_TABLES:
            for key, spec in (scope.get(table) or {}).items():
                names.add(key)
                if isinstance(spec, dict) and isinstance(spec.get("package"), str):
                    names.add(spec["package"])
    return names


def node_packages(files, read):
    """{manifest: package_dir} for every node package in scope."""
    out = {}
    for f in files:
        if not f.endswith("/Cargo.toml"):
            continue
        d = f[: -len("Cargo.toml")]
        if d + "package.xml" not in files and d + "system.toml" not in files:
            continue
        if ENTRY.search(os.path.basename(d.rstrip("/"))):
            continue
        if d + "package.xml" in files and PROVIDER.search(read(d + "package.xml") or ""):
            continue
        out[f] = d
    return out


def violations(files, read):
    """Sorted `(manifest, crate)` rows."""
    rows = []
    for manifest in node_packages(files, read):
        try:
            doc = tomllib.loads(read(manifest) or "")
        except tomllib.TOMLDecodeError:
            continue  # `check-manifests-parse` owns an unparseable manifest
        if "package" not in doc:
            continue  # a virtual workspace root is not a package
        rows.extend((manifest, n) for n in sorted(manifest_names(doc)) if offending(n))
    return sorted(set(rows))


def read_tracked(path):
    try:
        with open(os.path.join(ROOT, path), encoding="utf-8", errors="replace") as fh:
            return fh.read()
    except OSError:
        return None


def load_baseline(path=BASELINE):
    rows = []
    if not os.path.exists(path):
        return rows
    for line in open(path, encoding="utf-8"):
        line = line.split("#", 1)[0].split()
        if len(line) == 2:
            rows.append((line[0], line[1]))
    return rows


def judge(found, baseline):
    found, listed = set(found), set(baseline)
    return sorted(found - listed), sorted(listed - found)


def self_test():
    ok = True

    def expect(cond, what):
        nonlocal ok
        if not cond:
            ok = False
            print(f"check-node-package-selection-free self-test FAILED: {what}", file=sys.stderr)

    texts = {
        "examples/ws/src/talker/package.xml": "<package/>",
        "examples/ws/src/talker/Cargo.toml": '[package]\nname = "talker"\n[dependencies]\nnros = {}\n',
        "examples/leaf/system.toml": "",
        "examples/leaf/Cargo.toml":
            '[package]\nname = "leaf"\n[dependencies]\nleaf_nros_selection = { path = "x" }\n',
        "examples/ws/src/linux_entry/package.xml": "<package/>",
        "examples/ws/src/linux_entry/Cargo.toml":
            '[package]\nname = "linux_entry"\n[dependencies]\nnros-board-linux = {}\n',
        "examples/ws/src/my_board/package.xml": '<nano_ros_provides kind="board"/>',
        "examples/ws/src/my_board/Cargo.toml":
            '[package]\nname = "my_board"\n[dependencies]\nnros-board-common = {}\n',
    }
    files = set(texts)
    read = texts.get
    base = [("examples/leaf/Cargo.toml", "leaf_nros_selection")]
    expect(judge(violations(files, read), base) == ([], []),
           "the measured tree passes against its own list (entry and board packages exempt)")
    # A board crate named in a node package fails — in a target scope, renamed.
    t = dict(texts)
    t["examples/ws/src/talker/Cargo.toml"] += (
        '[target.\'cfg(unix)\'.dependencies]\nb = { package = "nros-board-linux" }\n')
    expect(judge(violations(files, t.get), base)[0]
           == [("examples/ws/src/talker/Cargo.toml", "nros-board-linux")],
           "a renamed board crate in a [target] table fails")
    # A selection crate named in a node package fails.
    t = dict(texts)
    t["examples/ws/src/talker/Cargo.toml"] += 'talker_nros_selection = { path = "s" }\n'
    expect(judge(violations(files, t.get), base)[0]
           == [("examples/ws/src/talker/Cargo.toml", "talker_nros_selection")],
           "a selection crate in a node package fails")
    # Removing an allowlisted site without deleting its row fails as stale.
    t = dict(texts)
    t["examples/leaf/Cargo.toml"] = '[package]\nname = "leaf"\n'
    expect(judge(violations(files, t.get), base) == ([], base),
           "a migrated manifest whose row stayed fails as stale")
    expect(offending("qemu-bsp-talker_nros_selection") and offending("nros_board_zephyr")
           and offending("demo_entry_nros_derived")
           and not offending("nros-board") and not offending("nros_selection_tool")
           and not offending("nros_derived_tool"),
           "the three spellings, and only those")
    return ok


def write_baseline(found):
    old = set(load_baseline())
    grown = [r for r in found if r not in old]
    if os.path.exists(BASELINE) and grown:
        print("check-node-package-selection-free: --write-baseline REFUSED — it may only "
              "shrink the list, and these are new:", file=sys.stderr)
        for m, c in grown:
            print(f"  {m}  {c}", file=sys.stderr)
        return 1
    with open(BASELINE, "w", encoding="utf-8") as fh:
        fh.write(HEADER)
        for m, c in found:
            fh.write(f"{m}  {c}\n")
    print(f"check-node-package-selection-free: wrote {len(found)} row(s) to "
          f"{os.path.relpath(BASELINE, ROOT)}")
    return 0


def main():
    if not self_test():
        return 1
    if "--self-test" in sys.argv:
        print("check-node-package-selection-free self-test: OK (5 cases)")
        return 0
    files = tracked_files()
    found = violations(files, read_tracked)
    if "--list" in sys.argv:
        print("\n".join(f"{m}  {c}" for m, c in found))
        return 0
    if "--write-baseline" in sys.argv:
        return write_baseline(found)
    new, stale = judge(found, load_baseline())
    if new or stale:
        print("check-node-package-selection-free: FAILED (RFC-0121 D7, phase-531 W2)",
              file=sys.stderr)
        for m, c in new:
            print(f"  {m}: names `{c}`", file=sys.stderr)
        if new:
            print("  A node package names no board and no selection crate: the image's\n"
                  "  generated entry names the board, from the bringup's system.toml\n"
                  "  (RFC-0121 D1/D6, RFC-0103 D1). Drop the dependency. The allowlist only\n"
                  "  shrinks: do not add the row to it.", file=sys.stderr)
        for m, c in stale:
            print(f"  {m}  {c}: listed in {os.path.relpath(BASELINE, ROOT)} but no longer "
                  "named — delete its line", file=sys.stderr)
        return 1
    n = len(load_baseline())
    print(f"check-node-package-selection-free: OK — no new selection or board crate in a "
          f"node package; {n} allowlisted row(s) left to migrate")
    return 0


if __name__ == "__main__":
    sys.exit(main())
