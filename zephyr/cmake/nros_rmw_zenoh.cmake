# FILE scope: an include() inside the function below would resolve
# CMAKE_CURRENT_LIST_DIR against its caller.
include("${CMAKE_CURRENT_LIST_DIR}/../../cmake/NanoRosLocate.cmake")

function(nros_zephyr_configure_rmw_zenoh)
# RFC-0071 D10 (phase-506 W3) — this backend's spelling of the transport
# ROLE. `entry_config.h` reads `NROS_ENTRY_LOCATOR` and names no backend; the
# mapping from zenoh's own Kconfig lives here, with the backend.
#
# Issue 1955 -- CMake DROPS a definition containing `#` from the compiler
# command line ("Preprocessor definitions containing '#' may not be passed on
# the compiler command line"; a configure warning, nothing else), and a serial
# locator has one: `serial/<dev>#baudrate=<n>`. Since this mapping moved here
# from `entry_config.h` (2ccf662c0d), every Zephyr serial image baked the
# empty bottom rung and opened no session (a board: `nros_cpp_init` -100,
# Transport ConnectionFailed at stage 2). So the `#` travels as the C octal
# escape `\043` (at most three digits, so a following digit cannot extend it);
# the string literal the compiler sees is the locator unchanged.
if(DEFINED CONFIG_NROS_ZENOH_LOCATOR)
    # RFC-0109 D10 (phase-514 W5) -- a CAN rate is the controller's (devicetree
    # `bus-speed`), never wiring, and zenoh-pico no longer applies one. This is
    # the Kconfig road's copy of `nros_zenoh_session_config::refuse_device_facts`
    # (`DEVICE_FACT_KEYS`), which every CLI road asks; keep the two in step.
    if(CONFIG_NROS_ZENOH_LOCATOR MATCHES "^(can|isotp)/[^#]*#(.*;)?(d?bitrate)=")
        message(FATAL_ERROR
            "CONFIG_NROS_ZENOH_LOCATOR=\"${CONFIG_NROS_ZENOH_LOCATOR}\" states "
            "`${CMAKE_MATCH_3}`, a fact about the CAN controller rather than wiring "
            "(RFC-0109 D10). Zephyr sets the rate from the controller's devicetree "
            "`bus-speed`, and the board starts it before any session opens. Drop "
            "`${CMAKE_MATCH_3}` from the locator.")
    endif()
    string(REPLACE "#" "\\043" _nros_entry_locator "${CONFIG_NROS_ZENOH_LOCATOR}")
    # And `;`, for the same reason one layer up: it is CMake's LIST separator,
    # so `zephyr_compile_definitions` split `isotp/can#tx_id=..;rx_id=..` into
    # two definitions and every TU failed on an unterminated string -- every
    # ISO-TP and multi-option CAN locator, since the `#` fix above moved this
    # mapping here. `\073` is `;` in the literal the compiler sees.
    string(REPLACE ";" "\\073" _nros_entry_locator "${_nros_entry_locator}")
    zephyr_compile_definitions("NROS_ENTRY_LOCATOR=\"${_nros_entry_locator}\"")
endif()

# -------------------------------------------------------------------------
# Zenoh-pico library (compiled from vendored submodule)
# -------------------------------------------------------------------------

# The zenoh-pico tree through the one ladder (RFC-0103 D5: a local edit in the
# checkout submodule, else the store copy at the pin, else the checkout), the
# same answer the cargo lane's `nros-zpico-build` gets. With no CLI the module
# answers nothing and this falls to the checkout's submodule.
nros_locate_source(ZENOH_PICO_DIR zenoh-pico)
if(NOT ZENOH_PICO_DIR)
    set(ZENOH_PICO_DIR ${NROS_REPO_DIR}/packages/rmw/zenoh/zpico-sys/zenoh-pico)
endif()

# --- zenoh-pico sources ---
#
# phase-420 W9 — THIS FILE OWNS NO SOURCE LIST. The set of vendored zenoh-pico
# TUs compiled here is read from
# `packages/rmw/zenoh/zpico-sys/zenoh-sources.txt`, which `nros-zpico-build`
# (the cargo lane, `add_zenoh_pico_core_sources`) reads too.
#
# There used to be two copies of that selection — nine directory globs here and
# the same nine subdirectory names in `nros-zpico-build/src/lib.rs` — with
# nothing checking that they agreed. That is the defect class issue 1068 fixed
# for micro-XRCE next door: a hand-mirrored thing drifts on append, so derive
# it once instead. The per-platform divergence that IS real (Zephyr compiles
# `system/zephyr/network.c` from the tree while every other platform gets those
# entry points from the alias TU) is carried in the manifest as a named
# condition, with the ABI reasoning; see the `zephyr_system` group there.
#
# Gate: `just check zenoh-source-manifest`.
set(_zenoh_manifest "${NROS_REPO_DIR}/packages/rmw/zenoh/zpico-sys/zenoh-sources.txt")
if(NOT EXISTS "${_zenoh_manifest}")
    message(FATAL_ERROR
        "nros_rmw_zenoh: shared source manifest not found at '${_zenoh_manifest}'. "
        "It is tracked in-repo — a missing one means the checkout is incomplete.")
