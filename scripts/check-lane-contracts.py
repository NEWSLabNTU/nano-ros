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
#
# AUTHORED, and deliberately so: these two make a claim in CLAUDE.md that a
# reader relies on. Everything else this gate checks is DERIVED from the
# workflows (see `ci_job_lanes`), because a hand-maintained list of "tiers CI
# runs" is exactly what went stale — phase-395 put `check-build` on the merge
# group, nothing here knew, and the required check was red for every pull
# request for a day (phase-396).
# Keyed by the recipe name as `just --show` resolves it. Both tiers now live in
# MODULES (`just ci l1`, `just check fast`), so the key is the module path —
# spelling them the old flat way made this gate report "no such recipe" for a
# lane that had simply moved, which is the same false-staleness it exists to
# catch elsewhere.
LANES = {
    "ci::l1": "compile + unit, NO fixture build (CLAUDE.md)",
    "check::fast": "buildless and source-only",
}

WORKFLOW_DIR = os.path.join(ROOT, ".github", "workflows")

# Recipes that PRODUCE the artifacts a tier may need. A CI job that runs a tier
# needing one of these must run the producer too — in the same job, since
# nothing carries a build dir between jobs.
PRODUCERS = (
    "build-test-fixtures",
    "build-compile-check-fixtures",
    "generate-bindings",
    "build-examples",
)


PRODUCER_CALL = re.compile(r"just\s+(build-test-fixtures|build-compile-check-fixtures"
                           r"|generate-bindings|build-examples)")


def required_producers(recipes, reached):
    """{producer: recipe} for every recipe in `reached` that HARD-FAILS telling
    you to run a producer first.

    The declaration is the recipe's own remediation text. `native::check` ends:

        echo "  Run 'just generate-bindings' (or 'just build-test-fixtures')..."
        exit 1

    which is a precondition stated in the only place it was ever stated. Keyed
    on the pair (mentions a producer, can exit non-zero) so an ADVISORY mention
    — a comment, a hint printed on success — does not count.
    """
    out = {}
    for r in reached:
        body = "\n".join(recipes.get(r, {}).get("body", []))
        if "exit 1" not in body and "exit 2" not in body:
            continue
        for m in PRODUCER_CALL.finditer(body):
            out.setdefault(m.group(1), r)
    return out


def workflow_jobs():
    """[(workflow, job, [just recipes the job runs], [producers it runs])].

    Text-scanned rather than YAML-parsed: the `run:` blocks are shell, the
    `if:` guards are GitHub expressions, and this gate only needs "which
    recipes does this job invoke" — a question the text answers exactly.
    """
    out = []
    if not os.path.isdir(WORKFLOW_DIR):
        return out
    job_re = re.compile(r"^  ([A-Za-z0-9_-]+):\s*$")
    just_re = re.compile(r"(?:^|[;&|]|\s)just\s+((?:[a-z0-9-]+\s*)+)")
    for fn in sorted(os.listdir(WORKFLOW_DIR)):
        if not fn.endswith((".yml", ".yaml")):
            continue
        path = os.path.join(WORKFLOW_DIR, fn)
        with open(path, encoding="utf8", errors="replace") as fh:
            lines = fh.read().split("\n")
        # Workflow-level events, used when a step carries no `if:`.
        wf_events = set(re.findall(r"^\s{2}(pull_request|merge_group|push|schedule"
                                   r"|workflow_dispatch|workflow_run)\s*:", "\n".join(lines),
                                   re.M))
        job, recipes = None, []
        step_if = ""
        in_jobs = False
        for line in lines:
            if line.startswith("jobs:"):
                in_jobs = True
                continue
            if not in_jobs:
                continue
            m = job_re.match(line)
            if m:
                if job:
                    out.append((fn, job, recipes))
                job, recipes, step_if = m.group(1), [], ""
                continue
            # A new step resets the guard; `if:` inside a step sets it. Steps
            # are the granularity that matters — one job runs `check-fast` on
            # every event and `check-build` on only some, and treating the job
            # as uniform is how a nightly-only tier reads as a required one.
            if re.match(r"^\s*- (name|uses|run):", line):
                if re.match(r"^\s*- (name|uses):", line):
                    step_if = ""
            if re.match(r"^\s+if:", line):
                step_if = line
            # Only `run:` shell counts. A step NAMED "just check build + no_std"
            # is a label, and reading it as an invocation attributed the recipe
            # to every event the workflow has — which is precisely the
            # nightly-vs-required distinction this is here to make.
            if re.match(r"^\s*-?\s*(name|uses):", line):
                continue
            if job and "just " in line and not line.lstrip().startswith("#"):
                ev = _events_of(step_if, wf_events)
                for jm in just_re.finditer(line):
                    recipes.extend(
                        (w, ev) for w in jm.group(1).split()
                        if not w.startswith("-")
                    )
        if job:
            out.append((fn, job, recipes))
    return [
        (w, j, r, [x for x, _ in r if x in PRODUCERS]) for w, j, r in out
    ]


