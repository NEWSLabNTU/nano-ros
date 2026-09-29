#!/usr/bin/env python3
"""The ONE way to read the justfiles — the gate recipes, and every other recipe.

`just` sees ONE graph: the root `justfile`, every file it `mod`s (a NAMESPACE),
and every file any of those `import`s (a MERGE into the importer's namespace),
recursively. `just/check.just` is the `check` module and itself an index — its
~290 gate recipes sit in `just/check/*.just` behind `import`. Any script that
reads recipes must read that closure, not a directory listing of it.

phase-472 W2 — the class this module exists to end. Gates had chosen their
justfile population by hand, and each hand-chosen set was smaller than the graph:

    `glob("just/*.just")`             misses `just/check/*.just` (13 files)
    `listdir("just")` as modules      the same, and keys `zephyr-ci.just` as a
                                      module when `just` merges it into `zephyr`
    `[justfile] + its imports`        misses every `mod` (all 18 modules)
    `just/*.just` without `justfile`  misses the 3000-line root file

So a bare `cargo nextest` in `just/zephyr-setup.just`, a `sudo apt` in
`just/check/*.just`, or a `just native <bogus>` in any module read as clean.
Before this module covered all recipes, six scripts reading only the `check`
index failed QUIETLY and differently (check-gate-lists derived 8 gates instead
of 234; check-gate-selftests found 0 gate scripts; ...).

Never glob: the set is what the graph REACHES. A file that is present but not
reachable contributes nothing to `just`, and counting it would make a recipe
visible to the checks and invisible to the runner. A non-optional `mod`/`import`
whose target is missing is an ERROR (`just` refuses to load it too), never a
smaller population.

API
    just_sources(root)       every file in the graph, root first, each once
    just_modules(root)       {module path: [files]} — "" is the root namespace,
                             "check" / "zephyr" / "threadx_linux" the `mod` names
    check_just_sources(root) the `check` module's files (index + its imports)
    check_just_text(root)    those files' text, joined

CLI (for shell gates)
    python3 scripts/lib/check_just_sources.py --list [--root DIR]
        repo-relative paths, one per line; exits 1 on a broken graph or an
        empty one. With no arguments it runs the self-test.
"""

import os
import re
import sys

# Top-level items only: `just` requires them at column 0, and a recipe body is
# indented, so an indented `import`/`mod` is shell text, not a declaration.
_IMPORT = re.compile(r"""^import(\?)?[ \t]+(['"])([^'"]+)\2""", re.MULTILINE)
_MOD = re.compile(
    r"""^mod(\?)?[ \t]+([A-Za-z_][A-Za-z0-9_-]*)(?:[ \t]+(['"])([^'"]+)\3)?[ \t]*(?:#.*)?$""",
    re.MULTILINE,
)


class JustGraphError(Exception):
    """A non-optional `mod`/`import` names a file that does not exist."""


def _read(path):
    with open(path, encoding="utf8", errors="replace") as fh:
        return fh.read()


def _mod_target(decl_dir, name, rel):
    if rel is not None:
        path = os.path.normpath(os.path.join(decl_dir, rel))
        return path if os.path.isfile(path) else None
    for cand in (f"{name}.just", os.path.join(name, "mod.just"),
                 os.path.join(name, "justfile"), os.path.join(name, ".justfile")):
        path = os.path.join(decl_dir, cand)
        if os.path.isfile(path):
            return os.path.normpath(path)
    return None


