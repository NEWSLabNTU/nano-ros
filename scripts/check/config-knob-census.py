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
import sys as _pi_sys

_pi_sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "lib"))
import per_item  # noqa: E402  phase-472 W6
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

# The ratchet. "The ladder knob count rises" is the wave gate, and a count that
# only ever rises is enforceable where a count that "should fall" is not: an
# env name legitimately SURVIVES migration as the front-end, so the env rows are
# reported, never gated. Raise this when a tenant lands.
LADDER_FLOOR = 42

# Every build-script env name, and WHAT IT IS. An unclassified name FAILS the
# gate rather than landing in a bucket by heuristic — the first version of this
# analysis split by read idiom (`env_usize` vs `env::var`) and misfiled three
# knobs that are numeric but read as strings and handed to a C `#define`.
#
#   ladder   already resolved over the RFC-0049 ladder
#   sizing   a sizing knob with no rung: one is added when a descriptor
#            needs a non-default value (issue 2007)
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
    # --- derived: the message type should size these ---
    "NROS_SUBSCRIPTION_BUFFER_SIZE": (
        "derived",
        "the runtime-owned take buffer. phase-403 makes it per-type; it is in "
        "the ladder today as the fallback for a type with no bound",
    ),
    # phase-412 — a SIZING knob, not a derived one, and it is the peer graph
    # that sizes it: the cache holds a liveliness keyexpr for every entity of
    # every node on the domain, remote included. Not a property of this image,
    # so no inventory can answer it and an operator states it.
    # phase-412 — the CAPABILITY spelling of the knob below: an operator states
    # how many entities share the domain, and the bytes follow from the measured
    # ~141 per keyexpr. Same category — nothing derives it, because the peer
    # graph is not a property of this image.
    "NROS_GRAPH_MAX_ENTITIES": (
        "sizing",
        "domain entity budget; sizes ZPICO_GRAPH_CACHE_SIZE, which still "
        "outranks it when someone names the bytes directly",
    ),
    "NROS_ZENOH_GRAPH_CACHE_SIZE": (
        "sizing",
        "per-session graph cache bytes; sized by the PEER graph, so it is "
        "stated rather than derived",
    ),
    # phase-473 W1 -- a 0/1, not a size: whether the graph cache exists at
    # all. Its default is DERIVED from the image's links (off when every zenoh
    # link is serial or CAN), and a statement outranks that in either direction.
    "NROS_ZENOH_GRAPH_DISCOVERY": (
        "derived",
        "phase-473 W1 -- a 0/1, not a size: off when every compiled zenoh link "
        "is serial or CAN (graph_discovery_derived)",
    ),
    # phase-412 W3b — the arena's QoS-depth multiplier. Classed `sizing` until
    # phase-400's 2026-10-21 re-sort, on the reading that the image-wide
    # statement outranking the derivation is "what the ladder is for". It is
    # not: the ladder gives a PLATFORM default, and the depth an image's
    # subscriptions keep is a property of the image. The derived answer is
    # the sizing descriptor's max_subscription_depth (issue 1655); an image
    # that declares no contract states its one number on the RFC-0049 APP
    # rung, `[image.<id>.knobs.*]`. Same shape as NROS_ZENOH_MAX_QUERYABLES:
    # a checked statement over a derived default. Default 10 is ROS 2's own
    # `rmw_qos_profile_default` KEEP_LAST(10).
    "NROS_PUBSUB_QOS_DEPTH": (
        "derived",
        "the arena's per-subscription history depth; the sizing descriptor's "
        "max_subscription_depth (issue 1655), a statement outranks it",
    ),
    "ZPICO_SUBSCRIBER_BUFFER_SIZE": ("derived", "SMALL_PAYLOADS class (phase-403)"),
    "NROS_ZENOH_SUBSCRIBER_LARGE_SIZE": ("derived", "LARGE_PAYLOADS class (phase-403)"),
    "NROS_ZENOH_SUBSCRIBER_SIZE_THRESHOLD": ("derived", "SMALL_CLASS_CEILING (phase-403)"),
    "NROS_ZENOH_PUBLISHER_TX_BUFFER_SIZE": ("derived", "TX half of the same split"),
    # phase-455 W5 / issue 1341 — the transient-local retention pool. Both are
    # DERIVED and neither is a ladder candidate, for the reason
    # ZPICO_MAX_LARGE_SUBSCRIBERS is not one: the answer is a property of what
    # the IMAGE declares, not of the platform it runs on.
    # `nros-rmw-zenoh/build.rs` counts the sizing descriptor's transient-local
    # publisher rows (plus one per `action_server` row, for the `/status`
    # publisher the declaration structurally cannot mention) and refuses a
    # stated value below that count.
    "NROS_ZENOH_MAX_TL_PUBLISHERS": (
        "derived",
        "retention slots for TRANSIENT_LOCAL publishers; counted from the "
        "image's declared endpoints, phase-455 W5",
    ),
    "NROS_ZENOH_TL_RETAIN_DEPTH": (
        "derived",
        "samples each transient-local retention slot keeps; the deepest "
        "KEEP_LAST a transient-local publisher declares, issue 1709",
    ),
    "NROS_ZENOH_TL_RETAIN_BYTES": (
        "derived",
        "the retained sample's slot size — the publisher-side twin of "
        "ZPICO_SUBSCRIBER_BUFFER_SIZE, so it belongs to the same phase-403 "
        "per-type campaign and is a global only until that reaches it",
    ),
    # --- sizing: the backlog ---
    # issue 0968 — the UDP/TCP twin of the line above. Documented in the book and
    # in Kconfig with a 512 default, resolved by cmake under the UNPREFIXED name
    # that no build script reads, and hardcoded to 4096 on the UDP path anyway —
    # so every Zephyr XRCE image paid 2 x 4096 x 16 for its stream buffers.
    "NROS_XRCE_TRANSPORT_MTU": ("sizing", "transport MTU (UDP/TCP); numeric"),
    # phase-484 W8 -- the XRCE session pools, SEEN since this census reads
    # `xrce-config.txt`'s rows (READ_TABLES below): the build reads them by
    # variable, so no read call spelled them and they were never counted.
    "NROS_XRCE_MAX_SUBSCRIBERS": ("derived", "subscriber slot count; NROS_DERIVED_MAX_SUBSCRIBERS from the entity inventory (issue 1033)"),
    "NROS_XRCE_MAX_SERVICE_SERVERS": ("derived", "service-server slot count; NROS_DERIVED_MAX_QUERYABLES from the entity inventory (issue 1033)"),
    "NROS_XRCE_MAX_SERVICE_CLIENTS": ("derived", "service-client slot count; the sizing descriptor's declared clients plus three per action client (phase-400 W6)"),
    "NROS_XRCE_SUBSCRIBER_RING_DEPTH": ("sizing", "samples per subscriber slot; a burst policy, not derivable"),
    "NROS_XRCE_BUFFER_SIZE": ("sizing", "the default of the three per-family receive sizes"),
    "NROS_XRCE_SUBSCRIBER_BUFFER_SIZE": ("derived", "subscriber ring entry; the largest declared subscription wire bound (phase-403 W8)"),
    "NROS_XRCE_SERVICE_REQUEST_BUFFER_SIZE": ("derived", "application service-server request entry; the declared servers' largest request, beside the builtin split (issue 1808)"),
    "NROS_XRCE_SERVICE_REPLY_BUFFER_SIZE": ("derived", "service-client reply slot; the declared clients' largest `reply_bound_bytes` (issue 1808)"),
    "NROS_XRCE_BUILTIN_SERVICE_SERVERS": ("derived", "the parameter / lifecycle share of the service-server slots; the runtime's servers, beside the total (issue 1808)"),
    "NROS_XRCE_BUILTIN_SERVICE_REQUEST_BUFFER_SIZE": ("derived", "builtin-family request entry; XRCE_BUFFER_SIZE raised to the parameter family's priced request, so lifecycle's unbounded ChangeState keeps the floor (issue 1808)"),
    "NROS_ZENOH_MAX_LARGE_SUBSCRIBERS": ("derived", "pool cardinality; multiplies LARGE_PAYLOADS, phase-392"),
    "NROS_ZENOH_SERVICE_BUFFER_SIZE": ("derived", "SERVICE_BUFFERS is MAX_SESSIONS x MAX_QUERYABLES; phase-392"),
    # phase-461 W1 / issue 1352 -- the service inbox per FAMILY. The two slot
    # sizes are DERIVED: W3 prices each family's `_Request` types and the
    # derivable call replaces the plain one. The two depths are policy defaults
    # like ZPICO_SUBSCRIBER_RING_DEPTH -- a burst absorber, not a bound -- and
    # a `[knobs.zenoh.limits]` rung is their migration, so they are the backlog.
    "NROS_SERVICE_INBOX_BYTES": ("derived", "user-service inbox slot; the largest declared service _Request bound (phase-461 W3)"),
    "NROS_SERVICE_INBOX_DEPTH": ("sizing", "user-service inbox ring depth; a policy default, rung to come"),
    "NROS_ACTION_INBOX_BYTES": ("derived", "action inbox slot; the largest declared action request bound (phase-461 W3)"),
    "NROS_ACTION_INBOX_DEPTH": ("sizing", "action inbox ring depth; the twin of ZPICO_MAX_PENDING_REPLIES, rung to come"),
    # phase-461 W2/W2b -- the parameter and lifecycle families' inbox. ONE pair
    # of names for one geometry, read by two build scripts on purpose: W2's
    # `nros-node` sizes the caller-owned `PARAM_INBOX` from it, and W2b's zenoh
    # shim sizes `BUILTIN_INBOX` from it, and the two derive the same number
    # from the same declared parameter shape (the sizing descriptor's
    # `[params]` since issue 1649 retired the NROS_DECLARED_PARAM_SERVICE_SHAPE
    # carrier). The slot is
    # DERIVED -- `request_max()` over the contract's declared parameters, the
    # three REQUEST fields only -- and a stated size that is short fails the
    # build. The depth is a policy default like its two siblings above.
    "NROS_PARAM_SERVICE_INBOX_BYTES": ("derived", "parameter/lifecycle inbox slot; the largest request the declared param shape can produce (phase-461 W2/W2b)"),
    "NROS_PARAM_SERVICE_INBOX_DEPTH": ("sizing", "parameter/lifecycle inbox ring depth; 1, the sequential-client default"),
    # --- phase-392 W6: the executor backing static ---
    # NOT a ladder candidate, and the DECISION is the interesting part. A rung
    # gives a global a per-platform default, which is exactly wrong here: the
    # question this answers is "has the image's author lowered the allocator
    # arena by the same amount?", which is per-IMAGE, not per-platform (issue
    # 1145). It is also not `derived` — nothing is computing it, and claiming a
    # campaign owns it would overstate the backlog. It is the escape hatch on a
    # placement decision, so: infra.
    "NROS_EXECUTOR_BACKING_U64S": (
        "infra",
        "opt-out/size override for the `.bss` executor backing (phase-392 W6). "
        "Per-image, not per-platform: the RTOS half of the move is lowering "
        "that image's allocator arena, which only its author can measure "
        "(issue 1145)",
    ),
    "NROS_EXECUTOR_BACKING_SECTION": (
        "infra",
        "linker section name for the executor backing (phase-392 W6, amendment "
        "A / issue 0880). A placement, not a size",
    ),
    # --- infra: not knobs ---
    # The sixteen below are read through a CONSTANT (`const X: &str =
    # "NROS_..."`, then `env::var(X)`), which this census could not see until
    # the const-binding arm. None of them is a size.
    "NROS_CC_STRICT_DECLS": ("infra", "debug flag: disables the strict C declaration flags"),
    "NROS_CENSUS_OUT": ("infra", "path the entity census writes"),
    "NROS_CPP_BAKED_ROS_ARGS": (
        "infra",
        "an image's own command line from system.toml, baked into nros-cpp (issue 2073)",
    ),
    "NROS_ENTRY_DOMAIN_ID": (
        "infra",
        "the image's DECLARED domain id, carried as an `[env]` row; a value the "
        "image states, not a size with a rung",
    ),
    "NROS_ENTRY_LOCATOR": (
        "infra",
        "the image's DECLARED locator, carried as an `[env]` row; a selection, "
        "not a size",
    ),
    "NROS_LEAF_CENSUS_HOST": ("infra", "marks the entity census's host build of a leaf"),
    "NROS_PROFILE_INPUTS": ("infra", "build-profile input path"),
    "NROS_PROFILE_OUT": ("infra", "build-profile output path"),
    "NROS_SETUP_JOBS": ("infra", "`nros setup` concurrency"),
    "NROS_SETUP_SESSION": ("infra", "path of the `nros setup` session ledger"),
    "NROS_SESSION_PROPERTIES_RS": ("infra", "issue 2051: path of the session-property table a Zephyr Rust west entry bakes"),
    "NROS_SIZES_PROBE_BUILD": ("infra", "marks the sizes probe's own build"),
    "NROS_SIZING_DESCRIPTOR": ("infra", "the sizing descriptor's path; an input to derivation"),
    "NROS_SKIP_VERSION_CHECK": ("infra", "policy flag on the codegen ABI guard"),
    "NROS_TEST_1659_ROOT": ("infra", "test hook (issue 1659)"),
    "NROS_TEST_1659_WANT": ("infra", "test hook (issue 1659)"),
    "NROS_TEST_PROBE_ENV_DIR": ("infra", "test hook for the setup probe"),
    "ZPICO_MULTICAST_TRANSPORT": ("infra", "link toggle: compiles in multicast transport + scouting"),
    "NROS_ALLOW_UNRESOLVED_DEPS": ("infra", "policy flag"),
    # phase-422 W8 — its own flag, deliberately NOT reusing the one above.
    # A wrong-role dep RESOLVES; we decline it. Overloading the
    # "unresolved" hatch would make one variable mean two different
    # judgements, and a user silencing one would silence the other.
    "NROS_ALLOW_INFRA_DEPS": ("infra", "policy flag"),
    "NROS_BOOT_REPORT": ("infra", "diagnostic toggle; a bool, so it has no rung"),
    "NROS_VIOLATION_DRAIN_REPORT": (
        "infra",
        "phase-474 I1 drain-and-report toggle; a bool, so it has no rung",
    ),
    "NROS_MONITOR_ARM_ON_CALL": (
        "infra",
        "phase-474 I2 monitor-arming toggle; a bool, so it has no rung",
    ),
    "NROS_MONITOR_ARM_GRACE_MS": (
        "infra",
        "phase-474 I2 arming deadline; a time, not a size",
    ),
    "NROS_TRACE_TAKES": (
        "infra",
        "phase-474 I3 take-event toggle for the callback trace; a bool, so it has no rung",
    ),
    "NROS_TRACE_TIMER_EVERY": (
        "infra",
        "phase-474 I3 timer-tick sampling of the callback trace; a ratio, not a size",
    ),
    "NROS_EXECUTOR_MAX_VIOLATIONS": (
        "sizing",
        "phase-474 I1 violation ring depth per executor (carved from the "
        "backing) and the SWD record's slot count; not yet an ExecutorKnobs "
        "tenant",
    ),
    "NROS_EXECUTOR_EXTRA_SESSIONS": (
        "derived",
        "issue 1982 extra-session slots per executor (carved from the backing); "
        "the sizing descriptor's backend_count - 1, else one per node",
    ),
    "NROS_EXECUTOR_RUNTIME_REMAPS": (
        "sizing",
        "issue 1971 runtime remap-table slots per executor (carved from the "
        "backing; 16 with argv, 0 on bare metal); not yet an ExecutorKnobs "
        "tenant",
    ),
    "NROS_ARENA_EXHAUSTION_IS_FATAL": ("infra", "fault-hook toggle (issue 1036); a bool, so it has no rung"),
    "NROS_HEAP_EXHAUSTION_IS_FATAL": ("infra", "fault-hook toggle for the bare-metal heap (issue 1640); a bool, so it has no rung"),
    "NROS_BUILD_ROOT": ("infra", "path"),
    # phase-440 W7 — `orchestration::dispatch` SETS this on the installer it
    # execs to fetch a pinned toolchain (RFC-0095 D8 job 2). Infra, and the
    # decision the census forces is worth stating: it is not a size, not a
    # sizing knob, and NOT ladder material either — a rung gives a global a
    # per-platform default, and "which nano-ros version" is per-PROJECT
    # (`nros-toolchain.toml`) rather than per-platform. Its owner is
    # `scripts/install.sh`, which reads it; nothing here reads it back.
    "NROS_INSTALL_VERSION": (
        "infra",
        "the version handed to scripts/install.sh when the launcher fetches a "
        "pinned toolchain; the pin is per-project, so it has no rung",
    ),
    # Issue 1102 — regenerates the entry-codegen goldens. A test-only escape
    # hatch, not a build input: it configures nothing about an image, and the
    # goldens it rewrites are compared byte-for-byte on every other run.
    "NROS_UPDATE_GOLDEN": ("infra", "test-only golden regeneration"),
    "NROS_RECORD_TOY_PACK_GOLDEN": ("infra", "test-only: records the phase-494 toy entry pack's goldens, scoped apart from NROS_UPDATE_GOLDEN"),
    # issue 1512 — the first BUILD SCRIPT to read it. `nros_cli_bin()` in
    # `scripts/build/cargo.sh` has documented it as rung 1 of "which nros binary"
    # for as long as it has existed; what is new is a `build.rs` that needs the
    # CLI, because a cargo-rooted C image generates its own C message bindings
    # (`examples/mps2-an385-baremetal/c/talker`). The decision the census forces:
    # it is a PATH to a tool, sizes nothing, and has no rung — a rung gives a
    # global a per-platform default, and which binary to exec is per-CHECKOUT.
    "NROS_CLI": ("infra", "path to the nros binary; rung 1 of nros_cli_bin()"),
    # phase-443 W1 (RFC-0097 D5) — the SDK index is FETCHED and cached in the
    # store rather than read out of the release asset, so an index that moves 70
    # times per 60 days stops forcing a CLI release. Both are infrastructure:
    # one names WHERE to fetch (the sibling of scripts/install.sh's
    # NROS_INSTALL_URL) and one is a bool. Neither sizes anything, so neither
    # has a rung.
    "NROS_INDEX_URL": ("infra", "where the SDK index is fetched from; a mirror or a file:// copy"),
    "NROS_INDEX_REFRESH": ("infra", "re-fetch even when the cache is warm; a bool, so it has no rung"),
    "NROS_LINK_IP": ("infra", "link toggle"),
    # Issue 2096 — the default-off zenoh-pico links, implied by the CLI from an
    # image's CAN / ISO-TP endpoints; a bool, so it has no rung.
    "NROS_LINK_CAN": ("infra", "link toggle, implied from a can/ endpoint"),
    "NROS_LINK_ISOTP": ("infra", "link toggle, implied from an isotp/ endpoint"),
    "NROS_PLATFORMS_DIR": ("infra", "the ladder's own search path"),
    "NROS_PLATFORM_NAME": ("infra", "the ladder's own platform rung input"),
    "NROS_PX4_BRIDGE_GEN": ("infra", "codegen toggle"),
    "NROS_REPO_DIR": ("infra", "path"),
    "NROS_TRACE": ("infra", "debug flag"),
    "NROS_ZEPHYR_WORKSPACE": ("infra", "path"),
    "NROS_ZEPHYR_VERSION": ("infra", "the selected Zephyr line (3.7 / 4.4), which picks a workspace — not a size"),
    # --- ladder: the zenoh tx tenant (phase-282), read in the build HELPER ---
    # --- sizing: the zenoh-pico entity caps and buffers. The largest single
    # --- family left, and the one the phase doc calls "the per-entity caps".
    "NROS_ZENOH_MAX_PUBLISHERS": ("derived", "entity cap; phase-392 is deciding whether it is derived from the declaration"),
    "NROS_ZENOH_MAX_SUBSCRIBERS": ("derived", "entity cap; phase-392"),
    "NROS_ZENOH_MAX_QUERYABLES": ("derived", "already a CHECKED OVERRIDE over a derived default (phase-392 W5.f)"),
    "NROS_ZENOH_MAX_SESSIONS": ("derived", "phase-392 poses it explicitly: joins the model, or stays a knob and that phase says so"),
    "NROS_ZENOH_MAX_LIVELINESS": ("derived", "entity cap; phase-392"),
    "NROS_ZENOH_MAX_PENDING_GETS": ("derived", "entity cap; phase-392"),
    "NROS_ZENOH_READ_PRIORITY": ("sizing", "transport-band priority (issue 0623)"),
    "NROS_ZENOH_LEASE_PRIORITY": ("sizing", "transport-band priority (issue 0623)"),
    # --- infra ---
    "NROS_DECLARED_INFRA_QUERYABLES": ("infra", "a COUNT the resolver passes down, not a knob"),
    # phase-426 W3 — the same shape a third time: the ROS parameter services are
    # registered once PER NODE, so the model's node count is a term in the
    # queryable pool. A COUNT the resolver passes down, never something a user
    # tunes; `ZPICO_MAX_QUERYABLES` / `CONFIG_NROS_MAX_QUERYABLES` is the knob
    # and still outranks it (it is the checked OVERRIDE this bounds from below).
    "NROS_DECLARED_NODES": ("infra", "a COUNT the resolver passes down, not a knob"),
    # issue 1378 — the same shape a FOURTH time, into the same pool. A
    # TRANSIENT_LOCAL publisher declares a cache queryable, and an action server
    # has one for a `/status` topic nothing declares, so an action server costs
    # four slots and not three. A COUNT the declaration passes down — the
    # DESCRIPTOR answers this on the cargo-leaf road (phase-455 W5) and this
    # carries it on the roads that name none to cargo (issue 1407) — never something a
    # user tunes. `ZPICO_MAX_QUERYABLES` / `CONFIG_NROS_MAX_QUERYABLES` is the
    # knob and still outranks it, bounded from below by this term.
    # issue 1649 -- the retention slot, the payload trio and two parameter
    # counts (issues 1498 / 1122 / 1199 / phase-446) retired from the DECLARED
    # road onto the sizing descriptor, so they are no longer read here.
    # phase-496 W1 (issue 1471) -- the contract-monitor tables. DERIVED, and
    # deliberately not ladder candidates: the row count is a property of the
    # CONTRACT the image bakes, never of the board it runs on (phase-496's rule:
    # a board file states board facts, never a count the contract determines).
    "NROS_EXECUTOR_MAX_MONITORS": (
        "derived",
        "rate/latency monitor rows per executor; counted from the contract's "
        "`monitor_rows` by the entity inventory, phase-496 W1",
    ),
    "NROS_EXECUTOR_MAX_AGE_MONITORS": (
        "derived",
        "age monitor rows per executor; counted from the contract's `age_rows` "
        "by the entity inventory, phase-496 W1",
    ),
    # phase-446 W4 -- the parameter store, from the contract's `params:`. Same
    # category: what the image DECLARED, handed down as a default that every
    # `NROS_MAX_*` rung outranks. The NEEDS facts are not numbers at all: they
    # name the declared parameter whose type needs a capacity the board must
    # state, so the build script can refuse naming it.
    "NROS_DECLARED_MAX_STRING_VALUE_LEN": ("infra", "a SIZE the resolver passes down, not a knob"),
    "NROS_DECLARED_MAX_ARRAY_LEN": ("infra", "a SIZE the resolver passes down, not a knob"),
    "NROS_DECLARED_MAX_BYTE_ARRAY_LEN": ("infra", "a SIZE the resolver passes down, not a knob"),
    # Issue 1649 retired the three PARAM_NEEDS rows, the service shape and the
    # two service-inbox slot sizes the resolver used to pass down: the sizing
    # descriptor carries them now, so the resolver forwards none of them.
    # phase-460 W2 (issue 1421) -- the inventory's VERDICT on the declaration,
    # not a number: `refused` stops nros-params' build script on a road that
    # forwards the status by environment (the CMake road stops in cmake).
    "NROS_PARAM_DECLARATION_STATUS": ("infra", "a STATUS the inventory passes down, not a knob"),
    "NROS_PARAM_DECLARATION_REASON": ("infra", "the refusal's reason, quoted back, not a knob"),
    # phase-382 W3' -- two DECLARATIONS read as 0/1, not sizes: whether the
    # image builds a parameter store, so the executor carves it from its
    # backing (`nros_params::IMPLIED_STORE_SLOTS`). The Zephyr west road sets
    # both from the bringup (`[system] features`, a launch `<param>`); nothing
    # derives them from a type, and they size nothing on their own.
    "NROS_CAPABILITY_PARAM_SERVICES": ("infra", "a DECLARATION the build passes down, not a knob"),
    "NROS_PARAM_STORE": ("infra", "a DECLARATION the build passes down, not a knob"),
    "NROS_PICOLIBC_SYSROOT": ("infra", "path"),
    "NROS_RISCV64_PREFIX": ("infra", "toolchain prefix"),
    "NROS_SIZES_PROBE_TARGET_DIR": ("infra", "path"),
    "NROS_ZPICO_DEBUG": ("infra", "debug flag"),
    "ZPICO_NO_SMOLTCP": ("infra", "link toggle"),
    "ZPICO_PLATFORMS_TOML": ("infra", "the ladder's own platform file pointer"),
    # --- found once the scan followed the `build-helpers` feature ---
    "NROS_CYCLONEDDS_MAX_TYPES": (
        "derived",
        "already DERIVED from the SystemModel (msg=1/srv=2/action=8+3, next_pow2) "
        "and forwarded; the env is the override, not the source",
    ),
    # phase-454 W6.c / RFC-0100 D5 — the C++ `descriptors.cpp` table, derived
    # from the SAME count as the Rust registry above. Read here (rather than by
    # `option_env!`) because a C++ TU has no reach into cargo's `[env]`: the
    # build script turns it into a `-D`.
    "NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES": (
        "derived",
        "DERIVED from the SystemModel by phase-454 W6.c, from the same distinct-type "
        "count as NROS_CYCLONEDDS_MAX_TYPES; the env is the override, not the source",
    ),
    # The descriptor builder's three stack arrays. DERIVED from codegen's own
    # per-type schema walk into `[types]` of the sizing descriptor, and forwarded
    # to the C++ half by the same build script for the same reason.
    "NROS_CYCLONEDDS_MAX_FIELDS": (
        "derived",
        "DERIVED by phase-454 W6.c from codegen's schema walk (RFC-0100 [types]); "
        "forwarded to the C++ TUs, which cannot read cargo's [env] themselves",
    ),
    "NROS_CYCLONEDDS_MAX_KINDS": (
        "derived",
        "DERIVED by phase-454 W6.c from codegen's schema walk (RFC-0100 [types]); "
        "forwarded to the C++ TUs, which cannot read cargo's [env] themselves",
    ),
    "NROS_CYCLONEDDS_MAX_NESTED_DEPTH": (
        "derived",
        "DERIVED by phase-454 W6.c from codegen's schema walk (RFC-0100 [types]); "
        "forwarded to the C++ TUs, which cannot read cargo's [env] themselves",
    ),
    # RFC-0100 D11 — the board's `[knobs.memory] heap_bytes`, carried to
    # the one backend that allocates everything. A FACT the ladder resolves
    # against, not a knob it resolves: nobody tunes it here, the board states it.
    "NROS_CYCLONEDDS_HEAP_BUDGET_BYTES": (
        "infra",
        "the board's declared heap, carried so CycloneDDS can assert it at boot "
        "(RFC-0100 D11); stated by the board rung, never set here",
    ),
    "NROS_ENTRY_SPIN_MS": ("sizing", "entry spin period; a duration, same ladder shape"),
    # --- infra: pointers, flags and orchestration inputs ---
    "NROS_BOARD_TOML": ("infra", "the ladder's own board rung pointer"),
    # phase-497 W2 (RFC-0103 D10f) -- an image's resolved network, the text the
    # CLI writes and riscv64's build script bakes into its C NROS_APP_CONFIG.
    # Not a knob: an address the image states in system.toml, never a size.
    "NROS_IMAGE_NET": ("infra", "the image's resolved network (RFC-0103 D10f), not a knob"),
    # issue 1143 -- names WHICH `[[board]]` entry of a multi-entry file the
    # rung comes from (`nros-board-nuttx` declares two that differ in ISA).
    # A selector, never a size.
    "NROS_BOARD": ("infra", "which board entry the rung reads, beside NROS_BOARD_TOML"),
    # Surfaced when the reader matcher stopped enumerating helper names
    # (READ_CALLEES). These are read through `req`/`list`/`env_get`/`flag`,
    # which the old fixed list did not know, so the census had never counted
    # them and the W6 backlog was measured 29 names short.
    #
    # The board descriptor's facts: identity and paths the CLI hands the build,
    # which are what the ladder RESOLVES AGAINST rather than knobs it resolves.
    # phase-403 step 3 -- the entity inventory's per-kind counts. INFRA, not
    # sizing: nobody tunes these, they are what the image DECLARED, and the
    # arena derivation sums bytes over them. Same category as a board
    # descriptor fact -- something the ladder resolves AGAINST rather than a
    # knob it resolves.
    "NROS_ENTITY_COUNT_SUBSCRIPTION": ("infra", "declared entity count"),
    "NROS_ENTITY_COUNT_TIMER": ("infra", "declared entity count"),
    "NROS_ENTITY_COUNT_SERVICE_SERVER": ("infra", "declared entity count"),
    "NROS_ENTITY_COUNT_ACTION_CLIENT": ("infra", "declared entity count"),
    "NROS_ENTITY_COUNT_ACTION_SERVER": ("infra", "declared entity count"),
    "NROS_ENTITY_COUNT_SERVICE_CLIENT": ("infra", "declared entity count"),
    "NROS_ENTITY_COUNT_GUARD_CONDITION": ("infra", "declared entity count"),
    # issue 1485 -- the application's share of the declared queryable table,
    # which the zenoh shim subtracts to size the builtin (parameter/lifecycle)
    # inbox partition on the Zephyr resolver road.
    "NROS_ENTITY_APP_QUERYABLES": ("infra", "declared entity count"),
    # issue 1227 -- the DECLARED QoS depths, and the count that says whether the
    # subscription half of that table is complete. Infra for the same reason the
    # counts above are: they carry what the image already declared into the
    # derivation, rather than offering a number for anyone to set.
    "NROS_ENTITY_DECLARED_DEPTHS": ("infra", "declared QoS depth table"),
    "NROS_ENTITY_UNDECLARED_DEPTH_COUNT_SUBSCRIPTION": (
        "infra",
        "subscriptions that declared no depth",
    ),
    # The receive payload class. DERIVED -- it is the largest bound among the
    # types this image subscribes to, which phase-403 W8 computes exactly. Read
    # here so the arena stops billing a subscription at the CLOSURE buffer,
    # which is also DEFAULT_TX_BUF and therefore larger.
    "NROS_SUBSCRIBER_BUFFER_SIZE": ("derived", "receive payload class"),
    # issue 1255 -- the per-type table the class above is the MAXIMUM over. It
    # is infra for the same reason the depth table is: it carries a fact the
    # message-bound inventory already derived into the arena derivation, and
    # holds no number for anyone to state.
    "NROS_SUBSCRIBED_TYPE_BOUNDS": ("infra", "per-type receive bound table"),
    "NROS_BOARD_ZEPHYR_ID": ("infra", "board descriptor fact"),
    "NROS_BOARD_TOOLCHAIN": ("infra", "board descriptor fact"),
    "NROS_BOARD_RUNNER": ("infra", "board descriptor fact"),
    "NROS_BOARD_RUST_TARGETS": ("infra", "board descriptor fact"),
    "NROS_BOARD_GATED_PKGS": ("infra", "board descriptor fact"),
    "NROS_BOARD_PRJ_CONF": ("infra", "board descriptor path"),
    "NROS_BOARD_BOARD_CONF": ("infra", "board descriptor path"),
    "NROS_BOARD_BOARD_OVERLAY": ("infra", "board descriptor path"),
    "NROS_BOARD_DEFAULT_RMW": ("infra", "board descriptor default, not a size"),
    "NROS_BOARD_DEFAULT_TRANSPORT": ("infra", "board descriptor default, not a size"),
    # Source and include directories.
    "NROS_C_INCLUDE": ("infra", "include dir"),
    "NROS_CPP_INCLUDE": ("infra", "include dir"),
    "NROS_PLATFORM_CFFI_INCLUDE": ("infra", "include dir"),
    "NROS_PLATFORM_POSIX_SRC": ("infra", "platform source dir"),
    "NROS_PLATFORM_FREERTOS_SRC": ("infra", "platform source dir"),
    "NROS_PLATFORM_THREADX_SRC": ("infra", "platform source dir"),
    "NROS_LAN9118_LWIP_DIR": ("infra", "driver source dir"),
    "NROS_VIRTIO_NET_NETX_DIR": ("infra", "driver source dir"),
    # Sizes, and none of them owned by another campaign: the only live doc
    # hits are usage records (phase-358 records `=8` as what recovered #271's
    # overflow; the dependency-weight audit names the RMW cap as a knob cmake
    # never resolves). Both are arguments FOR a board rung, the same shape the
    # ASI consumer's `NROS_MAX_PARAMETERS=256` had.
    # A RUNTIME trace toggle, not a build-time size — presence-tested with
    # `var_os` in `cffi/src/lib.rs`, never parsed. It became visible to this
    # census only when `nros-rmw-cffi` gained a `build-helpers` dependency and
    # its sources entered the scan; it configures nothing about the image.
    "NROS_RMW_TRACE_OPEN": ("infra", "runtime trace toggle, not a size"),
    "NROS_RMW_SUBSCRIBER_SLOTS": ("derived", "phase-412 W1 — COUNT_SUBSCRIPTION"),
    "NROS_RMW_LOCAL_QUERYABLE": (
        "derived",
        "issue 1549 — a 0/1, not a size: on when the entity inventory holds a "
        "service client AND a service server; nros-node's build.rs reads it "
        "only for the boot record"),
    "NROS_EXTRA_BOARD_PATH": ("infra", "extra board search roots"),
    "NROS_HOME": ("infra", "path"),
    "NROS_MODEL_DIR": ("infra", "path"),
    "NROS_WORKSPACE": ("infra", "path"),
    "NROS_WORKSPACE_ROOT": ("infra", "path"),
    "NROS_LAUNCH_RESOLVE": ("infra", "resolver binary path"),
    "NROS_METADATA_PROBE_CACHE": ("infra", "cache path"),
    "NROS_RMW": ("infra", "the selected backend, not a size"),
    "NROS_OFFLINE": ("infra", "policy flag"),
    "NROS_NO_AUTO_SETUP": ("infra", "policy flag"),
    "NROS_SKIP_STALE_CHECK": ("infra", "policy flag"),
    "NROS_SUPPRESS_DEPRECATION": ("infra", "policy flag"),
    "NROS_DEBUG_BRINGUP_RESOLVER": ("infra", "debug flag"),
    "NROS_TRACE_ABI_GUARD": ("infra", "debug flag"),
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


