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
    """The env names the census classifies as `ladder`.

    Imported rather than restated: two hand-kept lists of "what is in the
    ladder" is the same duplicate-fact shape the ladder itself exists to
    remove, and the census is already the file a new knob has to be added to
    (its gate fails on an unclassified name).
    """
    import importlib.util

    census = REPO / "scripts" / "check" / "config-knob-census.py"
    spec = importlib.util.spec_from_file_location("config_knob_census", census)
    mod = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(mod)
    return census_class(("ladder",))
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
    return 0


if __name__ == "__main__":
    # Normal path, every run.
    self_test()
    sys.exit(main())
