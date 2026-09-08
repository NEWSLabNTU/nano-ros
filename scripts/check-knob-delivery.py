#!/usr/bin/env python3
"""phase-412 W4 -- the value that reaches the COMPILE is the value the resolver produced.

WHY THIS EXISTS, and why it is not the gate that was first proposed.

phase-412 W1 wired four session pools and produced FOUR delivery failures in
one wave. Every existing gate stayed green through all four, because they check
the INVENTORY (is the count right) and the RESOLVER (does precedence work), and
all four failures were downstream of both:

  1. a second consumer     the zpico C defines read raw CONFIG_*, before the
                           resolver ran
  2. a name that emptied   a foreach built NROS_DERIVED_NROS_MAX_SUBSCRIBERS,
                           which names nothing; cmake yields EMPTY, not an error
  3. no consumer default   rung 4 leaves a knob UNRESOLVED so a Rust build
                           script uses its own literal; a C define has none, so
                           it expands to nothing and sizes an array to zero
  4. a loader whitelist    the fragment set the symbol, the loader did not name
                           it, and it died at the function boundary -- the
                           pools derived 10/14/0 and the SESSION was built with
                           8/8/8. Eight subscriber slots for ten subscriptions,
                           which fails at RUNTIME rather than at link

The originally proposed gate -- "every published symbol has a consumer" --
catches NONE of these. In all four the symbol had a consumer; a different
consumer read around it, or a name never matched, or the value was legitimately
absent, or it never crossed a scope.

What catches all four is the end-to-end identity: for each knob, the number the
compiler was handed must equal the number the resolver decided. This asserts
that, against a real configured build dir.

Usage:  check-knob-delivery.py <build-dir>
        check-knob-delivery.py --self-test
"""
import os
import re
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# The ONE file that owns the resolver road. Every `NROS_DERIVED_*` fact that
# reaches a Zephyr compile does it through a `_nros_resolve_derivable_knob`
# call here, so this is what `DERIVED_PAIRS` is cross-checked against.
RESOLVER_CMAKE = os.path.join(ROOT, "zephyr", "cmake", "nros_cargo_build.cmake")

# Knobs whose resolved value must reach a C compile definition, and the define
# it must reach. Extend this when a knob gains a C consumer -- the pairing is
# the thing under test, so it is stated rather than discovered.
C_DEFINE_KNOBS = {
    "ZPICO_MAX_PUBLISHERS": "ZPICO_MAX_PUBLISHERS",
    "ZPICO_MAX_SUBSCRIBERS": "ZPICO_MAX_SUBSCRIBERS",
    "ZPICO_MAX_QUERYABLES": "ZPICO_MAX_QUERYABLES",
    "ZPICO_MAX_LIVELINESS": "ZPICO_MAX_LIVELINESS",
}

