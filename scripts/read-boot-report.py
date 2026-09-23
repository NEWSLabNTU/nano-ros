#!/usr/bin/env python3
"""Decode the nano-ros boot self-report out of a target memory dump.

phase-412. The image writes a fixed record about itself into RAM
(`packages/core/nros-node/src/boot_report.rs`); this reads it back after the
core has been halted. See that module for WHY the record is not a log stream:
the failure it exists to catch halts the board during entity creation, before
any advisory can print, on a board whose console UART is not wired.

Two inputs, and the ELF is not optional: the record's address is resolved from
the image's own symbol table rather than hard-coded, so a relinked image cannot
be read at a stale address and silently decode as garbage.

    pyocd commander -t s32k344 -c halt -c "savemem ADDR LEN report.bin"
    python3 scripts/read-boot-report.py build/zephyr/zephyr.elf report.bin

`--addr-only` prints the address and length for that savemem line, so the two
steps can be scripted without anyone copying a hex number by hand:

    read $(python3 scripts/read-boot-report.py --addr-only build/zephyr/zephyr.elf)

`--heap-headroom <dump>` is the GATE half (phase-460 W5, issue 1424): it needs
no ELF, reads the two heap words out of the dump, and exits non-zero when the
image had less headroom than `CONFIG_NROS_ZEPHYR_HEAP_SIZE` was supposed to
buy -- or when the peak is 0, which means nothing measured it. `just check
heap-headroom` runs it, and `NROS_HEAP_HEADROOM_DUMP=<dump>` points that recipe
at a real board dump.

Everything this script prints ABOUT ITSELF goes to stderr, because stdout is
consumed: `--addr-only`'s line is read into a shell variable, and the record
decode is asserted on by `executor::tests`.
"""

from __future__ import annotations

import argparse
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

SYMBOL = "NROS_BOOT_REPORT"

# "NRSR". Must match boot_report.rs MAGIC.
MAGIC = 0x4E525352
# Layout this script knows how to decode. Must match boot_report.rs VERSION.
KNOWN_VERSION = 5

# The headroom `CONFIG_NROS_ZEPHYR_HEAP_SIZE` must keep above the measured
# peak, in bytes.
#
# The SAME 24576 as the configure-time gate in
# `zephyr/cmake/nros_cargo_build.cmake` (arena + 24576 <= heap size), which
# issue 1424 describes as "18352 measured, rounded up to 24 KiB" -- one
# measurement, on one image, that nothing has re-checked since. Duplicating the
# constant is deliberate and is the point of this gate: the cmake check applies
# it to a DECLARED arena at configure time, this applies it to a MEASURED peak
# from a board, and the two agreeing is what would retire the guess. If the
# number moves, it moves in both files and the island re-measures.
HEAP_HEADROOM_FLOOR = 24576

# Field order, matching `BootReport` and `Snapshot` in boot_report.rs. Every
# field is a u32; the record is all `AtomicU32`, which is repr(transparent).
FIELDS = (
    "magic",
    "version",
    "struct_size",
    "stage",
    "arena_size",
    "max_cbs",
    "max_sc",
    "max_nodes",
    "default_rx_buf_size",
    "arena_capacity",
    "arena_used",
    "alloc_count",
    "last_alloc_size",
    "failed_alloc_size",
    "failed_alloc_shortfall",
    "cpp_init_ret",
    "err_class",
    "err_transport",
    "err_backend_ptr",
    "err_backend_len",
    # phase-460 W5. APPENDED, never inserted: every word before these is still
    # where it was, so a dump from an older image reads the same by eye and
    # `version` alone says which words exist.
    "heap_peak_bytes",
    "heap_capacity_bytes",
    # phase-460 W7, appended on the same rule. Issue 1425: a sample received,
    # ACKed and then discarded for being bigger than its subscription buffer.
    "samples_dropped_too_small",
)

STAGES = {
    0: "Untouched -- the image never entered nros_cpp_init",
    1: "ReportReady -- entered nros_cpp_init; arguments NOT yet validated",
    2: "BootConfigResolved -- arguments accepted; executor not yet open",
    3: "ExecutorReady -- arena bound",
    4: "RegisteringEntities -- an entity claimed arena; registration in flight",
    5: "EntitiesReady -- RESERVED, no producer (registration has no single end)",
    6: "FirstSpin -- registration complete and spinning",
}


