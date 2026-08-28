#!/usr/bin/env python3
"""A tier's affordability claim must be TRUE, not aspirational — phase-395.

CLAUDE.md says `just ci-l1` is "compile + unit, NO FIXTURES … it needs no
fixture build, no SDK, no QEMU and no cross toolchain". That claim was FALSE,
and had been for as long as it had been written down: `ci-l1` -> `check-build`
-> `check-source-gates` -> `platform_header_compile`, which resolves a fixture.

Nothing noticed, because the push lane runs `check-fast` alone. It surfaced only
when the PR lane became a REQUIRED status check and every CI run went red on
`BuildFailed("Test fixture binary not prebuilt")` — a required check that could
never pass, which is the frozen-repo failure this campaign has now met four
times.

A tier claim nobody can check is a promise about COST that quietly stops being
true, and cost is the whole reason the tier exists: an instruction nobody can
afford gets followed selectively, which is worse than a smaller instruction
followed honestly.

THE DISTINCTION THAT MATTERS: COMPILE-STAGE vs RUNTIME

Not every `require_*` is equal, and collapsing them would ban something
legitimate:

  * COMPILE-STAGE — `require_compile_check{,_bin}`. A `.compile-ok` stamp from a
    `cargo check` of a small template crate: ~13 s, no SDK, no emulator. That is
    a compile artifact, so it BELONGS in a compile tier. The gate simply has to
    PRODUCE it rather than assume someone else did — which is the fix that ships
    alongside this gate.
  * RUNTIME — `require_entry_binary`, `require_cmake_fixture`,
    `require_idf_fixture`, `require_west_fixture`. These need
    `build-test-fixtures`: an SDK, a cross toolchain, sometimes QEMU. Nothing
    reachable from L1 may touch one, because L1's entire value is being
    affordable before every push.

Usage::

    check-lane-contracts.py [--selftest]
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
JUSTFILE = os.path.join(ROOT, "justfile")
TESTS_DIR = os.path.join(ROOT, "packages", "testing", "nros-tests", "tests")

# Tiers that promise affordability, and what each promises.
LANES = {
    "ci-l1": "compile + unit, NO fixture build (CLAUDE.md)",
    "check-fast": "buildless and source-only",
}

RUNTIME_RESOLVERS = (
    "require_entry_binary",
    "require_cmake_fixture",
    "require_idf_fixture",
    "require_west_fixture",
    "require_west_leaf_in_lane",
)
# Legitimate in a compile tier, when the gate produces them itself.
COMPILE_RESOLVERS = ("require_compile_check", "require_compile_check_bin")

RECIPE = re.compile(r"^([a-z][a-z0-9-]*)\s*(?:[a-z_]+=\S*\s*)*:(.*)$")
CARGO_TEST = re.compile(r"--test\s+([A-Za-z0-9_]+)")


def parse_justfile():
    """{recipe: {deps, body}} — enough to walk a dependency closure."""
    with open(JUSTFILE, encoding="utf8") as fh:
        lines = fh.read().split("\n")
    recipes, cur = {}, None
    i = 0
    while i < len(lines):
        line = lines[i]
        m = RECIPE.match(line)
        if m and not line.startswith((" ", "\t")):
            cur = m.group(1)
            dep_text = m.group(2)
            # A trailing `\` continues the dependency list onto later lines —
            # `check-build` spells its 30-odd dependencies that way.
            while dep_text.rstrip().endswith("\\") and i + 1 < len(lines):
                i += 1
                dep_text = dep_text.rstrip()[:-1] + " " + lines[i]
            deps = [d for d in re.split(r"[\s()]+", dep_text) if d and not d.startswith("#")]
            recipes[cur] = {"deps": deps, "body": []}
        elif cur and line.startswith((" ", "\t")):
            recipes[cur]["body"].append(line)
        elif not line.strip():
            pass
        else:
            cur = None
        i += 1
    return recipes


# `just a b c` inside a recipe body. Header dependencies are NOT the only edge
# in this justfile, and assuming they were made this gate pass over everything:
# `ci-l1` declares no dependencies at all and calls `@just check-cli-fresh
# check-fast check-build check-api-parity` in its body, so a header-only walk
# found a closure of size 1 and cheerfully reported "0 test target(s)" — a gate
# that verified nothing while printing OK.
JUST_CALL = re.compile(r"^\s*@?just\s+([a-z0-9-]+(?:\s+[a-z0-9-]+)*)\s*$")


def closure(recipes, root):
    seen, stack = set(), [root]
    while stack:
        r = stack.pop()
        if r in seen or r not in recipes:
            continue
        seen.add(r)
        stack.extend(recipes[r]["deps"])
        for line in recipes[r]["body"]:
            m = JUST_CALL.match(line)
            if m:
                stack.extend(m.group(1).split())
    return seen


def tests_invoked(recipes, names):
    """{test_name: recipe} for every `--test NAME` in the closure's bodies."""
    out = {}
    for r in names:
        for line in recipes.get(r, {}).get("body", []):
            if "cargo test" not in line and "nextest" not in line:
                continue
            for m in CARGO_TEST.finditer(line):
                out.setdefault(m.group(1), r)
    return out