# issue 0883's class, in a gate's own table — `KNOB_CLASS` was a per-PR conflict
# site. Three of eleven DIRTY pull requests on 2026-09-04 conflicted here and
# nowhere else, because it is one sorted dict that every knob change edits, and
# a phase-400 W6 migration flips entries from `sizing` to `ladder` by hand.
#
# A migration now edits ZERO lines of it: whether the ladder maps a knob is READ
# from the ladder's own source, which is the thing being changed anyway.
#
# Deliberately NOT the whole classification. "the ladder maps this env name" and
# "this knob is migrated" are different facts, and conflating them would hide
# remaining work: `NROS_SUBSCRIPTION_BUFFER_SIZE` is mapped and NOT migrated,
# because a build script still reads its env directly, and deriving its class
# from the mapping alone would silently move it out of the "still to migrate"
# count. (`NROS_SERVICE_TIMEOUT_MS` was the other case until issue 2007 gave
# its two readers one resolver.) Those cases are named in `LADDER_MAPPED_NOT_MIGRATED`, which is short,
# shrinks as they land, and is the only hand-kept part left.
LADDER_MAPPED_NOT_MIGRATED = {
    # Mapped, but `nros-node`'s build script still reads the env directly.
    "NROS_SUBSCRIPTION_BUFFER_SIZE",
}


