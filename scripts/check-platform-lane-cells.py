#!/usr/bin/env python3
"""Every platform module's `just <module> test` runs EVERY one of its cells.

Issue 1355, widened by issue 2997. Each platform's nightly job runs `just test
<module>`, which dispatches to that module's `test` recipe. 1355 caught a
recipe that forgot `rtos_e2e`; 2997 measured the general shape. The recipes
HAND-LISTED their test targets, and every list was a subset of the binaries
that answer the module's cells (`matrix::consumer_of` over `matrix::CELLS`):
NuttxArm's ran 3 of 6, ThreadxLinux's 1 of 5, QemuBaremetal's none, so three
boards read "no verdict" every night while their jobs passed.

The subject is DERIVED: `packages/testing/nros-tests/module-consumers.txt`,
generated from `matrix::module_consumers` and held current by its unit test
(`module_consumers_file_is_current`). The rule, for each module it names:

  * the module's `test` recipe takes its targets from
    `scripts/test/module-consumers.sh <module>` and passes them to
    `_nextest-platform`, so the list cannot be a subset of the cells;
  * it hands `_nextest-platform` no LITERAL target list (a quoted first
    argument), which is the shape that went stale.

And every `rtos_e2e` platform (`Platform`'s `Display`, whose strings ARE the
module names) has a line in the file, so a module cannot drop out of the
derivation unnoticed.

    python3 scripts/check-platform-lane-cells.py [--self-test]
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts/lib"))
from check_just_sources import just_modules  # noqa: E402

RTOS_E2E = ROOT / "packages/testing/nros-tests/tests/rtos_e2e.rs"

DISPLAY_IMPL = re.compile(
    r"impl\s+fmt::Display\s+for\s+Platform\s*\{(?P<body>.*?)\n\}", re.S
)
ARM = re.compile(r"Platform::(?P<variant>\w+)\s*=>\s*\"(?P<module>[a-z0-9_]+)\"")


def platforms(rust_text):
    """[(variant, module)] from `impl fmt::Display for Platform`."""
    m = DISPLAY_IMPL.search(rust_text)
    if not m:
        return []
    return [(a["variant"], a["module"]) for a in ARM.finditer(m["body"])]


def recipe_body(just_text, name):
    """The body of recipe `name` (its indented lines, comments dropped), or
    None if absent. A comment naming `rtos_e2e` selects nothing, so it must not
    satisfy the rule."""
    lines = just_text.splitlines()
    header = re.compile(rf"^{re.escape(name)}(\s|:)")
    for i, line in enumerate(lines):
        if not header.match(line):
            continue
        body = []
        for nxt in lines[i + 1 :]:
            if nxt.strip() and not nxt[0].isspace():
                break
            if not nxt.lstrip().startswith("#"):
                body.append(nxt)
        return "\n".join(body)
    return None


CONSUMERS = ROOT / "packages/testing/nros-tests/module-consumers.txt"
HELPER = "scripts/test/module-consumers.sh"
LITERAL_LIST = re.compile(r"_nextest-platform\s+'")


def modules_of(consumers_text):
    """The modules the derived consumer list names, in order."""
    out = []
    for line in consumers_text.splitlines():
        if line.strip() and not line.startswith("#"):
            m = line.split()[0]
            if m not in out:
                out.append(m)
    return out


def check(plats, module_text, consumers_text):
    """Problems, one line each. `module_text` maps a module name to its text."""
    problems = []
    if not plats:
        problems.append(
            f"{RTOS_E2E.relative_to(ROOT)}: no `impl fmt::Display for Platform` "
            "arms found — the gate cannot derive its subject"
        )
    modules = modules_of(consumers_text)
    if not modules:
        problems.append(
            f"{CONSUMERS.relative_to(ROOT)} names no module — the gate cannot "
            "derive its subject (regenerate it: see its header)"
        )
    for variant, module in plats:
        if modules and module not in modules:
            problems.append(
                f"Platform::{variant} displays as `{module}`, but "
                f"{CONSUMERS.name} has no `{module}` line: no Runtime cell of it "
                "is assigned, or the file is stale (regenerate it)"
            )
    for module in modules:
        text = module_text.get(module)
        if text is None:
            problems.append(
                f"{CONSUMERS.name} names module `{module}`, but no `just` module "
                "has that name — `just test {module}` cannot reach its cells"
            )
            continue
        body = recipe_body(text, "test")
        if body is None:
            problems.append(f"just module `{module}` has no `test` recipe")
            continue
        if f"{HELPER} {module}" not in body or "_nextest-platform" not in body:
            problems.append(
                f"`just {module} test` does not take its targets from "
                f"`{HELPER} {module}` and hand them to `_nextest-platform`: its "
                "nightly job runs a hand-picked subset of its cells, if any "
                "(issue 2997)."
            )
        if LITERAL_LIST.search(body):
            problems.append(
                f"`just {module} test` hands `_nextest-platform` a LITERAL target "
                "list: a hand list goes stale as cells are added. Pass the "
                f"derived list (`{HELPER} {module}`), plus any side target after "
                "it (issue 2997)."
            )
    return problems


def self_test():
    rust = (
        "impl fmt::Display for Platform {\n"
        "    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {\n"
        "        let s = match self {\n"
        '            Platform::Alpha => "alpha",\n'
        '            Platform::BetaTwo => "beta_two",\n'
        "        };\n"
        "    }\n"
        "}\n"
    )
    plats = platforms(rust)
    assert plats == [("Alpha", "alpha"), ("BetaTwo", "beta_two")], plats
    consumers = "# GENERATED\nalpha rtos_e2e\nbeta_two own\nbeta_two rtos_e2e\n"
    assert modules_of(consumers) == ["alpha", "beta_two"]

    def recipe(mod, line):
        return (
            '[group("main")]\n'
            'test verbose="": build-fixtures\n'
            "    #!/usr/bin/env bash\n"
            f"{line}"
            "\n"
            "test-all:\n"
            f"    bins=$(bash {HELPER} {mod})\n"
            "    just _nextest-platform \"$bins\" '' '' ''\n"
        )

    def good(mod, extra=""):
        return recipe(mod, f'    if ! bins="$(bash {HELPER} {mod})"; then exit 1; fi\n'
                           f"    just _nextest-platform \"$bins{extra}\" '{{{{verbose}}}}' '' ''\n")

    ok = {"alpha": good("alpha"), "beta_two": good("beta_two", " side_target")}
    assert check(plats, ok, consumers) == [], check(plats, ok, consumers)
    # The pre-2997 shape: a hand list (`test-all` below names the helper, so
    # this also proves the body stops at the next recipe).
    hand = dict(ok, beta_two=recipe(
        "beta_two", "    just _nextest-platform 'own rtos_e2e' '{{verbose}}' '' 'test(BetaTwo)'\n"))
    bad = check(plats, hand, consumers)
    assert len(bad) == 2 and all("beta_two" in b for b in bad), bad
    assert any("LITERAL" in b for b in bad) and any("does not take" in b for b in bad), bad
    # The helper for ANOTHER module does not count.
    other = dict(ok, beta_two=good("alpha"))
    assert len(check(plats, other, consumers)) == 1, check(plats, other, consumers)
    # A comment naming the helper runs nothing.
    commented = dict(ok, beta_two=recipe(
        "beta_two", f"    # bash {HELPER} beta_two _nextest-platform\n    true\n"))
    assert len(check(plats, commented, consumers)) == 1
    # An rtos_e2e platform the derived file does not name; a named module with
    # no `just` module; one with no `test` recipe; an empty file.
    assert any("has no `alpha` line" in p
               for p in check(plats, ok, "beta_two own\n"))
    assert "no `just` module" in check(plats[1:], {"beta_two": ok["beta_two"]},
                                       "gamma x\nbeta_two own\n")[0]
    assert "no `test` recipe" in check(plats[:1], {"alpha": "build:\n    true\n"},
                                       "alpha rtos_e2e\n")[0]
    assert check(plats, ok, "# only a header\n") != []
    assert check([], {}, consumers) != []
    assert platforms("enum Platform { A }") == []


def main(argv):
    self_test()
    if "--self-test" in argv:
        print("check-platform-lane-cells: self-test OK")
        return 0
    plats = platforms(RTOS_E2E.read_text())
    module_text = {
        name: "\n".join(Path(f).read_text() for f in files)
        for name, files in just_modules(ROOT).items()
        if name
    }
    consumers_text = CONSUMERS.read_text() if CONSUMERS.is_file() else ""
    problems = check(plats, module_text, consumers_text)
    if problems:
        print("check-platform-lane-cells: FAIL", file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        return 1
    mods = modules_of(consumers_text)
    print(f"check-platform-lane-cells: OK ({len(mods)} modules take their derived "
          f"cell consumers: {', '.join(mods)})")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