ID_RE = re.compile(r'require_compile_check(?:_bin)?\(\s*"([A-Za-z0-9_]+)"')


def stamp_ids_used(test_name):
    """The compile-check fixture ids a test names literally."""
    path = os.path.join(TESTS_DIR, f"{test_name}.rs")
    if not os.path.exists(path):
        return set()
    with open(path, encoding="utf8", errors="replace") as fh:
        text = fh.read()
    ids = set(ID_RE.findall(text))
    # Most tests keep the ids in a `const FOO: &[&str] = &["a", "b"];` and pass
    # the loop variable, so the literal call site names nothing. Fall back to
    # every string literal that the manifest actually knows as a fixture id —
    # over-broad is safe here, since an id that is not in the manifest is
    # dropped by the caller.
    ids |= set(re.findall(r'"([a-z][a-z0-9_]{3,})"', text))
    return ids


def builder_of_ids():
    """{fixture id: builder} from the manifest — READ, never inferred."""
    manifest = os.path.join(ROOT, "examples", "fixtures.toml")
    if not os.path.exists(manifest):
        return {}
    try:
        import tomllib
    except ModuleNotFoundError:  # python < 3.11
        try:
            import tomli as tomllib
        except ModuleNotFoundError:
            return {}
    with open(manifest, "rb") as fh:
        d = tomllib.load(fh)
    return {
        f["id"]: f.get("builder")
        for f in d.get("compile_check_fixture", [])
        if f.get("id")
    }


def lane_filters(recipes, reached):
    """Every NROS_COMPILE_CHECK_LANES=... value the lane sets, as a set of
    builder names. Empty set means the lane never filters (all builders)."""
    out, filtered = set(), False
    for r in reached:
        for line in recipes.get(r, {}).get("body", []):
            m = re.search(r"NROS_COMPILE_CHECK_LANES=([A-Za-z0-9,_-]+)", line)
            if m:
                filtered = True
                out |= {x for x in re.split(r"[,\s]+", m.group(1)) if x}
    return out if filtered else None


def resolvers_used(test_name):
    path = os.path.join(TESTS_DIR, f"{test_name}.rs")
    if not os.path.exists(path):
        return set(), False
    with open(path, encoding="utf8", errors="replace") as fh:
        text = fh.read()
    return {r for r in RUNTIME_RESOLVERS + COMPILE_RESOLVERS if r in text}, True


def main():
    if "--selftest" in sys.argv:
        return selftest(verbose=True)
    # Always, not only behind the flag: a negative control nobody runs decays
    # into a comment.
    selftest()

    recipes = parse_justfile()
    errs, checked = [], 0

    for lane, promise in LANES.items():
        if lane not in recipes:
            errs.append(f"{lane}: no such recipe — this gate's lane list is stale")
            continue
        reached = closure(recipes, lane)
        lane_builders = lane_filters(recipes, reached)
        id_builder = builder_of_ids()
        # Does the lane PRODUCE compile-stage stamps anywhere in its closure?
        produces_stamps = any(
            "compile-check-fixtures.sh" in line
            for r in reached
            for line in recipes.get(r, {}).get("body", [])
        )
        for test, via in sorted(tests_invoked(recipes, reached).items()):
            used, found = resolvers_used(test)
            if not found:
                continue
            checked += 1
            bad = sorted(u for u in used if u in RUNTIME_RESOLVERS)
            if bad:
                errs.append(
                    f"{lane} reaches `{test}` (via `{via}`), which resolves a RUNTIME "
                    f"fixture: {', '.join(bad)}.\n"
                    f"      {lane} promises: {promise}.\n"
                    f"      A runtime fixture needs `build-test-fixtures` — an SDK, a\n"
                    f"      cross toolchain, sometimes QEMU. Either move the test to a\n"
                    f"      fixture-bearing lane, or make it a compile-stage check whose\n"
                    f"      gate produces its own stamp."
                )
                continue
            # The rule that would have caught the real defect. A compile-stage
            # resolver is ALLOWED, but only because the stamp is cheap enough
            # for the lane to produce — so the lane must actually produce it.
            # `platform_header_compile` used one legitimately while nothing in
            # the closure ran `compile-check-fixtures.sh`, so the lane silently
            # depended on `build-test-fixtures` having been run by someone,
            # somewhere. On a CI runner nobody had, and the required check was
            # red on `BuildFailed("Test fixture binary not prebuilt")`.
            if used and not produces_stamps:
                errs.append(
                    f"{lane} reaches `{test}` (via `{via}`), which resolves a "
                    f"COMPILE-STAGE stamp ({', '.join(sorted(used))}), but NOTHING in\n"
                    f"      the lane produces one — no `compile-check-fixtures.sh` in the\n"
                    f"      whole closure. The lane therefore depends on\n"
                    f"      `build-test-fixtures` having been run by someone else, which\n"
                    f"      is exactly what `{lane}` promises it does not need:\n"
                    f"      {promise}.\n"
                    f"      Have the gate build its own stamps (~13 s), as\n"
                    f"      `check-source-gates` does."
                )
                continue
            # PRESENCE IS NOT ENOUGH, and this is the half that was missing.
            # The rule above asks only "does the lane run the fixture builder at
            # all", which a lane passes even while filtering OUT the very
            # builder the test needs. `check-source-gates` requested
            # `NROS_COMPILE_CHECK_LANES=cargo-check` while all nine
            # `platform_hdr_*` rows are `cxx-syntax`, so the gate produced
            # stamps, produced the WRONG ones, and this check stayed green. It
            # failed only in CI, because locally the stamps already existed from
            # an earlier unfiltered build.
            if used and lane_builders is not None:
                want = {
                    id_builder[i] for i in stamp_ids_used(test)
                    if i in id_builder and id_builder[i]
                }
                missing = sorted(want - lane_builders)
                if missing:
                    errs.append(
                        f"{lane} reaches `{test}` (via `{via}`), whose stamps are built by\n"
                        f"      {', '.join(missing)} — but the lane filters to\n"
                        f"      NROS_COMPILE_CHECK_LANES={','.join(sorted(lane_builders))}.\n"
                        f"      The builder RUNS and produces the wrong rows, so the test\n"
                        f"      fails on a fresh checkout with 'Test fixture binary not\n"
                        f"      prebuilt' while passing anywhere the stamps happen to\n"
                        f"      survive from an earlier build.\n"
                        f"      Builders are READ from examples/fixtures.toml; do not\n"
                        f"      infer them from the fixture ids."
                    )

    if errs:
        print(f"check-lane-contracts: {len(errs)} tier violation(s):\n", file=sys.stderr)
        for e in errs:
            print(f"  - {e}", file=sys.stderr)
        print(
            "\n  A tier claim nobody can check is a promise about COST that quietly\n"
            "  stops being true. This one was false long enough to make a REQUIRED\n"
            "  CI check permanently red.",
            file=sys.stderr,
        )
        return 1

    print(
        f"check-lane-contracts OK — {checked} test target(s) across "
        f"{len(LANES)} affordability tier(s); none resolves a runtime fixture."
    )
    return 0


