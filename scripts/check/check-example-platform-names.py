#!/usr/bin/env python3
"""check-example-platform-names -- every example platform a list names exists.

A platform that leaves the tree (an `examples/<platform>/` directory deleted,
a fixture platform dropped) tends to stay named in the lists around it: the
examples README's platform list and tables, a lint script's exclusion
pattern, a build script's `case "$platform"` arm. Nothing reads those names
back, so they rot quietly. The retired `esp32-c3-baremetal` platform was
still in seven places in `examples/README.md` and in three scripts after its
directory had gone.

A name is known when it is one of:

- a top-level entry under `examples/` (a platform directory);
- a fixture platform in `examples/fixtures.toml` (`platform = "..."`);
- a board key in the board index, `packages/boards/board-support.toml`
  (`board_keys = [...]`).

Subjects:

- `examples/README.md`: the "**Platform** (N):" list (its count too), the
  columns headed `Platform`, `trees` or `` `examples/<plat>/` ``, the path
  cells of the `Cell` column, and every `examples/<x>` path;
- shell, just and workflow files (`scripts/**`, `just/**`, `justfile`,
  `.github/workflows/**`), outside comments: every `examples/<x>/` path,
  every hyphenated `<x>/{rust,c,cpp}/` example path, and every arm of a
  `case "$platform"` / `case "$PLATFORM"`. Python scripts are not read:
  they derive their lists from `examples/fixtures.toml`, and their
  self-tests build synthetic `examples/<x>/` trees.

`scripts/oneshot/` is the record of past migrations, written against layouts
that no longer exist on purpose, so it is not read. No allowlist.
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
NAME = "check-example-platform-names"
README = "examples/README.md"
SCRIPT_GLOBS = ["scripts/**", "just/**", "justfile", ".github/workflows/**"]
SKIP_PREFIXES = ("scripts/oneshot/",)
SCRIPT_SUFFIXES = (".sh", ".just", ".yml", ".yaml", "")

TOKEN = r"[a-z0-9][a-z0-9_-]*"
EXAMPLES_PATH = re.compile(rf"(?<![\w.$/{{-])examples/({TOKEN})/")
LANG_PATH = re.compile(r"(?<![\w./${-])([a-z0-9]+(?:-[a-z0-9]+)+)/(?:rust|c|cpp)/")
CASE_OPEN = re.compile(r'case\s+"?\$\{?(platform|PLATFORM)\}?"?\s+in')
PLATFORM_LINE = re.compile(r"^- \*\*Platform\*\* \((\d+)\): (.*)$")
BACKTICK = re.compile(r"`([^`]+)`")


def known_names(root: Path) -> set[str]:
    names = {p.name for p in (root / "examples").iterdir()}
    fixtures = (root / "examples/fixtures.toml").read_text()
    names |= set(re.findall(r'^\s*platform\s*=\s*"([^"]+)"', fixtures, re.M))
    index = (root / "packages/boards/board-support.toml").read_text()
    for keys in re.findall(r"^\s*board_keys\s*=\s*\[([^\]]*)\]", index, re.M):
        names |= set(re.findall(r'"([^"]+)"', keys))
    return names


def table_rows(lines: list[str]):
    """Yield (line number, header cells, row cells) for every markdown table row."""
    header = None
    for i, line in enumerate(lines, 1):
        if not line.startswith("|"):
            header = None
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if header is None:
            header = cells
            continue
        if all(set(c) <= set("-: ") for c in cells):
            continue
        yield i, header, cells


def readme_names(text: str) -> list[tuple[int, str, str]]:
    """(line, name, where) for every platform name the examples README lists."""
    out: list[tuple[int, str, str]] = []
    lines = text.split("\n")
    for i, line in enumerate(lines, 1):
        for m in EXAMPLES_PATH.finditer(line):
            out.append((i, m.group(1), "an `examples/` path"))
        m = PLATFORM_LINE.match(line)
        if m:
            listed = BACKTICK.findall(m.group(2))
            for n in listed:
                out.append((i, n, "the Platform list"))
            if int(m.group(1)) != len(listed):
                out.append((i, f"<count {m.group(1)}>",
                            f"the Platform list, which names {len(listed)}"))
    for i, header, cells in table_rows(lines):
        for col, head in enumerate(header):
            if col >= len(cells):
                continue
            head_l = head.strip("`").lower()
            if head_l in ("platform", "trees", "examples/<plat>/"):
                for tok in BACKTICK.findall(cells[col]):
                    first = tok.split("/")[0]
                    if re.fullmatch(TOKEN, first):
                        out.append((i, first, f"the `{head}` column"))
            elif head_l == "cell":
                for tok in BACKTICK.findall(cells[col]):
                    if "/" in tok and re.fullmatch(TOKEN, tok.split("/")[0]):
                        out.append((i, tok.split("/")[0], "the `Cell` column"))
    return out


def is_comment(line: str, path: str) -> bool:
    s = line.lstrip()
    return s.startswith("#")


def script_files(root: Path) -> list[str]:
    files = subprocess.run(
        ["git", "ls-files", "--", *SCRIPT_GLOBS], cwd=root,
        capture_output=True, text=True, check=True,
    ).stdout.split()
    return [
        f for f in files
        if not f.startswith(SKIP_PREFIXES) and Path(f).suffix in SCRIPT_SUFFIXES
        and f != "scripts/check/check-example-platform-names.py"
    ]


def case_arms(lines: list[str]):
    """(line, pattern word) for each arm of a `case "$platform"` block."""
    depth = 0
    for i, line in enumerate(lines, 1):
        if CASE_OPEN.search(line):
            depth = 1
            continue
        if not depth:
            continue
        s = line.strip()
        if re.match(r"^esac\b", s) or s.startswith("esac"):
            depth = 0
            continue
        m = re.match(r"^\(?\s*([A-Za-z0-9_|*\"' -]+)\)", s)
        if m and not s.startswith("#"):
            for w in m.group(1).split("|"):
                w = w.strip().strip("\"'")
                if w and "*" not in w and " " not in w and re.fullmatch(r"[A-Za-z0-9_-]+", w):
                    yield i, w


def script_names(root: Path, path: str) -> list[tuple[int, str, str]]:
    text = (root / path).read_text(errors="replace")
    lines = text.split("\n")
    out: list[tuple[int, str, str]] = []
    for i, line in enumerate(lines, 1):
        if is_comment(line, path):
            continue
        for m in EXAMPLES_PATH.finditer(line):
            out.append((i, m.group(1), "an `examples/` path"))
        for m in LANG_PATH.finditer(line):
            out.append((i, m.group(1), "an example path"))
    if not path.endswith((".yml", ".yaml")):
        for i, w in case_arms(lines):
            out.append((i, w, 'a `case "$platform"` arm'))
    return out


def check(root: Path, files: list[str] | None = None) -> list[str]:
    known = known_names(root)
    bad = []
    for ln, name, where in readme_names((root / README).read_text()):
        if name.startswith("<count") or name not in known:
            bad.append(f"  {README}:{ln}: `{name}` in {where}")
    for path in (script_files(root) if files is None else files):
        for ln, name, where in script_names(root, path):
            if name not in known:
                bad.append(f"  {path}:{ln}: `{name}` in {where}")
    return bad


def self_test() -> int:
    import tempfile
    t = Path(tempfile.mkdtemp(prefix="example-platform-names-"))
    (t / "examples/linux").mkdir(parents=True)
    (t / "examples/fixtures.toml").write_text('[[fixture]]\nplatform = "baremetal"\n')
    (t / "packages/boards").mkdir(parents=True)
    (t / "packages/boards/board-support.toml").write_text('[[board]]\nboard_keys = ["zephyr"]\n')
    (t / README).write_text(
        "- **Platform** (2): `linux`, `gone-baremetal`\n\n"
        "| Platform | Language |\n| --- | --- |\n| `linux` | rust |\n| `gone-baremetal` | rust |\n\n"
        "| Cell | Why |\n| --- | --- |\n| `gone-baremetal/{c,cpp}/*` | x |\n| `cyclonedds` on bare | y |\n"
    )
    (t / "scripts").mkdir()
    (t / "scripts/a.sh").write_text(
        '# examples/gone-baremetal in a comment is history\n'
        'ls examples/linux/ examples/gone-baremetal/x\n'
        'grep -v "gone-baremetal/rust/dds/"\n'
        'case "$platform" in\n  baremetal | gone) x ;;\n  zephyr) y ;;\n  *) z ;;\nesac\n'
    )
    bad = check(t, files=["scripts/a.sh"])
    want = [
        f"{README}:1: `gone-baremetal` in the Platform list",
        f"{README}:6: `gone-baremetal` in the `Platform` column",
        f"{README}:10: `gone-baremetal` in the `Cell` column",
        "scripts/a.sh:2: `gone-baremetal` in an `examples/` path",
        "scripts/a.sh:3: `gone-baremetal` in an example path",
        "scripts/a.sh:5: `gone` in a `case \"$platform\"` arm",
    ]
    got = [b.strip() for b in bad]
    missing = [w for w in want if w not in got]
    extra = [g for g in got if g not in want]
    if missing or extra:
        print(f"{NAME} --self-test FAILED\n  missing: {missing}\n  extra: {extra}", file=sys.stderr)
        return 1
    print(f"{NAME} --self-test OK ({len(want)} cases)")
    return 0


def main() -> int:
    if self_test():
        return 1
    if sys.argv[1:] == ["--self-test"]:
        return 0
    bad = check(ROOT)
    if bad:
        print(f"{NAME}: {len(bad)} name(s) of an example platform that does not exist:",
              file=sys.stderr)
        print("\n".join(bad), file=sys.stderr)
        print(
            "\nA known platform is an `examples/<platform>/` directory, a fixture\n"
            "platform in examples/fixtures.toml, or a `board_keys` entry in\n"
            "packages/boards/board-support.toml. Drop the stale name, or correct it\n"
            "to the platform that replaced it.",
            file=sys.stderr,
        )
        return 1
    print(f"{NAME} OK -- every example platform the README and the scripts name exists.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
