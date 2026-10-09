#!/usr/bin/env python3
"""Where the buildable projects of a copy-out template are — issue 1764.

`check-template-copy-out.sh` used to treat a TEMPLATE as the unit: if any
tracked `system.toml` under it declared an `[image.*]`, it built the template
ROOT. That holds for a workspace template (`src/<bringup>/system.toml`, which
`nros build --workspace <root>` discovers), and it is false for a template whose
images live in SUB-PROJECTS. phase-482 W3 gave `cpp-port-minimal-publisher` two
of them — `mps2-an385-freertos/` and `zephyr/`, each a standalone leaf with its
own `CMakeLists.txt`, `package.xml` and `system.toml` — and the gate then ran
`nros build` at a root that declares no image and went red.

The unit is the PROJECT. A `system.toml` belongs to:

* the directory ABOVE `src/` when it sits under a `src/` component — that is a
  workspace bringup, found the way `nros build`'s discovery finds packages
  (`<root>` and `<root>/src`, `builder::discover`);
* otherwise its OWN directory — a single-package leaf, which is what
  `nros ws leaf-system` and `find_package(nano_ros)` read it as.

Usage:
    template_projects.py <template-dir> <tracked-file>...
        prints one `<project-rel>\\t<image>\\t<board>` line per declared image,
        `<project-rel>` relative to the template dir (`.` for the root)
    template_projects.py descriptor <nros-board.toml> <board> <key>
        prints `[board.<section>] <key>` for the `[[board]]` answering to
        <board> (`cmake.toolchain_file`, `zephyr.west_board`), or nothing
    template_projects.py --self-test
"""

from __future__ import annotations

import sys
from pathlib import PurePosixPath

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11 — the repo's interpreter is 3.10
    import tomli as tomllib


def project_of(manifest_rel: str) -> str:
    """The project directory a template-relative `system.toml` belongs to."""
    parent = PurePosixPath(manifest_rel).parent
    parts = parent.parts
    if "src" in parts:
        root = PurePosixPath(*parts[: parts.index("src")]) if parts.index("src") else PurePosixPath(".")
        return str(root)
    return str(parent) if parts else "."


def images(text: str) -> list[tuple[str, str]]:
    """`(image-id, board)` for every `[image.*]` a system.toml declares."""
    doc = tomllib.loads(text)
    out = []
    for image_id, block in (doc.get("image") or {}).items():
        if isinstance(block, dict):
            out.append((image_id, str(block.get("board", ""))))
    return out


def projects(template_dir: str, tracked: list[str]) -> list[tuple[str, str, str]]:
    rows = []
    prefix = template_dir.rstrip("/") + "/"
    for f in sorted(tracked):
        if not f.startswith(prefix) or PurePosixPath(f).name != "system.toml":
            continue
        rel = f[len(prefix):]
        with open(f, "rb") as fh:
            text = fh.read().decode()
        for image_id, board in images(text):
            rows.append((project_of(rel), image_id, board))
    return rows


def descriptor_value(path: str, board: str, key: str) -> str:
    with open(path, "rb") as fh:
        doc = tomllib.load(fh)
    section, _, field = key.partition(".")
    blocks = doc.get("board", []) or []
    # `nros ws board-facts` already chose this FILE for the board; a file with
    # several `[[board]]`s (nros-board-nuttx-qemu) is disambiguated by `names`,
    # and a single-block file is the answer whatever its aliases say
    # (`nros-board-mps2-an385-freertos` names itself `freertos`).
    chosen = [b for b in blocks if board in (b.get("names") or [])]
    if not chosen and len(blocks) == 1:
        chosen = blocks
    if len(chosen) != 1:
        return ""
    value = (chosen[0].get(section) or {}).get(field)
    return "" if value is None else str(value)


def self_test() -> int:
    cases = {
        "system.toml": ".",
        "src/demo_bringup/system.toml": ".",
        "mps2-an385-freertos/system.toml": "mps2-an385-freertos",
        "zephyr/system.toml": "zephyr",
        "ws/src/bringup/system.toml": "ws",
        "a/b/system.toml": "a/b",
    }
    bad = [(k, project_of(k), v) for k, v in cases.items() if project_of(k) != v]
    if bad:
        for k, got, want in bad:
            print(f"template_projects SELF-TEST FAILED: {k} -> {got!r}, want {want!r}", file=sys.stderr)
        return 1
    got = images('[system]\nname="x"\n[image.a]\nboard="native"\n[image.b]\nboard="zephyr"\n')
    if got != [("a", "native"), ("b", "zephyr")]:
        print(f"template_projects SELF-TEST FAILED: images() -> {got!r}", file=sys.stderr)
        return 1
    print("self-test OK: a sub-project leaf is its own project; a src/ bringup belongs to the root.")
    return 0


def main(argv: list[str]) -> int:
    if argv[:1] == ["--self-test"]:
        return self_test()
    if argv[:1] == ["descriptor"] and len(argv) == 4:
        print(descriptor_value(argv[1], argv[2], argv[3]))
        return 0
    if len(argv) < 1:
        print(__doc__, file=sys.stderr)
        return 2
    for project, image_id, board in projects(argv[0], argv[1:]):
        print(f"{project}\t{image_id}\t{board}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