endif()
# Re-configure when the shared list changes; without this a manifest edit would
# leave a configured build dir compiling yesterday's set.
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${_zenoh_manifest}")

# NROS-ZENOH-CONDITIONS-BEGIN — the boolean this lane supplies for each
# condition token the manifest uses. `check-zenoh-source-manifest` asserts the
# cargo lane answers exactly this token set; the manifest, not this block,
# decides which sources a token covers.
set(_zenoh_cond_always TRUE)
# This lane IS the Zephyr lane — it is reached only from a west build, so the
# platform test the cargo lane makes is a constant here. The cargo lane answers
# FALSE (Zephyr is `compiled_by = "platform"`, issue 0541).
set(_zenoh_cond_zephyr TRUE)
# NROS-ZENOH-CONDITIONS-END

set(_zenoh_tree_zenoh_pico "${ZENOH_PICO_DIR}/src")

# `group <name> <condition>` records → `_zenoh_group_cond_<name>`.
file(STRINGS "${_zenoh_manifest}" _zenoh_group_lines REGEX "^[ \t]*group[ \t]")
foreach(_line IN LISTS _zenoh_group_lines)
    string(REGEX REPLACE "^[ \t]*group[ \t]+([^ \t]+)[ \t]+([^ \t#]+).*$" "\\1;\\2" _g "${_line}")
    list(GET _g 0 _gname)
    list(GET _g 1 _gcond)
    if(NOT DEFINED _zenoh_cond_${_gcond})
        message(FATAL_ERROR
            "nros_rmw_zenoh: ${_zenoh_manifest} group '${_gname}' uses condition token "
            "'${_gcond}', which this lane does not answer. Add a `set(_zenoh_cond_${_gcond} …)` "
            "here AND an arm in nros-zpico-build's add_zenoh_pico_core_sources — a token only "
            "one lane answers is the same defect one conditional over.")
    endif()
    set(_zenoh_group_cond_${_gname} "${_gcond}")
endforeach()

# `dir <group> <tree> <path>` / `src <group> <tree> <path>` records → the
# library's source list.
#
# `CONFIGURE_DEPENDS` on the `dir` expansion, which the hand-written globs it
# replaces did NOT carry: without it a `.c` added under one of these
# directories — by an upstream rebase of the `nano-ros` patch line, say — gets
# no rebuild edge at all, and the build keeps linking yesterday's set with no
# diagnostic. Same shape as issue 0475 one layer over. It costs a glob per
# build; the alternative costs a silently wrong image.
set(_zenoh_pico_sources "")
file(STRINGS "${_zenoh_manifest}" _zenoh_src_lines REGEX "^[ \t]*(dir|src)[ \t]")
foreach(_line IN LISTS _zenoh_src_lines)
    string(REGEX REPLACE "^[ \t]*(dir|src)[ \t]+([^ \t]+)[ \t]+([^ \t]+)[ \t]+([^ \t#]+).*$"
           "\\1;\\2;\\3;\\4" _r "${_line}")
    list(GET _r 0 _rkind)
    list(GET _r 1 _rgroup)
    list(GET _r 2 _rtree)
    list(GET _r 3 _rpath)
    if(NOT DEFINED _zenoh_group_cond_${_rgroup})
        message(FATAL_ERROR
            "nros_rmw_zenoh: ${_zenoh_manifest} record '${_rpath}' names group '${_rgroup}', "
            "which no `group` line declares.")
    endif()
    if(NOT DEFINED _zenoh_tree_${_rtree})
        message(FATAL_ERROR
            "nros_rmw_zenoh: ${_zenoh_manifest} names unknown source tree '${_rtree}'.")
    endif()
    set(_cond "${_zenoh_group_cond_${_rgroup}}")
    if(NOT _zenoh_cond_${_cond})
        continue()
    endif()
    set(_root "${_zenoh_tree_${_rtree}}")
    if(_rkind STREQUAL "dir")
        if(NOT IS_DIRECTORY "${_root}/${_rpath}")
            message(FATAL_ERROR
                "nros_rmw_zenoh: ${_zenoh_manifest} lists directory '${_rtree}/${_rpath}', "
                "which is not a directory at ${_root}/${_rpath}. An upstream bump that renamed "
                "or removed it needs the manifest updated, not this file.")
        endif()
        file(GLOB_RECURSE _expanded CONFIGURE_DEPENDS "${_root}/${_rpath}/*.c")
        list(APPEND _zenoh_pico_sources ${_expanded})
    else()
        if(NOT EXISTS "${_root}/${_rpath}")
            message(FATAL_ERROR
                "nros_rmw_zenoh: ${_zenoh_manifest} lists source '${_rtree}/${_rpath}', which "
                "does not exist at ${_root}/${_rpath}.")
        endif()
        list(APPEND _zenoh_pico_sources "${_root}/${_rpath}")
    endif()