def _registry_rows():
    try:
        import tomllib
    except ModuleNotFoundError:  # Python < 3.11 — the repo's interpreter is 3.10
        import tomli as tomllib
    path = os.path.join(ROOT, "packages/tooling/nros-zephyr-build/knobs.toml")
    if not os.path.exists(path):
        return []
    with open(path, "rb") as fh:
        return tomllib.load(fh).get("knob", [])


def registry_spellings(ladder_only=False):
    """Every env spelling the knob registry holds, canonical and legacy.

    `ladder_only`: just the rows with a `toml` key — the ones the platform/board
    ladder resolves. Since phase-484 W5a slice 2 the ladder's `*_env_key`
    functions ASK the registry for those spellings, so `platform_config.rs`
    no longer spells them and its match arms cannot be the evidence.
    """
    out = set()
    for r in _registry_rows():
        if ladder_only and not r.get("toml"):
            continue
        out.add("NROS_" + r["id"])
        out.update(r.get("legacy", []))
    return out


def ladder_env_keys():
    """Env names the ladder READS: every spelling of a registry row with a
    `toml` key (the `*_env_key` functions derive from those), plus what
    `platform_config.rs` still spells itself.

    Three idioms in that file: a match arm mapping a struct field to
    its env name, a direct `env("...")`, and `flag("...", default)` for a bool.
    The `#[cfg(test)]` module is cut first — its fixtures name knobs that the
    production reader does not, and counting those would classify a knob as
    laddered because a UNIT TEST mentioned it.
    """
    src = read(os.path.join(ROOT, "packages/tooling/nros-platform-config/src/platform_config.rs"))
    src = per_item.rust_cfg_test_blank(src)  # phase-472 W6: each item, not a cut
    src = re.sub(r"(?m)^\s*///.*$", "", src)
    out = set()
    for pat in (
        r'=>\s*"((?:NROS|ZPICO)_[A-Z0-9_]+)"',
        r'\benv\(\s*"((?:NROS|ZPICO)_[A-Z0-9_]+)"',
        r'\bflag\(\s*"((?:NROS|ZPICO)_[A-Z0-9_]+)"',
    ):
        out |= set(re.findall(pat, src))
    return out | registry_spellings(ladder_only=True)


