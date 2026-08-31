#!/usr/bin/env python3
"""Every `just <recipe>` inside a recipe body must name a recipe that exists.

Issue 0660. `just` resolves a recipe reference only when the recipe RUNS, so a
deleted recipe leaves its callers parsing fine and failing on invocation:

    $ just native test-rmw
    error: Justfile does not contain recipe `build-zenohd`

phase-362 W4 retired the vendored zenoh router and deleted `build-zenohd`
correctly — and left twelve callers across `native.just` (nine),
`qemu-baremetal.just` (one) and `zephyr-dev.just` (two). All twelve were dead on
arrival and nothing noticed, because:

* `just check` never invokes them;
* `just ci` runs `test-all`, not the per-family `native test-*` recipes;
* `check-doc-refs` covers documents, not recipe references.

So tier 1 was green with twelve broken recipes — and these are the ones a
developer reaches for by hand (`just native test-c`), which is the worst place
for a silent break.

## What is checked

For every `just <token>` in a recipe body, `<token>` must be a root recipe or a
`<module> <recipe>` pair defined in the justfiles. A recipe body shells out
to `just`, which resolves against the ROOT justfile — that is why a call inside
`just/native.just` to `build-zenohd` meant the ROOT recipe, not a sibling.

## What is deliberately NOT checked

* interpolated targets (`just {{something}}`) — the name is not known until run
  time, and guessing would produce false positives on the one shape a human
  cannot verify either;
* `just` with only flags (`just --list`), or with no argument;
* `cargo`/`make`/other tools that happen to be called `just` in prose — only
  lines whose command position is `just` count.

Run: python3 scripts/check-just-recipe-refs.py [--self-test]
"""

import argparse
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

# `just` at a command position: line start, after `&&`/`||`/`;`/`(`, or after a
# leading `@`/`-` recipe prefix. Captures the first argument.
JUST_CALL = re.compile(
    # `run:\s*just …` is the workflow spelling. Without it the whole
    # `.github/workflows` arm is inert: the files are read, no line matches, and
    # the gate reports OK — a scope widening that silently checks nothing.
    r"(?:^|[;&|(]\s*|^\s*[@-]\s*|\brun:\s*)\s*just\s+((?:--?[A-Za-z0-9-]+\s+)*)([A-Za-z0-9_][A-Za-z0-9_-]*)"
    r"(?:\s+([A-Za-z0-9_][A-Za-z0-9_-]*))?",
    re.M,
)

# A line that is a comment in the shell/just sense.
COMMENT = re.compile(r"^\s*#")


# A recipe DEFINITION: name at column 0, optional parameters, then `:`.
# `just --summary` is not usable here — it omits `[private]` and `_`-prefixed
# recipes, and those are called from bodies constantly (`just _count-real-failures`).
# A gate that cannot see them reports every one as missing, which is the same
# defect in the other direction.
RECIPE_DEF = re.compile(r"^([A-Za-z_][A-Za-z0-9_-]*)(?:\s+[^:\n]*)?:(?!=)", re.M)
ALIAS_DEF = re.compile(r"^alias\s+([A-Za-z0-9_-]+)\s*:=", re.M)
MOD_DEF = re.compile(r"^mod\s+([A-Za-z0-9_-]+)\s+'([^']+)'", re.M)
# `import "just/x.just"` merges that file's recipes into the ROOT namespace —
# unlike `mod`, which namespaces them. This gate followed `mod` and not
# `import`, so every imported recipe read as UNDEFINED.
#
# It had been blind to `sdk-env.just`'s recipes since that import was added and
# nobody noticed, because no recipe body happened to call one by name. phase-399
# moved 200 `check-*` recipes into an imported `just/check.just`, and the gate
# reported all of them missing at once — a latent hole surfacing as a flood
# rather than as a single wrong answer.
IMPORT_DEF = re.compile(r'^import\s+[\'"]([^\'"]+)[\'"]', re.M)


def names_in(path, _seen=None):
    """Recipe + alias names defined by one justfile, following its `import`s.

    A module file may `import` siblings — `just/zephyr.just` pulls in
    `zephyr-setup`, `zephyr-ci` and `zephyr-dev` — and `import` is a namespace
    MERGE, so those names belong to the importing file. Reading only the named
    file returned `{default}` for the zephyr module, which is 1 of its ~40
    recipes.

    Import paths are relative to the importing FILE, not the repo root.
    """
    if not path.is_file():
        return set()
    _seen = _seen if _seen is not None else set()
    resolved = path.resolve()
    if resolved in _seen:
        return set()
    _seen.add(resolved)
    body = path.read_text()
    names = set(RECIPE_DEF.findall(body)) | set(ALIAS_DEF.findall(body))
    for rel in IMPORT_DEF.findall(body):
        names |= names_in(path.parent / rel, _seen)
    return names