# Assigned by `node_error_class` in packages/api/nros-cpp/src/lib.rs. Append
# only, and kept in step by hand -- the Rust side is exhaustive, so a new
# variant fails THERE first, which is the half that matters.
ERR_CLASS = {
    0: "(none)", 1: "Transport", 2: "NameTooLong", 3: "Serialization",
    4: "Deserialization", 5: "BufferTooSmall", 6: "ActionCreationFailed",
    7: "ServiceRequestFailed", 8: "ServiceReplyFailed", 9: "Timeout",
    10: "NotInitialized", 11: "RequestInFlight", 12: "NoSchedContextSlot",
    13: "InvalidSchedContextBinding", 14: "NodeTableFull", 15: "ExecutorFull",
    16: "BackendMismatch", 17: "ShutdownCallbacksFull",
}

# Assigned by `transport_error_class` in the same file.
ERR_TRANSPORT = {
    0: "(none)", 1: "ConnectionFailed", 2: "Disconnected",
    3: "PublisherCreationFailed", 4: "SubscriberCreationFailed",
    5: "ServiceServerCreationFailed", 6: "ServiceClientCreationFailed",
    7: "PublishFailed", 8: "ServiceRequestFailed", 9: "ServiceReplyFailed",
    10: "SerializationError", 11: "DeserializationError", 12: "BufferTooSmall",
    13: "MessageTooLarge", 14: "Timeout", 15: "InvalidConfig", 16: "WouldBlock",
    17: "TooLarge", 18: "TaskStartFailed", 19: "PollFailed",
    20: "KeepaliveFailed", 21: "JoinFailed", 22: "InvalidArgument",
    23: "Unsupported", 24: "BadAlloc", 25: "IncompatibleQos",
    26: "TopicNameInvalid", 27: "NodeNameNonExistent", 28: "LoanNotSupported",
    29: "NoData", 30: "IncompatibleAbi", 31: "Backend", 32: "BackendDynamic",
}

# Pools and tables map to NROS_CPP_RET_FULL, never to the transport code, so
# seeing any of these here rules a sizing knob IN -- and seeing a transport
# class rules every one of them OUT.
POOL_CLASSES = {5, 12, 14, 15, 17}


# --- the subscriber payload-class record (packages/rmw/zenoh/nros-rmw-zenoh) --
#
# A SECOND symbol, because the crate that owns these facts cannot call into the
# boot report: nros-node depends on nros-rmw-zenoh, so the edge runs the other
# way and a call would be a cycle. See SubscriberAllocReport's own doc comment.
ALLOC_SYMBOL = "NROS_SUBSCRIBER_ALLOC_REPORT"
ALLOC_MAGIC = 0x53554241  # "SUBA"
ALLOC_VERSION = 7
ALLOC_FIELDS = (
    "magic",
    "version",
    "struct_size",
    "refusal",
    "rx_hint",
    "largest_payload_class",
    "small_class_ceiling",
    "max_large_subscribers",
    "subscriber_large_size",
    "subscriber_buffer_size",
    "small_taken",
    "large_taken",
    "keyexpr_len",
    "keyexpr_cap",
    "zpico_err",
    "buffer_taken",
    "zpico_exit",
    "zpico_ret",
    "heap_used",
    "heap_peak",
    "heap_capacity",
)
# Assigned by `zpico_err_class` in the zenoh shim. The variant is recorded on
# the NEAR side of the C ABI because crossing it collapses Generic and Session
# into one ConnectionFailed and then into an indistinguishable
# Backend("rmw_ret error") -- issues 0870 and 0465.
ZPICO_ERR = {
    0: "(none -- the declare succeeded)",
    1: "Generic", 2: "Config", 3: "Session", 4: "Task", 5: "KeyExpr",
    6: "Full", 7: "Invalid", 8: "Publish", 9: "NotOpen", 10: "Timeout",
}

# Exit markers stamped by `zpico_declare_subscriber_ring` in the C shim.
ZPICO_EXIT = {
    0: "stamped NOTHING -- the function did not run, or ran a path with no marker",
    1: "session not open",
    2: "bad ring descriptor",
    3: "the zpico subscriber table is full",
    4: "zenoh-pico rejected the keyexpr",
    5: "z_declare_subscriber itself failed (zpico_ret carries its code)",
    6: "success",
}

ALLOC_REFUSAL = {
    0: "none -- every subscription got a payload block",
    1: "the hint is above EVERY class, so it has nowhere legal to go",
    2: "the LARGE class is full (MAX_LARGE_SUBSCRIBERS)",
    3: "the SMALL class is full (ZPICO_MAX_SUBSCRIBERS)",
    4: "the METADATA slots are full (ZPICO_MAX_SUBSCRIBERS) -- guarded FIRST",
}