# Knobs that are DERIVED and must not silently lose their derivation on the way
# to the resolver: fragment value -> resolved value. A mismatch here is failure
# 4 above, and it is invisible in the fragment and in the build alike.
#
# ONE FACT, SEVERAL KNOBS (phase-412 #7). This map used to be 1:1, and the
# resolver road is not: `nros_resolve_knobs()` calls
# `_nros_resolve_derivable_knob` THIRTEEN times over ELEVEN facts.
# `NROS_DERIVED_MAX_SUBSCRIBERS` feeds both `NROS_MAX_SUBSCRIBERS` and the XRCE
# backend's `NROS_XRCE_MAX_SUBSCRIBERS`; `NROS_DERIVED_MAX_QUERYABLES` feeds
# both `NROS_MAX_QUERYABLES` and `NROS_XRCE_MAX_SERVICE_SERVERS`. A 1:1 map can
# name only one of each pair, so a value that ARRIVED at the zenoh slot and was
# dropped at the XRCE one read GREEN -- the gate reported on the half it could
# see, which is the same shape as the one-fragment bug `read_fragment` records
# one layer down.
#
# The map is AUTHORED (the pairing is the thing under test, so it is stated
# rather than discovered) and an authored map DRIFTS -- the `rmw-api-parity`
# lesson, where an authored map and a discovering tool disagreed by 25 symbols
# while both reported green. `check_registry_against_source` closes that: it
# harvests the call sites and requires this map to match them in BOTH
# directions, on every run.
DERIVED_PAIRS = {
    "NROS_DERIVED_MAX_SUBSCRIBERS": (
        "NROS_RESOLVED_NROS_MAX_SUBSCRIBERS",
        "NROS_RESOLVED_NROS_XRCE_MAX_SUBSCRIBERS",
    ),
    "NROS_DERIVED_MAX_PUBLISHERS": ("NROS_RESOLVED_NROS_MAX_PUBLISHERS",),
    "NROS_DERIVED_MAX_QUERYABLES": (
        "NROS_RESOLVED_NROS_MAX_QUERYABLES",
        # issue 1033 -- the XRCE service-server pool answers the SAME question
        # the zenoh queryable table asks, so it takes the same fact rather than
        # the raw CONFIG_.
        "NROS_RESOLVED_NROS_XRCE_MAX_SERVICE_SERVERS",
    ),
    "NROS_DERIVED_RMW_SUBSCRIBER_SLOTS": ("NROS_RESOLVED_NROS_RMW_SUBSCRIBER_SLOTS",),
    "NROS_DERIVED_EXECUTOR_MAX_CBS": ("NROS_RESOLVED_NROS_EXECUTOR_MAX_CBS",),
    "NROS_DERIVED_EXECUTOR_MAX_NODES": ("NROS_RESOLVED_NROS_EXECUTOR_MAX_NODES",),
    # phase-412 #7 -- carried by the resolver AND by the leaf sidecar
    # (`DERIVED_ENV_KEYS`), and named in no registry until now. Issue 0900's
    # arena budget reads it beside MAX_CBS and the two are only meaningful
    # together (`build.rs` clamps this to that), so a dropped ACTION_CLIENTS is
    # an arena sized at the wrong entry class rather than a missing pool.
    "NROS_DERIVED_EXECUTOR_ACTION_CLIENTS": (
        "NROS_RESOLVED_NROS_EXECUTOR_ACTION_CLIENTS",
    ),
    # This pair is why the gate exists in the form it does. It used to land in
    # NROS_RESOLVED_ZPICO_SUBSCRIBER_BUFFER_SIZE, which no pairing here names,
    # so a derived 880 delivered as 1496 for four consecutive island builds and
    # nothing said so -- over-sized, therefore silent.
    "NROS_DERIVED_SUBSCRIBER_BUFFER_SIZE": ("NROS_RESOLVED_NROS_SUBSCRIBER_BUFFER_SIZE",),
    # phase-412 #7 -- the TAKE buffer, resolver road only, unnamed here until
    # now. NOT the same knob as SUBSCRIBER_BUFFER_SIZE above: that one sizes the
    # backend's staging pool over the SUBSCRIBED set, this one sizes the
    # runtime-owned take buffer (`RX_BUF`, which `DEFAULT_TX_BUF` aliases) over
    # the linked CLOSURE. Two names one edit apart, two derivations, two bases.
    "NROS_DERIVED_SUBSCRIPTION_BUFFER_SIZE": (
        "NROS_RESOLVED_NROS_SUBSCRIPTION_BUFFER_SIZE",
    ),
    # Issues 1122 / 1125 — the other two thirds of the payload-class trio,
    # omitted here since the trio landed. They resolve in the same
    # `if(CONFIG_NROS_RMW_ZENOH)` block as the pair above and under the ZPICO_
    # spelling, which is what the derivable resolver is called with; the
    # `NROS_`-prefixed guess is the mismatch this map's third entry exists to
    # record, so these are written from the call sites in
    # `zephyr/cmake/nros_cargo_build.cmake` rather than derived from the name.
    #
    # `NROS_DERIVED_MAX_LARGE_SUBSCRIBERS` is the one whose derived answer is
    # commonly ZERO, and a zero that fails to arrive is 131,072 bytes of
    # `LARGE_PAYLOADS` in an image that subscribes to nothing (issue 1125,
    # measured).
    "NROS_DERIVED_MAX_LARGE_SUBSCRIBERS": ("NROS_RESOLVED_ZPICO_MAX_LARGE_SUBSCRIBERS",),
    "NROS_DERIVED_SUBSCRIBER_LARGE_SIZE": ("NROS_RESOLVED_ZPICO_SUBSCRIBER_LARGE_SIZE",),
    # phase-446 W4 -- the parameter store, from the contract's `params:`. The
    # three capacities are commonly derived as ZERO, and a zero that fails to
    # arrive is the 8.9 KiB-per-slot store the phase exists to remove.
    "NROS_DERIVED_MAX_PARAMETERS": ("NROS_RESOLVED_NROS_MAX_PARAMETERS",),
    "NROS_DERIVED_MAX_PARAM_NAME_LEN": ("NROS_RESOLVED_NROS_MAX_PARAM_NAME_LEN",),
    "NROS_DERIVED_MAX_STRING_VALUE_LEN": ("NROS_RESOLVED_NROS_MAX_STRING_VALUE_LEN",),
    "NROS_DERIVED_MAX_ARRAY_LEN": ("NROS_RESOLVED_NROS_MAX_ARRAY_LEN",),
    "NROS_DERIVED_MAX_BYTE_ARRAY_LEN": ("NROS_RESOLVED_NROS_MAX_BYTE_ARRAY_LEN",),
}

