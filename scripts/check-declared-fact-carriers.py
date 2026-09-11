#!/usr/bin/env python3
"""Every `NROS_DECLARED_*` fact is produced, consumed, and WATCHED — issue 1122.

A declared fact is how a number CMake computed crosses into cargo on a lane
that has no Kconfig. `NROS_DERIVED_*` is the Zephyr road (`nros_resolve_knobs()`
-> `NROS_RESOLVED_*`, gated by `check-knob-delivery`); `NROS_DECLARED_*` is the
lane-independent one, carried by `corrosion_set_env_vars` and read as a DEFAULT
by a build script.

Issue 1122 measured what happens when the second road is missing: every derived
pool knob in the tree was computed, written to `message_bound_knobs.cmake`, and
consumed only under `zephyr/`, so a FreeRTOS image carried 131,072 B of
`LARGE_PAYLOADS` while its own build dir held
`set(NROS_DERIVED_MAX_LARGE_SUBSCRIBERS 0)`.

Three rules, and each is a defect this tree has already had:

1. PRODUCED — the name appears in a `corrosion_set_env_vars` payload under
   `cmake/`. A fact nothing writes is a default that silently never applies.

2. CONSUMED — some Rust build-side file reads it with `std::env::var`. A fact
   nothing reads is 1122's own shape one step earlier: computed, delivered,
   discarded.

3. WATCHED — the file that reads it also declares
   `cargo:rerun-if-env-changed=<name>`. This is the rule with a measured
   history: `resolve_queryable_default` consumed `NROS_DECLARED_SERVICE_SERVERS`
   and `NROS_DECLARED_INFRA_QUERYABLES` without declaring either, so an entry
   that gained or lost a service server kept its previously-sized tables until
   something else forced a rebuild -- the sizing read as applied while being
   stale, and setting the variable by hand produced a byte-identical image.

Deliberately NOT checked: that the VALUE is right. That is a build's job, and
`check-knob-delivery` already owns the Zephyr half. This gate answers the
cheaper question the tree kept getting wrong -- whether the wire is connected at
all.
"""

import importlib.util
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def _sibling(name):
    """Import a dash-named sibling gate.

    The resolver road is HARVESTED, and it is harvested in exactly one place --
    `check-knob-delivery.py`, which owns that road. A second parser here would
    be a second derivation of one thing, which is the failure this whole file
    is about one level up.
    """
    spec = importlib.util.spec_from_file_location(
        name.replace("-", "_"), ROOT / "scripts" / name)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


_KNOB_DELIVERY = _sibling("check-knob-delivery.py")

# issue 1199 — the two roads a derived number takes into cargo, paired.
#
# `leaf_entity_env.rs` writes cargo `[env]` for a Rust LEAF; `NanoRosEntityFacts.cmake`
# writes `NROS_DECLARED_*` through `corrosion_set_env_vars` for a CMake image
# with no leaf. They must deliver the same KNOBS, or an image's sizing depends
# on which lane built it -- which is issue 1122 with the roads swapped.
#
# The names differ by construction: the sidecar may write the knob itself
# (cargo `[env]` without `force` is still overridable), while the CMake road
# must not (the child environment is not), so it hands over a DECLARED fact the
# build script takes as a default. This map is the one place that pairing is
# written down.
# =============================================================================
# phase-412 #7 — every PUBLISHED fact has a stated DISPOSITION on every road
# =============================================================================
#
# A `NROS_DERIVED_*` fact is published by one of the two inventories below. It
# then takes zero or more of THREE roads into a compile:
#
#   resolver   `nros_resolve_knobs()` -> `NROS_RESOLVED_*`   (Zephyr images)
#   sidecar    `leaf_entity_env.rs` cargo `[env]`            (Rust leaves)
#   declared   `NROS_DECLARED_*` via `corrosion_set_env_vars` (CMake, no leaf)
#
# The three registries that used to describe this described DIFFERENT SUBSETS of
# it, and the difference was invisible because each was internally consistent.
# Measured at phase-412 #7, over 14 published facts:
#
#   * 3 facts (LARGEST_TYPE, LARGEST_RX, LARGE_TYPES) were on NO road and named
#     in no registry. They are provenance -- correct, but indistinguishable
#     from a wire nobody connected.
#   * EXECUTOR_ACTION_CLIENTS was on TWO roads and in no registry at all.
#   * SUBSCRIPTION_BUFFER_SIZE was on one road and in no registry at all.
#   * EXECUTOR_MAX_NODES and SUBSCRIPTION_BUFFER_SIZE are on the resolver road
#     only, and the tree says nowhere why.
#
# So: every fact appears here, and every road of every fact carries either the
# names it is delivered under, or a REASON. A `NotCarried` reason must QUOTE a
# comment that already exists in the tree -- the gate re-reads the file and
# fails if the quote is gone, so a deliberate omission cannot decay into an
# undocumented one. Where no such comment exists, the entry is an `OpenGap`:
# reported on every run, not silently absorbed, and never invented.