def resolve_symbol(elf: Path, symbol: str = SYMBOL) -> tuple[int, int]:
    """Address and size of the record, from the ELF's symbol table.

    `nm` rather than a parser, because every toolchain that produced one of
    these images ships one, and the alternative is another dependency on the
    host side of a debugging tool.
    """
    for tool in ("nm", "arm-zephyr-eabi-nm", "arm-none-eabi-nm", "llvm-nm"):
        try:
            out = subprocess.run(
                [tool, "-S", "--defined-only", str(elf)],
                capture_output=True,
                text=True,
                check=False,
            )
        except FileNotFoundError:
            continue
        if out.returncode != 0:
            continue
        for line in out.stdout.splitlines():
            parts = line.split()
            # "<addr> <size> <type> <name>" -- the size column is why -S.
            if len(parts) == 4 and parts[3] == symbol:
                return int(parts[0], 16), int(parts[1], 16)
        # The tool worked and the symbol is not there. Say so rather than
        # trying the next tool and blaming its absence.
        raise SystemExit(
            f"{elf}: no `{symbol}` symbol.\n"
            "The image was built WITHOUT the boot report. Rebuild with\n"
            "  NROS_BOOT_REPORT=1        (Zephyr: CONFIG_NROS_BOOT_REPORT=y)"
        )
    raise SystemExit("no usable `nm` found (tried nm, arm-zephyr-eabi-nm, llvm-nm)")


def decode(blob: bytes, fields: tuple[str, ...] = FIELDS) -> dict[str, int]:
    want = len(fields) * 4
    if len(blob) < want:
        raise SystemExit(
            f"dump is {len(blob)} bytes, need at least {want} for one record"
        )
    values = struct.unpack_from(f"<{len(fields)}I", blob, 0)
    return dict(zip(fields, values, strict=True))


def heap_headroom(rec: dict[str, int]) -> tuple[bool, list[str]]:
    """Verdict on `CONFIG_NROS_ZEPHYR_HEAP_SIZE` against the measured peak.

    Returns (passed, lines). Issue 1424: the knob is the largest RAM item the
    map attributes to anything tunable, it was chosen rather than measured, and
    the high-water reporter that could have sized it has been compiled into
    every Zephyr image since phase-412 with no reader.

    A peak of 0 REFUSES. That is the case the whole gate turns on: an
    unmeasured image and a measured-and-fine image must not produce the same
    verdict, or the gate passes by default on exactly the board nobody could
    read. 0 means "no sample", never "used nothing" -- an image that opened a
    node allocated.
    """
    peak, cap = rec["heap_peak_bytes"], rec["heap_capacity_bytes"]

    if peak == 0:
        return False, [
            "HEAP HEADROOM: REFUSED -- the peak is 0, so nothing measured it.",
            "",
            "  The record carries no sample of the platform heap. Either the",
            "  image is older than phase-460 W5 (check `version` above), or its",
            "  Rust half was built without the report while its C half had",
            "  CONFIG_NROS_BOOT_REPORT=y, or it genuinely never allocated --",
            "  which an image that opened a node did not.",
            "",
            "  A zero cannot be read as `used nothing`: that verdict and `never",
            "  sampled` are the same bytes, and only one of them says the knob",
            "  is safe.",
        ]

    if cap == 0:
        return False, [
            "HEAP HEADROOM: REFUSED -- a peak of "
            f"{peak} with a capacity of 0.",
            "",
            "  The two words are written together by `nros_boot_report_note_heap`,",
            "  so a capacity of 0 means the heap reported none. There is nothing",
            "  to size against.",
        ]

    if peak > cap:
        return False, [
            f"HEAP HEADROOM: REFUSED -- the peak ({peak}) is above the capacity ({cap}).",
            "",
            "  Impossible for a live high-water figure, so the counter is not",
            "  one. DO NOT size a knob from this dump: find out which of the two",
            "  is wrong first.",
        ]

    headroom = cap - peak
    if headroom < HEAP_HEADROOM_FLOOR:
        return False, [
            f"HEAP HEADROOM: REFUSED -- {headroom} bytes, floor is "
            f"{HEAP_HEADROOM_FLOOR}.",
            "",
            f"  peak {peak} of {cap} bytes. The image booted; what it did not",
            "  keep is the margin for the allocations this run did not make --",
            "  a larger sample, a reconnect, a service reply.",
            "",
            f"  set CONFIG_NROS_ZEPHYR_HEAP_SIZE >= {peak + HEAP_HEADROOM_FLOOR}",
            "",
            "  and record THIS DUMP beside it in the board `.conf`, as a",
            "  comment naming the image and the date. A knob whose comment says",
            "  `a guess` is the state issue 1424 exists to end.",
        ]

    return True, [
        f"HEAP HEADROOM: ok -- {headroom} bytes spare "
        f"(peak {peak} of {cap}, floor {HEAP_HEADROOM_FLOOR}).",
        "",
        f"  CONFIG_NROS_ZEPHYR_HEAP_SIZE could go as low as "
        f"{peak + HEAP_HEADROOM_FLOOR} on this",
        "  evidence. Lowering it is a measurement on one run of one image:",
        "  record the dump beside the knob so the next reader can see what it",
        "  was sized from.",
    ]