def ladder_knobs():
    """Fields on the typed `[knobs.*]` structs — the migrated knobs."""
    # phase-400 W6 — the ladder reader MOVED to a leaf crate so a driver build
    # script can read it without the `nros-board-common -> nros-platform ->
    # ... -> driver` cycle. `nros-board-common` re-exports it, so consumers are
    # unchanged; this path is not, and a stale one here reports "the ladder
    # holds 0 knobs" rather than "I cannot find the file".
    src = read(os.path.join(ROOT, "packages/tooling/nros-platform-config/src/platform_config.rs"))
    total = {}
    for struct in (
        "TxKnobs",
        "ExecutorKnobs",
        "MemoryKnobs",
        "ParamKnobs",
        "RmwKnobs",
        "NetKnobs",
        "RuntimeKnobs",
        "LogKnobs",
        "WireKnobs",
        "XrceKnobs",
        "ZenohLimitKnobs",
    ):
        fields = _struct_fields(src, struct)
        if fields:
            total[struct] = fields
    return total


# Issue 2013 — the row prefix `nros config explain` prints for each ladder
# struct. A struct missing here, or a field missing from `explain`'s defaults,
# is a knob the ladder resolves and the provenance report never shows. RFC-0086
# D6 makes that report the acceptance test, so the census refuses it.
EXPLAIN_PREFIX = {
    "TxKnobs": "{rmw}.tx.",
    "ExecutorKnobs": "executor.",
    "MemoryKnobs": "memory.",
    "ParamKnobs": "params.",
    "RmwKnobs": "rmw.",
    "NetKnobs": "net.",
    "RuntimeKnobs": "runtime.",
    "LogKnobs": "log.",
    "WireKnobs": "zenoh.wire.",
    "XrceKnobs": "xrce.",
    "ZenohLimitKnobs": "zenoh.limits.",
}