class NotCarried:
    """Deliberately off this road, in the tree's OWN words.

    `quote` must appear VERBATIM in `path`. Inventing a reason here would make
    this registry the authority on a decision it did not make; requiring the
    quote keeps the decision where it was taken and turns deleting the comment
    into a gate failure rather than a silent loss.
    """

    def __init__(self, path, quote, note=""):
        self.path, self.quote, self.note = path, quote, note

    def describe(self):
        return "not carried: %s (%s: \"%s\")" % (
            self.note or "see the cited comment", self.path, self.quote)


class OpenGap:
    """Off this road with no reason anywhere in the tree.

    NOT a failure -- the gap may well be correct, and guessing at a
    justification is worse than recording that none exists. It is REPORTED on
    every run and carries a tracked issue id, the same rule
    `check-rmw-api-parity` holds its `gap` rows to.
    """

    def __init__(self, issue, note):
        self.issue, self.note = issue, note

    def describe(self):
        return "OPEN (issue %s): %s" % (self.issue, self.note)


_MSG_BOUNDS = "cmake/NanoRosMessageBounds.cmake"
_LEAF = "packages/cli/nros-cli-core/src/leaf_entity_env.rs"


def _provenance(what):
    """The three facts the bounds derivation publishes as EVIDENCE, not as
    knobs. Its own header says so, and that sentence is the citation."""
    return NotCarried(_MSG_BOUNDS, what,
                      "provenance, not a knob: it explains a number that IS "
                      "carried, and no consumer sizes anything from it")


_PROV_TYPE = "_LARGEST_RX  provenance for the two above"
_PROV_LARGE = "which types drove MAX_LARGE"

# `ZPICO_MAX_QUERYABLES` is off both cargo roads for one stated reason, and the
# leaf sidecar is where that reason is written down at length.
_QUERYABLES_NOT_DERIVED = "`ZPICO_MAX_QUERYABLES` is never stated here as a COUNT"
_QUERYABLES_COMPLETED = "sizes the queryable table from"