endforeach()
if(NOT _zenoh_pico_sources)
    message(FATAL_ERROR
        "nros_rmw_zenoh: ${_zenoh_manifest} selected no sources — manifest or condition drift.")
endif()

zephyr_library_sources(${_zenoh_pico_sources})

# zenoh-pico include directory
zephyr_include_directories(${ZENOH_PICO_DIR}/include)

# --- zenoh-pico compile definitions ---

# Zephyr platform backend
zephyr_compile_definitions(ZENOH_ZEPHYR)

# Opt-in zenoh-pico internal logging. -DNROS_ZENOH_DEBUG=<1..4> on the west
# command line; nothing changes without it. zenoh-pico's own CMakeLists sets
# ZENOH_DEBUG, but nano-ros compiles those sources itself, so that path never
# reaches this build and there was no way to see why a declaration failed.
if(DEFINED NROS_ZENOH_DEBUG AND NOT NROS_ZENOH_DEBUG STREQUAL "")
    zephyr_compile_definitions(ZENOH_DEBUG=${NROS_ZENOH_DEBUG})
    message(STATUS "nano-ros: zenoh-pico logging at ZENOH_DEBUG=${NROS_ZENOH_DEBUG}")
endif()

# Router-backed client-to-client routing requires interest declarations
# so zenohd knows which peers should receive each keyexpr. Keep matching
# callbacks disabled on Zephyr; they are not needed for routing and can
# create high-rate executor wakeups.
zephyr_compile_definitions(Z_FEATURE_INTEREST=1 Z_FEATURE_MATCHING=0)

# zsock serializes send/recv on a per-fd mutex, so total tx throughput is
# capped at ~one send per recv window — make the window Kconfig-tunable
# (issues 0129/0139; the vendored config.h default is #ifndef-guarded).
if(CONFIG_NROS_ZENOH_SOCKET_TIMEOUT_MS)
    zephyr_compile_definitions(Z_CONFIG_SOCKET_TIMEOUT=${CONFIG_NROS_ZENOH_SOCKET_TIMEOUT_MS})
endif()

# The transport lease. Both tokens are #ifndef-guarded in the vendored
# config.h for exactly this reason — upstream only exposes them as CMake cache
# variables, which needs zenoh-pico's own CMakeLists, and the Zephyr module
# compiles the sources directly. See NROS_ZENOH_LEASE_MS's help for why a
# serial link cares.
if(CONFIG_NROS_ZENOH_LEASE_MS)
    zephyr_compile_definitions(Z_TRANSPORT_LEASE=${CONFIG_NROS_ZENOH_LEASE_MS})
endif()


if(CONFIG_NROS_ZENOH_LEASE_FACTOR)
    zephyr_compile_definitions(Z_TRANSPORT_LEASE_EXPIRE_FACTOR=${CONFIG_NROS_ZENOH_LEASE_FACTOR})
endif()

# RFC-0110 D6 (phase-508) — zenoh-pico 1.9's executor. Its task table is three
# static containers sized by Z_RUNTIME_MAX_TASKS, and upstream's 64 is what the
# vendored config.h (generated by a default configure) would give every Zephyr
# image; a client session spawns five (read, lease, keep-alive, query timeout,
# reconnect), so this is the same 8 the cargo lane's generated header states.
# The idle-read sleep is 0 because Zephyr runs the executor on its own thread
# (THREAD mode), where an idle read waits in its receive.
zephyr_compile_definitions(Z_RUNTIME_MAX_TASKS=8 Z_RUNTIME_IDLE_READ_TASK_SLEEP=0)