EXPLAIN_SRC = "packages/cli/nros-cli-core/src/cmd/config.rs"


def explain_gaps(ladder, src):
    """Ladder knobs `explain` cannot print, as `Struct.field` strings.

    A struct is printed when its prefix appears in a `format!` row; a field is
    printed when its name is a string literal in the `explain` body, which is
    where every tenant's defaults table lives.
    """
    start = src.find("fn explain(")
    end = src.find("\nfn ", start + 1)
    body = src[start:end] if start >= 0 else ""
    literals = set(re.findall(r'"([a-z0-9_]+)"', body))
    gaps = []
    for struct, fields in sorted(ladder.items()):
        prefix = EXPLAIN_PREFIX.get(struct)
        if prefix is None or f'format!("{prefix}{{' not in body:
            gaps.append(f"{struct} (no `explain` row)")
            continue
        gaps.extend(f"{struct}.{f}" for f in fields if f not in literals)
    return gaps


def kconfig_symbols():
    syms = set()
    for p in tracked("*Kconfig", "*Kconfig.*", "*/Kconfig"):
        for m in re.finditer(r"(?m)^\s*(?:menu)?config\s+(NROS_[A-Z0-9_]+)", read(p)):
            syms.add(m.group(1))
    return syms


# How a build script READS a knob, and how it does everything else with one.
#
# The census used to carry only the first list, and matching nothing else meant
# a build script that introduced a helper of its own silently dropped its knobs
# from the count: `nros-params/build.rs` wrapped its rungs in a local `knob()`
# and five names vanished while `--check` still passed, because the gate only
# fails on a name it SEES and cannot classify.
#
# Matching ANY call instead is the opposite error -- `.define("ZPICO_X", ..)`
# emits a C macro and `.with_env("NROS_X", ..)` sets a CHILD's environment;
# counting those as readers added 67 phantom names. So both lists are explicit
# and a callee in neither is a FAILURE: the same "a new knob forces a decision"
# rule this file already applies to knobs, applied to the idioms that read them.
READ_CALLEES = {
    # Issue 2096. `nros-board-common::policy::knob_on`: is a default-off
    # zenoh-pico link's build knob (`NROS_LINK_CAN` / `NROS_LINK_ISOTP`) on.
    "knob_on",
    # phase-403 step 3. Reads the environment like `env_usize`, but returns
    # Option: for an entity COUNT, absence and zero are different answers and
    # a default would erase the difference.
    "env_opt_usize",
    # phase-400 W6. `env_usize` with a ladder rung between the env and the
    # builtin.
    "env_usize_rung",
    # phase-448 W5. `env_opt_usize` with the board/platform rungs spliced in
    # below Kconfig. Option rather than a default, for
    # `NROS_EXECUTOR_BACKING_U64S`: its "default" is a const only the TARGET
    # compiler can evaluate, so absence has to stay absence — a sentinel would
    # then have to be told apart from a stated `0`, which is that knob's
    # documented opt-out.
    "env_opt_usize_laddered",
    # issue 1199's DECLARED-road readers (`env_usize_declared`, and issue
    # 1595's `env_usize_declared_or`) were deleted with the take-buffer carrier
    # they last served (phase-486 W9).
    # issue 1649 -- the ladder with the descriptor's fact where a retired
    # DECLARED carrier used to sit.
    "env_usize_described",
    "declared_usize",
    "declared_floored",
    # issue 1227 -- the declared QoS depth TABLE is a string, not a usize, and
    # absence is not the empty table: an image that declares nothing must keep
    # the worst case rather than sum zero endpoints.
    "env_opt_string",
    # phase-446 W4 -- nros-params' readers. `capacity` resolves a per-slot
    # capacity over the stated rungs; `declared` and `needs` read what the
    # contract declared, as a number and as the parameter that needs one.
    "capacity", "declared", "needs",
    # phase-446 F3 -- nros-node's reader for the same DECLARED road: the
    # parameter services' shape, a string the entity inventory writes.
    "declared_fact",
    # phase-484 W3d -- `nros_build_paths::env_path` reads a PATH-valued
    # variable (re-rooted, issue 1280); the ladder's project rung reads
    # `NROS_WORKSPACE_ROOT` through it.
    "env_path",
    "env", "env_get", "env_bool", "env_usize", "env_usize_min",
    "env_usize_compat", "env_or_repo_path", "env_path_or", "flag", "knob",
    "knob_usize", "knob_bool", "req", "list", "var", "var_os",
    # phase-484 W5a -- rung 1 through an injected accessor (the ladder's
    # `knob_env`, and the registry tests that drive it).
    "read_spelled",
    # The const-binding arm -- `nros-rmw-xrce-cffi`'s rungs 1-3 for one
    # manifest row (`Self::stated(env_name, ..)`), resolved through
    # `nros_zephyr_build::knob`.
    "stated",
}