def report(rec: dict[str, int]) -> int:
    """Print the record. Returns the process exit code."""
    if rec["magic"] != MAGIC:
        print(
            f"NO RECORD: magic is 0x{rec['magic']:08x}, expected 0x{MAGIC:08x}.\n"
            "\n"
            "The magic is written LAST by boot_report::init(), so this means the\n"
            "image did not get as far as constructing an Executor -- not that the\n"
            "dump is at the wrong address, which the ELF lookup already ruled out.\n"
            "An image that halted even earlier than that is itself the finding.",
            file=sys.stderr,
        )
        return 2

    if rec["version"] != KNOWN_VERSION:
        print(
            f"record version {rec['version']}, this script decodes {KNOWN_VERSION}.\n"
            "Refusing to decode rather than misreading it -- update this script.",
            file=sys.stderr,
        )
        return 2

    expect = len(FIELDS) * 4
    if rec["struct_size"] != expect:
        print(
            f"record says {rec['struct_size']} bytes, this script expects {expect}.\n"
            "Same version, different layout: one side changed a field without\n"
            "bumping VERSION.",
            file=sys.stderr,
        )
        return 2

    stage = rec["stage"]
    print(f"stage      {stage}  {STAGES.get(stage, 'UNKNOWN -- newer image than this script')}")
    print()
    print("compiled in (compare against what the build believed it delivered):")
    print(f"  NROS_EXECUTOR_ARENA_SIZE      {rec['arena_size']}")
    print(f"  NROS_EXECUTOR_MAX_CBS         {rec['max_cbs']}")
    print(f"  NROS_EXECUTOR_MAX_SC          {rec['max_sc']}")
    print(f"  NROS_EXECUTOR_MAX_NODES       {rec['max_nodes']}")
    print(f"  NROS_SUBSCRIPTION_BUFFER_SIZE {rec['default_rx_buf_size']}")
    print()
    print("measured on the board:")
    cap = rec["arena_capacity"]
    used = rec["arena_used"]
    print(f"  arena capacity                {cap}")
    print(f"  arena used                    {used}", end="")
    if cap:
        print(f"   ({100.0 * used / cap:.1f}%)")
    else:
        print()
    print(f"  arena allocations             {rec['alloc_count']}")
    print(f"  last allocation               {rec['last_alloc_size']} bytes")

    # The platform heap, which is a DIFFERENT arena from the executor's above:
    # the rlsf heap `nros_platform_alloc` hands out of, sized by
    # CONFIG_NROS_ZEPHYR_HEAP_SIZE. Issue 1424 -- this is the figure that knob
    # was never set from.
    peak, hcap = rec["heap_peak_bytes"], rec["heap_capacity_bytes"]
    print(f"  platform heap PEAK            {peak} bytes", end="")
    if hcap and peak:
        print(f"   ({100.0 * peak / hcap:.1f}% of the heap)")
    else:
        print()
    print(f"  platform heap capacity        {hcap} bytes   (NROS_ZEPHYR_HEAP_SIZE)")
    print(f"  samples dropped (too small)   {rec['samples_dropped_too_small']}")
    # PRINTED here, SCORED only under `--heap-headroom`. This function's exit
    # code means "the boot failed", and a thin heap on a board that booted is
    # not that -- it is a sizing verdict, and folding it in would turn every
    # existing caller of the decoder into a heap gate it did not ask for.
    print()
    for line in heap_headroom(rec)[1]:
        print(line)

    if cap and cap != rec["arena_size"]:
        print()
        print(
            f"  NOTE: the executor was handed {cap} bytes but the image compiled\n"
            f"  {rec['arena_size']}. Both are legitimate -- the arena's placement is the\n"
            "  caller's choice (issue 0900) -- but size against the one in use."
        )

    if rec["failed_alloc_size"]:
        print()
        print(
            f"ARENA EXHAUSTED: an allocation of {rec['failed_alloc_size']} bytes did not fit,\n"
            f"short by {rec['failed_alloc_shortfall']} bytes.\n"
            "\n"
            f"  set NROS_EXECUTOR_ARENA_SIZE >= {cap + rec['failed_alloc_shortfall']}\n"
            "  (Zephyr: CONFIG_NROS_EXECUTOR_ARENA_SIZE)\n"
            "\n"
            "  or, if a subscription was created deeper than the QoS depth the\n"
            "  arena was budgeted for, raise NROS_PUBSUB_QOS_DEPTH instead\n"
            "  (Zephyr: CONFIG_NROS_PUBSUB_QOS_DEPTH). The arena is re-derived\n"
            "  from the depth, so it cannot go stale the way a hand-set size does.\n"
            "  The console line (`arena exhausted at ...`) names the entity and\n"
            "  the depth the image budgeted.\n"
            "\n"
            "That is the FIRST failure, which is the one that explains the boot;\n"
            "later allocations may also have failed as a consequence."
        )
        return 1

    dropped = rec["samples_dropped_too_small"]
    if dropped:
        print()
        print(
            f"SAMPLES DROPPED (TOO SMALL): {dropped}.\n"
            "\n"
            "  Received, ACKed, and then thrown away because the subscription\n"
            "  buffer is smaller than the sample. NOT a boot failure and not\n"
            "  scored as one -- the image is running; it is a QoS fact, and the\n"
            "  application simply never saw those messages.\n"
            "\n"
            f"  raise NROS_SUBSCRIPTION_BUFFER_SIZE (compiled in here as\n"
            f"  {rec['default_rx_buf_size']}), or on Zephyr\n"
            "  CONFIG_NROS_SUBSCRIPTION_BUFFER_SIZE, to the type's\n"
            "  SERIALIZED_SIZE_MAX. The console lines name the buffer size; the\n"
            "  SAMPLE size is not in this record because the RMW ABI does not\n"
            "  report it (issue 0757)."
        )

    cls = rec["err_class"]
    if cls:
        print()
        print(f"LAST ERROR   {ERR_CLASS.get(cls, f'unknown class {cls}')}")
        if cls == 1:
            tr = rec["err_transport"]
            print(f"  transport  {ERR_TRANSPORT.get(tr, f'unknown transport {tr}')}")
            if rec["err_backend_ptr"]:
                print(
                    f"  backend message at 0x{rec['err_backend_ptr']:08x}, "
                    f"{rec['err_backend_len']} bytes. Read it with:\n"
                    f"    pyocd commander -t <target> --connect attach \\\n"
                    f"      -c \"savemem 0x{rec['err_backend_ptr']:08x} "
                    f"{rec['err_backend_len']} msg.bin\""
                )
        if cls in POOL_CLASSES:
            print(
                "  This is a POOL or TABLE exhaustion, so a sizing knob IS the\n"
                "  cause. Raise the one the name points at."
            )
        else:
            print(
                "  NOT a pool or table exhaustion -- those map to a different\n"
                "  code. No sizing knob explains this one."
            )

    ret = rec["cpp_init_ret"]
    if ret:
        signed = ret - (1 << 32) if ret >= (1 << 31) else ret
        print()
        print(
            f"nros_cpp_init RETURNED {signed} (nros_cpp_ret_t).\n"
            "The stage above says how far it got; this says why it stopped.\n"
            "Stage 1 with a return code means an argument was rejected before\n"
            "anything was opened; stage 2 means the backend refused."
        )
        return 1

    if stage < 6:
        print()
        if rec["failed_alloc_size"]:
            pass  # already reported above
        elif stage == 4:
            print(
                f"Halted DURING REGISTRATION: {rec['alloc_count']} allocation(s) succeeded,\n"
                f"{rec['arena_used']} of {cap} arena bytes are claimed, and none of them\n"
                "failed. So the arena is not the cause -- registration began and\n"
                "stopped for some other reason. `LAST ERROR` above, if set, is it."
            )
        elif rec["alloc_count"]:
            print(
                f"Never reached the first spin, and the arena is NOT why: all\n"
                f"{rec['alloc_count']} allocations succeeded and "
                f"{rec['arena_used']} of {cap} bytes are claimed.\n"
                "The executor was built and its entities took their arena; what\n"
                "did not happen is the spin. Look at what the application does\n"
                "between registering and spinning -- a blocking wait there\n"
                "presents exactly like this, and no knob is involved."
            )
        else:
            print(
                "The executor was built but claimed NO arena, so registration\n"
                "never started (the stage would read 4 if it had). That is\n"
                "earlier than any sizing knob can explain."
            )
        return 1

    return 0


