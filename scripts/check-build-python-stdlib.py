#!/usr/bin/env python3
"""A Python script that a USER's build runs imports only what the oldest
supported Python has in its standard library.

A user build runs repo scripts on the user's own Python. The oldest one we
support is 3.10 (Ubuntu 22.04's system Python), and it has no `tomllib`. So a
script on the build path that needs anything outside 3.10's standard library
fails on a stock 22.04 host and nowhere we test. The stack-floor CHECK did
exactly that: it read its table with `tomllib`, fell back to the third-party
`tomli`, and stopped a user's Zephyr build with `ModuleNotFoundError`.

REACH (derived, never a list): every `.py` a tracked CMake file names, outside
comments — `cmake/**`, `zephyr/**`, any `CMakeLists.txt` or `*.cmake` — plus,
transitively, every repo module those scripts import (a sibling file or
`scripts/lib/`).

RULE: each import names either a module in Python 3.10's standard library, a
repo module (checked in turn), or sits in the body of a `try` whose handler
catches ImportError / ModuleNotFoundError, so its absence is already handled.

Usage: check-build-python-stdlib.py [--selftest]
"""

from __future__ import annotations

import ast
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OLDEST = (3, 10)
# Standard-library top-level modules added after 3.10, by the release that
# added them. The checker runs on whatever Python is at hand, so 3.10's set is
# this interpreter's set minus these.
ADDED_AFTER_OLDEST = {
    "tomllib": (3, 11),
    "annotationlib": (3, 14),
    "compression": (3, 14),
}
# The newest Python whose additions are listed above. A newer interpreter may
# have added a module this table does not know, which would then pass as
# stdlib, so the check refuses to answer there rather than answer wrongly.
KNOWN_THROUGH = (3, 14)
CMAKE_GLOBS = ["cmake/*", "zephyr/*", "*CMakeLists.txt", "*.cmake"]
PY_REF = re.compile(r"[A-Za-z0-9_./${}-]*?([A-Za-z0-9_-]+(?:/[A-Za-z0-9_-]+)*\.py)\b")
IMPORT_ERRORS = {"ImportError", "ModuleNotFoundError", "Exception", "BaseException"}


def stdlib_310() -> set[str]:
    return set(sys.stdlib_module_names) - set(ADDED_AFTER_OLDEST)


def tracked(globs: list[str]) -> list[str]:
    out = subprocess.run(["git", "-C", str(ROOT), "ls-files", "--", *globs],
                         check=True, capture_output=True, text=True).stdout.split()
    return [p for p in out if not p.startswith("third-party/") and "/third-party/" not in p]


def entry_scripts(cmake_files: list[str], read=lambda p: (ROOT / p).read_text(errors="replace"),
                  exists=lambda p: (ROOT / p).is_file()) -> dict[str, str]:
    """`script -> the CMake file that names it`, for every tracked script a
    non-comment CMake line names."""
    found: dict[str, str] = {}
    for cf in cmake_files:
        for line in read(cf).splitlines():
            code = line.split("#", 1)[0]
            for m in PY_REF.finditer(code):
                ref = m.group(1)
                tail = ref.split("scripts/", 1)[-1]
                for cand in (f"scripts/{tail}", ref):
                    if exists(cand):
                        found.setdefault(cand, cf)
                        break
    return found


def handled(node: ast.AST, parents: dict[ast.AST, ast.AST]) -> bool:
    """Is `node` inside the body of a `try` that catches an import error?"""
    child, cur = node, parents.get(node)
    while cur is not None:
        # a try statement (and 3.11's try/except*) is the node with handlers
        if getattr(cur, "handlers", None) is not None and child in cur.body:
            for h in cur.handlers:
                names = set()
                if h.type is None:
                    return True
                for t in (h.type.elts if isinstance(h.type, ast.Tuple) else [h.type]):
                    if isinstance(t, ast.Name):
                        names.add(t.id)
                if names & IMPORT_ERRORS:
                    return True
        child, cur = cur, parents.get(cur)
    return False


def imports(src: str) -> list[tuple[str, int, bool]]:
    """`(top-level module, line, handled)` for every absolute import."""
    tree = ast.parse(src)
    parents = {c: p for p in ast.walk(tree) for c in ast.iter_child_nodes(p)}
    out = []
    for n in ast.walk(tree):
        if isinstance(n, ast.Import):
            for a in n.names:
                out.append((a.name.split(".")[0], n.lineno, handled(n, parents)))
        elif isinstance(n, ast.ImportFrom) and n.level == 0 and n.module:
            if n.module == "__future__":
                continue
            out.append((n.module.split(".")[0], n.lineno, handled(n, parents)))
    return out


