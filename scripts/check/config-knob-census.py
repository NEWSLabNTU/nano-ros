#!/usr/bin/env python3
"""phase-400 — count where configuration lives, reproducibly.

The phase doc opens with a table (78 Kconfig symbols, 21 `NROS_*` env, 9
`ZPICO_*` env, against 3 knobs in the RFC-0049 ladder) and every wave's gate is
stated as "measured the same way as the table at the top". No method was
recorded, so the gate could not be evaluated: two people counting by hand get
two numbers, and a wave cannot show it moved anything.

This is that method. It is deliberately BUILDLESS — grep and TOML, no cargo —
so it can run on the fast line and so the number does not depend on a build
succeeding.

What each row counts:

  ladder knobs      fields on the typed `[knobs.*]` structs in
                    `nros-board-common`. These are the knobs that have a
                    platform and board rung and that `nros config explain`
                    reports. This is the number the phase exists to RAISE.

  Kconfig symbols   `config NROS_*` DECLARATIONS in Kconfig files — declared,
                    not referenced, because a symbol read in ten places is one
                    knob.

  build-script env  distinct `NROS_*` names a `build.rs` reads from the
                    environment. Not every `NROS_*` in the tree: a name that
                    only cmake sets and cmake reads is not a build-script knob.

  ZPICO_* env       the same, for the zenoh-pico tenant's own namespace.

A knob counted in the ladder MAY also still appear as env — that is the point:
migrating keeps the env name as the front-end. The ladder number rising is the
signal, not the env number falling to zero.
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

# The ratchet. "The ladder knob count rises" is the wave gate, and a count that
# only ever rises is enforceable where a count that "should fall" is not: an
# env name legitimately SURVIVES migration as the front-end, so the env rows are
# reported, never gated. Raise this when a tenant lands.
LADDER_FLOOR = 15

# Every build-script env name, and WHAT IT IS. An unclassified name FAILS the
# gate rather than landing in a bucket by heuristic — the first version of this
# analysis split by read idiom (`env_usize` vs `env::var`) and misfiled three
# knobs that are numeric but read as strings and handed to a C `#define`.
#
#   ladder   already resolved over the RFC-0049 ladder
#   sizing   a sizing knob still to migrate — THE BACKLOG
#   derived  a number ANOTHER CAMPAIGN is making derived rather than set:
#            phase-403 / phase-408 for the receive and transmit buffers (from
#            the message type), phase-392 for the zenoh entity caps and pools
#            (from what the image declares). Migrating one of these into the
#            ladder would be WRONG: a rung gives a global a per-platform
#            default, and the point of that work is that it stops being a
#            global. The reason column names the owner.
#   infra    a path, a flag, or an input to the ladder itself. Not a knob, and
#            counting it as one overstates the backlog — which the first
#            census did, by ~40%.
KNOB_CLASS = {
    # --- ladder (phase-400 W6) ---
    "NROS_EXECUTOR_MAX_CBS": ("ladder", "executor tenant"),
    "NROS_EXECUTOR_MAX_SC": ("ladder", "executor tenant"),
    "NROS_EXECUTOR_MAX_NODES": ("ladder", "executor tenant"),
    "NROS_EXECUTOR_MAX_SHUTDOWN_CBS": ("ladder", "executor tenant"),
    "NROS_EXECUTOR_ACTION_CLIENTS": ("ladder", "executor tenant"),
    "NROS_EXECUTOR_ARENA_SIZE": ("ladder", "executor tenant; 0 = derive"),
    "NROS_PARAM_SERVICE_BUFFER_SIZE": ("ladder", "executor tenant"),
    # --- derived: the message type should size these ---
    "NROS_SUBSCRIPTION_BUFFER_SIZE": (
        "derived",
        "the runtime-owned take buffer. phase-403 makes it per-type; it is in "
        "the ladder today as the fallback for a type with no bound",
    ),
    "ZPICO_SUBSCRIBER_BUFFER_SIZE": ("derived", "SMALL_PAYLOADS class (phase-403)"),
    "ZPICO_SUBSCRIBER_LARGE_SIZE": ("derived", "LARGE_PAYLOADS class (phase-403)"),
    "ZPICO_SUBSCRIBER_SIZE_THRESHOLD": ("derived", "SMALL_CLASS_CEILING (phase-403)"),
    "ZPICO_PUBLISHER_TX_BUFFER_SIZE": ("derived", "TX half of the same split"),
    # --- sizing: the backlog ---
    "NROS_MAX_ARRAY_LEN": ("sizing", "parameter value bound (nros-params)"),
    "NROS_MAX_BYTE_ARRAY_LEN": ("sizing", "parameter value bound (nros-params)"),
    "NROS_MAX_STRING_VALUE_LEN": ("sizing", "parameter value bound (nros-params)"),
    "NROS_MAX_PARAM_NAME_LEN": ("sizing", "parameter bound"),
    "NROS_MAX_PARAMETERS": ("sizing", "parameter cap"),
    "NROS_RUNTIME_COMPONENT_SLOT_BYTES": ("sizing", "component cap"),
    "NROS_RUNTIME_MAX_CELL_ENTITIES": ("sizing", "component cap"),
    "NROS_RUNTIME_MAX_CLASS_INSTANCES": ("sizing", "component cap"),
    "NROS_RUNTIME_MAX_COMPONENTS": ("sizing", "component cap"),
    "NROS_ZEPHYR_HEAP_SIZE": ("sizing", "platform heap"),
    "NROS_FREERTOS_HEAP_KB": ("ladder", "memory tenant; KiB front-end over a bytes rung"),
    "NROS_FREERTOS_APP_STACK_KB": ("ladder", "memory tenant; KiB front-end over a bytes rung"),
    "NROS_KEYEXPR_STRING_SIZE": ("sizing", "keyexpr bound"),
    "NROS_SERVICE_TIMEOUT_MS": ("sizing", "a timeout, not a size, but the same ladder shape"),
    "NROS_XRCE_CUSTOM_TRANSPORT_MTU": ("sizing", "transport MTU; numeric, read as a string"),
    "ZPICO_MAX_LARGE_SUBSCRIBERS": ("derived", "pool cardinality; multiplies LARGE_PAYLOADS, phase-392"),
    "ZPICO_SERVICE_BUFFER_SIZE": ("derived", "SERVICE_BUFFERS is MAX_SESSIONS x MAX_QUERYABLES; phase-392"),
    # --- infra: not knobs ---
    "NROS_ALLOW_UNRESOLVED_DEPS": ("infra", "policy flag"),
    "NROS_BUILD_ROOT": ("infra", "path"),
    "NROS_CARGO_FLAGS": ("infra", "the --locked shim"),
    "NROS_LINK_IP": ("infra", "link toggle"),
    "NROS_PLATFORMS_DIR": ("infra", "the ladder's own search path"),
    "NROS_PLATFORM_NAME": ("infra", "the ladder's own platform rung input"),
    "NROS_PX4_BRIDGE_GEN": ("infra", "codegen toggle"),
    "NROS_REPO_DIR": ("infra", "path"),
    "NROS_TRACE": ("infra", "debug flag"),
    "NROS_ZEPHYR_WORKSPACE": ("infra", "path"),
    # --- ladder: the zenoh tx tenant (phase-282), read in the build HELPER ---
    "ZPICO_TX_BATCH": ("ladder", "zenoh.tx tenant"),
    "ZPICO_TX_SPLIT_LOCK": ("ladder", "zenoh.tx tenant"),
    "ZPICO_TX_BATCH_FLUSH_MS": ("ladder", "zenoh.tx tenant"),
    # --- sizing: the zenoh-pico entity caps and buffers. The largest single
    # --- family left, and the one the phase doc calls "the per-entity caps".
    "ZPICO_MAX_PUBLISHERS": ("derived", "entity cap; phase-392 is deciding whether it is derived from the declaration"),
    "ZPICO_MAX_SUBSCRIBERS": ("derived", "entity cap; phase-392"),
    "ZPICO_MAX_QUERYABLES": ("derived", "already a CHECKED OVERRIDE over a derived default (phase-392 W5.f)"),
    "ZPICO_MAX_SESSIONS": ("derived", "phase-392 poses it explicitly: joins the model, or stays a knob and that phase says so"),
    "ZPICO_MAX_LIVELINESS": ("derived", "entity cap; phase-392"),
    "ZPICO_MAX_PENDING_GETS": ("derived", "entity cap; phase-392"),
    "ZPICO_BATCH_UNICAST_SIZE": ("sizing", "wire batch buffer"),
    "ZPICO_BATCH_MULTICAST_SIZE": ("sizing", "wire batch buffer"),
    "ZPICO_FRAG_MAX_SIZE": ("sizing", "fragmentation ceiling"),
    "ZPICO_GET_REPLY_BUF_SIZE": ("sizing", "reply staging block"),
    "ZPICO_GET_POLL_INTERVAL_MS": ("sizing", "a poll interval, not a size; same ladder shape"),
    "ZPICO_READ_TASK_PRIORITY": ("sizing", "transport-band priority (issue 0623)"),
    "ZPICO_LEASE_TASK_PRIORITY": ("sizing", "transport-band priority (issue 0623)"),
    "NROS_LET_BUFFER_SIZE": ("sizing", "logical-execution-time buffer"),
    # --- infra ---
    "NROS_DECLARED_INFRA_QUERYABLES": ("infra", "a COUNT the resolver passes down, not a knob"),
    "NROS_DECLARED_SERVICE_SERVERS": ("infra", "a COUNT the resolver passes down, not a knob"),
    "NROS_PICOLIBC_SYSROOT": ("infra", "path"),
    "NROS_RISCV64_PREFIX": ("infra", "toolchain prefix"),
    "NROS_SDK_STORE": ("infra", "path"),
    "NROS_SIZES_PROBE_TARGET_DIR": ("infra", "path"),
    "NROS_ZPICO_DEBUG": ("infra", "debug flag"),
    "ZPICO_NO_SMOLTCP": ("infra", "link toggle"),
    "ZPICO_PLATFORMS_TOML": ("infra", "the ladder's own platform file pointer"),
}


def tracked(*globs):
    out = subprocess.check_output(
        ["git", "-C", ROOT, "ls-files", *globs], text=True
    ).split()
    return [os.path.join(ROOT, p) for p in out]


def read(path):
    try:
        with open(path, encoding="utf-8", errors="replace") as fh:
            return fh.read()
    except OSError:
        return ""


def _struct_fields(src, struct):
    """The `pub` field names of one struct. Factored out so the selftest can
    drive it on synthetic input rather than on the real file."""
    m = re.search(r"pub struct " + struct + r"\s*\{(.*?)\n\}", src, re.S)
    if not m:
        return []
    body = re.sub(r"(?m)^\s*(///|//).*$", "", m.group(1))
    return sorted(set(re.findall(r"(?m)^\s*pub ([a-z_][a-z0-9_]*)\s*:", body)))


def ladder_knobs():
    """Fields on the typed `[knobs.*]` structs — the migrated knobs."""
    src = read(os.path.join(ROOT, "packages/boards/nros-board-common/src/platform_config.rs"))
    total = {}
    for struct in ("TxKnobs", "ExecutorKnobs", "TransportKnobs", "MemoryKnobs"):
        fields = _struct_fields(src, struct)
        if fields:
            total[struct] = fields
    return total


def kconfig_symbols():
    syms = set()
    for p in tracked("*Kconfig", "*Kconfig.*", "*/Kconfig"):
        for m in re.finditer(r"(?m)^\s*(?:menu)?config\s+(NROS_[A-Z0-9_]+)", read(p)):
            syms.add(m.group(1))
    return syms


def _env_names_in(txt, prefix):
    """`<PREFIX>_*` names READ from the environment in one source string.

    Comments are stripped first: a knob named in a comment is documentation,
    not a reader, and counting it inflates the number this gate exists to
    track. Factored out so the selftest drives it on synthetic input.
    """
    stripped = re.sub(r"(?m)^\s*(///|//).*$", "", txt)
    return set(
        m.group(1)
        for m in re.finditer(
            r'(?:env::var|env::var_os|env|env_usize|env_bool|knob_usize|knob_bool)'
            r'\s*\(\s*&?"(' + prefix + r'_[A-Z0-9_]+)"',
            stripped,
        )
    )


def build_env_sources():
    """Files that run at BUILD time and may read a knob.

    Not just `build.rs`: a build script that grew past a few lines moved its
    body into a helper crate, and the knobs moved with it. 21 `ZPICO_*` names
    live in `nros-zpico-build/src/`, and a census that scanned only `build.rs`
    reported six — so both this file's first number and the phase doc's
    original table undercounted the zenoh surface by a factor of four.

    The convention is the crate NAME: `*-build` crates and everything under
    `packages/tooling/` are build-time by construction.
    """
    out = list(tracked("*build.rs"))
    out += [p for p in tracked("packages/*/*-build/src/*.rs", "packages/*/*/*-build/src/*.rs")]
    out += list(tracked("packages/tooling/*/src/*.rs"))
    return sorted(set(out))


def build_script_env(prefix):
    """`<PREFIX>_*` names a build-time source reads from the environment."""
    names = set()
    for p in build_env_sources():
        names |= _env_names_in(read(p), prefix)
    return names


def self_test() -> None:
    """Negative controls for both counters, on synthetic input.

    On the normal path, not behind a flag — `check-gate-selftests` requires it,
    on the reasoning that a control nobody runs decays into a comment. This
    census earns the scepticism twice over: it is a COUNTING gate, so a regex
    that silently matches too much or too little still prints a plausible
    number, and a wrong baseline is worse than none because it looks measured.
    """
    # The env matcher must see the idioms build scripts actually use...
    for src in (
        'let n = env_usize("NROS_EXECUTOR_MAX_CBS", 4);',
        'std::env::var("NROS_THING").ok()',
        'nros_zephyr_build::knob_usize("NROS_OTHER", &k, 1)',
    ):
        assert _env_names_in(src, "NROS"), f"selftest: missed a read in {src!r}"
    # ...and must not count a name it merely MENTIONS, or another prefix.
    for src in (
        '// NROS_EXECUTOR_MAX_CBS was read here once',
        'println!("set NROS_THING");',
        'let n = env_usize("ZPICO_TX_BATCH", 0);',
    ):
        assert not _env_names_in(src, "NROS"), f"selftest: false positive on {src!r}"

    # The ladder counter reads STRUCT FIELDS, so it must not count a doc
    # comment, a non-`pub` field, or a field of a struct it was not asked for.
    src = (
        "pub struct TxKnobs {\n"
        "    /// pub not_a_field: usize,\n"
        "    pub batch: Option<bool>,\n"
        "    private: usize,\n"
        "}\n"
        "pub struct Unrelated {\n    pub nope: usize,\n}\n"
    )
    fields = _struct_fields(src, "TxKnobs")
    assert fields == ["batch"], f"selftest: ladder counter read {fields!r}"


def main() -> int:
    self_test()
    ladder = ladder_knobs()
    n_ladder = sum(len(v) for v in ladder.values())
    kconfig = kconfig_symbols()
    nros_env = build_script_env("NROS")
    zpico_env = build_script_env("ZPICO")

    seen = sorted(nros_env | zpico_env)
    by_class = {}
    unclassified = []
    for n in seen:
        cls = KNOB_CLASS.get(n)
        if cls is None:
            unclassified.append(n)
        else:
            by_class.setdefault(cls[0], []).append(n)

    print("phase-400 config census")
    print()
    print(f"  {'where configuration lives':<38} count")
    print(f"  {'-' * 38} -----")
    print(f"  {'knobs in the RFC-0049 ladder':<38} {n_ladder}")
    for struct, fields in sorted(ladder.items()):
        print(f"      {struct:<34} {len(fields)}")
    print(f"  {'Kconfig CONFIG_NROS_* declarations':<38} {len(kconfig)}")
    print()
    print(f"  build-script env names, by what they ARE ({len(seen)} total)")
    print(f"  {'-' * 38} -----")
    for cls, label in (
        ("sizing", "sizing knobs still to migrate  <-- W6"),
        ("derived", "another campaign is DERIVING these"),
        ("ladder", "already on the ladder"),
        ("infra", "paths / flags / ladder inputs (not knobs)"),
    ):
        print(f"  {label:<38} {len(by_class.get(cls, []))}")

    if "--verbose" in sys.argv:
        print()
        for cls in ("sizing", "derived", "ladder", "infra"):
            print(f"  {cls}:")
            for n in by_class.get(cls, []):
                print(f"    {n:<36} {KNOB_CLASS[n][1]}")

    if unclassified:
        sys.stderr.write(
            "config-knob-census: FAILED — build scripts read env name(s) that "
            "KNOB_CLASS does not classify:\n"
        )
        for n in unclassified:
            sys.stderr.write(f"    {n}\n")
        sys.stderr.write(
            "  Classify each in scripts/check/config-knob-census.py. The point\n"
            "  is that a new knob forces a DECISION — is it a sizing knob for\n"
            "  the ladder, a size the message type should derive (phase-403),\n"
            "  or infrastructure? A heuristic would guess, and guessing is how\n"
            "  the backlog number stops meaning anything.\n"
        )
        return 1

    if "--check" in sys.argv and n_ladder < LADDER_FLOOR:
        sys.stderr.write(
            f"config-knob-census: FAILED — the ladder holds {n_ladder} knob(s), "
            f"below the {LADDER_FLOOR} recorded floor.\n"
            "  A knob leaving the ladder is a knob losing its platform and board\n"
            "  rung and its `nros config explain` row. If that is deliberate,\n"
            "  lower LADDER_FLOOR in this file and say why in the commit.\n"
        )
        return 1

    return 0


if __name__ == "__main__":
    sys.exit(main())