def report_alloc(rec: dict[str, int]) -> int:
    """Print the subscriber payload-class record."""
    if rec["magic"] != ALLOC_MAGIC:
        print(
            f"NO ALLOC RECORD: magic is 0x{rec['magic']:08x}, expected "
            f"0x{ALLOC_MAGIC:08x}.\n"
            "alloc_payload_block was never called, so no subscription got as far\n"
            "as asking for a payload block.",
            file=sys.stderr,
        )
        return 2
    if rec["version"] != ALLOC_VERSION:
        print(f"alloc record version {rec['version']}, this script decodes "
              f"{ALLOC_VERSION}.", file=sys.stderr)
        return 2

    print("payload classes, as compiled:")
    print(f"  SUBSCRIBER_BUFFER_SIZE (small)  {rec['subscriber_buffer_size']}")
    print(f"  SMALL_CLASS_CEILING             {rec['small_class_ceiling']}")
    print(f"  SUBSCRIBER_LARGE_SIZE           {rec['subscriber_large_size']}")
    print(f"  MAX_LARGE_SUBSCRIBERS           {rec['max_large_subscribers']}")
    print(f"  LARGEST_PAYLOAD_CLASS           {rec['largest_payload_class']}")
    if rec["max_large_subscribers"] == 0:
        print(
            "  NOTE: no large slots, so the large class does not exist and the\n"
            "  ceiling is the small block. Any hint above it is refused."
        )
    print()
    print("measured on the board:")
    print(f"  metadata slots taken            {rec['buffer_taken']}")
    print(f"  small blocks taken              {rec['small_taken']}")
    print(f"  large blocks taken              {rec['large_taken']}")
    print(f"  last hint seen                  {rec['rx_hint']}")
    hp, hc = rec["heap_peak"], rec["heap_capacity"]
    if hc:
        pct = f"   ({100.0 * hp / hc:.1f}% of the arena)" if hp else ""
        print(f"  platform heap used              {rec['heap_used']}")
        print(f"  platform heap PEAK              {hp}{pct}")
        print(f"  platform heap capacity          {hc}   (NROS_ZEPHYR_HEAP_SIZE)")
        if hp > hc:
            print(
                "  DO NOT SIZE A KNOB FROM THIS. A peak above capacity is impossible\n"
                "  for a live figure, so the counter is not one: zpico-alloc\n"
                "  decrements used_bytes only on the SLAB free path, never on the\n"
                "  rlsf path, so `used` is cumulative-allocated and `peak` tracks it.\n"
                "  Fixing it needs the block size at free -- rlsf 0.2.3 exposes\n"
                "  allocation_usable_size only under its `unstable` feature."
            )
        elif hp:
            print(
                f"  -> size NROS_ZEPHYR_HEAP_SIZE from {hp}, not from a round number.\n"
                "     This is the peak at the LAST subscription, so add the headroom\n"
                "     the application's own later allocations need."
            )
    else:
        # NOT `nros-platform/heap-stats`, which this line said for three
        # phases and which has never existed. The features are `alloc-stats`
        # (the Rust allocator's counter) and `zpico-alloc/stats` (the arena's),
        # and the second is unconditional on Zephyr since phase-412 -- so a
        # zero here is an arena that was never asked, not a feature that was
        # left off. Issue 1424 found the same stale name in platform.c.
        print("  platform heap                   not instrumented (zpico-alloc/stats off)")
    print()
    kl, kc = rec["keyexpr_len"], rec["keyexpr_cap"]
    print(f"  last keyexpr length             {kl} of {kc}")
    if kc and kl >= kc:
        print(
            "  TRUNCATED: the keyexpr filled its buffer exactly. `to_key_wildcard`\n"
            "  discards the overflow, so this is a silently shortened key, and the\n"
            "  length guard cannot see it -- a truncated string always fits.\n"
            "  Raise NROS_KEYEXPR_STRING_SIZE (note: currently fails to compile,\n"
            "  service.rs hardcodes 257)."
        )
    zx = rec["zpico_exit"]
    if rec["zpico_err"] or zx:
        zr = rec["zpico_ret"]
        zr_s = zr - (1 << 32) if zr >= (1 << 31) else zr
        print(f"  zpico declare exit              {zx} -- {ZPICO_EXIT.get(zx, 'unknown')}")
        if zx == 5:
            print(f"  z_declare_subscriber returned   {zr_s}")
        elif zx == 0:
            print(
                "  The caller saw an error it believes came from this function,\n"
                "  and the function stamped no exit. Either it never ran, or the\n"
                "  error came from somewhere else."
            )
    ze = rec["zpico_err"]
    if ze:
        print(f"  zpico declare returned          {ZPICO_ERR.get(ze, f'unknown {ze}')}")
        if ze == 3:
            print(
                "  Session -- the zenoh session was NOT OPEN when the declare ran.\n"
                "  Nothing about sizing; the transport went away mid-registration.\n"
                "  This exit has no printk, which is why the console showed nothing."
            )
        elif ze == 1:
            print("  Generic -- z_declare_subscriber itself failed; zpico printk carries the code.")
        elif ze == 6:
            print("  Full -- the zpico subscriber table is exhausted (ZPICO_MAX_SUBSCRIBERS).")
    print()

    r = rec["refusal"]
    print(f"refusal    {r}  {ALLOC_REFUSAL.get(r, 'unknown')}")
    if r == 0:
        return 1 if ze else 0
    print()
    if r == 1:
        print(
            f"  A subscription asked for {rec['rx_hint']} bytes and the largest class\n"
            f"  is {rec['largest_payload_class']}. Raise NROS_SUBSCRIBER_LARGE_SIZE and give the\n"
            "  image large slots (NROS_MAX_LARGE_SUBSCRIBERS), or lower the type's bound."
        )
    elif r == 2:
        print(
            f"  {rec['large_taken']} large blocks were taken and MAX_LARGE_SUBSCRIBERS is\n"
            f"  {rec['max_large_subscribers']}. Raise NROS_MAX_LARGE_SUBSCRIBERS."
        )
    elif r == 4:
        print(
            f"  {rec['buffer_taken']} metadata slots were taken and the pool is that size.\n"
            "  Compare against the number of subscriptions the image DECLARES: if\n"
            "  the image needed MORE, something other than the application is\n"
            "  taking slots, and the derivation of NROS_RMW_SUBSCRIBER_SLOTS from\n"
            "  COUNT_SUBSCRIPTION is short by that many."
        )
    else:
        print(
            f"  {rec['small_taken']} small blocks were taken. Compare that against the number\n"
            "  of subscriptions the image DECLARES: if it is higher, something\n"
            "  other than the application is taking blocks, and the derivation\n"
            "  from the entity inventory is short by that many."
        )
    return 1