FACT_DISPOSITION = {
    # ---- the entity inventory's counts -----------------------------------
    "NROS_DERIVED_EXECUTOR_MAX_CBS": {
        "resolver": ("NROS_RESOLVED_NROS_EXECUTOR_MAX_CBS",),
        "sidecar": ("NROS_EXECUTOR_MAX_CBS",),
        "declared": ("NROS_DECLARED_EXECUTOR_MAX_CBS",),
    },
    "NROS_DERIVED_EXECUTOR_ACTION_CLIENTS": {
        "resolver": ("NROS_RESOLVED_NROS_EXECUTOR_ACTION_CLIENTS",),
        "sidecar": ("NROS_EXECUTOR_ACTION_CLIENTS",),
        "declared": ("NROS_DECLARED_EXECUTOR_ACTION_CLIENTS",),
    },
    "NROS_DERIVED_MAX_SUBSCRIBERS": {
        # TWO resolved knobs from one fact -- the zenoh session pool and the
        # XRCE one. See `DERIVED_PAIRS` in check-knob-delivery.py.
        "resolver": ("NROS_RESOLVED_NROS_MAX_SUBSCRIBERS",
                     "NROS_RESOLVED_NROS_XRCE_MAX_SUBSCRIBERS"),
        "sidecar": ("ZPICO_MAX_SUBSCRIBERS",),
        "declared": ("NROS_DECLARED_MAX_SUBSCRIBERS",),
    },
    "NROS_DERIVED_MAX_PUBLISHERS": {
        "resolver": ("NROS_RESOLVED_NROS_MAX_PUBLISHERS",),
        "sidecar": ("ZPICO_MAX_PUBLISHERS",),
        "declared": ("NROS_DECLARED_MAX_PUBLISHERS",),
    },
    "NROS_DERIVED_RMW_SUBSCRIBER_SLOTS": {
        "resolver": ("NROS_RESOLVED_NROS_RMW_SUBSCRIBER_SLOTS",),
        "sidecar": ("NROS_RMW_SUBSCRIBER_SLOTS",),
        "declared": ("NROS_DECLARED_RMW_SUBSCRIBER_SLOTS",),
    },
    # phase-412 W2 -- the liveliness pool. Every token is declared by THIS
    # session (one per node name, one per publisher/subscriber/service server
    # and client), so the demand is the inventory's own count; the C define
    # `ZPICO_MAX_LIVELINESS` is floored from the resolved value.
    "NROS_DERIVED_MAX_LIVELINESS": {
        "resolver": ("NROS_RESOLVED_NROS_MAX_LIVELINESS",),
        "sidecar": ("NROS_MAX_LIVELINESS",),
        "declared": ("NROS_DECLARED_MAX_LIVELINESS",),
    },
    # issue 1130 -- the knob-capped cell registries. Per-IMAGE, composed across
    # entries by MAX; an explicit per-CLASS `ENTITY_BOUNDS` is always tighter
    # and keeps winning.
    "NROS_DERIVED_RUNTIME_MAX_CELL_ENTITIES": {
        "resolver": ("NROS_RESOLVED_NROS_RUNTIME_MAX_CELL_ENTITIES",),
        "sidecar": ("NROS_RUNTIME_MAX_CELL_ENTITIES",),
        "declared": ("NROS_DECLARED_RUNTIME_MAX_CELL_ENTITIES",),
    },
    "NROS_DERIVED_MAX_QUERYABLES": {
        "resolver": ("NROS_RESOLVED_NROS_MAX_QUERYABLES",
                     "NROS_RESOLVED_NROS_XRCE_MAX_SERVICE_SERVERS"),
        "sidecar": NotCarried(
            _LEAF, _QUERYABLES_NOT_DERIVED,
            "the count excludes the param and lifecycle service families a "
            "feature enables, and a leaf has no channel to complete it; a "
            "sidecar stating the bare count would be SHORT, which is a "
            "registration failure at boot rather than a smaller pool"),
        "declared": NotCarried(
            _LEAF, _QUERYABLES_COMPLETED,
            "the declared road carries the three RAW inputs "
            "(NROS_DECLARED_SERVICE_SERVERS + NROS_DECLARED_INFRA_QUERYABLES "
            "+ NROS_DECLARED_NODES, the last since phase-426 W3) and the "
            "consumer completes the sum, so the derived count itself has "
            "nothing to carry"),
    },
    "NROS_DERIVED_EXECUTOR_MAX_NODES": {
        "resolver": ("NROS_RESOLVED_NROS_EXECUTOR_MAX_NODES",),
        "sidecar": OpenGap(
            "1233",
            "on the resolver road only. Nothing in the tree says why a cargo "
            "leaf should not size its node table from the same count -- no "
            "comment, no issue, no `NOT_DERIVED_*` constant. Recorded as "
            "unexplained rather than justified"),
        "declared": OpenGap(
            "1233",
            "same gap on the declared road: a CMake image with no Rust leaf "
            "takes the crate default for a number its own configure computed"),
    },
    # ---- the message-bound inventory's sizes -----------------------------
    "NROS_DERIVED_SUBSCRIBER_BUFFER_SIZE": {
        "resolver": ("NROS_RESOLVED_NROS_SUBSCRIBER_BUFFER_SIZE",),
        "sidecar": ("NROS_SUBSCRIBER_BUFFER_SIZE",),
        "declared": ("NROS_DECLARED_SUBSCRIBER_BUFFER_SIZE",),
    },
    "NROS_DERIVED_SUBSCRIBER_LARGE_SIZE": {
        "resolver": ("NROS_RESOLVED_ZPICO_SUBSCRIBER_LARGE_SIZE",),
        "sidecar": ("ZPICO_SUBSCRIBER_LARGE_SIZE",),
        "declared": ("NROS_DECLARED_SUBSCRIBER_LARGE_SIZE",),
    },
    "NROS_DERIVED_MAX_LARGE_SUBSCRIBERS": {
        "resolver": ("NROS_RESOLVED_ZPICO_MAX_LARGE_SUBSCRIBERS",),
        "sidecar": ("ZPICO_MAX_LARGE_SUBSCRIBERS",),
        "declared": ("NROS_DECLARED_LARGE_SUBSCRIBERS",),
    },
    "NROS_DERIVED_SUBSCRIPTION_BUFFER_SIZE": {
        "resolver": ("NROS_RESOLVED_NROS_SUBSCRIPTION_BUFFER_SIZE",),
        "sidecar": OpenGap(
            "1233",
            "the take buffer is on the resolver road only, and the tree gives "
            "no reason. It is derived over the linked CLOSURE, which a cargo "
            "leaf's own graph does have -- so the usual `a leaf cannot see it` "
            "argument is not obviously available here, and no comment makes it"),
        "declared": OpenGap(
            "1233",
            "same gap on the declared road. Issue 1122 was exactly this shape "
            "for the payload trio; this is the fourth size knob and it was "
            "not swept in with them"),
    },
    # ---- provenance: published, carried by nothing, and that is correct ---
    "NROS_DERIVED_LARGEST_TYPE": {
        "resolver": _provenance(_PROV_TYPE),
        "sidecar": _provenance(_PROV_TYPE),
        # It never even reaches the fragment as a `set()`: the writer emits it
        # as a COMMENT line ("# <type> is the largest type in the closure.").
        "declared": _provenance(_PROV_TYPE),
    },
    "NROS_DERIVED_LARGEST_RX": {
        # Numerically identical to NROS_DERIVED_SUBSCRIPTION_BUFFER_SIZE by
        # construction -- both are published from `${_max_rx}` two lines apart.
        # Carrying it would deliver the same number twice under two names.
        "resolver": _provenance(_PROV_TYPE),
        "sidecar": _provenance(_PROV_TYPE),
        "declared": _provenance(_PROV_TYPE),
    },
    "NROS_DERIVED_LARGE_TYPES": {
        # A LIST OF TYPE NAMES, not a number. It explains
        # MAX_LARGE_SUBSCRIBERS, which is carried on all three roads.
        "resolver": _provenance(_PROV_LARGE),
        "sidecar": _provenance(_PROV_LARGE),
        "declared": _provenance(_PROV_LARGE),
    },
}