# Events on which a merge cannot happen without the step passing. A tier that
# runs here MUST be satisfiable by its own job; anywhere else a broken tier is a
# bad lane, not a frozen repository.
GATING_EVENTS = {"pull_request", "merge_group"}


EVENT = r"(?:pull_request|merge_group|push|schedule|workflow_dispatch|workflow_run)"
# `contains(fromJSON('["a","b"]'), github.event_name)` — the shape every step
# guard in this repo uses, and the only one that is unambiguous.
_IN_LIST = re.compile(r"fromJSON\(\s*'\[([^\]]*)\]'\s*\)\s*,\s*github\.event_name")
# `github.event_name == 'x'` / `!= 'x'` — only when `github.event_name` is the
# left side. A bare quoted event elsewhere in the expression is NOT a claim
# about which events run.
_EQ = re.compile(r"github\.event_name\s*==\s*['\"](" + EVENT + r")['\"]")
_NE = re.compile(r"github\.event_name\s*!=\s*['\"](" + EVENT + r")['\"]")


def _events_of(step_if, wf_events):
    """Which events this guard can run on. Over-approximates on purpose.

    phase-396 follow-up. The first version keyed on "the guard mentions an
    event" and treated `!=` as exclusion unless an `==` appeared anywhere. That
    is wrong on the one guard that matters most — `pr-checks`'s `check` job:

        always() && (github.event_name != 'pull_request'
                     || needs.changes.outputs.code == 'true')

    The `==` there is about `needs.changes.outputs.code`, not about an event, so
    the old rule saw "both operators" and fell through to `named & wf_events`,
    concluding the job runs on `pull_request` ONLY. It runs on every event, and
    merely narrows the pull-request case to code-touching diffs.

    So: read only comparisons whose left side IS `github.event_name`, and when
    an exclusion is OR-ed with anything else, do not treat it as an exclusion —
    the other arm can still admit the event.

    The bias is deliberate. This feeds "does this lane gate a merge", where
    over-including costs an extra check and under-including silently drops a
    gating lane from the contract. Fail toward checking more.
    """
    if not step_if:
        return set(wf_events)
    g = str(step_if)

    in_list = _IN_LIST.search(g)
    if in_list:
        named = set(re.findall(r"['\"](" + EVENT + r")['\"]", in_list.group(1)))
        if "!" in g[: in_list.start()].rstrip()[-1:]:
            return set(wf_events) - named
        return (named & set(wf_events)) or named

    eq = set(_EQ.findall(g))
    if eq:
        return (eq & set(wf_events)) or eq

    ne = set(_NE.findall(g))
    if ne:
        # An exclusion that is one arm of an `||` does not exclude: the other
        # arm can admit the event. Only a lone negation narrows.
        if "||" in g:
            return set(wf_events)
        return set(wf_events) - ne

    return set(wf_events)

RUNTIME_RESOLVERS = (
    "require_entry_binary",
    "require_cmake_fixture",
    "require_idf_fixture",
    "require_west_fixture",
    "require_west_leaf_in_lane",
)
# Legitimate in a compile tier, when the gate produces them itself.
COMPILE_RESOLVERS = ("require_compile_check", "require_compile_check_bin")

# Parameters may be UPPERCASE and their defaults quoted — `check JOBS="75%":`
# is a real recipe header, and the old `[a-z_]+=\S*` matched neither the
# name nor the `"75%"`. It therefore skipped `native::check`, which is the
# one recipe whose missing precondition froze the merge queue (phase-396).
RECIPE = re.compile(
    r"^([a-z][a-z0-9-]*)"
    r"(?:\s+[A-Za-z_][A-Za-z0-9_]*(?:=(?:\"[^\"]*\"|'[^']*'|\S+))?)*"
    r"\s*:(.*)$"
)
CARGO_TEST = re.compile(r"--test\s+([A-Za-z0-9_]+)")


def parse_justfile():
    """{recipe: {deps, body}} across the root justfile AND `just/*.just`.

    phase-396 W5 — modules were invisible, and that is where the defect lived.
    `check-build`'s last dependency is `native::check`, which hard-requires
    generated message bindings; the closure walk stopped at the root justfile,
    so the gate could not see it and reported the tier clean while the required
    check was red for every pull request.

    Module recipes are keyed `<module>::<recipe>`, which is how the root
    justfile already spells them.
    """
    recipes = {}
    sources = [(None, JUSTFILE)]
    mod_dir = os.path.join(ROOT, "just")
    if os.path.isdir(mod_dir):
        sources += [
            (fn[:-5], os.path.join(mod_dir, fn))
            for fn in sorted(os.listdir(mod_dir)) if fn.endswith(".just")
        ]
    for mod, path in sources:
        try:
            with open(path, encoding="utf8", errors="replace") as fh:
                recipes.update(_parse_one(fh.read().split("\n"), mod))
        except OSError:
            continue
    return recipes