# Not reads. Named rather than ignored by default, so the failure below stays
# meaningful.
NON_READ_CALLEES = {
    # phase-468 W4 -- maps an env NAME to its Kconfig symbol. It touches no
    # environment; `knob()` (already a read above) is what resolves through it.
    "kconfig_key_for",
    # phase-484 W5a -- `cargo:rerun-if-env-changed` for every registry spelling
    # of the name. A watch, not a read; `knob_env` beside it is the read.
    "watch_knob_env",
    # phase-484 W8 -- the refusal sentence for a retired spelling. It takes a
    # NAME and reads no environment; `knob()` / `read_spelled` are the reads.
    "retired_spelling_refusal",
    # phase-484 W8 -- the registry ROW a name is (`knob_row`), and which knob an
    # `[image.<id>] env` row's key names (`registry_knob_of_env_row`). Both are
    # table lookups on a NAME and read no environment.
    "knob_row", "registry_knob_of_env_row",
    "define",          # emits a C preprocessor macro
    "set_var", "remove_var", "with_env",  # WRITES an environment
    "push", "insert",  # builds a list / a fact map
    # issue 1808 -- drops a DERIVED value from the fact map (`XrceDemand`'s
    # `keep_stated_server_total`). It writes the map, never the environment.
    "remove",
    # phase-454 W6.c. `WrittenDescriptor::cyclonedds_env`'s local closure: it
    # inserts a STATED fact into the `[env]` table the image's cargo config
    # gets, so the name is written for a consumer to read, never read here.
    "put",
    "get", "contains", "contains_key", "starts_with",  # inspects one
    # phase-443 W5. `Some("NROS_X")` CONSTRUCTS an Option holding the name --
    # in practice an `assert_eq!(probe.env.as_deref(), Some("NROS_X"))` checking
    # that an index entry declares the right override variable. It cannot be a
    # read: the environment is never consulted by wrapping a string literal.
    "Some",
    # issue 1549. `nros-node/build.rs`'s `knob_source_code(knob)` reads
    # `NROS_KNOB_SOURCE_<knob>` -- the RUNG the cmake resolver recorded for a
    # knob, never the knob's value. The value's one read is the
    # `env_opt_string(knob)` beside it, which this census already counts.
    "knob_source_code",
    # The const-binding arm. `XrceDemand::for_knob(env)` looks a name up in
    # the map of values the image's DECLARATIONS imply; it reads no
    # environment (`stated` beside it is the read). `Command::env_remove`
    # clears a CHILD's variable.
    "for_knob", "env_remove",
}

_CALL_RE = r'([A-Za-z_][A-Za-z0-9_:]*)\s*\(\s*&?"({prefix}_[A-Z0-9_]+)"'

# A name read THROUGH A CONSTANT. `const DISABLE_ENV: &str = "NROS_X";` and
# then `env::var(DISABLE_ENV)` is a read the literal matcher above cannot see,
# because no call spells the name: ~20 names were read that way and absent from
# this census, the gap phase-484 W8 found for `xrce-config.txt` one idiom over.
# The binding and the call can sit in different files (a helper crate exports
# the constant), so the table is built over every build source; an identifier
# bound to two different names is resolved only in the file that binds it.
_CONST_RE = (
    r'\b(?:pub(?:\([a-z]+\))?\s+)?(?:const|static)\s+([A-Z][A-Z0-9_]*)\s*:'
    r'\s*&(?:\'static\s+)?str\s*=\s*"({prefix}_[A-Z0-9_]+)"\s*;'
)
_CONST_CALL_RE = (
    r'([A-Za-z_][A-Za-z0-9_:]*)\s*\(\s*&?(?:[A-Za-z_][A-Za-z0-9_]*::)*'
    r'([A-Z][A-Z0-9_]*)(?=\s*[,)])'
)


def _strip_comments(txt):
    return re.sub(r"(?m)^\s*(///|//!|//).*$", "", txt)


def _const_bindings(txt, prefix):
    """{IDENT: name} for every `const|static IDENT: &str = "<PREFIX>_*";`."""
    if f'"{prefix}_' not in txt:  # the pattern needs that literal (phase-489 W5)
        return {}
    return {
        m.group(1): m.group(2)
        for m in re.finditer(_CONST_RE.format(prefix=prefix), _strip_comments(txt))
    }


def shared_const_bindings(sources, prefix):
    """The cross-file binding table: identifiers bound to exactly ONE name."""
    seen = {}
    for path in sources:
        for ident, name in _const_bindings(read(path), prefix).items():
            seen.setdefault(ident, set()).add(name)
    return {ident: next(iter(n)) for ident, n in seen.items() if len(n) == 1}