ROADS = ("resolver", "sidecar", "declared")

# The two files that PUBLISH facts. Every `NROS_DERIVED_*` token in them is a
# published fact -- both publish through a `_nros_*_publish` helper AND through
# a `foreach(_pool ...)` that expands the name, so matching the helper call
# alone would miss five of the seven counts.
PUBLISHER_CMAKE = ("cmake/NanoRosEntityInventory.cmake", _MSG_BOUNDS)


def _carried(entry, road):
    v = entry[road]
    return tuple(v) if isinstance(v, (tuple, list)) else ()


# phase-446 W4 -- the parameter-store facts, from the contract's `params:`.
# Carried by the resolver and the declared road; the leaf road reads no
# SystemModel, so it has nothing to carry (the reason is quoted from
# `cmake/NanoRosEntityFacts.cmake`, where the decision lives).
for _fact, _knob in (
    ("NROS_DERIVED_MAX_PARAMETERS", "NROS_MAX_PARAMETERS"),
    ("NROS_DERIVED_MAX_PARAM_NAME_LEN", "NROS_MAX_PARAM_NAME_LEN"),
    ("NROS_DERIVED_MAX_STRING_VALUE_LEN", "NROS_MAX_STRING_VALUE_LEN"),
    ("NROS_DERIVED_MAX_ARRAY_LEN", "NROS_MAX_ARRAY_LEN"),
    ("NROS_DERIVED_MAX_BYTE_ARRAY_LEN", "NROS_MAX_BYTE_ARRAY_LEN"),
):
    FACT_DISPOSITION[_fact] = {
        "resolver": ("NROS_RESOLVED_" + _knob,),
        "sidecar": NotCarried(
            "cmake/NanoRosEntityFacts.cmake",
            "The cargo-LEAF road carries none of this",
            "the leaf sidecar comes from a metadata probe that sees no "
            "SystemModel, so it has no parameter declaration to forward"),
        "declared": ("NROS_DECLARED_" + _knob[len("NROS_"):],),
    }

# Derived, not authored twice (issue 1199's map). The leaf-road <-> CMake-road
# pairing is a PROJECTION of the disposition table: a fact carried on both
# cargo roads pairs the name it takes on each.
ROAD_PAIRS = {
    _carried(e, "sidecar")[0]: _carried(e, "declared")[0]
    for e in FACT_DISPOSITION.values()
    if len(_carried(e, "sidecar")) == 1 and len(_carried(e, "declared")) == 1
}