def recipe_namespace():
    """(root recipe names, {module: {recipe names}}).

    Parsed from the files rather than from `just --summary`, so private recipes
    are included — a recipe body resolves against everything `just` knows, not
    everything it advertises.
    """
    root_file = REPO / "justfile"
    text = root_file.read_text()
    roots = names_in(root_file)
    # `import` is a MERGE, so its names belong to the root namespace.
    for rel in IMPORT_DEF.findall(text):
        f = REPO / rel
        if f.exists():
            roots |= names_in(f)
    mods = {}
    for name, rel in MOD_DEF.findall(text):
        mods[name] = names_in(REPO / rel)
    return roots, mods


# `-p <pkg> … --test <target>` on one line. Cargo resolves `--test` against the
# package's `tests/` dir, so a deleted test file leaves the recipe naming a
# target that cannot exist — the same "a deletion left a caller" shape as a
# dangling recipe reference, and just as invisible until someone runs it.
#
# Found by fixing issue 0660: with the `build-zenohd` error gone,
# `just native test-rmw` reached cargo and said "no test target named `rmw`".
# `tests/rmw.rs` was deleted in `6e56ce202` (phase-115.L.7/L.8, "collapse to
# vtable-only public RMW surface") — so that recipe had been dead far longer
# than the router calls were, behind an error that masked it.
PKG_FLAG = re.compile(r"-p\s+([A-Za-z0-9_-]+)")
TEST_FLAG = re.compile(r"--test\s+([A-Za-z0-9_-]+)")


def package_dirs():
    """{package name: directory} for in-repo crates, from their manifests."""
    out = {}
    for man in REPO.glob("packages/**/Cargo.toml"):
        if any(part in ("target", "third-party", "generated") for part in man.parts):
            continue
        m = re.search(r'^\s*name\s*=\s*"([^"]+)"', man.read_text(errors="replace"), re.M)
        if m:
            out.setdefault(m.group(1), man.parent)
    return out


def missing_test_targets(pkgs):
    """[(path, lineno, pkg, target, line)] for `--test` names with no file."""
    bad = []
    for path in just_files():
        if not path.is_file():
            continue
        for lineno, line in enumerate(path.read_text().splitlines(), 1):
            if COMMENT.match(line):
                continue
            tests = TEST_FLAG.findall(line)
            if not tests:
                continue
            pkg_names = PKG_FLAG.findall(line)
            for pkg in pkg_names:
                d = pkgs.get(pkg)
                if d is None:
                    continue
                for target in tests:
                    if not (d / "tests" / f"{target}.rs").is_file():
                        bad.append((path, lineno, pkg, target, line.strip()))
    return bad


def just_files():
    yield REPO / "justfile"
    yield from sorted((REPO / "just").glob("*.just"))
    # Workflows are `just <recipe>` callers by convention
    # (docs/development/ci-workflow-reorg.md), so a recipe deleted here breaks
    # CI silently — this gate read only `just/` and never `.github/`.
    #
    # That is not hypothetical. `just zenohd setup` built the VENDORED router;
    # RFC-0075 / phase-362 deleted it and made the router ROS's own
    # `rmw_zenohd`, but two host-tests steps kept calling the old form. The
    # top-level recipe is `zenohd locator="tcp/..."`, so `setup` was passed as a
    # LOCATOR and the step could never work. host-tests was red on it for days,
    # and nothing in the tree could see it.
    yield from sorted((REPO / ".github" / "workflows").glob("*.yml"))


def offenders(roots, mods):
    bad = []
    for path in just_files():
        if not path.is_file():
            continue
        for lineno, line in enumerate(path.read_text().splitlines(), 1):
            if COMMENT.match(line):
                continue
            for m in JUST_CALL.finditer(line):
                first, second = m.group(2), m.group(3)
                if "{{" in line[m.start(): m.end()]:
                    continue
                if first in roots:
                    continue
                if first in mods and (second is None or second in mods[first]):
                    continue
                # A module named without a recipe is `just <mod>` listing it.
                if first in mods:
                    continue
                bad.append((path, lineno, first, line.strip()))
    return bad


SELF_TESTS = [
    ("just build-zenohd", "build-zenohd", True),
    ("    just build-fixtures", "build-fixtures", True),
    ("@just check", "check", True),
    ("just native test-c", "native", False),
    ("cd x && just setup-cli", "setup-cli", True),
    ("# just build-zenohd", None, False),
    ("just {{recipe}}", None, False),
    ("just --list", None, False),
]