def make_dump(**fields: int) -> bytes:
    """A synthetic record, for the gate's own negative controls.

    Built from `FIELDS` rather than from a literal byte string, so a field
    appended to the record cannot leave the fixtures decoding one word short --
    the failure this file's whole positional decode is exposed to.
    """
    rec = dict.fromkeys(FIELDS, 0)
    rec["magic"] = MAGIC
    rec["version"] = KNOWN_VERSION
    rec["struct_size"] = len(FIELDS) * 4
    unknown = set(fields) - set(rec)
    if unknown:
        raise SystemExit(f"make_dump: no such field(s): {sorted(unknown)}")
    rec.update(fields)
    return struct.pack(f"<{len(FIELDS)}I", *(rec[f] for f in FIELDS))


def check_heap_headroom(dump: Path, quiet: bool = False) -> int:
    """`just check heap-headroom` on one dump. Returns the process exit code.

    No ELF: this reads a dump alone on purpose. The gate has to be runnable by
    whoever holds the `savemem` output, on a machine that may not have the
    image -- and the ELF's only job in the decoder is to resolve the address,
    which has already happened by the time a dump exists.

    `quiet` is for the selftest's own fixtures, which would otherwise print six
    verdicts in front of every real run. It suppresses the OUTPUT, never the
    formatting: the lines are still built, so a broken message is still a
    failing selftest rather than a surprise on the day someone needs it.
    """
    rec = decode(dump.read_bytes())

    def say(lines: list[str], ok: bool) -> None:
        text = "\n".join([f"heap-headroom: {dump}", *lines])
        if not quiet:
            print(text, file=sys.stdout if ok else sys.stderr)

    if rec["magic"] != MAGIC:
        say(
            [
                f"REFUSED -- magic is 0x{rec['magic']:08x}, expected 0x{MAGIC:08x}.",
                "  Not a boot report, or an image that never reached",
                "  boot_report::init(). Either way there is no measurement here.",
            ],
            False,
        )
        return 2
    if rec["version"] != KNOWN_VERSION:
        say(
            [
                f"REFUSED -- record version {rec['version']}, this script decodes "
                f"{KNOWN_VERSION}.",
                "  Refusing rather than reading a word that may not be the one",
                "  it names.",
            ],
            False,
        )
        return 2

    ok, lines = heap_headroom(rec)
    say(lines, ok)
    return 0 if ok else 1


