#!/usr/bin/env python3
"""phase-400 W8 — a migrated knob has exactly ONE reader.

Retirement is a wave, not a side effect. A mechanism that still resolves is a
mechanism people still use, and a fallback left in place winning silently is how
issues 0135 and 0316 happened: two consumers disagreeing about one value with no
diagnostic. Both were "a struct's size differed between TUs"; neither failed
loudly.

So once a knob is migrated into the RFC-0049 ladder, the ladder must be the only
thing that resolves it. Concretely: a knob listed in KNOB_ENV_NAMES may be

  * READ once, by the resolver that owns it, and
  * mentioned freely in comments, docs and tests,

but must not be read a second time by a build script that would then disagree
with the resolver about the value.

The check is deliberately narrow. It looks for the env-reading IDIOMS this tree
uses -- `env_usize("X"`, `env::var("X")`, `env::var_os("X")` -- and not for the
bare string, because the whole point is that the NAME stays valid as a front-end
spelling. Finding the name in a comment is correct and expected.

=============================================================================
phase-454 W9 -- and this file is also the RETIREMENT LEDGER
=============================================================================

The sentence above is a rule about migrating a knob; W9 applied it to a WAVE,
and the wave's output belongs where the rule lives rather than in a phase doc
that stops being read. Two registries below:

  RETIRED -- a mechanism a wave removed, the one thing that answers the fact
             now, and the PATTERN that must not come back. A second resolver of
             a retired path is a hard red, which is the same failure this file
             already catches one level down.

  KEPT    -- a carrier that was IN the retirement's scope and did not retire,
             with the issue that tracks closing the gap. Completeness is
             DERIVED, not asserted: the carrier set is read from
             `check-declared-fact-carriers.py`, so a new carrier with no ledger
             row fails and a row for a carrier nothing produces fails.

A ledger with only retirements in it is a ledger that cannot say what was
considered and rejected, which is how "the surface is smaller than it looks"
turns into "the surface was never measured". phase-454 W9 retired 2 mechanisms
and KEPT all 26 `NROS_DECLARED_*` carriers, each for a reason that is checked
here rather than remembered.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]

# Knobs migrated into the ladder, with the file that legitimately reads each.
#
# phase-400 W8 — the LIST is no longer maintained here. Which knobs are in the
# ladder is the census's answer (`KNOB_CLASS`, class `ladder`), and this file
# supplies only the OWNER. That coupling is the point: the previous version
# said so itself — "a knob that is in the ladder but not here is simply
# unchecked, which is why W6 and W8 move together" — and then relied on
# whoever migrated a knob remembering to add a row. Two of them did not, and
# the memory tenant's pair would have been three.
#
# Now a knob that reaches the ladder with no owner here FAILS, so the second
# half of migrating a knob cannot be skipped, only done or deliberately
# excused.
OWNERS: dict[str, str] = {
    "NROS_EXECUTOR_MAX_CBS": "packages/core/nros-node/build.rs",
    "NROS_EXECUTOR_MAX_SC": "packages/core/nros-node/build.rs",
    "NROS_EXECUTOR_MAX_NODES": "packages/core/nros-node/build.rs",
    "NROS_EXECUTOR_MAX_SHUTDOWN_CBS": "packages/core/nros-node/build.rs",
    "NROS_EXECUTOR_ACTION_CLIENTS": "packages/core/nros-node/build.rs",
    "NROS_EXECUTOR_ARENA_SIZE": "packages/core/nros-node/build.rs",
    # phase-448 W5 / issue 1145. `nros-node` OWNS it: it sizes `EXECUTOR_BACKING`
    # from it and carries the Kconfig rung this knob also has. The ThreadX byte
    # pool has to subtract exactly `8 *` the same number, and does NOT parse the
    # env for it — `threadx_sources` calls `platform_config::executor_rung_opt`,
    # the shared front-end beside `executor_env_key`, which is the file already
    # EXEMPT below as "the resolver names every knob; that is the map, not a
    # second reader". Two readers of one rung cannot disagree; two env-parsing
    # sites can, which is this gate's whole subject.
    "NROS_EXECUTOR_BACKING_U64S": "packages/core/nros-node/build.rs",
    # Classed `derived` by the census (phase-403 makes it per-type) but still
    # ON the ladder as the fallback for a type with no declared bound, so it
    # keeps a single owner. Listed here deliberately: it is checked, and the
    # membership check below skips it because the census does not call it
    # `ladder`.
    "NROS_SUBSCRIPTION_BUFFER_SIZE": "packages/core/nros-node/build.rs",
    "NROS_PARAM_SERVICE_BUFFER_SIZE": "packages/core/nros-node/build.rs",
    # The memory tenant (phase-400 W6). The stack is read inside the crate that
    # owns the ladder; the heap by the board crate that sizes `ucHeap`.
    "NROS_FREERTOS_APP_STACK_KB": "packages/boards/nros-board-common/src/freertos_build.rs",
    # The Zephyr heap joined the memory tenant once `nros-platform` could reach
    # the ladder — it could not while the reader lived in `nros-board-common`,
    # which depends on `nros-platform`.
    "NROS_ZEPHYR_HEAP_SIZE": "packages/platform/nros-platform/build.rs",
    "NROS_FREERTOS_HEAP_KB": "packages/boards/nros-board-freertos/build.rs",
    # The transport and zenoh-tx tenants resolve inside the ladder itself, so
    # the resolver IS the owner. `nros-zpico-build` re-read the tx trio for its
    # no-platform case until W8 replaced that with `tx_env_only`.
    # The parameter tenant (phase-400 W6).
    "NROS_MAX_PARAMETERS": "packages/core/nros-params/build.rs",
    "NROS_MAX_PARAM_NAME_LEN": "packages/core/nros-params/build.rs",
    "NROS_MAX_STRING_VALUE_LEN": "packages/core/nros-params/build.rs",
    "NROS_MAX_ARRAY_LEN": "packages/core/nros-params/build.rs",
    "NROS_MAX_BYTE_ARRAY_LEN": "packages/core/nros-params/build.rs",
    "NROS_MAX_PARAM_DESCRIPTION_LEN": "packages/core/nros-params/build.rs",
    # phase-417 W4.a -- the descriptor's OTHER free text. Its own capacity,
    # default 0, so no image pays for a field it never sets.
    "NROS_MAX_PARAM_CONSTRAINTS_LEN": "packages/core/nros-params/build.rs",
    # The RMW static-pool tenant (phase-400 W6). `NROS_RMW_SUBSCRIBER_SLOTS`
    # is NOT here: it lives in the same build script and looks identical, but
    # phase-412 W1 derives it from the entity inventory, so the census classes
    # it `derived` and this gate leaves it alone.
    "NROS_RMW_MAX_BACKENDS": "packages/rmw/cffi/build.rs",
    "NROS_RMW_MAX_NODES": "packages/rmw/cffi/build.rs",
    "NROS_RMW_MESSAGE_INFO_SLOTS": "packages/rmw/cffi/build.rs",
    # The smoltcp net tenant (phase-400 W6). The driver reads the ladder from
    # the LEAF crate; it cannot see `nros-board-common` without a cycle.
    "NROS_SMOLTCP_MAX_SOCKETS": "packages/drivers/net/nros-smoltcp/build.rs",
    "NROS_SMOLTCP_MAX_UDP_SOCKETS": "packages/drivers/net/nros-smoltcp/build.rs",
    "NROS_SMOLTCP_BUFFER_SIZE": "packages/drivers/net/nros-smoltcp/build.rs",
    "NROS_SMOLTCP_CONNECT_TIMEOUT_MS": "packages/drivers/net/nros-smoltcp/build.rs",
    "NROS_SMOLTCP_SOCKET_TIMEOUT_MS": "packages/drivers/net/nros-smoltcp/build.rs",
    # The component-runtime tenant (phase-400 W6). phase-391 emits the consts
    # from these; the ladder decides their values.
    "NROS_RUNTIME_MAX_COMPONENTS": "packages/api/nros/build.rs",
    "NROS_RUNTIME_COMPONENT_SLOT_BYTES": "packages/api/nros/build.rs",
    "NROS_RUNTIME_MAX_CLASS_INSTANCES": "packages/api/nros/build.rs",
    "NROS_RUNTIME_MAX_CELL_ENTITIES": "packages/api/nros/build.rs",
    # The zenoh WIRE tenant (phase-400 W6). The two transport-band PRIORITIES
    # are deliberately absent: `ZPICO_READ_TASK_PRIORITY` mirrors a C `#define`
    # and `FreertosScheduling` already carries a per-board `zenoh_read_priority`
    # in raw FreeRTOS units, so a rung would be a THIRD path to one number.
    "ZPICO_BATCH_UNICAST_SIZE": "packages/rmw/zenoh/nros-zpico-build/src/runner.rs",
    "ZPICO_BATCH_MULTICAST_SIZE": "packages/rmw/zenoh/nros-zpico-build/src/runner.rs",
    "ZPICO_FRAG_MAX_SIZE": "packages/rmw/zenoh/nros-zpico-build/src/runner.rs",
    "ZPICO_GET_REPLY_BUF_SIZE": "packages/rmw/zenoh/nros-zpico-build/src/runner.rs",
    "ZPICO_GET_POLL_INTERVAL_MS": "packages/rmw/zenoh/nros-zpico-build/src/runner.rs",
    # The zenoh limits + xrce tenants, and the LET buffer (phase-400 W6).
    # `NROS_SERVICE_TIMEOUT_MS` is NOT here: it has two readers by design (a
    # Rust const and a C define), and this gate's one-reader rule is what keeps
    # them equal. Migrating it needs a single emission point first.
    "NROS_KEYEXPR_STRING_SIZE": "packages/rmw/zenoh/nros-rmw-zenoh/build.rs",
    "ZPICO_SUBSCRIBER_RING_DEPTH": "packages/rmw/zenoh/nros-rmw-zenoh/build.rs",
    "NROS_XRCE_CUSTOM_TRANSPORT_MTU": "packages/rmw/xrce/nros-rmw-xrce-cffi/build.rs",
    "NROS_XRCE_STREAM_HISTORY": "packages/rmw/xrce/nros-rmw-xrce-cffi/build.rs",
    "NROS_LET_BUFFER_SIZE": "packages/tooling/nros-build-helpers/src/c.rs",
    "NROS_TRANSPORT_KIND": "packages/boards/nros-board-common/src/platform_config.rs",
    "NROS_TRANSPORT_ENDPOINT": "packages/boards/nros-board-common/src/platform_config.rs",
    "ZPICO_TX_BATCH": "packages/boards/nros-board-common/src/platform_config.rs",
    "ZPICO_TX_SPLIT_LOCK": "packages/boards/nros-board-common/src/platform_config.rs",
    "ZPICO_TX_BATCH_FLUSH_MS": "packages/boards/nros-board-common/src/platform_config.rs",
}


def census_class(classes: tuple[str, ...]) -> set[str]:
    """The env names the census puts in any of `classes`."""
    import importlib.util

    census = REPO / "scripts" / "check" / "config-knob-census.py"
    spec = importlib.util.spec_from_file_location("config_knob_census", census)
    mod = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(mod)
    return {n for n, (cls, _) in mod.KNOB_CLASS.items() if cls in classes}


def ladder_knobs_from_census() -> set[str]:
    """The env names the LADDER maps, read from the ladder's own source.

    Imported rather than restated: two hand-kept lists of "what is in the
    ladder" is the same duplicate-fact shape the ladder itself exists to
    remove.

    This used to read the census's `KNOB_CLASS` rows with class `ladder`. Those
    rows are gone — they restated `platform_config.rs`, and being hand-kept made
    that table a per-PR conflict site (three of eleven dirty PRs on 2026-09-04
    conflicted there and nowhere else, because a phase-400 W6 migration flipped
    entries `sizing` -> `ladder` by hand). The census now DERIVES the mapping
    and refuses a row that restates it, so this reads the same derivation.

    `LADDER_MAPPED_NOT_MIGRATED` is subtracted for the same reason the census
    subtracts it: a knob the ladder maps but that still has two readers is not
    migrated, and this gate's whole subject is how many readers a knob has.
    """
    import importlib.util

    census = REPO / "scripts" / "check" / "config-knob-census.py"
    spec = importlib.util.spec_from_file_location("config_knob_census", census)
    mod = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(mod)
    return mod.ladder_env_keys() - mod.LADDER_MAPPED_NOT_MIGRATED


# =============================================================================
# phase-454 W9 -- THE RETIREMENT LEDGER
# =============================================================================


class Retired:
    """A mechanism a retirement wave removed, and what answers the fact now.

    `forbid` is a list of `(glob, regex)`. A match anywhere the glob reaches is
    a hard failure unless the file is named in `cite` -- the wave's own record
    of what it did has to be allowed to quote the thing it removed, or writing
    the explanation trips the check (the same concession `strip_comments` makes
    one rule up).

    `block` narrows the search to ONE named Rust item's body, for a pattern
    whose bare spelling is common: `entities` is a legitimate field on three
    other structs in the same file, and a rule that cannot tell them apart is a
    rule nobody can leave switched on.
    """

    def __init__(self, what, resolves_now, wave, forbid, cite=(), block=None):
        self.what = what
        self.resolves_now = resolves_now
        self.wave = wave
        self.forbid = forbid
        self.cite = set(cite)
        self.block = block


RETIRED = {
    # Measured at W9: zero readers and zero declaring leaves. The field was
    # PARSED AND DROPPED -- `leaf_system::read` lost its manifest fallback in
    # phase-445 W5 -- which is worse than an absent surface, because it accepts
    # what a user writes and sizes nothing from it.
    "[package.metadata.nros.component] entities": Retired(
        what="the cargo-manifest spelling of a component's declared entities",
        resolves_now=(
            "`system.toml`'s `[[component]] entities` (RFC-0098 D8), read by "
            "`nros_orchestration_ir::leaf_system::read`"
        ),
        wave="phase-454 W9",
        # Scoped to the struct that HELD it. `SystemComponentEntry::entities`
        # in the same file is the surviving surface and must stay.
        block=(
            "packages/cli/nros-cli-core/src/orchestration/cargo_metadata_schema.rs",
            "pub struct ComponentMetadata {",
        ),
        forbid=[(None, r"^\s*pub entities\s*:")],
    ),
    # The metadata JSON key the grammar used to travel on. Its writer had been
    # the empty string on every path since phase-412, so it emitted nothing; a
    # splice point for a key that can never be written is a producer a
    # retirement would otherwise still have to account for.
    'the `"entities"` key of `nros-metadata.json`': Retired(
        what="the cmake producer of a component's declared entities",
        resolves_now=(
            "the contract sidecar the bringup resolves, through "
            "`EntityInventory::from_model`; `nano_ros_node_register(... ENTITIES "
            "...)` is a FATAL_ERROR (phase-412)"
        ),
        wave="phase-454 W9",
        # `:(glob)` MAGIC, and it is load-bearing. Git's default pathspec is
        # wildmatch WITHOUT `WM_PATHNAME`, so a bare `cmake/**/*.cmake` means
        # "at least one directory level" and reaches 53 of the 142 files --
        # every module directly under `cmake/`, including the one that produced
        # this key, is invisible to it. W9's own negative control caught that:
        # the planted violation went undetected. Same shape as the four gates
        # the 2026-07-28 audit found with a reach narrower than their rule,
        # which is why `check_ledger` now also fails a glob that matches
        # nothing at all.
        forbid=[
            (":(glob)cmake/**/*.cmake", r"_entities_field"),
            (":(glob)cmake/**/*.cmake", r'\\"entities\\"\s*:'),
        ],
        # NO `cite`, deliberately. The first draft exempted
        # `cmake/NanoRosNodeRegister.cmake` on the reasoning that its
        # FATAL_ERROR necessarily names the keyword -- which put a blind spot
        # in the one file the producer actually lived in, so re-adding the
        # real thing there would have passed. It is not needed: the refusal
        # names `ENTITIES`, not `_entities_field`, and the prose explaining
        # the removal is a `#` comment that `strip_comments_hash` drops.
        # Verified by running the gate with the exemption removed.
    ),
}


class Kept:
    """A carrier in a retirement's scope that could NOT retire.

    `blocked_by` is a tracked issue id and is checked to exist and be OPEN. A
    reason with no issue behind it decays into a reason nobody re-examines,
    which is the `check-rmw-api-parity` `gap` rule applied to this ledger.
    """

    def __init__(self, blocked_by, why):
        self.blocked_by = blocked_by
        self.why = why


# Issue 1393 -- the payload class. `wire_bound_bytes` / `storage_bytes` are
# REFUSED by the model-only producer, so on two roads of three the descriptor
# states no size at all and this carrier is the only one that does.
_PAYLOAD = "the descriptor REFUSES the bound this sizes from on 2 roads of 3"

# Issue 1407 -- the count class. Three independent mechanisms, none of which
# 1393's remedy touches: the descriptor's producer reads a POORER inventory
# (model only, where the carrier's is metadata + model, and only the carrier's
# can refuse on a component that declared nothing); no model means no
# descriptor, which is every standalone leaf; and a multi-entry configure names
# no descriptor to cargo at all while the facts still travel by MAX.
_COUNTS = "the descriptor's producer sees a poorer inventory on this road"

# Issue 1407 -- the queryable raw inputs, whose live road is the STANDALONE
# LEAF (`facts_from_leaf`). That road has no SystemModel, so it can never have
# a model-written descriptor; it is also the road issue 1378 measured failing.
_LEAF_ROAD = "carried for a standalone leaf, which has no model and so no descriptor"

# Issue 1408 -- the parameter store. The gap that KEPT these has CLOSED at both
# ends: `[params]` exists in the D4 schema and the shared composer fills it on
# BOTH producer roads, and `nros-params`/`nros-node` read it at the rung the
# carrier occupies. What is left is the RETIREMENT, which is deliberately its
# own wave (W9's lesson, and this whole ledger's reason for existing): a carrier
# comes out only once both roads are MEASURED delivering on every road that
# carries it, and three do not have a descriptor at all today --
#
#   * a STANDALONE cargo leaf with no resolved model (the 1407 `_LEAF_ROAD`
#     shape one section up: no model, so no `write_for_model`, and a leaf with
#     no `system.contract.yaml` has no `ParamDeclarations` either);
#   * a MULTI-ENTRY cmake configure, which names no descriptor to cargo while
#     the facts still travel;
#   * the Zephyr west lane, which has no `--config` seam of its own (1288).
#
# So both roads run, ranked with the descriptor first -- measured identical on
# the road that has both (`nros_params_config.rs` byte-for-byte, and
# `DECLARED_PARAM_SERVICE_SHAPES` byte-for-byte).
_BOTH_ROADS = (
    "the descriptor now states it and is read FIRST; the carrier is the only "
    "road for a standalone leaf, a multi-entry cmake configure and the Zephyr "
    "west lane, so retirement is its own wave"
)

# The three BOARD capacities. These are not waiting on a wave at all: they are
# RFC-0100 D1 *target* facts owned by `[board.knobs.params]`, so they have no
# descriptor spelling BY DESIGN and never will -- an MCU and a PC want different
# string lengths for the same node, so a contract cannot name the number. What
# the descriptor carries is the NEED (`needs_max_*`), which is the half the
# contract owns; these carry the RESOLVED number for the cmake road.
_BOARD_CAPACITY = (
    "a BOARD capacity (RFC-0100 D1 target fact), deliberately absent from the "
    "descriptor -- the contract states the NEED, never the size"
)

KEPT = {
    # ---- payload class (issue 1393) -------------------------------------
    "NROS_DECLARED_SUBSCRIBER_BUFFER_SIZE": Kept(1393, _PAYLOAD),
    "NROS_DECLARED_SUBSCRIPTION_BUFFER_SIZE": Kept(1393, _PAYLOAD),
    "NROS_DECLARED_LARGE_SUBSCRIBERS": Kept(1393, _PAYLOAD),
    "NROS_DECLARED_SUBSCRIBER_LARGE_SIZE": Kept(1393, _PAYLOAD),
    # ---- the entity counts (issue 1407) ---------------------------------
    # Three of these the schema could not state even with 1407 closed, and
    # each is a DIFFERENT structural reason -- worth keeping distinct, because
    # "the counts" is exactly the grouping W9 was told not to assume.
    "NROS_DECLARED_EXECUTOR_MAX_CBS": Kept(
        1407,
        _COUNTS + "; and `max_cbs` sums `callback_slots()` over Timer and "
        "GuardCondition, which `endpoint_kind` drops (they carry no type and "
        "no topic, so no endpoint table can key on them)",
    ),
    "NROS_DECLARED_EXECUTOR_MAX_SC": Kept(
        1407,
        _COUNTS + "; and the scheduling-context count comes from "
        "`execution.tiers` -- the SCHEDULE, which the schema does not model",
    ),
    "NROS_DECLARED_RUNTIME_MAX_CELL_ENTITIES": Kept(
        1407,
        _COUNTS + "; and it is a max over PER-COMPONENT per-kind counts, while "
        "`[[endpoint]]` rows carry no component attribution",
    ),
    "NROS_DECLARED_EXECUTOR_ACTION_CLIENTS": Kept(
        1407,
        _COUNTS + "; and `heavy_slots` has no `[image]` field -- counting rows "
        "and multiplying is the third mirror RFC-0100 D4 refuses",
    ),
    "NROS_DECLARED_MAX_PUBLISHERS": Kept(
        1407,
        _COUNTS + "; and unlike `subscriber_count` it has no `[image]` field, "
        "so a consumer would have to restate the action expansion",
    ),
    "NROS_DECLARED_EXECUTOR_MAX_NODES": Kept(1407, _COUNTS),
    "NROS_DECLARED_MAX_SUBSCRIBERS": Kept(1407, _COUNTS),
    "NROS_DECLARED_RMW_SUBSCRIBER_SLOTS": Kept(1407, _COUNTS),
    # ---- the queryable raw inputs (issue 1407) --------------------------
    "NROS_DECLARED_SERVICE_SERVERS": Kept(1407, _LEAF_ROAD),
    "NROS_DECLARED_TL_PUBLISHERS": Kept(1407, _LEAF_ROAD),
    "NROS_DECLARED_NODES": Kept(
        1407,
        _LEAF_ROAD + "; and it is emitted even for a model that describes NO "
        "wiring, which is exactly where `write_for_model` writes no file",
    ),
    "NROS_DECLARED_INFRA_QUERYABLES": Kept(
        1407,
        _LEAF_ROAD + "; and it is a FEATURE token from `execution.features`, "
        "not a count -- the schema has no field of that kind",
    ),
    # ---- the parameter store (issue 1408) -------------------------------
    "NROS_DECLARED_MAX_PARAMETERS": Kept(1408, _BOTH_ROADS),
    "NROS_DECLARED_MAX_PARAM_NAME_LEN": Kept(1408, _BOTH_ROADS),
    "NROS_DECLARED_MAX_STRING_VALUE_LEN": Kept(1408, _BOARD_CAPACITY),
    "NROS_DECLARED_MAX_ARRAY_LEN": Kept(1408, _BOARD_CAPACITY),
    "NROS_DECLARED_MAX_BYTE_ARRAY_LEN": Kept(1408, _BOARD_CAPACITY),
    "NROS_DECLARED_PARAM_NEEDS_MAX_STRING_VALUE_LEN": Kept(1408, _BOTH_ROADS),
    "NROS_DECLARED_PARAM_NEEDS_MAX_ARRAY_LEN": Kept(1408, _BOTH_ROADS),
    "NROS_DECLARED_PARAM_NEEDS_MAX_BYTE_ARRAY_LEN": Kept(1408, _BOTH_ROADS),
    "NROS_DECLARED_PARAM_SERVICE_SHAPE": Kept(1408, _BOTH_ROADS),
    # ---- QoS depth (issue 1407) -----------------------------------------
    # The one carrier whose FACT the descriptor states on all three roads. It
    # stays for the road reason above, and because its sibling
    # `NROS_ENTITY_DECLARED_DEPTHS` is deliberately UNIONED with the descriptor
    # rather than ranked (phase-454 W10/W13): the two are disjoint in practice
    # and a `(type, topic)` both state with different depths fails the build.
    "NROS_DECLARED_MAX_QOS_DEPTH": Kept(
        1407,
        _COUNTS + "; and the reduction it carries (the MAX, guarded on every "
        "subscription having declared) is a consumer-side restatement nothing "
        "shares today",
    ),
}


def declared_carriers() -> set[str]:
    """The `NROS_DECLARED_*` names cmake actually produces.

    Imported from the gate that owns that harvest rather than re-grepped, for
    the reason `ladder_knobs_from_census` gives one rule up: two hand-kept
    lists of one fact is the drift this file exists to refuse.
    """
    import importlib.util

    src = REPO / "scripts" / "check-declared-fact-carriers.py"
    spec = importlib.util.spec_from_file_location("declared_fact_carriers", src)
    mod = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(mod)
    return mod.produced()


def rust_block(text: str, header: str) -> str | None:
    """The brace-balanced body following `header`, or None if it is absent."""
    i = text.find(header)
    if i < 0:
        return None
    i += len(header)
    depth, out = 1, []
    for ch in text[i:]:
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                break
        out.append(ch)
    return "".join(out)


def retired_hits(entry: Retired, read):
    """Every forbidden match for one retired mechanism.

    `read` is `(relpath) -> text | None`, injected so the self-test drives this
    on synthetic input rather than on the tree -- the control must not depend
    on the thing it controls.
    """
    hits = []
    if entry.block is not None:
        rel, header = entry.block
        text = read(rel)
        if text is not None:
            body = rust_block(strip_comments(text), header)
            if body is not None:
                for _glob, rx in entry.forbid:
                    if re.search(rx, body, re.M):
                        hits.append((rel, rx))
        return hits
    for glob, rx in entry.forbid:
        for rel in tracked(glob):
            if rel in entry.cite:
                continue
            text = read(rel)
            if text is None:
                continue
            if re.search(rx, strip_comments_hash(text), re.M):
                hits.append((rel, rx))
    return hits


def strip_comments_hash(src: str) -> str:
    """`#` comments, for the CMake half. Same concession, other language."""
    return re.sub(r"#[^\n]*", "", src)


def tracked(glob: str):
    listing = subprocess.run(
        ["git", "ls-files", "-z", "--", glob],
        cwd=REPO,
        capture_output=True,
        check=True,
    )
    return [r for r in listing.stdout.decode("utf-8", "ignore").split("\0") if r]


def check_ledger() -> list[str]:
    """The retirement ledger: nothing retired resolves, nothing kept is unaccounted."""
    failures = []

    def read(rel):
        try:
            return (REPO / rel).read_text(encoding="utf-8", errors="ignore")
        except OSError:
            return None

    for name, entry in sorted(RETIRED.items()):
        # A forbid rule whose glob reaches no file is a rule that can never
        # fire, and it reports OK forever. W9 shipped one by accident (see the
        # `:(glob)` note in RETIRED) and only the negative control found it.
        for glob, rx in entry.forbid:
            if glob is not None and not tracked(glob):
                failures.append(
                    f"  {name}: the forbid rule `{rx}` is scoped to `{glob}`,\n"
                    "      which matches NO tracked file. A rule with no reach\n"
                    "      cannot fail, so it reports this mechanism retired on\n"
                    "      evidence it never gathered. Fix the pathspec -- note\n"
                    "      git's default wildmatch has no `WM_PATHNAME`, so\n"
                    "      `a/**/*.x` means \"at least one directory deep\"."
                )
        if entry.block is not None:
            rel, header = entry.block
            text = read(rel)
            if text is None or rust_block(strip_comments(text), header) is None:
                failures.append(
                    f"  {name}: the block this rule reads -- `{header}` in\n"
                    f"      {rel} -- is gone. The rule is scoped to an item that\n"
                    "      no longer exists, so it can never fire. Re-scope it or\n"
                    "      retire the row."
                )
        for rel, rx in retired_hits(entry, read):
            failures.append(
                f"  {name} was RETIRED ({entry.wave}) and {rel} matches `{rx}`.\n"
                f"      What answers this now: {entry.resolves_now}.\n"
                "      A mechanism that still resolves is a mechanism people still\n"
                "      use, and two declaration surfaces nobody joins is how one\n"
                "      of them comes to be silently ignored. Remove the second\n"
                "      reader, or -- if the retirement is being reversed -- take\n"
                "      the row out of RETIRED and say so in the wave that does it."
            )

    produced = declared_carriers()
    for knob in sorted(produced - set(KEPT)):
        failures.append(
            f"  {knob} is a declared-fact carrier with no row in the phase-454 W9\n"
            "      ledger. Every carrier is either RETIRED (and registered above)\n"
            "      or KEPT with the issue that tracks closing the gap. A carrier\n"
            "      with neither is one nobody has asked the retirement question\n"
            "      about: can the sizing descriptor state this fact, on ALL THREE\n"
            "      roads, today? Add a `Kept(<issue>, \"<why>\")` row."
        )
    for knob in sorted(set(KEPT) - produced):
        failures.append(
            f"  {knob} has a KEPT row here and no cmake file produces it. Either\n"
            "      it retired -- then move it to RETIRED with its forbidden\n"
            "      pattern -- or the ledger is stale."
        )

    for knob, kept in sorted(KEPT.items()):
        issues = list((REPO / "docs" / "issues").glob(f"{kept.blocked_by}-*.md"))
        if not issues:
            failures.append(
                f"  {knob} is KEPT against issue {kept.blocked_by} and no such\n"
                "      issue file exists. A reason with no tracked issue behind it\n"
                "      is a reason nobody re-examines."
            )
            continue
        head = issues[0].read_text(encoding="utf-8", errors="ignore")[:800]
        if not re.search(r"^status:\s*open\s*$", head, re.M):
            failures.append(
                f"  {knob} is KEPT against issue {kept.blocked_by}, which is no\n"
                "      longer `status: open`. If the blocker closed, this carrier\n"
                "      is due for retirement -- re-run the per-fact test rather\n"
                "      than re-pointing the row at another issue."
            )
    return failures


def ledger_self_test() -> None:
    """Negative control for the ledger, on SYNTHETIC input.

    Three mutations, one per rule. The retired-path case is driven through an
    injected reader so it proves the DETECTOR rather than the current tree --
    the tree being clean is what the normal run reports, and a control that
    only asserts that can never fail.
    """
    field = Retired(
        what="x", resolves_now="y", wave="w",
        block=("fake.rs", "pub struct Held {"),
        forbid=[(None, r"^\s*pub entities\s*:")],
    )
    clean = "pub struct Held {\n    pub name: String,\n}\npub struct Other {\n    pub entities: Vec<String>,\n}\n"
    dirty = "pub struct Held {\n    pub entities: Vec<String>,\n}\n"
    assert not retired_hits(field, lambda _r: clean), (
        "ledger selftest: a sibling struct's field registered as a retired one"
    )
    assert retired_hits(field, lambda _r: dirty), (
        "ledger selftest: a planted second reader was not detected"
    )
    # A mention in a COMMENT is how a retirement gets explained, and must not
    # register -- the same concession the reader rule makes.
    commented = "pub struct Held {\n    // pub entities: Vec<String>, retired in W9\n}\n"
    assert not retired_hits(field, lambda _r: commented), (
        "ledger selftest: comment stripping ate the explanation allowance"
    )
    # And a header that is gone entirely is not a silent pass for the glob
    # form: `rust_block` returning None means the struct was renamed, which
    # the tree-level run reports through the carrier completeness check.
    assert rust_block("struct Q {}", "pub struct Held {") is None


# The resolver itself names every knob in its front-end table; that is the map,
# not a second reader.
EXEMPT = {
    "packages/boards/nros-board-common/src/platform_config.rs",
}

READ_IDIOMS = [
    r'env_usize\(\s*"{k}"',
    r'env_bool\(\s*"{k}"',
    r'env::var\(\s*"{k}"',
    r'env::var_os\(\s*"{k}"',
    r'std::env::var\(\s*"{k}"',
    r'std::env::var_os\(\s*"{k}"',
]


def strip_comments(src: str) -> str:
    """Drop `//` and `/* */` comments.

    The docstring promises a knob may be "mentioned freely in comments", and the
    gate has to actually honour that: prose explaining WHY a read was removed
    naturally quotes the idiom verbatim, and matching it would make writing the
    explanation trip the check. Not a full Rust lexer — a `//` inside a string
    literal over-strips — but this only ever causes a MISSED reader, never a
    false one, and the failure mode of a config gate should be quiet rather than
    crying wolf.
    """
    src = re.sub(r"/\*.*?\*/", "", src, flags=re.S)
    return re.sub(r"//[^\n]*", "", src)


def readers_in(text: str, knob: str) -> bool:
    """Does this source TEXT read `knob` through one of the env idioms?

    Factored out so the selftest can drive it on synthetic input rather than on
    the tree, which would make the control depend on the very thing it checks.
    """
    stripped = strip_comments(text)
    if knob not in stripped:
        return False
    return any(
        re.compile(i.format(k=re.escape(knob))).search(stripped) for i in READ_IDIOMS
    )


def self_test() -> None:
    """Negative control: prove the detector FAILS on a planted second reader.

    On the normal path, not behind a flag — `check-gate-selftests` requires it,
    on the reasoning that a control nobody runs decays into a comment. This gate
    earned that scepticism: its first draft matched a knob name inside a COMMENT
    and reported a reader that did not exist, so both directions are pinned here.
    """
    k = "NROS_EXECUTOR_MAX_CBS"

    # positive: each idiom the gate claims to detect
    for src in (
        f'let n = env_usize("{k}", 4);',
        f'std::env::var("{k}").ok()',
        f'env::var_os("{k}")',
    ):
        assert readers_in(src, k), f"selftest: missed a real reader in {src!r}"

    # negative: a mention that is NOT a read must not register
    for src in (
        f"// this used to be std::env::var(\"{k}\"), removed in phase-400",
        f'/* {k} is documented here */',
        f'panic!("set `{k}` to at least {{n}}")',
    ):
        assert not readers_in(src, k), f"selftest: false positive on {src!r}"

    # and the gate must still see a read that FOLLOWS a comment mentioning it
    mixed = f'// {k} note\nlet n = env_usize("{k}", 4);'
    assert readers_in(mixed, k), "selftest: comment stripping ate a real read"


def main() -> int:
    # The ladder's membership comes from the census; the owner comes from here.
    # A knob in one and not the other is the drift this pairing removes.
    ladder = ladder_knobs_from_census()
    unowned = sorted(ladder - OWNERS.keys())
    # A `derived` knob may legitimately keep an owner: it is on the ladder as a
    # fallback while another campaign takes over its value. What must not
    # happen is an owner for a knob the census does not know at all.
    derived = census_class(("derived",))
    stale = sorted(OWNERS.keys() - ladder - derived)
    MIGRATED = {k: v for k, v in OWNERS.items() if k in ladder}

    # Single pass over the sources: read each file once and test every knob
    # against it. The naive shape (a pass per knob) re-reads several thousand
    # files eight times and takes minutes, which is a gate nobody will run.
    pats = {
        knob: [re.compile(i.format(k=re.escape(knob))) for i in READ_IDIOMS]
        for knob in MIGRATED
    }
    readers: dict[str, set[str]] = {k: set() for k in MIGRATED}

    # `git ls-files`, not a filesystem walk: an index lookup skips the vendored
    # trees and build outputs for free, and `check-no-tracked-file-find` forbids
    # the walk outright -- it measured 7m36s versus 0.8s for the same paths, and
    # notes that pruning does not help because find still stats every directory
    # it considers pruning. It caught this script's first draft.
    listing = subprocess.run(
        ["git", "ls-files", "-z", "--", "*.rs"],
        cwd=REPO,
        capture_output=True,
        check=True,
    )
    for rel in listing.stdout.decode("utf-8", "ignore").split("\0"):
        if not rel or rel in EXEMPT:
            continue
        path = REPO / rel
        try:
            text = path.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        if "NROS_" not in text:
            continue

        for knob in pats:
            if readers_in(text, knob):
                readers[knob].add(rel)

    # Drift between the two halves, reported before the reader check so the
    # message names the cause rather than a symptom.
    if unowned or stale:
        print("check-knob-single-reader: the ladder and its owners disagree\n")
        for knob in unowned:
            print(
                f"  {knob}: in the ladder (census class `ladder`) and names no\n"
                f"      owning reader here. Migrating a knob has TWO halves — the\n"
                f"      rung, and the single reader. Add it to OWNERS."
            )
        for knob in stale:
            print(
                f"  {knob}: names an owner here, but the census no longer classes\n"
                f"      it `ladder`. If it left the ladder, drop the row; if it was\n"
                f"      reclassified, the reason column there should say why."
            )
        return 1

    failures = []
    for knob, owner in sorted(MIGRATED.items()):
        extra = sorted(readers[knob] - {owner})
        if extra:
            failures.append(
                f"  {knob}: migrated, owner is {owner}, but also read by:\n"
                + "".join(f"      {r}\n" for r in extra)
            )

    if failures:
        print("check-knob-single-reader: a migrated knob has more than one reader\n")
        print("".join(failures))
        print(
            "A second reader is not a fallback, it is a disagreement waiting to\n"
            "happen: the two can resolve different values and nothing reports it\n"
            "(issues 0135, 0316). Delete the second reader, or -- if it is the\n"
            "legitimate owner -- update OWNERS in this script."
        )
        return 1

    print(
        f"check-knob-single-reader: OK - {len(MIGRATED)} migrated knob(s), "
        "one reader each"
    )

    # phase-454 W9 -- the retirement ledger, reported after the reader rule
    # because it is the same rule at wave scale and reads as its continuation.
    ledger = check_ledger()
    if ledger:
        print("\ncheck-knob-single-reader: the retirement ledger is violated\n")
        print("\n".join(ledger))
        return 1
    # LISTED on the success path, not just counted. The ledger's job is to say
    # what was retired and what answers the fact now; a gate that prints only
    # a number makes the reader open the source to learn either.
    print(f"\ncheck-knob-single-reader: {len(RETIRED)} retired path(s) resolve nowhere")
    for name, entry in sorted(RETIRED.items()):
        print(f"  - {name} ({entry.wave}) -- {entry.what}")
        print(f"      now: {entry.resolves_now}")
    by_issue: dict[int, list[str]] = {}
    for knob, kept in KEPT.items():
        by_issue.setdefault(kept.blocked_by, []).append(knob)
    print(
        f"check-knob-single-reader: {len(KEPT)} carrier(s) KEPT -- the retirement\n"
        "  question was asked and the answer was no, per issue:"
    )
    for issue in sorted(by_issue):
        print(f"  - issue {issue}: {len(by_issue[issue])} carrier(s)")
    return 0


if __name__ == "__main__":
    # Normal path, every run. A control nobody runs decays into a comment.
    self_test()
    ledger_self_test()
    sys.exit(main())