# phase-279 (#145) / phase-290 (RFC-0049) — tx batching: one send per executor
# spin instead of one send per put. TRI-STATE forward: the definition is
# ALWAYS emitted (0 or 1) so an explicit Kconfig `n` can override an
# on-default anywhere below it, and every TU sees the same definite value.
if(CONFIG_NROS_ZENOH_TX_BATCH)
    zephyr_compile_definitions(ZPICO_TX_BATCH=1)
    # phase-282 (#145) — flush cadence: period of the dedicated tx-flush
    # thread / rate limit of the spin-driven fallback flush.
    if(CONFIG_NROS_ZENOH_TX_BATCH_FLUSH_MS)
        zephyr_compile_definitions(
            ZPICO_TX_BATCH_FLUSH_MS=${CONFIG_NROS_ZENOH_TX_BATCH_FLUSH_MS})
    endif()
else()
    zephyr_compile_definitions(ZPICO_TX_BATCH=0)
endif()

# phase-282 (#145) — split tx locking (steal batch under tx mutex, send under a
# link-write mutex). Gates transport-struct fields: applied to ALL zephyr TUs
# (issue-0135 ABI rule) — always defined, 0 or 1.
if(CONFIG_NROS_ZENOH_TX_SPLIT_LOCK)
    zephyr_compile_definitions(Z_FEATURE_TX_SPLIT_LOCK=1)
else()
    zephyr_compile_definitions(Z_FEATURE_TX_SPLIT_LOCK=0)
endif()

# Intra-image topic delivery (RFC-0015 Model 1): every node in the image
# shares ONE zenoh session, and neither zenoh-pico nor the router loops a
# publication back to the session it came from. Without local subscriber
# dispatch a same-image pub→sub pair (e.g. ws-qos-rust's reliable_talker →
# qos_listener) silently never delivers. LOCAL_SUBSCRIBER routes each put
# to matching subscribers on the local session in addition to the wire.
zephyr_compile_definitions(Z_FEATURE_LOCAL_SUBSCRIBER=1)

# Issue 1549 -- the same gap for SERVICES: a client's query never comes back to
# the session it left, so a client and a server in one image never meet
# without LOCAL_QUERYABLE. Unlike the topic path it is not always on: it is
# DERIVED (`nros_resolve_knobs`, on when the entity inventory holds both a
# client and a server), because an image with one side pays flash for a path it
# cannot use. It gates struct fields (`api/types.h`), so it is ALWAYS emitted,
# 0 or 1, to every TU -- the issue-0135 ABI rule -- and `check-knob-delivery`
# holds this define to the resolved value.
if(NOT DEFINED NROS_RESOLVED_NROS_RMW_LOCAL_QUERYABLE
   OR "${NROS_RESOLVED_NROS_RMW_LOCAL_QUERYABLE}" STREQUAL "")
    message(FATAL_ERROR
        "nros: NROS_RMW_LOCAL_QUERYABLE reached nros_rmw_zenoh.cmake "
        "unresolved; nros_resolve_knobs() must run first (issue 1549).")
endif()
zephyr_compile_definitions(
    Z_FEATURE_LOCAL_QUERYABLE=${NROS_RESOLVED_NROS_RMW_LOCAL_QUERYABLE})

# Map NROS_ZENOH_* Kconfig options to Z_FEATURE_* compile definitions.
# The function strips the CONFIG_NROS_ZENOH_ prefix and replaces it with
# Z_FEATURE_, then sets =1 or =0 based on the Kconfig boolean value.
function(_nros_configure_zenoh_feature config)
    string(REPLACE "CONFIG_NROS_ZENOH_" "Z_FEATURE_" feature "${config}")
    if(${config})
        zephyr_compile_definitions(${feature}=1)
    else()
        zephyr_compile_definitions(${feature}=0)
    endif()
endfunction()

_nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_MULTI_THREAD)
_nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_PUBLICATION)
_nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_SUBSCRIPTION)
_nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_QUERY)
_nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_QUERYABLE)
_nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_LINK_TCP)
_nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_LINK_UDP_UNICAST)
_nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_LINK_UDP_MULTICAST)
_nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_SCOUTING)
_nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_LINK_SERIAL)
# The serial link's ISR -> read-task ring (zenoh-pico's own #ifndef default is
# the Kconfig default, 1024). Only network.c reads it; no struct layout depends
# on it, so it is not an issue-0135 ABI flag, but it is applied like one.
if(CONFIG_NROS_ZENOH_SERIAL_RX_RING_BYTES)
    zephyr_compile_definitions(
        _Z_ZEPHYR_SERIAL_RX_RING_BYTES=${CONFIG_NROS_ZENOH_SERIAL_RX_RING_BYTES})