# Facts with no leaf-road twin, each for a stated reason.
ROAD_UNPAIRED = {
    "NROS_DECLARED_INFRA_QUERYABLES":
        "completes ZPICO_MAX_QUERYABLES, which no producer states as a count: "
        "the consumer derives it from the facts (issue 0460). Since phase-445 "
        "W1 the cargo-leaf road carries this FACT itself "
        "(`leaf_entity_env::leaf_facts`), so there is no knob to pair.",
    "NROS_DECLARED_SERVICE_SERVERS":
        "the raw declared count behind the same queryable sizing (phase-392 W5); "
        "carried as-is on both roads since phase-445 W1.",
    "NROS_DECLARED_NODES":
        "a third raw input to the same queryable sizing (phase-426 W3): the "
        "parameter services register once PER NODE, so the node count is a "
        "term in ZPICO_MAX_QUERYABLES, which is DELIBERATELY NOT DERIVED. It "
        "is NOT the executor node table -- NROS_DERIVED_EXECUTOR_MAX_NODES's "
        "declared road is issue 1233's open gap, and only the zpico build "
        "script reads this.",
    "NROS_DECLARED_MAX_QOS_DEPTH":
        "the largest DECLARED QoS depth (phase-412 W3), reduced at the "
        "producer from the inventory's `type|topic=depth` triples and only "
        "when every endpoint stated one. No inventory publishes it as an "
        "NROS_DERIVED_* fact and no leaf road carries it; nros-node's build "
        "script reads it to size from depth.",
    "NROS_DECLARED_QOS_MODELS": "QoS wiring, not a pool size.",
    "NROS_DECLARED_QOS_PENDING": "QoS wiring, not a pool size.",
    "NROS_DECLARED_QOS_SCHEDULED": "QoS wiring, not a pool size.",
}

# phase-446 W4 -- the parameter-store facts come from the SystemModel's
# `contracts.node_params`. The five capacities are PUBLISHED `NROS_DERIVED_*`
# facts, so they live in FACT_DISPOSITION above (declared-carried, sidecar
# NotCarried). The three `PARAM_NEEDS_*` are raw inputs no inventory publishes
# as a derived fact, so they have no leaf twin and are recorded here.
_PARAM_STORE_REASON = (
    "the parameter store, from the contract's `params:` (phase-446 W4). The "
    "leaf road reads no SystemModel, so it has no declaration to carry."
)
for _fact in (
    "NROS_DECLARED_PARAM_NEEDS_MAX_STRING_VALUE_LEN",
    "NROS_DECLARED_PARAM_NEEDS_MAX_ARRAY_LEN",
    "NROS_DECLARED_PARAM_NEEDS_MAX_BYTE_ARRAY_LEN",
    # phase-446 F3 -- the parameter services' shape, same source, same road.
    "NROS_DECLARED_PARAM_SERVICE_SHAPE",
):
    ROAD_UNPAIRED[_fact] = _PARAM_STORE_REASON

LEAF_ENV = "packages/cli/nros-cli-core/src/leaf_entity_env.rs"

NAME_RE = re.compile(r"\bNROS_DECLARED_[A-Z0-9_]+\b")
WATCH_RE = re.compile(r"cargo:rerun-if-env-changed=(NROS_DECLARED_[A-Z0-9_]+)")
# A quoted literal in a build-side file is the READ. It used to be
# `env::var("…")`, which stopped being where the name appears once the readers
# became helpers taking the name as a parameter — and a rule that only sees one
# spelling reports a wired fact as unconsumed (issue 1199, caught by this gate
# on itself). Quoted, so a name mentioned in prose does not count; comments in
# this tree spell them in backticks.
READ_RE = re.compile(r'"(NROS_DECLARED_[A-Z0-9_]+)"')


def tracked(*globs):
    out = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "--", *globs],
        capture_output=True, text=True, check=True,
    ).stdout.split()
    return [ROOT / p for p in out]


def produced():
    """Names appearing in a `corrosion_set_env_vars` payload under cmake/."""
    names = set()
    for f in tracked("cmake/*.cmake", "cmake/**/*.cmake"):
        text = f.read_text(errors="replace")
        # The payload is built up in a list variable and passed through, so the
        # honest test is "this file both names the fact and calls the setter" —
        # matching the call's arguments would miss every accumulating site,
        # which is how every producer in this tree is written.
        if "corrosion_set_env_vars" not in text:
            continue
        names |= set(NAME_RE.findall(text))
    return names


def consumed():
    """name -> {file: (reads, watches)} over the Rust build side."""
    seen = {}
    # BUILD-SIDE only. `packages/**/*.rs` also sweeps CLI and runtime sources,
    # where a `NROS_DECLARED_*` literal is the PRODUCER naming what it writes,
    # not a consumer reading it — and demanding `rerun-if-env-changed` of a
    # non-build script is a finding that cannot be acted on.
    for f in tracked("packages/**/build.rs", "packages/**/*-build/src/*.rs"):
        text = f.read_text(errors="replace")
        if "NROS_DECLARED_" not in text:
            continue
        reads = set(READ_RE.findall(text))
        watches = set(WATCH_RE.findall(text))
        for n in reads | watches:
            seen.setdefault(n, []).append((f, n in reads, n in watches))
    return seen


