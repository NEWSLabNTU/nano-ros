#!/usr/bin/env python3
"""Every `rtos_e2e` platform's own `just <module> test` must run its cells.

Issue 1355. `packages/testing/nros-tests/tests/rtos_e2e.rs` holds the
pubsub/service/action cells of every RTOS platform, one `Platform` variant each,
and each platform's nightly job runs `just test <module>`, which dispatches to
that module's `test` recipe. A recipe that runs only its platform's OWN target
and forgets `rtos_e2e` reports a verdict over a handful of side tests while the
cells the coordinate exists for never run: `threadx_riscv64` did exactly that,
and its nightly job went GREEN on 2026-10-21 over four Cyclone/errno tests.

Derived, not authored: the platforms come from `Platform`'s `Display` impl, whose
strings ARE the `just` module names (`"threadx_riscv64"`), and the modules come
from `scripts/lib/check_just_sources.py`, the one reader of the `just` graph. A
platform added to the enum is checked with nobody remembering to list it.

The rule: the module's `test` recipe names the `rtos_e2e` target and the
`test(<Variant>)` filter that selects its cells. Matching on text is deliberate:
both are literals the recipe hands to `_nextest-platform`, and a recipe that
reaches them some other way should spell them where a reader can see them too.

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


def check(plats, module_text):
    """Problems, one line each. `module_text` maps a module name to its text."""
    problems = []
    if not plats:
        problems.append(
            f"{RTOS_E2E.relative_to(ROOT)}: no `impl fmt::Display for Platform` "
            "arms found — the gate cannot derive its subject"
        )
    for variant, module in plats:
        text = module_text.get(module)
        if text is None:
            problems.append(
                f"Platform::{variant} displays as `{module}`, but no `just` module "
                "has that name — `just test {module}` cannot reach its cells"
            )
            continue
        body = recipe_body(text, "test")
        if body is None:
            problems.append(f"just module `{module}` has no `test` recipe")
            continue
        missing = [
            want
            for want in ("rtos_e2e", f"test({variant})")
            if want not in body
        ]
        if missing:
            problems.append(
                f"`just {module} test` does not select its rtos_e2e cells "
                f"(missing {', '.join(repr(w) for w in missing)}). Its nightly "
                "job would report a verdict that never ran "
                f"`Platform__{variant}` — pass `rtos_e2e` and "
                f"`test({variant})` to `_nextest-platform` (issue 1355)."
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

    good = (
        '[group("main")]\n'
        'test verbose="": build-fixtures\n'
        "    #!/usr/bin/env bash\n"
        "    just _nextest-platform 'own rtos_e2e' '{{verbose}}' '' "
        "'binary(=own) | test(BetaTwo)'\n"
        "\n"
        "test-all:\n"
        "    cargo nextest run --test rtos_e2e -E 'test(Alpha)'\n"
    )
    # The pre-1355 riscv64 shape: own target only. `test-all` below it names
    # both literals, so this also proves the body stops at the next recipe.
    own_only = good.replace("'own rtos_e2e'", "own").replace(
        " | test(BetaTwo)", ""
    ).replace("test(Alpha)", "test(BetaTwo)")
    assert check(plats[1:], {"beta_two": good}) == []
    bad = check(plats[1:], {"beta_two": own_only})
    assert len(bad) == 1 and "'rtos_e2e'" in bad[0], bad
    assert "test(BetaTwo)" in bad[0], bad
    # A comment that names both literals runs nothing.
    commented = own_only.replace(
        "    #!/usr/bin/env bash\n",
        "    #!/usr/bin/env bash\n    # runs rtos_e2e test(BetaTwo) (not really)\n",
    )
    assert len(check(plats[1:], {"beta_two": commented})) == 1
    # A platform with no module, and a module with no `test` recipe.
    assert "no `just` module" in check(plats[:1], {})[0]
    assert "no `test` recipe" in check(plats[:1], {"alpha": "build:\n    true\n"})[0]
    # No enum found is a failure, never a vacuous pass.
    assert check([], {}) != []
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
    problems = check(plats, module_text)
    if problems:
        print("check-platform-lane-cells: FAIL", file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        return 1
    names = ", ".join(m for _, m in plats)
    print(f"check-platform-lane-cells: OK ({len(plats)} platforms: {names})")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