endif()
# ... and the sender -> ISR ring of interrupt-driven TX (issue 1534), same terms.
if(CONFIG_NROS_ZENOH_SERIAL_TX_RING_BYTES)
    zephyr_compile_definitions(
        _Z_ZEPHYR_SERIAL_TX_RING_BYTES=${CONFIG_NROS_ZENOH_SERIAL_TX_RING_BYTES})
endif()
# phase-474 I13 -- the serial ISR's cost per call. ISR_STATS compiles the
# cycle/call counters into network.c; TCM moves network.c's code to ITCM and
# its zero-initialised data (the RX and TX rings, the framing buffers) to DTCM.
if(CONFIG_NROS_ZENOH_SERIAL_ISR_STATS)
    zephyr_compile_definitions(_Z_ZEPHYR_SERIAL_ISR_STATS=1)
endif()
# phase-474 I14 -- LPUART FIFO watermarks, the ISR reads/fills the FIFO itself.
if(CONFIG_NROS_ZENOH_SERIAL_FIFO_WATERMARK)
    zephyr_compile_definitions(_Z_ZEPHYR_SERIAL_FIFO_WATERMARK=1)
    zephyr_compile_definitions(_Z_ZEPHYR_SERIAL_LPUART_RXWATER=${CONFIG_NROS_ZENOH_SERIAL_FIFO_RXWATER})
endif()
# phase-474 I15+I16 -- the link on the async UART API (DMA both directions).
if(CONFIG_NROS_ZENOH_SERIAL_UART_ASYNC)
    zephyr_compile_definitions(_Z_ZEPHYR_SERIAL_UART_ASYNC=1
        _Z_ZEPHYR_SERIAL_ASYNC_RX_TIMEOUT_US=${CONFIG_NROS_ZENOH_SERIAL_UART_ASYNC_RX_TIMEOUT_US})
endif()
if(CONFIG_NROS_ZENOH_SERIAL_TCM)
    set(_nros_serial_network_c "")
    foreach(_src ${_zenoh_pico_sources})
        if(_src MATCHES "/system/zephyr/network\\.c$")
            set(_nros_serial_network_c "${_src}")
        endif()
    endforeach()
    if(NOT _nros_serial_network_c)
        message(FATAL_ERROR "CONFIG_NROS_ZENOH_SERIAL_TCM: zenoh-pico's system/zephyr/network.c is not in this build's sources")
    endif()
    zephyr_code_relocate(FILES ${_nros_serial_network_c} LOCATION ITCM_TEXT)
    zephyr_code_relocate(FILES ${_nros_serial_network_c} LOCATION DTCM_BSS)
endif()
# RFC-0080 — CONFIG_NROS_ZENOH_LINK_CAN -> Z_FEATURE_LINK_CAN.
_nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_LINK_CAN)
# RFC-0083 — CONFIG_NROS_ZENOH_LINK_ISOTP -> Z_FEATURE_LINK_ISOTP.
_nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_LINK_ISOTP)
_nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_LINK_WS)
# issue 0882 — CONFIG_NROS_ZENOH_AUTO_RECONNECT -> Z_FEATURE_AUTO_RECONNECT.
# zenoh-pico defaults it ON and there was no way to say otherwise from an image.
_nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_AUTO_RECONNECT)
_nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_RAWETH_TRANSPORT)

# -------------------------------------------------------------------------
# Shared sources: zenoh shim + zpico-zephyr platform support
# -------------------------------------------------------------------------

# zpico.c — the C API layer over zenoh-pico
zephyr_library_sources(
    ${NROS_REPO_DIR}/packages/rmw/zenoh/zpico-sys/c/zpico/zpico.c
)
zephyr_library_sources(${NROS_ZEPHYR_DIR}/nros_zenoh_zephyr_system.c)
# nros_zp_condvar_slice.h — the wait slice this file shares with the alias
# shim (phase-502 W1). Private to this library, so no other Zephyr library
# gains the shim directory on its include path.
zephyr_library_include_directories(
    ${NROS_REPO_DIR}/packages/rmw/zenoh/zpico-sys/c/zpico
)

# zpico_zephyr.c — Zephyr platform support (network wait, session init)
zephyr_library_sources(
    ${NROS_REPO_DIR}/packages/rmw/zenoh/zpico-zephyr/src/zpico_zephyr.c
)

# Include directories for zpico and platform headers
zephyr_include_directories(${NROS_REPO_DIR}/packages/rmw/zenoh/zpico-sys/c/include)
zephyr_include_directories(${NROS_REPO_DIR}/packages/rmw/zenoh/zpico-zephyr/include)