# The `if(...)` conditions that make a resolver call BACKEND-CONDITIONAL. A knob
# inside one is legitimately ABSENT from a build dir configured for the other
# backend; an UNGUARDED knob is not, and its absence is failure 4.
#
# Before phase-412 #7 the gate demanded every mapped knob be present, which made
# `NROS_SUBSCRIBER_BUFFER_SIZE` -- zenoh-guarded since issue 1125 -- a standing
# false positive on any XRCE build dir. Nobody had hit it because nobody had
# pointed the gate at one.
GUARD_MARKER = "CONFIG_NROS_RMW_"

# The call spans up to four lines and the FACT is the third argument, after an
# env name and a `"${CONFIG_...}"` value that may sit on either line.
_CALL_RE = re.compile(
    r"_nros_resolve_derivable_knob\(\s*([A-Za-z0-9_]+)\s+"
    r"(?:\"[^\"]*\"|\S+)\s+(NROS_DERIVED_[A-Z0-9_]+)", re.S)


def resolver_road(text=None):
    """fact -> {resolved-knob: guard-or-None}, harvested from the ONE resolver.

    The guard is the enclosing `if(CONFIG_NROS_RMW_*)`, tracked with a plain
    if/endif stack. It is what lets an absent knob be read as "this backend was
    not built" rather than "the value was dropped", without the gate having to
    be told which backend a build dir is for.
    """
    if text is None:
        with open(RESOLVER_CMAKE, encoding="utf8", errors="ignore") as fh:
            text = fh.read()
    road = {}
    stack = []
    lines = text.splitlines()
    for i, raw in enumerate(lines):
        line = raw.strip()
        if line.startswith("#"):
            continue
        m = re.match(r"if\s*\((.*)\)\s*$", line)
        if m:
            stack.append(m.group(1))
            continue
        if re.match(r"endif\s*\(", line):
            if stack:
                stack.pop()
            continue
        if "_nros_resolve_derivable_knob(" not in line:
            continue
        m = _CALL_RE.search("\n".join(lines[i:i + 4]))
        if not m:
            continue
        guard = next((c for c in stack if GUARD_MARKER in c), None)
        road.setdefault(m.group(2), {})["NROS_RESOLVED_" + m.group(1)] = guard
    return road