def selftest(verbose=False):
    """Prove it can fail. Runs on every invocation."""
    import tempfile

    real = (JUSTFILE, TESTS_DIR)
    ok = fail = 0

    def chk(desc, cond):
        nonlocal ok, fail
        if verbose or not cond:
            print(f"  {'ok   ' if cond else 'FAIL '} {desc}")
        if cond:
            ok += 1
        else:
            fail += 1

    with tempfile.TemporaryDirectory() as d:
        jf = os.path.join(d, "justfile")
        td = os.path.join(d, "tests")
        os.makedirs(td)
        globals()["JUSTFILE"], globals()["TESTS_DIR"] = jf, td

        with open(jf, "w", encoding="utf8") as fh:
            fh.write("ci-l1: gate-a\n\ngate-a:\n    cargo test -p x --test t_runtime\n")
        with open(os.path.join(td, "t_runtime.rs"), "w", encoding="utf8") as fh:
            fh.write('fn f() { require_cmake_fixture("a", "b"); }\n')
        r = parse_justfile()
        chk("a dependency is followed into the closure", "gate-a" in closure(r, "ci-l1"))
        chk("a RUNTIME resolver in a reached test is detected",
            "require_cmake_fixture" in resolvers_used("t_runtime")[0])

        with open(os.path.join(td, "t_compile.rs"), "w", encoding="utf8") as fh:
            fh.write('fn f() { require_compile_check("a"); }\n')
        used = resolvers_used("t_compile")[0]
        chk("a COMPILE-stage resolver is NOT a violation",
            bool(used) and not any(u in RUNTIME_RESOLVERS for u in used))

        with open(jf, "w", encoding="utf8") as fh:
            fh.write("ci-l1: \\\n    gate-a \\\n    gate-b\n\n"
                     "gate-b:\n    cargo test --test t_runtime\n")
        r = parse_justfile()
        chk("a backslash-continued dependency list is parsed",
            "gate-b" in closure(r, "ci-l1"))

        chk("a test file that does not exist is not a violation",
            resolvers_used("no_such_test") == (set(), False))

        # The shape `ci-l1` actually has: no header dependencies, gates invoked
        # from the BODY. Missing this made the gate report OK over an empty set.
        with open(jf, "w", encoding="utf8") as fh:
            fh.write("ci-l1:\n    @just gate-a check-other\n\n"
                     "gate-a:\n    cargo test --test t_runtime\n")
        r = parse_justfile()
        chk("a `just a b` call in a BODY is an edge, not just header deps",
            "gate-a" in closure(r, "ci-l1"))
        chk("...and the reached test is then actually inspected",
            "t_runtime" in tests_invoked(r, closure(r, "ci-l1")))

    globals()["JUSTFILE"], globals()["TESTS_DIR"] = real
    if verbose:
        print(f"\n{ok} passed, {fail} failed")
    if fail:
        print("check-lane-contracts self-test: FAILED", file=sys.stderr)
        raise SystemExit(1)
    return 0


if __name__ == "__main__":
    sys.exit(main())