# -------------------------------------------------------------------------
# Transport tuning: Kconfig → C preprocessor flags
# -------------------------------------------------------------------------

# Map transport tuning to ZPICO_* defines consumed by zpico.c.
#
# These read NROS_RESOLVED_* rather than CONFIG_* so the C TUs get exactly the
# value the cargo build got. Reading CONFIG_* here would reintroduce issue 0135:
# an environment override would reach the Rust side only, and the two would
# disagree about a struct's size with no diagnostic (issue 0316).
# phase-412 W1 -- a C define has NO default of its own, unlike a Rust build
# script. `_nros_resolve_derivable_knob`'s rung 4 deliberately leaves a knob
# UNRESOLVED so the reading build script falls to its own literal
# (`env_usize("ZPICO_MAX_SUBSCRIBERS", 8)`), which is the one place that literal
# is written. That contract does not reach here: an unresolved knob expands to
# nothing, `-DZPICO_MAX_SUBSCRIBERS=` reaches the compiler, and zpico.c reports
#     error: flexible array member not at end of struct
# on a struct nobody edited.
#
# It only bites since these knobs gained the `-1` DERIVE sentinel: before that
# Kconfig always carried a number. So the sentinel is safe only where the
# consumer supplies a default, and this consumer is the exception.
foreach(_zp MAX_PUBLISHERS MAX_SUBSCRIBERS MAX_QUERYABLES)
    if(NOT DEFINED NROS_RESOLVED_NROS_ZENOH_${_zp} OR
       "${NROS_RESOLVED_NROS_ZENOH_${_zp}}" STREQUAL "")
        set(NROS_RESOLVED_NROS_ZENOH_${_zp} 8)
        message(STATUS
            "nros: NROS_ZENOH_${_zp} left at the zpico literal default 8 -- nothing "
            "stated it and the entity inventory derived nothing (a first "
            "configure on a clean build dir always lands here, issue 0991)")
    endif()
endforeach()
# phase-412 W2 -- the liveliness pool joined the three above on the DERIVE
# sentinel, so it needs the same rescue, and its literal is zpico.c's 16, not 8.
if(NOT DEFINED NROS_RESOLVED_NROS_ZENOH_MAX_LIVELINESS OR
   "${NROS_RESOLVED_NROS_ZENOH_MAX_LIVELINESS}" STREQUAL "")
    set(NROS_RESOLVED_NROS_ZENOH_MAX_LIVELINESS 16)
    message(STATUS
        "nros: NROS_ZENOH_MAX_LIVELINESS left at the zpico literal default 16 -- "
        "nothing stated it and the entity inventory derived nothing")
endif()

zephyr_compile_definitions(
    ZPICO_MAX_PUBLISHERS=${NROS_RESOLVED_NROS_ZENOH_MAX_PUBLISHERS}
    ZPICO_MAX_SUBSCRIBERS=${NROS_RESOLVED_NROS_ZENOH_MAX_SUBSCRIBERS}
    ZPICO_MAX_QUERYABLES=${NROS_RESOLVED_NROS_ZENOH_MAX_QUERYABLES}
    ZPICO_MAX_LIVELINESS=${NROS_RESOLVED_NROS_ZENOH_MAX_LIVELINESS}
    ZPICO_MAX_PENDING_GETS=${NROS_RESOLVED_NROS_ZENOH_MAX_PENDING_GETS}
)
# Read straight from CONFIG_, unlike the block above, and deliberately: the
# graph cache is allocated and consumed entirely inside zpico.c. The Rust side
# copies it out through a caller-supplied buffer and never encodes its size, so
# there is no second consumer to disagree with -- the issue-0135/0316 hazard the
# NROS_RESOLVED_* indirection exists to prevent does not apply here.
# D9 -- graph discovery, RESOLVED (not raw CONFIG_) because the environment
# rung outranks Kconfig for it and `check-knob-delivery` holds the define to the
# resolved value. Always 0 or 1: `zpico.c` defaults it to 1, so an empty value
# here would compile the cache in under an image that asked for none.
if(NOT DEFINED NROS_RESOLVED_NROS_ZENOH_GRAPH_DISCOVERY OR
   "${NROS_RESOLVED_NROS_ZENOH_GRAPH_DISCOVERY}" STREQUAL "")
    message(FATAL_ERROR
        "nros: NROS_ZENOH_GRAPH_DISCOVERY was not resolved before nros_rmw_zenoh.cmake "
        "ran; nros_resolve_knobs() must run first (zephyr/CMakeLists.txt)")