def self_test() -> int:
    """The three cases from phase-460 W5's gate, on fixtures built here.

    Runs on EVERY invocation, not behind a flag: a negative control nobody runs
    decays into a comment, and this one guards a verdict about a number nobody
    can re-derive without a board. On stderr, because stdout belongs to
    `--addr-only`'s caller and to `executor::tests`.

    The board run is the island's (issue 1036 records why no nano-ros lane can
    perform one), so these fixtures are the only place the REFUSALS are ever
    exercised in this repo.
    """
    cases = [
        # (name, fields, expected pass?)
        ("never sampled", {"heap_peak_bytes": 0, "heap_capacity_bytes": 94208}, False),
        ("both zero", {}, False),
        (
            "headroom below the floor",
            {"heap_peak_bytes": 94208 - 24575, "heap_capacity_bytes": 94208},
            False,
        ),
        (
            "headroom exactly at the floor",
            {"heap_peak_bytes": 94208 - HEAP_HEADROOM_FLOOR, "heap_capacity_bytes": 94208},
            True,
        ),
        (
            "real headroom",
            {"heap_peak_bytes": 18352, "heap_capacity_bytes": 94208},
            True,
        ),
        # A peak above capacity is not a thin heap, it is a broken counter.
        ("peak above capacity", {"heap_peak_bytes": 99999, "heap_capacity_bytes": 94208}, False),
    ]
    ok = True
    with tempfile.TemporaryDirectory() as d:
        for name, fields, want in cases:
            path = Path(d) / (name.replace(" ", "-") + ".bin")
            path.write_bytes(make_dump(**fields))
            got = check_heap_headroom(path, quiet=True) == 0
            if got != want:
                ok = False
                print(
                    f"  self-test FAIL {name}: "
                    f"{'passed' if got else 'refused'}, expected "
                    f"{'pass' if want else 'refusal'}",
                    file=sys.stderr,
                )
        # A dump that is not a record at all must refuse rather than decode.
        junk = Path(d) / "junk.bin"
        junk.write_bytes(b"\x00" * (len(FIELDS) * 4))
        if check_heap_headroom(junk, quiet=True) != 2:
            ok = False
            print("  self-test FAIL junk: a record-less dump did not refuse", file=sys.stderr)
    print(
        "read-boot-report --self-test: " + ("OK" if ok else "FAILED"),
        file=sys.stderr,
    )
    return 0 if ok else 1