def check_registry_against_source(pairs=None, road=None):
    """DERIVED_PAIRS must name exactly the call sites, in BOTH directions.

    An authored map nothing cross-checks drifts silently. When phase-412 #7
    measured this one, two facts were carried by this road and named in no
    registry at all (EXECUTOR_ACTION_CLIENTS, SUBSCRIPTION_BUFFER_SIZE) and two
    more were named for only one of their two knobs (MAX_SUBSCRIBERS,
    MAX_QUERYABLES) -- 4 of 11 facts, with the gate reporting success.
    """
    pairs = DERIVED_PAIRS if pairs is None else pairs
    road = resolver_road() if road is None else road
    problems = []
    for fact in sorted(set(road) - set(pairs)):
        problems.append(
            "%s is resolved by nros_cargo_build.cmake into %s and is named in "
            "no DERIVED_PAIRS entry -- so this gate reports NOTHING about it."
            % (fact, ", ".join(sorted(road[fact]))))
    for fact in sorted(set(pairs) - set(road)):
        problems.append(
            "DERIVED_PAIRS names %s but no `_nros_resolve_derivable_knob` call "
            "resolves it -- the road went away and the registry did not."
            % fact)
    for fact in sorted(set(pairs) & set(road)):
        want, got = set(pairs[fact]), set(road[fact])
        for knob in sorted(got - want):
            problems.append(
                "%s is resolved into %s and DERIVED_PAIRS does not name that "
                "knob. A value that arrives at one consumer and is dropped at "
                "another is precisely what a 1:1 map cannot see."
                % (fact, knob))
        for knob in sorted(want - got):
            problems.append(
                "DERIVED_PAIRS pairs %s to %s and no call site produces it."
                % (fact, knob))
    return problems


def read_cache(build_dir):
    """NROS_RESOLVED_* from CMakeCache.txt. Absent is DIFFERENT from empty."""
    out = {}
    path = os.path.join(build_dir, "CMakeCache.txt")
    with open(path, encoding="utf8", errors="ignore") as fh:
        for line in fh:
            m = re.match(r"^(NROS_RESOLVED_[A-Z0-9_]+):[A-Z]+=(.*)$", line.strip())
            if m:
                out[m.group(1)] = m.group(2)
    return out


def read_fragment(build_dir):
    """NROS_DERIVED_* as the inventories state them.

    BOTH fragments, not one. The first version read only
    `entity_inventory.cmake`, so every knob derived by the MESSAGE-BOUND
    inventory was outside the gate entirely -- including
    NROS_DERIVED_SUBSCRIBER_BUFFER_SIZE, the pair whose mismatch this gate was
    extended to catch. A gate that reads one of two sources reports success
    about the half it can see, which is the failure it exists to prevent.
    """
    out = {}
    for name in ("entity_inventory.cmake", "message_bound_knobs.cmake"):
        path = os.path.join(build_dir, "nros", name)
        if not os.path.exists(path):
            continue
        with open(path, encoding="utf8", errors="ignore") as fh:
            for m in re.finditer(r"^set\((NROS_DERIVED_[A-Z0-9_]+)\s+([^)]*)\)", fh.read(), re.M):
                out[m.group(1)] = m.group(2).strip().strip('"')
    return out


def read_defines(build_dir):
    """The -D values the compiler is actually handed, from build.ninja.

    The ninja file rather than the cmake state on purpose: it is the last
    artifact before the compiler, so it cannot agree with cmake and disagree
    with the build.
    """
    out = {}
    path = os.path.join(build_dir, "build.ninja")
    if not os.path.exists(path):
        return out
    with open(path, encoding="utf8", errors="ignore") as fh:
        for m in re.finditer(r"-D([A-Z0-9_]+)=([0-9]*)", fh.read()):
            name, val = m.group(1), m.group(2)
            if name in C_DEFINE_KNOBS.values():
                out.setdefault(name, set()).add(val)
    return out