def local_module(name: str, script: Path) -> Path | None:
    for d in (script.parent, ROOT / "scripts" / "lib"):
        if (d / f"{name}.py").is_file():
            return d / f"{name}.py"
    return None


def violations(entries: dict[str, str], stdlib: set[str]) -> list[str]:
    out, seen = [], set()
    todo = [(ROOT / s, f"named by {cf}") for s, cf in sorted(entries.items())]
    while todo:
        path, why = todo.pop()
        if path in seen:
            continue
        seen.add(path)
        rel = path.relative_to(ROOT)
        for mod, line, ok in imports(path.read_text()):
            local = local_module(mod, path)
            if local is not None:
                todo.append((local, f"imported by {rel}"))
            elif mod not in stdlib and not ok:
                out.append(f"{rel}:{line}: imports `{mod}`, which Python "
                           f"{OLDEST[0]}.{OLDEST[1]}'s standard library does not have "
                           f"({why}); a user build runs this on the user's Python")
    return out


def selftest() -> int:
    bad = []
    std = stdlib_310()
    if "tomllib" in std or "json" not in std:
        bad.append("3.10 set: tomllib is out, json is in")
    src = ("import json\nimport tomllib\nfrom yaml import x\n"
           "try:\n    import tomli\nexcept ModuleNotFoundError:\n    tomli = None\n"
           "try:\n    import foo\nexcept ValueError:\n    pass\n")
    got = {(m, ok) for m, _, ok in imports(src)}
    if got != {("json", False), ("tomllib", False), ("yaml", False), ("tomli", True),
               ("foo", False)}:
        bad.append(f"imports: {sorted(got)}")
    # nested handler: the fallback import is itself in a guarded try body
    src2 = ("try:\n    import tomllib\nexcept ModuleNotFoundError:\n"
            "    try:\n        import tomli as tomllib\n"
            "    except ModuleNotFoundError:\n        raise SystemExit(1)\n")
    if not all(ok for _, _, ok in imports(src2)):
        bad.append("a guarded fallback inside a handler is handled")
    files = {"cmake/A.cmake": 'set(_s "${D}/../scripts/x.py")  # scripts/gone.py\n',
             "zephyr/CMakeLists.txt": "# python3 scripts/y.py\nCOMMAND scripts/build/z.py\n"}
    ents = entry_scripts(list(files), read=lambda p: files[p],
                         exists=lambda p: p in {"scripts/x.py", "scripts/build/z.py",
                                                "scripts/gone.py", "scripts/y.py"})
    if ents != {"scripts/x.py": "cmake/A.cmake", "scripts/build/z.py": "zephyr/CMakeLists.txt"}:
        bad.append(f"entry scripts (comments are not reach): {ents}")
    for b in bad:
        print(f"check-build-python-stdlib selftest: FAIL {b}", file=sys.stderr)
    print(f"check-build-python-stdlib selftest: {'FAIL' if bad else 'OK'}")
    return 1 if bad else 0


def main() -> int:
    if selftest() != 0:
        return 1
    if "--selftest" in sys.argv[1:]:
        return 0
    if sys.version_info[:2] > KNOWN_THROUGH:
        print(f"check-build-python-stdlib: Python {sys.version_info[0]}.{sys.version_info[1]} "
              f"is newer than ADDED_AFTER_OLDEST knows ({KNOWN_THROUGH[0]}.{KNOWN_THROUGH[1]}); "
              "add the standard-library modules it introduced, then raise KNOWN_THROUGH",
              file=sys.stderr)
        return 1
    entries = entry_scripts(tracked(CMAKE_GLOBS))
    problems = violations(entries, stdlib_310())
    for p in problems:
        print(f"  - {p}", file=sys.stderr)
    if problems:
        print(f"check-build-python-stdlib: {len(problems)} import(s) a stock "
              f"Python {OLDEST[0]}.{OLDEST[1]} cannot satisfy", file=sys.stderr)
        return 1
    print(f"check-build-python-stdlib: OK, {len(entries)} script(s) a build names, and the "
          f"repo modules they import, need only Python {OLDEST[0]}.{OLDEST[1]}'s standard library")
    return 0


if __name__ == "__main__":
    sys.exit(main())