def published_facts():
    """Every `NROS_DERIVED_*` an inventory publishes into the caller's scope."""
    names = set()
    for rel in PUBLISHER_CMAKE:
        names |= set(re.findall(r"\bNROS_DERIVED_[A-Z0-9_]+\b",
                                (ROOT / rel).read_text(errors="replace")))
    return names


def leaf_keys():
    """The cargo `[env]` keys the leaf sidecar writes -- the SIDECAR road."""
    leaf = (ROOT / LEAF_ENV).read_text(errors="replace")
    keys = set()
    for const in ("DERIVED_ENV_KEYS", "DERIVED_PAYLOAD_ENV_KEYS"):
        m = re.search(const + r"[^=]*=\s*&\[(.*?)\];", leaf, re.S)
        if m:
            keys |= set(re.findall(r'"([A-Z0-9_]+)"', m.group(1)))
    return keys


def check_dispositions(prod, facts=None, roads=None, disposition=None):
    """Every published fact states what each road does with it.

    Returns (findings, open_gaps). An `OpenGap` is not a finding: it is the
    honest record of a gap nobody has justified, and it is printed either way.

    `roads` is the HARVESTED delivery per road; the registry is the CLAIM.
    Both directions are checked, because a registry that only has to be a
    subset drifts one entry at a time and reports success throughout.
    """
    disposition = FACT_DISPOSITION if disposition is None else disposition
    facts = published_facts() if facts is None else facts
    if roads is None:
        roads = {
            # fact -> set(resolved knob), harvested by the gate that owns it
            "resolver": {f: set(k) for f, k in
                         _KNOB_DELIVERY.resolver_road().items()},
            "sidecar": leaf_keys(),
            "declared": prod - set(ROAD_UNPAIRED),
        }
    findings, gaps = [], []

    for fact in sorted(facts - set(disposition)):
        findings.append(
            "%s is PUBLISHED by an inventory and has no entry in\n"
            "       FACT_DISPOSITION. Every published fact states what each of\n"
            "       the three roads does with it -- carried, or deliberately\n"
            "       not with the tree's reason quoted. A fact with no stated\n"
            "       disposition is indistinguishable from a wire nobody\n"
            "       connected (phase-412 #7)." % fact)
    for fact in sorted(set(disposition) - facts):
        findings.append(
            "FACT_DISPOSITION names %s and no inventory publishes it." % fact)

    claimed = {road: set() for road in ROADS}
    for fact in sorted(set(disposition) & facts):
        entry = disposition[fact]
        for road in ROADS:
            if road not in entry:
                findings.append(
                    "%s states no disposition for the %s road." % (fact, road))
                continue
            v = entry[road]
            if isinstance(v, OpenGap):
                gaps.append((fact, road, v))
                continue
            if isinstance(v, NotCarried):
                text = (ROOT / v.path).read_text(errors="replace")
                if v.quote not in text:
                    findings.append(
                        "%s is recorded as deliberately off the %s road, and\n"
                        "       the comment it cites is GONE from %s:\n"
                        "         \"%s\"\n"
                        "       A deliberate omission whose reason has been\n"
                        "       deleted is an undocumented one."
                        % (fact, road, v.path, v.quote))
                continue
            for name in v:
                claimed[road].add(name)

    # Direction 2: nothing may travel a road unattributed.
    for road in ROADS:
        harvested = roads[road]
        if road == "resolver":
            flat = {k for ks in harvested.values() for k in ks}
            for fact, ks in sorted(harvested.items()):
                entry = disposition.get(fact)
                if entry is None:
                    continue  # already reported as unpublished/unstated
                want = set(_carried(entry, "resolver")) if not isinstance(
                    entry.get("resolver"), (NotCarried, OpenGap)) else set()
                for name in sorted(set(ks) - want):
                    findings.append(
                        "%s is delivered to %s on the resolver road and\n"
                        "       FACT_DISPOSITION does not say so." % (fact, name))
                for name in sorted(want - set(ks)):
                    findings.append(
                        "%s claims the resolver road delivers it to %s, and no\n"
                        "       call site does." % (fact, name))
            unattributed = claimed[road] - flat
            for name in sorted(unattributed):
                findings.append(
                    "FACT_DISPOSITION claims %s on the resolver road and the\n"
                    "       resolver produces no such knob." % name)
            continue
        for name in sorted(harvested - claimed[road]):
            findings.append(
                "%s travels the %s road and FACT_DISPOSITION attributes it to\n"
                "       no fact. A number delivered by a road nobody wrote down\n"
                "       is how the three registries came to describe three\n"
                "       different subsets (phase-412 #7)." % (name, road))
        for name in sorted(claimed[road] - harvested):
            findings.append(
                "FACT_DISPOSITION says the %s road carries %s and it does not.\n"
                "       Either the road dropped it -- which is issue 1122 -- or\n"
                "       the registry is stale." % (road, name))
    return findings, gaps