def main() -> int:
    # The selftest runs on the NORMAL path, before anything is parsed -- a
    # negative control nobody runs decays into a comment, which is what
    # `check-gate-selftests` enforces of every script a gate invokes.
    rc = self_test()
    if rc != 0:
        return rc

    ap = argparse.ArgumentParser(
        description="Decode the nano-ros boot self-report out of a target memory dump."
    )
    ap.add_argument("elf", type=Path, nargs="?", help="the image the board is running")
    ap.add_argument("dump", type=Path, nargs="?", help="memory dumped from the target")
    ap.add_argument(
        "--alloc",
        action="store_true",
        help="decode the subscriber payload-class record instead of the boot report",
    )
    ap.add_argument(
        "--addr-only",
        action="store_true",
        help="print '<addr> <len>' for a pyocd savemem line, and exit",
    )
    ap.add_argument(
        "--self-test",
        action="store_true",
        help="run the negative controls and stop there (they run on every "
        "invocation either way -- this only says not to decode anything after)",
    )
    ap.add_argument(
        "--heap-headroom",
        type=Path,
        metavar="DUMP",
        help="gate the heap knob against the dump's measured peak, and exit "
        "non-zero when the headroom is thin or the peak was never sampled "
        "(no ELF needed)",
    )
    args = ap.parse_args()

    # `self_test()` has already run, at the top of this function. The flag only
    # says to stop here -- it is what `just check heap-headroom` invokes when
    # no board dump is available, which is every run inside this repo.
    if args.self_test:
        return 0

    if args.heap_headroom is not None:
        if not args.heap_headroom.is_file():
            raise SystemExit(f"{args.heap_headroom}: not a file")
        return check_heap_headroom(args.heap_headroom)

    if args.elf is None:
        ap.error("an ELF is required unless --heap-headroom is given")
    if not args.elf.is_file():
        raise SystemExit(f"{args.elf}: not a file")

    sym = ALLOC_SYMBOL if args.alloc else SYMBOL
    addr, size = resolve_symbol(args.elf, sym)
    if args.addr_only:
        print(f"0x{addr:08x} {size}")
        return 0

    if args.dump is None:
        ap.error("a dump is required unless --addr-only is given")
    if not args.dump.is_file():
        raise SystemExit(f"{args.dump}: not a file")

    blob = args.dump.read_bytes()
    if args.alloc:
        return report_alloc(decode(blob, ALLOC_FIELDS))
    return report(decode(blob))


if __name__ == "__main__":
    sys.exit(main())