def check(build_dir, road=None):
    problems = []
    cache = read_cache(build_dir)
    frag = read_fragment(build_dir)
    defines = read_defines(build_dir)
    if road is None:
        road = resolver_road()

    if not cache:
        return ["no NROS_RESOLVED_* in %s/CMakeCache.txt -- not a configured "
                "nano-ros build dir, or the resolver stopped running" % build_dir]

    # 1. derivation must survive the trip to the resolver -- to EVERY knob that
    #    consumes it, not to the first one a registry happened to name.
    for derived, resolved_knobs in DERIVED_PAIRS.items():
        if derived not in frag:
            continue  # nothing derived; rung 4 is legitimate
        want = frag[derived]
        guards = road.get(derived, {})
        present = [k for k in resolved_knobs if k in cache]
        for resolved in resolved_knobs:
            if resolved in cache:
                if cache[resolved] != want:
                    problems.append(
                        "%s=%s but %s=%s -- the resolver did not carry the "
                        "derived value through%s."
                        % (derived, want, resolved, cache[resolved],
                           " to this consumer (it reached %s)"
                           % ", ".join(k for k in present if k != resolved)
                           if len(present) > 1 else ""))
                continue
            if guards.get(resolved) is not None:
                # Behind `if(CONFIG_NROS_RMW_*)`: this build dir is for the
                # other backend, so the knob is absent by construction.
                continue
            problems.append(
                "%s=%s was DERIVED but %s never reached the resolver. A loader "
                "whitelist that does not name the symbol drops it at the "
                "function boundary, and the knob then falls to a default that "
                "may be SMALLER than the demand." % (derived, want, resolved))

    # 2. every C define must equal its resolved knob, and never be empty
    for knob, define in C_DEFINE_KNOBS.items():
        rname = "NROS_RESOLVED_" + knob
        seen = defines.get(define)
        if seen is None:
            continue  # backend not built here
        if "" in seen:
            problems.append(
                "-D%s= reached the compiler EMPTY. An unresolved knob expands "
                "to nothing and sizes a C array to zero; the diagnostic names "
                "the struct, never the knob." % define)
            continue
        if len(seen) > 1:
            problems.append(
                "-D%s has %d different values in build.ninja (%s) -- two "
                "consumers disagree about one knob."
                % (define, len(seen), ", ".join(sorted(seen))))
            continue
        got = next(iter(seen))
        want = cache.get(rname)
        if want is None or want == "":
            # The knob is unresolved but a value still reached the compiler:
            # a literal fallback. Legitimate, but it must be SAID, because it
            # is how an under-size hides.
            continue
        if got != want:
            problems.append(
                "-D%s=%s but %s=%s -- the compiler was handed a different "
                "number than the resolver decided." % (define, got, rname, want))
    return problems