def check_roads(prod):
    """The two roads must deliver the same knobs (issue 1199)."""
    findings = []
    keys = leaf_keys()
    for key in sorted(keys - set(ROAD_PAIRS)):
        findings.append(
            "%s is on the cargo-LEAF road and has no CMake twin in ROAD_PAIRS.\n"
            "       An image sized one way by a leaf build and another way by a\n"
            "       CMake build is issue 1122 with the roads swapped." % key
        )
    for key, fact in sorted(ROAD_PAIRS.items()):
        if key not in keys:
            findings.append(
                "%s is paired to %s here but is no longer on the\n"
                "       cargo-LEAF road. Either the leaf dropped it deliberately --\n"
                "       then say so in ROAD_UNPAIRED -- or the roads have drifted."
                % (key, fact)
            )
        elif fact not in prod:
            findings.append(
                "%s is on the cargo-LEAF road; its CMake twin %s\n"
                "       is produced by no cmake file, so a CMake image with no Rust\n"
                "       leaf keeps the crate default (issue 1122)." % (key, fact)
            )
    return findings


def check():
    prod = produced()
    cons = consumed()
    findings = []

    for name in sorted(prod - set(cons)):
        findings.append(
            "%s is PRODUCED by cmake and read by no build script.\n"
            "       A fact nothing consumes is a default that never applies —\n"
            "       issue 1122's shape one step earlier." % name
        )

    for name in sorted(set(cons) - prod):
        findings.append(
            "%s is read on the Rust side and PRODUCED by no cmake file.\n"
            "       Nothing will ever set it, so the default silently wins."
            % name
        )

    for name in sorted(set(cons) & prod):
        for path, reads, watches in cons[name]:
            rel = path.relative_to(ROOT)
            if reads and not watches:
                findings.append(
                    "%s is read in %s with no\n"
                    "       `cargo:rerun-if-env-changed=%s`. cargo will not\n"
                    "       re-run this script when the declaration changes, so the\n"
                    "       sizing reads as applied while being STALE (issue 1122)."
                    % (name, rel, name)
                )
    findings += check_roads(prod)
    disposition_findings, gaps = check_dispositions(prod)
    findings += disposition_findings
    return findings, gaps