endif()
zephyr_compile_definitions(
    ZPICO_GRAPH_DISCOVERY=${NROS_RESOLVED_NROS_ZENOH_GRAPH_DISCOVERY})
zephyr_compile_definitions(
    ZPICO_GRAPH_CACHE_SIZE=${CONFIG_NROS_GRAPH_CACHE_SIZE}
    ZPICO_GET_REPLY_BUF_SIZE=${NROS_RESOLVED_NROS_ZENOH_GET_REPLY_BUF_SIZE}
    ZPICO_GET_POLL_INTERVAL_MS=${NROS_RESOLVED_NROS_ZENOH_GET_POLL_INTERVAL_MS}
    ZPICO_FRAG_MAX_SIZE=${NROS_RESOLVED_NROS_ZENOH_FRAG_MAX_SIZE}
    ZPICO_BATCH_UNICAST_SIZE=${NROS_RESOLVED_NROS_ZENOH_BATCH_UNICAST_SIZE}
)

# RFC-0074 / phase-462 W6a -- a budget on zenoh-pico's unicast read task,
# RESOLVED so the environment rung outranks Kconfig exactly as it does for the
# cargo lane. Negative or empty states nothing (an unbounded task, no define);
# 0 is refused here rather than at zenoh-pico's own `#error`, so the message
# names the knob.
if(DEFINED NROS_RESOLVED_NROS_ZENOH_RX_TASK_BUDGET_FRAMES AND
   NOT "${NROS_RESOLVED_NROS_ZENOH_RX_TASK_BUDGET_FRAMES}" STREQUAL "" AND
   NROS_RESOLVED_NROS_ZENOH_RX_TASK_BUDGET_FRAMES GREATER_EQUAL 0)
    if(NROS_RESOLVED_NROS_ZENOH_RX_TASK_BUDGET_FRAMES EQUAL 0)
        message(FATAL_ERROR
            "nros: NROS_ZENOH_RX_TASK_BUDGET_FRAMES = 0 is refused: a budget of zero "
            "frames is not \"unbounded\". Use -1 (the default) for an unbounded read "
            "task, or state at least 1 frame (RFC-0074).")
    endif()
    # phase-462 W6b -- REST_US defaults to the `-1` DERIVE sentinel, so a
    # frame budget stated with no rest (and none derived) leaves it
    # unresolved; 1000 us is the same builtin `nros-zpico-build` uses.
    set(_nros_rx_rest_us 1000)
    if("${NROS_RESOLVED_NROS_ZENOH_RX_TASK_BUDGET_REST_US}" STREQUAL "0")
        message(FATAL_ERROR
            "nros: NROS_ZENOH_RX_TASK_BUDGET_REST_US = 0 with a frame budget is "
            "refused: a budget that never rests bounds nothing (RFC-0074).")
    endif()
    if(DEFINED NROS_RESOLVED_NROS_ZENOH_RX_TASK_BUDGET_REST_US AND
       NROS_RESOLVED_NROS_ZENOH_RX_TASK_BUDGET_REST_US GREATER 0)
        set(_nros_rx_rest_us "${NROS_RESOLVED_NROS_ZENOH_RX_TASK_BUDGET_REST_US}")
    endif()
    zephyr_compile_definitions(
        Z_RX_TASK_BUDGET_FRAMES=${NROS_RESOLVED_NROS_ZENOH_RX_TASK_BUDGET_FRAMES}
        Z_RX_TASK_BUDGET_REST_US=${_nros_rx_rest_us})
endif()

# Issue 0626 — the read task priority, normalized 0-31. The read task is
# zenoh-pico 1.10's executor thread and also runs the lease, so the old
# CONFIG_NROS_ZENOH_LEASE_PRIORITY is gone (RFC-0110 D2): a conf that still
# assigns it fails Kconfig as an undefined symbol.
#
# Read from CONFIG_* directly, NOT through `_nros_resolve_knob`, and that is
# deliberate. The resolver exists for knobs with TWO consumers — a `build.rs`
# env var and a C define — that must agree or a struct's size splits between
# the Rust and C halves (issues 0135 / 0316). `check-kconfig-knob-forwarding`
# enforces exactly that pairing, and correctly rejected these two when they
# was routed through the resolver: no Rust build script reads it, because
# it gates no layout. One consumer, one path.
#
# `DEFINED`, not truthiness: 0 is a legal priority on this band (the least
# urgent), and a bare `if(CONFIG_...)` would silently drop it — the same
# "declared value discarded" failure this issue exists to fix.
if(DEFINED CONFIG_NROS_ZENOH_READ_PRIORITY)
    zephyr_compile_definitions(ZPICO_READ_TASK_PRIORITY=${CONFIG_NROS_ZENOH_READ_PRIORITY})