def just_modules(root):
    """{module path: [files in merge order]} for the graph rooted at `justfile`.

    Module paths join nested `mod` names with `::` (`just`'s own spelling).
    Raises JustGraphError on a missing non-optional target.
    """
    root_file = os.path.join(root, "justfile")
    if not os.path.isfile(root_file):
        return {}
    modules = {}
    seen = set()

    def visit(module, path):
        queue = [path]
        files = modules.setdefault(module, [])
        while queue:
            cur = queue.pop(0)
            real = os.path.realpath(cur)
            if real in seen:
                continue
            seen.add(real)
            files.append(cur)
            text = _read(cur)
            decl_dir = os.path.dirname(cur)
            for m in _IMPORT.finditer(text):
                target = os.path.normpath(os.path.join(decl_dir, m.group(3)))
                if os.path.isfile(target):
                    queue.append(target)
                elif not m.group(1):
                    raise JustGraphError(f"{cur}: `import '{m.group(3)}'` — no such file")
            for m in _MOD.finditer(text):
                optional, name, rel = m.group(1), m.group(2), m.group(4)
                target = _mod_target(decl_dir, name, rel)
                if target is None:
                    if optional:
                        continue
                    raise JustGraphError(f"{cur}: `mod {name}` — no source file")
                visit(f"{module}::{name}" if module else name, target)

    visit("", os.path.normpath(root_file))
    return modules


def just_sources(root):
    """Every justfile `just` loads from `root`, root first, each exactly once."""
    return [f for files in just_modules(root).values() for f in files]


def check_just_sources(root):
    """`just/check.just` and every topic file it imports, in import order."""
    return just_modules(root).get("check", [])


def check_just_text(root):
    """Every gate recipe as one string, for scripts that grep rather than parse."""
    return "\n".join(_read(p) for p in check_just_sources(root))


def self_test():
    """Runs on every import-and-call and every CLI use — phase-472 W9."""
    import tempfile

    def write(base, rel, text):
        path = os.path.join(base, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as fh:
            fh.write(text)

    with tempfile.TemporaryDirectory() as tmp:
        write(tmp, "justfile",
              "import 'just/env.just'\nmod check 'just/check.just'\n"
              "mod plat_a 'just/plat-a.just'\nmod? gone 'just/gone.just'\n"
              "# mod commented 'just/commented.just'\nroot-r:\n    @true\n"
              "indented:\n    mod x 'just/x.just'\n")
        write(tmp, "just/env.just", "export X := '1'\n")
        write(tmp, "just/check.just",
              "import 'check/a.just'\nimport? 'check/missing.just'\nfast:\n    @true\n")
        write(tmp, "just/check/a.just", "gate-a:\n    @python3 scripts/check-a.py\n")
        write(tmp, "just/plat-a.just", "import \"plat-a-setup.just\"\nbuild:\n    @true\n")
        write(tmp, "just/plat-a-setup.just", "setup:\n    @true\n")
        write(tmp, "just/check/orphan.just", "gate-orphan:\n    @true\n")
        mods = just_modules(tmp)
        rel = {k: [os.path.relpath(p, tmp) for p in v] for k, v in mods.items()}
        assert rel[""] == ["justfile", "just/env.just"], rel
        assert rel["check"] == ["just/check.just", "just/check/a.just"], rel
        # A `mod` is keyed by its NAME, and its imports join it — not the filename.
        assert rel["plat_a"] == ["just/plat-a.just", "just/plat-a-setup.just"], rel
        assert set(rel) == {"", "check", "plat_a"}, rel  # no comment/indented/optional mods
        srcs = [os.path.relpath(p, tmp) for p in just_sources(tmp)]
        assert srcs[0] == "justfile" and "just/check/a.just" in srcs, srcs
        # An un-reached file must NOT contribute.
        assert "just/check/orphan.just" not in srcs
        assert "gate-orphan" not in check_just_text(tmp)
        assert "gate-a:" in check_just_text(tmp)
        # A non-optional missing target is an error, never a smaller population.
        write(tmp, "just/check.just", "import 'check/a.just'\nimport 'check/missing.just'\n")
        try:
            just_modules(tmp)
        except JustGraphError:
            pass
        else:
            raise AssertionError("a missing non-optional import must raise")


def main(argv):
    self_test()
    if "--list" not in argv:
        print("check_just_sources self-test: OK")
        return 0
    root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    if "--root" in argv:
        root = argv[argv.index("--root") + 1]
    try:
        paths = just_sources(root)
    except JustGraphError as e:
        print(f"check_just_sources: broken justfile graph: {e}", file=sys.stderr)
        return 1
    if not paths:
        print(f"check_just_sources: no `justfile` under {root}", file=sys.stderr)
        return 1
    for p in paths:
        print(os.path.relpath(p, root))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