def _calls_in(txt, prefix, shared=None):
    """(callee, name) for every call taking a `<PREFIX>_*` name first --
    spelled as a literal, or through a constant bound to one.

    Comments are stripped first: a knob named in a comment is documentation,
    not a reader, and counting it inflates the number this gate exists to
    track. Macros are excluded by the absence of `!` from the callee pattern,
    which keeps `panic!("NROS_X unset")` and friends out.
    """
    # Prefilter (phase-489 W5): both literal matchers need `"<PREFIX>_` in the
    # text, and the constant-call matcher needs one of the bound identifiers.
    # Nearly every file names neither, and these regexes are the gate's cost
    # (the scan was ~7 CPU-s). Same output: a file that fails the prefilter
    # could not have matched.
    shared = shared or {}
    has_literal = f'"{prefix}_' in txt
    if not has_literal and not any(k in txt for k in shared):
        return []
    stripped = _strip_comments(txt)
    out = []
    if has_literal:
        out = [
            (m.group(1).rsplit("::", 1)[-1], m.group(2))
            for m in re.finditer(_CALL_RE.format(prefix=prefix), stripped)
        ]
    bindings = dict(shared)
    if has_literal:
        bindings.update(_const_bindings(txt, prefix))
    if bindings:
        out += [
            (m.group(1).rsplit("::", 1)[-1], bindings[m.group(2)])
            for m in re.finditer(_CONST_CALL_RE, stripped)
            if m.group(2) in bindings
        ]
    return out


def _env_names_in(txt, prefix, shared=None):
    """`<PREFIX>_*` names READ from the environment in one source string.

    Factored out so the selftest drives it on synthetic input.
    """
    return {n for callee, n in _calls_in(txt, prefix, shared) if callee in READ_CALLEES}


def unknown_idioms(sources, prefixes):
    """Callees that neither read nor demonstrably-don't. See READ_CALLEES."""
    out = {}
    shared = {prefix: shared_const_bindings(sources, prefix) for prefix in prefixes}
    for path in sources:
        txt = read(path)
        for prefix in prefixes:
            for callee, name in _calls_in(txt, prefix, shared[prefix]):
                if callee not in READ_CALLEES and callee not in NON_READ_CALLEES:
                    out.setdefault(callee, set()).add(name)
    return out


def build_env_sources():
    """Files that run at BUILD time and may read a knob.

    Not just `build.rs`: a build script that grew past a few lines moved its
    body into a helper crate, and the knobs moved with it. 21 `ZPICO_*` names
    live in `nros-zpico-build/src/`, and a census that scanned only `build.rs`
    reported six — so both this file's first number and the phase doc's
    original table undercounted the zenoh surface by a factor of four.

    Two conventions find them. The crate NAME (`*-build`, and everything under
    `packages/tooling/`), and — because a crate can be a build helper without
    saying so in its name — a declared `build-helpers` FEATURE. The second was
    added after a refactor moved the env-pointer reads out of a `build.rs` and
    into `nros-board-common`, whose name says "board" and whose
    `build-helpers` feature says what it actually is: the census lost sight of
    a knob because the code moved, which is the failure mode a name-based scan
    always has.
    """
    out = list(tracked("*build.rs"))
    out += [p for p in tracked("packages/*/*-build/src/*.rs", "packages/*/*/*-build/src/*.rs")]
    out += list(tracked("packages/tooling/*/src/*.rs"))
    for manifest in tracked("packages/*/*/Cargo.toml", "packages/*/Cargo.toml"):
        # A crate that DECLARES the feature, not one that merely enables it on a
        # dependency (`nros-tests` turns on `nros-board-common/build-helpers`
        # to render a QEMU `-nic`, phase-497 W3, and is no build script).
        try:
            import tomllib
        except ModuleNotFoundError:  # Python < 3.11 — the repo's interpreter is 3.10
            import tomli as tomllib
        try:
            features = tomllib.loads(read(manifest)).get("features", {})
        except tomllib.TOMLDecodeError:
            features = {}
        if "build-helpers" not in features:
            continue
        crate = os.path.dirname(manifest)
        rel = os.path.relpath(crate, ROOT)
        out += [str(q) for q in tracked(f"{rel}/src/*.rs")]
    return sorted(set(out))


# READ TABLES — files whose ROWS are read sites (phase-484 W8).
#
# `nros-rmw-xrce-cffi/build.rs` reads every XRCE knob by VARIABLE: it walks the
# `knob` and `define` rows of `xrce-config.txt` and resolves each row's `<env>`
# column, so no read call in any build source spells the name. The call matcher
# above could therefore never see them, and nine XRCE knobs were read by a
# build and absent from this census — which is how they stayed out of the knob
# registry while every gate here passed. `check-knob-ends` had already learnt
# the same lesson (`xrce_manifest_readers`); this is that arm, for this census.
#
# The grammar is not re-implemented: `check-xrce-config-manifest.py` owns the
# parser, and a second copy is how two readers come to disagree about a record
# the gate accepts.
XRCE_CONFIG_MANIFEST = "packages/rmw/xrce/xrce-config.txt"


def _xrce_manifest_module():
    mod = _xrce_manifest_module.__dict__.get("_mod")
    if mod is None:
        import importlib.util
        spec = importlib.util.spec_from_file_location(
            "_census_xrce_manifest",
            os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                         "check-xrce-config-manifest.py"),
        )
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        _xrce_manifest_module._mod = mod
    return mod


def xrce_manifest_env(text):
    """The env names `xrce-config.txt`'s `knob` and `define` rows bind."""
    _values, knobs, _flags, defines = _xrce_manifest_module().parse_manifest(
        text, XRCE_CONFIG_MANIFEST
    )
    return {env for _t, _token, env, _d, _m in knobs} | {env for _m, env, _min in defines}


# path -> the function that names what its rows read. A new table that a build
# script walks by variable belongs here, or its knobs are invisible again.
READ_TABLES = {XRCE_CONFIG_MANIFEST: xrce_manifest_env}


def read_table_env(prefix):
    """`<PREFIX>_*` names a READ TABLE binds. A table that is missing is an
    error, not an empty set: a moved file must not read as "reads nothing"."""
    names = set()
    for rel, names_of in READ_TABLES.items():
        path = os.path.join(ROOT, rel)
        if not os.path.exists(path):
            raise SystemExit(f"config-knob-census: read table {rel} is missing; "
                             "update READ_TABLES")
        names |= {n for n in names_of(read(path)) if n.startswith(prefix + "_")}
    return names


def build_script_env(prefix):
    """`<PREFIX>_*` names a build-time source reads from the environment —
    through a read call, or through a READ TABLE's rows."""
    names = set()
    sources = build_env_sources()
    shared = shared_const_bindings(sources, prefix)
    for p in sources:
        names |= _env_names_in(read(p), prefix, shared)
    return names | read_table_env(prefix)