endif()

# Issue 1534 / 2164 / phase-474 T3 -- on a SERIAL link the main thread must
# register BELOW the read task. Registration is CPU work: at Zephyr's default
# CONFIG_MAIN_THREAD_PRIORITY 0 the main thread outranks the read task (band
# 200 = k_thread 4 on a 15-priority image) for the whole registration burst,
# the UART ISR fills the RX ring while the read task is READY and not running,
# and the frames lost to that overflow carry the router's answers to the
# publishers' write-filter interests -- measured on the safety island
# (phase8-W10): 3 of 3 joins into a live domain failed at main 0 and passed at
# main 5. Derived from the image's own .config with the map
# `nros_zephyr_native_priority` uses (band -> SCHED_RR -> k_thread), so this is
# the number the read task actually gets.
#
# A REFUSAL, not a warning (issue 2164). It was a warning on the grounds that
# an image joining an IDLE domain registers before any peer sends, but the
# same main thread then spins the executor above the read task for the life of
# the image, which is the app-above-transport inversion issue 0623 retired for
# every other thread: the shipped bands put the transport above the app. No
# nano-ros image wants main above the read task on a byte-stream link, and a
# warning in a configure log was the only thing standing between a board and a
# join that silently loses its interest replies.
#
# Not DERIVED, because Zephyr owns the symbol and Kconfig cannot compute
# `read_k + 1` from two symbol VALUES (NUM_PREEMPT_PRIORITIES and the band);
# an enumerated `configdefault` would be O(N^2) lines over Zephyr's 0..128
# preempt range. The refusal prints the derived number instead, so stating it
# is one copied line.
if(CONFIG_NROS_ZENOH_LINK_SERIAL
   AND CONFIG_POSIX_PRIORITY_SCHEDULING
   AND DEFINED CONFIG_NROS_ZENOH_READ_PRIORITY
   AND DEFINED CONFIG_NUM_PREEMPT_PRIORITIES
   AND DEFINED CONFIG_MAIN_THREAD_PRIORITY
   AND CONFIG_NUM_PREEMPT_PRIORITIES GREATER 1)
    math(EXPR _nros_rr_hi "${CONFIG_NUM_PREEMPT_PRIORITIES} - 1")
    set(_nros_band ${CONFIG_NROS_ZENOH_READ_PRIORITY})
    if(_nros_band GREATER 255)
        set(_nros_band 255)
    endif()
    math(EXPR _nros_read_k "${_nros_rr_hi} - (${_nros_band} * ${_nros_rr_hi}) / 255")
    if(NOT CONFIG_MAIN_THREAD_PRIORITY GREATER _nros_read_k)
        math(EXPR _nros_main_want "${_nros_read_k} + 1")
        if(_nros_main_want GREATER _nros_rr_hi)
            # The read task already sits on the least urgent preemptible
            # slot, so no main priority is below it: the band is the input
            # to move, not main.
            set(_nros_main_fix
                "raise the read task's band (knob ZENOH_READ_PRIORITY: "
                "`[image.<id>.knobs.zenoh] read_priority` in system.toml): at band "
                "${_nros_band} the read task is on the least urgent preemptible slot "
                "(k_thread ${_nros_read_k}), so no CONFIG_MAIN_THREAD_PRIORITY is below it")
        else()
            set(_nros_main_fix
                "set CONFIG_MAIN_THREAD_PRIORITY=${_nros_main_want} (or a larger, less "
                "urgent number up to ${_nros_rr_hi}) in the board's or image's conf")
        endif()
        string(CONCAT _nros_main_fix ${_nros_main_fix})
        message(FATAL_ERROR
            "nros: CONFIG_MAIN_THREAD_PRIORITY=${CONFIG_MAIN_THREAD_PRIORITY} is not "
            "below the zenoh-pico read task (k_thread ${_nros_read_k}, from "
            "CONFIG_NROS_ZENOH_READ_PRIORITY=${CONFIG_NROS_ZENOH_READ_PRIORITY} on "
            "${CONFIG_NUM_PREEMPT_PRIORITIES} preempt priorities), and this image "
            "links zenoh over a SERIAL link. The main thread's registration then "
            "starves the RX drain, and a board that joins while peers already send "
            "loses the router's interest replies (issues 1534, 2164). To fix it, "
            "${_nros_main_fix}. Also raise CONFIG_NROS_ZENOH_SERIAL_RX_RING_BYTES if "
            "`_z_zephyr_serial_stats.ring_high_water` reaches its size.")
    endif()
endif()

endfunction()