def self_test():
    """The parser must find the shapes that broke, and not the ones that did not."""
    failures = 0
    for line, expect_name, should_match in SELF_TESTS:
        if COMMENT.match(line):
            got = None
        else:
            ms = [m for m in JUST_CALL.finditer(line) if "{{" not in line]
            got = ms[0].group(2) if ms else None
        ok = (got == expect_name) if should_match else (got in (None, expect_name))
        if should_match and got != expect_name:
            ok = False
        if not ok:
            failures += 1
            print(f"  FAIL  {line!r}: expected {expect_name!r}, parsed {got!r}")
        else:
            print(f"  ok    {line!r} -> {got!r}")
    if failures:
        print(f"\ncheck-just-recipe-refs --self-test: {failures} case(s) FAILED")
        return 1
    print(f"\ncheck-just-recipe-refs --self-test: {len(SELF_TESTS)} case(s) OK")
    return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        return self_test()

    roots, mods = recipe_namespace()
    if not roots:
        raise SystemExit(
            "check-just-recipe-refs: parsed NO recipes out of the justfile — "
            "the definition regex has rotted, and a gate with an empty "
            "expectation passes forever"
        )
    bad = offenders(roots, mods)
    pkgs = package_dirs()
    missing = missing_test_targets(pkgs)
    if not bad and not missing:
        print(
            f"check-just-recipe-refs: OK ({len(roots)} root recipe(s), "
            f"{len(mods)} module(s), every `just <recipe>` and `--test` target resolves)"
        )
        return 0
    if missing:
        print("check-just-recipe-refs: a recipe names a cargo test target that "
              "does not exist:", file=sys.stderr)
        for path, lineno, pkg, target, line in missing:
            print(f"  {path.relative_to(REPO)}:{lineno}: -p {pkg} --test {target} "
                  f"(no {pkg}/tests/{target}.rs)", file=sys.stderr)
            print(f"      {line}", file=sys.stderr)
        print(
            "\n"
            "  Same shape as a dangling recipe reference: a deletion left a caller,\n"
            "  and cargo only says so when the recipe RUNS. Point it at the target\n"
            "  that replaced the coverage, or delete the recipe — but do not leave\n"
            "  it naming a file nobody has had since the deletion.\n",
            file=sys.stderr,
        )
    if not bad:
        return 1

    print("check-just-recipe-refs: a recipe body calls a recipe that does not exist:",
          file=sys.stderr)
    for path, lineno, name, line in bad:
        print(f"  {path.relative_to(REPO)}:{lineno}: `just {name}`", file=sys.stderr)
        print(f"      {line}", file=sys.stderr)
    print(
        "\n"
        "  `just` resolves a recipe reference only when the recipe RUNS, so this\n"
        "  parses fine and dies on invocation. Issue 0660 is twelve such callers\n"
        "  left behind when phase-362 W4 deleted `build-zenohd`; tier 1 stayed\n"
        "  green because `ci` runs `test-all` and never these.\n"
        "\n"
        "  Delete the call, or point it at the recipe that replaced it.\n",
        file=sys.stderr,
    )
    return 1


def selftest(verbose=False):
    """Prove the import/mod distinction is honoured. Runs on every invocation."""
    import tempfile
    ok = fail = 0

    def chk(desc, cond):
        nonlocal ok, fail
        if verbose or not cond:
            print(f"  {'ok   ' if cond else 'FAIL '} {desc}")
        ok += 1 if cond else 0
        fail += 0 if cond else 1

    chk("`import \"x.just\"` is recognised",
        IMPORT_DEF.findall('import "just/check.just"\n') == ["just/check.just"])
    chk("single-quoted import too",
        IMPORT_DEF.findall("import 'just/check.just'\n") == ["just/check.just"])
    chk("`mod` is NOT read as an import — it namespaces, it does not merge",
        IMPORT_DEF.findall("mod native 'just/native.just'\n") == [])
    chk("an indented `import` inside a body is not a declaration",
        IMPORT_DEF.findall("recipe:\n    import 'x'\n") == [])

    with tempfile.TemporaryDirectory() as d:
        f = Path(d) / "x.just"
        f.write_text("# c\nfoo-bar:\n    echo hi\n\nalias fb := foo-bar\n")
        got = names_in(f)
        chk("names_in reads recipes and aliases from an imported file",
            {"foo-bar", "fb"} <= got)

    if verbose:
        print(f"\n{ok} passed, {fail} failed")
    if fail:
        print("check-just-recipe-refs self-test: FAILED", file=sys.stderr)
        raise SystemExit(1)
    return 0


if __name__ == "__main__":
    if "--selftest" in sys.argv:
        sys.exit(selftest(verbose=True))
    # Always, not only behind the flag: a negative control nobody runs decays
    # into a comment.
    selftest()
    sys.exit(main())