def _parse_one(lines, mod):
    """Parse one justfile; `mod` prefixes every recipe name when not None."""
    recipes, cur = {}, None
    i = 0
    while i < len(lines):
        line = lines[i]
        m = RECIPE.match(line)
        if m and not line.startswith((" ", "\t")):
            cur = f"{mod}::{m.group(1)}" if mod else m.group(1)
            dep_text = m.group(2)
            # A trailing `\` continues the dependency list onto later lines —
            # `check-build` spells its 30-odd dependencies that way.
            while dep_text.rstrip().endswith("\\") and i + 1 < len(lines):
                i += 1
                dep_text = dep_text.rstrip()[:-1] + " " + lines[i]
            deps = [d for d in re.split(r"[\s()]+", dep_text) if d and not d.startswith("#")]
            # A bare dep inside a module refers to that module's own recipe.
            if mod:
                deps = [d if "::" in d else f"{mod}::{d}" for d in deps]
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
# `ci-l1` declares no dependencies at all and calls `@just check cli-fresh
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

    recipes = recipes_map = parse_justfile()
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

    # ---- phase-396 W5 — the same rule, for every tier a CI JOB invokes ----
    #
    # The loop above checks two AUTHORED tiers. That is not where this bit us:
    # phase-395 put `check-build` on the merge group, `check-build` reaches
    # `native::check` (generated message bindings) and `check-source-gates`
    # (`.compile-ok` stamps), the job produces neither, and the required check
    # was red for EVERY pull request for a day. Nothing here looked, because
    # `check-build` was not in LANES.
    #
    # So the lane list is now DERIVED: every `just <recipe>` any workflow job
    # runs is a lane, and the artifacts it may resolve are the ones that JOB
    # produces — not the ones some other job, or a developer's tree, happens to
    # have.
    #
    # A RATCHET, because the derived set legitimately contains known-bad states
    # today (the nightly arm of `pr-checks/check` still runs `check-build`
    # without a producer — same defect, on a lane issue 0878 has already
    # established nobody is watching). Recording them is the point: a new one
    # fails, and refreshing the baseline is a deliberate act.
    ci_findings = []
    for wf, job, recipes, producers in workflow_jobs():
        if producers:
            continue  # the job builds artifacts; it may resolve them
        for recipe, events in sorted({(r, tuple(sorted(e))) for r, e in recipes}):
            if recipe not in recipes_map:
                continue
            # Only lanes that GATE a merge. A broken tier on `schedule` is a bad
            # nightly (issue 0878's territory); a broken tier on `merge_group`
            # or `pull_request` is a repository nobody can merge into, which is
            # a different severity and the one this gate exists for.
            if not (set(events) & GATING_EVENTS):
                continue
            reached = closure(recipes_map, recipe)
            job_makes_stamps = any(
                "compile-check-fixtures.sh" in line
                for r in reached
                for line in recipes_map.get(r, {}).get("body", [])
            )
            for producer, via in sorted(required_producers(recipes_map, reached).items()):
                ci_findings.append(f"{wf}:{job} runs `{recipe}` -> `{via}` "
                                   f"hard-requires `just {producer}`, which the job never runs")
            for test, via in sorted(tests_invoked(recipes_map, reached).items()):
                used, found = resolvers_used(test)
                if not found or not used:
                    continue
                runtime = sorted(u for u in used if u in RUNTIME_RESOLVERS)
                if runtime:
                    ci_findings.append(f"{wf}:{job} runs `{recipe}` -> `{test}` "
                                       f"needs RUNTIME fixture ({','.join(runtime)})")
                elif not job_makes_stamps:
                    ci_findings.append(f"{wf}:{job} runs `{recipe}` -> `{test}` "
                                       f"needs a COMPILE stamp nothing in the job builds")

    base_path = os.path.join(ROOT, ".config", "lane-contract-baseline.json")
    if "--update" in sys.argv:
        os.makedirs(os.path.dirname(base_path), exist_ok=True)
        import json as _json
        with open(base_path, "w", encoding="utf8") as fh:
            _json.dump({
                "_comment": (
                    "CI jobs that run a tier needing an artifact the job does not "
                    "build (phase-396 W5). Each line is a required check that "
                    "cannot pass on a clean runner. Refresh with --update and say "
                    "why in the commit."
                ),
                "findings": sorted(set(ci_findings)),
            }, fh, indent=2)
            fh.write("\n")
        print(f"check-lane-contracts: baseline written — {len(set(ci_findings))} known.")
        return 0

    known = set()
    if os.path.exists(base_path):
        import json as _json
        with open(base_path, encoding="utf8") as fh:
            known = set(_json.load(fh)["findings"])
    for f in sorted(set(ci_findings) - known):
        errs.append(
            f"{f}.\n"
            f"      A CI job may resolve an artifact only if that JOB builds it —\n"
            f"      nothing carries a build dir between jobs, and a developer tree\n"
            f"      where `build-test-fixtures` has run is not the runner. This is\n"
            f"      the shape that froze the merge queue (phase-396): a required\n"
            f"      check red for every input looks exactly like a broken PR.\n"
            f"      Add the producer to the job, or take the tier off that event.\n"
            f"      If it is intended, record it:\n"
            f"        python3 scripts/check-lane-contracts.py --update"
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

    gating = sum(
        1 for _w, _j, r, p in workflow_jobs() if not p
        for _rec, ev in {(a, tuple(sorted(b))) for a, b in r}
        if set(ev) & GATING_EVENTS
    )
    print(
        f"check-lane-contracts OK — {checked} test target(s) across "
        f"{len(LANES)} affordability tier(s) and {gating} merge-gating CI "
        f"lane invocation(s); none resolves an artifact its job does not build."
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
    # ---- phase-396 W5: the pieces that were BLIND, each with a case ----

    # The recipe header regex skipped `check JOBS="75%":` — uppercase parameter,
    # quoted default with a `%`. That one miss hid `native::check`, which is the
    # recipe whose unmet precondition froze the queue.
    mod = _parse_one(['check JOBS="75%":', '    echo hi', '', 'other:', '    x'], "native")
    chk("a recipe with an UPPERCASE quoted-default parameter is parsed",
        "native::check" in mod)
    chk("module recipes are keyed <module>::<recipe>", "native::other" in mod)
    bare = _parse_one(["a: b", "    x", "b:", "    y"], "native")
    chk("a bare dep inside a module resolves to that module",
        bare["native::a"]["deps"] == ["native::b"])

    # The producer rule: a hard-failing remediation IS the declaration.
    rp = {"r": {"deps": [], "body": ["    echo \"Run 'just generate-bindings' first\"",
                                     "    exit 1"]}}
    chk("a hard-failing 'run just <producer> first' is a declared precondition",
        required_producers(rp, ["r"]) == {"generate-bindings": "r"})
    advisory = {"r": {"deps": [], "body": ["    echo 'hint: just generate-bindings'"]}}
    chk("an ADVISORY mention with no failure path is not a precondition",
        required_producers(advisory, ["r"]) == {})

    # Event attribution decides required-vs-nightly, which is the whole severity
    # split. A step's `if:` wins over the workflow's event list.
    allev = {"pull_request", "merge_group", "push", "schedule"}
    chk("no `if:` means every event the workflow declares",
        _events_of("", allev) == allev)
    chk("a fromJSON list narrows to exactly those events",
        _events_of("""if: ${{ contains(fromJSON('["schedule","workflow_dispatch"]'), github.event_name) }}""",
                   allev) == {"schedule"})
    chk("a merge_group step is still gating",
        bool(_events_of("""if: ${{ contains(fromJSON('["merge_group"]'), github.event_name) }}""",
                        allev) & GATING_EVENTS))
    chk("a schedule-only step is NOT gating",
        not (_events_of("""if: ${{ contains(fromJSON('["schedule"]'), github.event_name) }}""",
                        allev) & GATING_EVENTS))
    chk("a LONE `!=` guard subtracts rather than selects",
        _events_of("if: ${{ github.event_name != 'pull_request' }}", allev)
        == allev - {"pull_request"})
    # The guard this got wrong. `pr-checks`'s `check` job runs on EVERY event
    # and merely narrows the pull-request case to code-touching diffs; the old
    # rule read "both != and == are present" and concluded pull_request ONLY.
    chk("an exclusion OR-ed with a non-event condition does NOT exclude",
        _events_of("if: ${{ always() && (github.event_name != 'pull_request'"
                   " || needs.changes.outputs.code == 'true') }}", allev) == allev)
    chk("a quoted event that is not compared to github.event_name is ignored",
        _events_of("if: ${{ needs.x.outputs.name == 'merge_group' }}", allev) == allev)
    chk("`github.event_name == 'x'` selects exactly x",
        _events_of("if: ${{ github.event_name == 'pull_request' }}", allev)
        == {"pull_request"})

    if verbose:
        print(f"\n{ok} passed, {fail} failed")
    if fail:
        print("check-lane-contracts self-test: FAILED", file=sys.stderr)
        raise SystemExit(1)
    return 0


if __name__ == "__main__":
    sys.exit(main())