def self_test(quiet=False):
    """Each case asserts a failure this gate must catch, plus the clean case,
    so a gate that stopped matching anything cannot report success.

    `quiet` suppresses the per-case OK lines, not the failures: the normal path
    runs this on every invocation and a control that narrates itself there is
    noise nobody reads."""
    def build(tmp, cache, frag, ninja):
        os.makedirs(os.path.join(tmp, "nros"), exist_ok=True)
        with open(os.path.join(tmp, "CMakeCache.txt"), "w") as fh:
            fh.write(cache)
        with open(os.path.join(tmp, "nros", "entity_inventory.cmake"), "w") as fh:
            fh.write(frag)
        with open(os.path.join(tmp, "build.ninja"), "w") as fh:
            fh.write(ninja)

    CLEAN_CACHE = ("NROS_RESOLVED_NROS_MAX_SUBSCRIBERS:INTERNAL=10\n"
                   "NROS_RESOLVED_ZPICO_MAX_SUBSCRIBERS:INTERNAL=10\n")
    CLEAN_FRAG = "set(NROS_DERIVED_MAX_SUBSCRIBERS 10)\n"
    CLEAN_NINJA = "cc -DZPICO_MAX_SUBSCRIBERS=10 -c x.c\n"

    # A synthetic road, so the cases assert the RULE rather than today's cmake.
    # `NROS_XRCE_MAX_SUBSCRIBERS` is guarded exactly as the real one is, which
    # is what makes "absent" clean for it and a finding for the unguarded knob.
    ROAD = {
        "NROS_DERIVED_MAX_SUBSCRIBERS": {
            "NROS_RESOLVED_NROS_MAX_SUBSCRIBERS": None,
            "NROS_RESOLVED_NROS_XRCE_MAX_SUBSCRIBERS": "CONFIG_NROS_RMW_XRCE",
        },
    }

    cases = [
        ((CLEAN_CACHE, CLEAN_FRAG, CLEAN_NINJA), 0, "a delivered value agrees end to end"),
        # failure 4: derived, but the loader whitelist dropped it
        (("NROS_RESOLVED_ZPICO_MAX_SUBSCRIBERS:INTERNAL=8\n",
          CLEAN_FRAG, "cc -DZPICO_MAX_SUBSCRIBERS=8 -c x.c\n"), 1,
         "derived but never reached the resolver"),
        # failure 3: unresolved knob reaches the compiler empty
        ((CLEAN_CACHE, CLEAN_FRAG, "cc -DZPICO_MAX_SUBSCRIBERS= -c x.c\n"), 1,
         "an empty define"),
        # failure 1: a second consumer handed a different number
        ((CLEAN_CACHE, CLEAN_FRAG, "cc -DZPICO_MAX_SUBSCRIBERS=12 -c x.c\n"), 1,
         "compiler and resolver disagree"),
        # two consumers disagreeing with each other
        ((CLEAN_CACHE, CLEAN_FRAG,
          "cc -DZPICO_MAX_SUBSCRIBERS=10 -c x.c\ncc -DZPICO_MAX_SUBSCRIBERS=8 -c y.c\n"), 1,
         "one knob, two values"),
        # the derivation carried through wrongly
        (("NROS_RESOLVED_NROS_MAX_SUBSCRIBERS:INTERNAL=4\n"
          "NROS_RESOLVED_ZPICO_MAX_SUBSCRIBERS:INTERNAL=4\n",
          CLEAN_FRAG, "cc -DZPICO_MAX_SUBSCRIBERS=4 -c x.c\n"), 1,
         "resolver did not carry the derived value"),
        # phase-412 #7, THE 1:many case: the fact arrives at one of its two
        # consumers and is dropped at the other. Green under a 1:1 map, which
        # is the whole reason the map stopped being one.
        ((CLEAN_CACHE + "NROS_RESOLVED_NROS_XRCE_MAX_SUBSCRIBERS:INTERNAL=8\n",
          CLEAN_FRAG, CLEAN_NINJA), 1,
         "delivered to one consumer, wrong at the second"),
        # ...and its control: a guarded knob that is simply ABSENT is the other
        # backend not being built, not a dropped value.
        ((CLEAN_CACHE, CLEAN_FRAG, CLEAN_NINJA), 0,
         "a backend-guarded knob absent from the cache is clean"),
    ]
    failures = 0
    for (cache, frag, ninja), want, name in cases:
        with tempfile.TemporaryDirectory() as tmp:
            build(tmp, cache, frag, ninja)
            got = len(check(tmp, road=ROAD))
            ok = (got >= 1) if want else (got == 0)
            if not ok:
                print("  self-test FAIL: %s -- got %d problem(s), want %s"
                      % (name, got, "at least 1" if want else "0"))
                failures += 1
            elif not quiet:
                print("  ok    %s" % name)

    # The registry-vs-source cross-check, on synthetic inputs so the case
    # asserts the rule and not today's cmake.
    PAIRS = {"NROS_DERIVED_A": ("NROS_RESOLVED_A1", "NROS_RESOLVED_A2")}
    SRC = {"NROS_DERIVED_A": {"NROS_RESOLVED_A1": None, "NROS_RESOLVED_A2": None}}
    reg_cases = [
        (PAIRS, SRC, 0, "registry names exactly the call sites"),
        (PAIRS, {"NROS_DERIVED_A": {"NROS_RESOLVED_A1": None}}, 1,
         "registry names a knob no call site produces"),
        ({"NROS_DERIVED_A": ("NROS_RESOLVED_A1",)}, SRC, 1,
         "a second consumer of one fact is unregistered"),
        ({}, SRC, 1, "a carried fact is in no registry entry"),
        (PAIRS, {}, 1, "a registered fact is carried by no road"),
    ]
    for pairs, src, want, name in reg_cases:
        got = len(check_registry_against_source(pairs, src))
        ok = (got >= 1) if want else (got == 0)
        if not ok:
            print("  self-test FAIL: %s -- got %d problem(s), want %s"
                  % (name, got, "at least 1" if want else "0"))
            failures += 1
        elif not quiet:
            print("  ok    %s" % name)

    # And the HARVESTER, on a synthetic resolver: a rule fed by a parser that
    # has stopped matching reports success about an empty world.
    harvested = resolver_road(
        'if(CONFIG_NROS_RMW_XRCE)\n'
        '    _nros_resolve_derivable_knob(NROS_XRCE_MAX_SUBSCRIBERS\n'
        '        "${CONFIG_NROS_XRCE_MAX_SUBSCRIBERS}" NROS_DERIVED_MAX_SUBSCRIBERS\n'
        '        "entity inventory" "x.cmake")\n'
        'endif()\n'
        '_nros_resolve_derivable_knob(NROS_MAX_SUBSCRIBERS\n'
        '    "${CONFIG_NROS_MAX_SUBSCRIBERS}" NROS_DERIVED_MAX_SUBSCRIBERS\n'
        '    "entity inventory" "x.cmake")\n')
    if harvested != ROAD:
        print("  self-test FAIL: the resolver harvester -- got %r" % (harvested,))
        failures += 1
    elif not quiet:
        print("  ok    the resolver harvester reads a guard and a call site")

    if failures:
        print("check-knob-delivery self-test: FAILED (%d)" % failures)
        return 1
    return 0