def self_test():
    """The gate must reject each of the shapes it exists to catch."""
    failures = 0

    def case(label, prod_set, cons_map, want):
        nonlocal failures
        global produced, consumed, check_roads, check_dispositions
        p_orig, c_orig, r_orig = produced, consumed, check_roads
        d_orig = check_dispositions
        check_roads = lambda _p: []                      # noqa: E731
        check_dispositions = lambda _p: ([], [])         # noqa: E731
        produced = lambda: prod_set                      # noqa: E731
        consumed = lambda: cons_map                      # noqa: E731
        try:
            got = len(check()[0])
        finally:
            produced, consumed, check_roads = p_orig, c_orig, r_orig
            check_dispositions = d_orig
        ok = (got > 0) == want
        if not ok:
            failures += 1
        print("  %-46s %s" % (label, "ok" if ok else "FAILED"))

    def dcase(label, want, **kw):
        """The DISPOSITION rule, on synthetic inputs.

        A clean world plus one mutation each, so a rule that has stopped
        matching cannot report that every fact has a stated disposition."""
        nonlocal failures
        base = dict(
            facts={"NROS_DERIVED_A", "NROS_DERIVED_B"},
            disposition={
                "NROS_DERIVED_A": {"resolver": ("NROS_RESOLVED_A",),
                                   "sidecar": ("A_ENV",),
                                   "declared": ("NROS_DECLARED_A",)},
                "NROS_DERIVED_B": {
                    "resolver": NotCarried(
                        "scripts/check-declared-fact-carriers.py",
                        "SELF-TEST CITATION ANCHOR", "a quoted reason"),
                    "sidecar": OpenGap("1233", "nobody wrote down why"),
                    "declared": OpenGap("1233", "nor here"),
                },
            },
            roads={"resolver": {"NROS_DERIVED_A": {"NROS_RESOLVED_A"}},
                   "sidecar": {"A_ENV"},
                   "declared": {"NROS_DECLARED_A"}},
        )
        base.update(kw)
        got, _gaps = check_dispositions(set(), **base)
        ok = (len(got) > 0) == want
        if not ok:
            failures += 1
            print("       %s" % "\n       ".join(got))
        print("  %-46s %s" % (label, "ok" if ok else "FAILED"))

    f = ROOT / "x.rs"
    case("produced, consumed and watched -> clean",
         {"NROS_DECLARED_X"}, {"NROS_DECLARED_X": [(f, True, True)]}, False)
    case("produced, never consumed -> finding",
         {"NROS_DECLARED_X"}, {}, True)
    case("consumed, never produced -> finding",
         set(), {"NROS_DECLARED_X": [(f, True, True)]}, True)
    case("consumed without a watch -> finding",
         {"NROS_DECLARED_X"}, {"NROS_DECLARED_X": [(f, True, False)]}, True)
    # A watch with no read is fine: a script may watch a fact it forwards.
    case("watched but not read -> clean",
         {"NROS_DECLARED_X"}, {"NROS_DECLARED_X": [(f, False, True)]}, False)

    # phase-412 #7 — the disposition rule. The `NotCarried` case cites this
    # very file, so the anchor string above is load-bearing: deleting it turns
    # the clean case into a failure, which is the rule demonstrating itself.
    dcase("every published fact has a disposition -> clean", False)
    dcase("a published fact with no entry -> finding", True,
          facts={"NROS_DERIVED_A", "NROS_DERIVED_B", "NROS_DERIVED_C"})
    dcase("an entry for a fact nothing publishes -> finding", True,
          facts={"NROS_DERIVED_A"})
    dcase("a road delivers a name no fact claims -> finding", True,
          roads={"resolver": {"NROS_DERIVED_A": {"NROS_RESOLVED_A"}},
                 "sidecar": {"A_ENV", "B_ENV"},
                 "declared": {"NROS_DECLARED_A"}})
    dcase("a claimed road no longer carries the fact -> finding", True,
          roads={"resolver": {"NROS_DERIVED_A": {"NROS_RESOLVED_A"}},
                 "sidecar": set(),
                 "declared": {"NROS_DECLARED_A"}})
    dcase("a second resolved knob the registry omits -> finding", True,
          roads={"resolver": {"NROS_DERIVED_A": {"NROS_RESOLVED_A",
                                                 "NROS_RESOLVED_A_XRCE"}},
                 "sidecar": {"A_ENV"},
                 "declared": {"NROS_DECLARED_A"}})
    dcase("a NotCarried whose quoted comment is gone -> finding", True,
          disposition={
              "NROS_DERIVED_A": {"resolver": ("NROS_RESOLVED_A",),
                                 "sidecar": ("A_ENV",),
                                 "declared": ("NROS_DECLARED_A",)},
              "NROS_DERIVED_B": {
                  # ASSEMBLED, never written as one literal: the first attempt
                  # spelled the missing sentence out here, which PUT it in the
                  # file and made the case pass for the wrong reason -- the
                  # rule catching its own self-test.
                  "resolver": NotCarried(
                      "scripts/check-declared-fact-carriers.py",
                      "no-such-quote-" + "z" * 40, "gone"),
                  "sidecar": OpenGap("1233", "x"),
                  "declared": OpenGap("1233", "y"),
              },
          })

    print("check-declared-fact-carriers --self-test: %d check(s) failed" % failures)
    return 1 if failures else 0


def main():
    # The negative control runs on the NORMAL path, not only behind a flag:
    # a gate that can no longer fail is a comment, and nobody types the flag.
    if self_test():
        return 1
    if "--self-test" in sys.argv:
        return 0
    findings, gaps = check()
    if findings:
        print("check-declared-fact-carriers: %d problem(s)" % len(findings),
              file=sys.stderr)
        for f in findings:
            print("  " + f, file=sys.stderr)
        return 1
    prod = produced()
    print("check-declared-fact-carriers: %d declared fact(s) produced, "
          "consumed and watched: %s" % (len(prod), ", ".join(sorted(prod))))
    facts = published_facts()
    print("check-declared-fact-carriers: %d published NROS_DERIVED_* fact(s), "
          "each with a stated disposition on all %d road(s)."
          % (len(facts), len(ROADS)))
    # Printed on the SUCCESS path deliberately. An unexplained gap is not a
    # failure -- it may be the right call -- but it must not be silent, which
    # is the state phase-412 #7 found all four of these in.
    if gaps:
        print("check-declared-fact-carriers: %d road(s) OPEN and unexplained "
              "-- recorded, not justified:" % len(gaps))
        for fact, road, gap in gaps:
            print("  - %s: %s road -- %s" % (fact, road, gap.describe()))
    return 0


if __name__ == "__main__":
    sys.exit(main())