def unregistered_knobs(by_class, spelled):
    """Names classified as a KNOB that are no spelling in the knob registry."""
    return sorted(
        n for cls in ("ladder", "sizing", "derived") for n in by_class.get(cls, [])
        if n not in spelled
    )


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
        'nros_zephyr_build::knob("NROS_LADDERED").resolve(4)',
    ):
        assert _env_names_in(src, "NROS"), f"selftest: missed a read in {src!r}"
    # ...and must not count a name it merely MENTIONS, or another prefix.
    for src in (
        '// NROS_EXECUTOR_MAX_CBS was read here once',
        'println!("set NROS_THING");',
        'let n = env_usize("ZPICO_TX_BATCH", 0);',
        # ...including the shapes that TOUCH a knob without reading one.
        'cfg.define("NROS_THING", "1");',
        'cmd.with_env("NROS_THING", "");',
        'flags.push("NROS_THING");',
    ):
        assert not _env_names_in(src, "NROS"), f"selftest: false positive on {src!r}"

    # The const-binding arm: a name read through a CONSTANT is a read, whether
    # the constant is bound in the same file or exported by a helper crate.
    src = 'pub const DISABLE_ENV: &str = "NROS_VIA_CONST";\nlet v = std::env::var(DISABLE_ENV);'
    assert _env_names_in(src, "NROS") == {"NROS_VIA_CONST"}, "selftest: missed a const-bound read"
    src = 'let v = env::var(&helper::OTHER_ENV).ok();'
    assert _env_names_in(src, "NROS", {"OTHER_ENV": "NROS_ELSEWHERE"}) == {"NROS_ELSEWHERE"}, (
        "selftest: missed a read through a constant bound in another file"
    )
    # ...but binding a constant reads nothing, a comment binds nothing, and a
    # constant handed to a non-read (`set_var`) is not a read.
    for src in (
        'pub const DISABLE_ENV: &str = "NROS_VIA_CONST";',
        '// const X: &str = "NROS_VIA_CONST";\nstd::env::var(X);',
        'const X: &str = "NROS_VIA_CONST";\nstd::env::set_var(X, "1");',
    ):
        assert not _env_names_in(src, "NROS"), f"selftest: false positive on {src!r}"
    # An identifier bound to two names across files is not resolved globally.
    import tempfile as _tf
    with _tf.TemporaryDirectory() as d:
        a, b = os.path.join(d, "a.rs"), os.path.join(d, "b.rs")
        with open(a, "w") as fh:
            fh.write('const ENV: &str = "NROS_A";')
        with open(b, "w") as fh:
            fh.write('const ENV: &str = "NROS_B";\nconst ONLY: &str = "NROS_C";')
        assert shared_const_bindings([a, b], "NROS") == {"ONLY": "NROS_C"}, (
            "selftest: an ambiguous constant was resolved across files"
        )

    # An idiom in NEITHER list must be reported rather than silently dropped:
    # that silent drop is exactly what hid five knobs behind a local `knob()`.
    assert _calls_in('let n = brand_new_helper("NROS_THING", 4);', "NROS") == [
        ("brand_new_helper", "NROS_THING")
    ], "selftest: the call matcher missed an unknown idiom"
    assert not _env_names_in('let n = brand_new_helper("NROS_THING", 4);', "NROS"), (
        "selftest: an unknown idiom was counted as a read"
    )

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

    # The ladder-mapping reader: three idioms, and NOTHING from the test module.
    # Counting a test fixture would classify a knob as laddered because a unit
    # test mentioned it — the failure that makes a derived fact worse than a
    # hand-kept one.
    src = (
        '        "field" => "NROS_FROM_MATCH_ARM",\n'
        '        env("NROS_FROM_ENV_CALL")\n'
        '        flag("ZPICO_FROM_FLAG_CALL", BUILTIN)\n'
        '        /// env("NROS_IN_A_DOC_COMMENT")\n'
        '#[cfg(test)]\n'
        '        env("NROS_ONLY_IN_TESTS")\n'
    )
    import tempfile
    with tempfile.TemporaryDirectory() as d:
        sub = os.path.join(d, "packages/tooling/nros-platform-config/src")
        os.makedirs(sub)
        with open(os.path.join(sub, "platform_config.rs"), "w") as fh:
            fh.write(src)
        global ROOT
        real, ROOT = ROOT, d
        try:
            got = ladder_env_keys()
        finally:
            ROOT = real
    assert got == {
        "NROS_FROM_MATCH_ARM",
        "NROS_FROM_ENV_CALL",
        "ZPICO_FROM_FLAG_CALL",
    }, f"selftest: ladder-mapping reader got {sorted(got)!r}"

    # The READ TABLE arm (phase-484 W8): a name bound only by an
    # `xrce-config.txt` row — read by variable, spelled in no read call — must
    # be SEEN, so that an unregistered one reaches the registry check below
    # rather than passing unseen. Both row kinds that bind an env name count; a
    # `value` row binds none, and a comment is not a row.
    table = (
        "# define XRCE_IN_A_COMMENT NROS_XRCE_COMMENTED 1\n"
        "value  uxr  UCLIENT_X  1\n"
        "knob   uxr  UCLIENT_Y  NROS_XRCE_FROM_KNOB_ROW  4096  128\n"
        "define XRCE_Z          NROS_XRCE_FROM_DEFINE_ROW 1\n"
    )
    got = xrce_manifest_env(table)
    assert got == {"NROS_XRCE_FROM_KNOB_ROW", "NROS_XRCE_FROM_DEFINE_ROW"}, (
        f"selftest: read-table reader got {sorted(got)!r}"
    )
    with tempfile.TemporaryDirectory() as d:
        os.makedirs(os.path.join(d, os.path.dirname(XRCE_CONFIG_MANIFEST)))
        with open(os.path.join(d, XRCE_CONFIG_MANIFEST), "w") as fh:
            fh.write(table)
        real, ROOT = ROOT, d
        try:
            seen = read_table_env("NROS")
            unregistered = unregistered_knobs(
                {"sizing": sorted(seen)}, {"NROS_XRCE_FROM_KNOB_ROW"}
            )
        finally:
            ROOT = real
    assert unregistered == ["NROS_XRCE_FROM_DEFINE_ROW"], (
        "selftest: a read-table name with no registry row was not reported "
        f"({unregistered!r})"
    )

    # Issue 2013 — `explain_gaps` must report a tenant with no printed row and
    # a field missing from a printed tenant's defaults, and nothing else.
    fake = (
        'fn explain(a: A) {\n'
        '    let d = &[("max_cbs", 4)];\n'
        '    println!("{}", format!("executor.{name}"));\n'
        '}\n'
        'fn after() { let _ = "stream_history"; }\n'
    )
    gaps = explain_gaps(
        {"ExecutorKnobs": ["max_cbs", "max_sc"], "XrceKnobs": ["stream_history"]},
        fake,
    )
    assert gaps == ["ExecutorKnobs.max_sc", "XrceKnobs (no `explain` row)"], (
        f"selftest: explain_gaps reported {gaps!r}"
    )


def main() -> int:
    self_test()
    ladder = ladder_knobs()
    n_ladder = sum(len(v) for v in ladder.values())
    kconfig = kconfig_symbols()
    nros_env = build_script_env("NROS")
    zpico_env = build_script_env("ZPICO")

    seen = sorted(nros_env | zpico_env)
    mapped = ladder_env_keys()

    # The table must not restate what the ladder source already says. This is
    # the half that stops `KNOB_CLASS` being a conflict site: a migration edits
    # the ladder and nothing here.
    restated = sorted(
        n for n, c in KNOB_CLASS.items()
        if c[0] == "ladder" and n in mapped
    )
    if restated:
        print("config-knob-census: KNOB_CLASS restates the ladder for "
              f"{len(restated)} knob(s):")
        for n in restated:
            print(f"    {n}")
        print("  Whether the ladder maps a knob is read from")
        print("  packages/tooling/nros-platform-config/src/platform_config.rs.")
        print("  Delete these lines — a migration should edit the ladder, not this")
        print("  table, which is why three of eleven dirty PRs conflicted here.")
        return 1

    by_class = {}
    unclassified = []
    for n in seen:
        if n in mapped and n not in LADDER_MAPPED_NOT_MIGRATED:
            by_class.setdefault("ladder", []).append(n)
            continue
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
        ("sizing", "sizing knobs with no rung (2007)"),
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
                why = KNOB_CLASS[n][1] if n in KNOB_CLASS else "mapped by the ladder"
                print(f"    {n:<36} {why}")

    unknown = unknown_idioms(build_env_sources(), ("NROS", "ZPICO"))
    if unknown:
        sys.stderr.write(
            "config-knob-census: FAILED — build script(s) touch a knob name "
            "through an idiom this census does not know:\n"
        )
        for callee, names in sorted(unknown.items()):
            sample = ", ".join(sorted(names)[:3])
            sys.stderr.write(f"    {callee}(...)  e.g. {sample}\n")
        sys.stderr.write(
            "  Add it to READ_CALLEES if it READS the environment, or to\n"
            "  NON_READ_CALLEES if it does something else with the name (emits\n"
            "  a C macro, sets a child's env, builds a list). Neither list may\n"
            "  be skipped: a helper in neither is counted as nothing, which is\n"
            "  how a local `knob()` wrapper took five migrated knobs out of\n"
            "  this census while the gate stayed green.\n"
        )
        return 1

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

    # phase-484 W5a — a name classified as a KNOB must be a spelling in the
    # knob registry, which is what every reader resolves it through. A knob
    # this census sees and the registry does not is a reader the registry
    # cannot rename, alias or derive a Kconfig symbol for.
    unregistered = unregistered_knobs(by_class, registry_spellings())
    if unregistered:
        sys.stderr.write(
            "config-knob-census: FAILED — knob(s) with no row in "
            "packages/tooling/nros-zephyr-build/knobs.toml:\n"
        )
        for n in unregistered:
            sys.stderr.write(f"    {n}\n")
        sys.stderr.write(
            "  Add a row (or the name as a `legacy` spelling of an existing one)\n"
            "  and run `python3 scripts/gen-knob-registry.py`.\n"
        )
        return 1

    gaps = explain_gaps(ladder, read(os.path.join(ROOT, EXPLAIN_SRC)))
    if gaps:
        sys.stderr.write(
            "config-knob-census: FAILED — `nros config explain` does not print "
            f"{len(gaps)} ladder knob(s):\n"
        )
        for g in gaps:
            sys.stderr.write(f"    {g}\n")
        sys.stderr.write(
            f"  Add the tenant's resolve call and defaults to {EXPLAIN_SRC}\n"
            "  (and its row prefix to EXPLAIN_PREFIX here). RFC-0086 D6 makes\n"
            "  the provenance report the acceptance test; a knob it omits is\n"
            "  one whose source nobody can see (issue 2013).\n"
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