def report_registry_drift():
    """The cross-check against the real resolver, on EVERY path.

    Not behind the build-dir argument: `just check knob-delivery` passes
    `--self-test` and nothing else, so a rule that needed a configured build
    dir would never run in any lane. This one is static -- it reads one cmake
    file -- so it is affordable on the fast tier and is where a new
    `_nros_resolve_derivable_knob` call site is caught the day it lands.
    """
    problems = check_registry_against_source()
    if not problems:
        road = resolver_road()
        print("check-knob-delivery: DERIVED_PAIRS names every one of the %d "
              "resolver call site(s) over %d fact(s)."
              % (sum(len(v) for v in road.values()), len(road)))
        return 0
    print("check-knob-delivery: DERIVED_PAIRS and the resolver disagree:")
    for p in problems:
        print("  - %s" % p)
    print("\n  phase-412 #7. An authored map that nothing cross-checks drifts, "
          "and a gate reporting on the half it can see reads exactly like a "
          "gate that passed.")
    return 1


def main(argv):
    if len(argv) == 2 and argv[1] == "--self-test":
        return self_test() or report_registry_drift()
    if len(argv) != 2:
        print(__doc__.strip().splitlines()[-3])
        return 2
    # Always, not only behind the flag: a negative control nobody runs decays
    # into a comment, and this rule's whole job is to fire. Same shape as
    # `scripts/check-board-tiers.py`; gated by `check-gate-selftests`.
    #
    # It runs BEFORE the real check so a gate that has stopped matching
    # anything cannot report "every knob reached the compile as resolved" --
    # which is exactly the sentence this gate exists to make trustworthy.
    rc = self_test(quiet=True) or report_registry_drift()
    if rc:
        return rc
    problems = check(argv[1])
    if problems:
        print("check-knob-delivery: a knob did not arrive as resolved:")
        for p in problems:
            print("  - %s" % p)
        print("\n  phase-412 W4. The derived value being RIGHT is not the "
              "question; whether it ARRIVED is.")
        return 1
    print("check-knob-delivery: every knob reached the compile as resolved.")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
