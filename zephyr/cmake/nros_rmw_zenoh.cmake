function(nros_zephyr_configure_rmw_zenoh)
# -------------------------------------------------------------------------
# Zenoh-pico library (compiled from vendored submodule)
# -------------------------------------------------------------------------

# Vendored zenoh-pico submodule (single source of truth for all builds)
set(ZENOH_PICO_DIR ${NROS_REPO_DIR}/packages/zpico/zpico-sys/zenoh-pico)

# --- zenoh-pico sources ---

file(GLOB_RECURSE _zenoh_pico_sources
    "${ZENOH_PICO_DIR}/src/api/*.c"
    "${ZENOH_PICO_DIR}/src/collections/*.c"
    "${ZENOH_PICO_DIR}/src/link/*.c"
    "${ZENOH_PICO_DIR}/src/net/*.c"
    "${ZENOH_PICO_DIR}/src/protocol/*.c"
    "${ZENOH_PICO_DIR}/src/session/*.c"
    "${ZENOH_PICO_DIR}/src/transport/*.c"
    "${ZENOH_PICO_DIR}/src/utils/*.c"
    "${ZENOH_PICO_DIR}/src/system/common/*.c"
)
# Zephyr platform: system.c (clock/memory/sleep/random/threading/time)
# is replaced by the alias TU (`packages/zpico/zpico-sys/c/zpico/
# platform_aliases.c`) compiled inside the cargo-built Rust
# staticlib (`librustapp.a` / `libnros_c.a`). That TU forwards each
# `_z_*` to the canonical `nros_platform_*` ABI provided by
# `nros-platform-zephyr`. Phase 129 retired `zpico-platform-shim`;
# the alias TU is the single replacement provider.
#
# Phase 160.C — network.c (TCP/UDP/multicast) MUST come from
# zenoh-pico's `src/system/zephyr/network.c`, NOT from the alias
# TU. The alias TU's `_z_open_tcp` / `_z_send_tcp` / etc. see a
# generic 32-byte `_z_sys_net_socket_t` opaque (from
# `nros_zenoh_generic_platform.h`); the Zephyr-side `tx.c` /
# `link.c` (compiled here under `ZENOH_ZEPHYR`) see the
# 4-byte `{int _fd}` socket from `system/platform/zephyr.h`.
# The endpoint layouts diverge too (alias 16 B opaque vs.
# `{struct addrinfo*}` 8 B). The size mismatch propagates
# through the by-value endpoint arg and the by-pointer socket
# arg, corrupting the connect-time state → `Transport(
# ConnectionFailed)` on every Zephyr Rust app at session open.
# Same family as Phase 159 NuttX (`_z_send_tcp` ABI gate). The
# paired build.rs change extends `NROS_ZENOH_PLATFORM_USES_UNIX`
# to zephyr so the alias TU's network section is `#ifndef`-elided
# at cargo compile time — without that, both providers land and
# the alias version wins under `--allow-multiple-definition`.
zephyr_library_sources(${_zenoh_pico_sources}
    "${ZENOH_PICO_DIR}/src/system/zephyr/network.c")

# zenoh-pico include directory
zephyr_include_directories(${ZENOH_PICO_DIR}/include)

# --- zenoh-pico compile definitions ---

# Zephyr platform backend
zephyr_compile_definitions(ZENOH_ZEPHYR)

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

# phase-279 (#145) — opt-in tx batching: one send per executor spin instead of
# one send per put. Forwards the Kconfig to zpico.c's ZPICO_TX_BATCH gate
# (zp_batch_start at open + zp_batch_flush at the top of zpico_spin_once).
if(CONFIG_NROS_ZENOH_TX_BATCH)
    zephyr_compile_definitions(ZPICO_TX_BATCH=1)
    # phase-282 (#145) — flush cadence: period of the dedicated tx-flush
    # thread / rate limit of the spin-driven fallback flush.
    if(CONFIG_NROS_ZENOH_TX_BATCH_FLUSH_MS)
        zephyr_compile_definitions(
            ZPICO_TX_BATCH_FLUSH_MS=${CONFIG_NROS_ZENOH_TX_BATCH_FLUSH_MS})
    endif()
endif()

# phase-282 (#145) — split tx locking (steal batch under tx mutex, send under a
# link-write mutex). Gates transport-struct fields: applied to ALL zephyr TUs.
if(CONFIG_NROS_ZENOH_TX_SPLIT_LOCK)
    zephyr_compile_definitions(Z_FEATURE_TX_SPLIT_LOCK=1)
endif()

# Intra-image topic delivery (RFC-0015 Model 1): every node in the image
# shares ONE zenoh session, and neither zenoh-pico nor the router loops a
# publication back to the session it came from. Without local subscriber
# dispatch a same-image pub→sub pair (e.g. ws-qos-rust's reliable_talker →
# qos_listener) silently never delivers. LOCAL_SUBSCRIBER routes each put
# to matching subscribers on the local session in addition to the wire.
zephyr_compile_definitions(Z_FEATURE_LOCAL_SUBSCRIBER=1)

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
_nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_LINK_WS)
_nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_RAWETH_TRANSPORT)

# -------------------------------------------------------------------------
# Shared sources: zenoh shim + zpico-zephyr platform support
# -------------------------------------------------------------------------

# zpico.c — the C API layer over zenoh-pico
zephyr_library_sources(
    ${NROS_REPO_DIR}/packages/zpico/zpico-sys/c/zpico/zpico.c
)
zephyr_library_sources(${NROS_ZEPHYR_DIR}/nros_zenoh_zephyr_system.c)

# zpico_zephyr.c — Zephyr platform support (network wait, session init)
zephyr_library_sources(
    ${NROS_REPO_DIR}/packages/zpico/zpico-zephyr/src/zpico_zephyr.c
)

# Include directories for zpico and platform headers
zephyr_include_directories(${NROS_REPO_DIR}/packages/zpico/zpico-sys/c/include)
zephyr_include_directories(${NROS_REPO_DIR}/packages/zpico/zpico-zephyr/include)

# -------------------------------------------------------------------------
# Transport tuning: Kconfig → C preprocessor flags
# -------------------------------------------------------------------------

# Map Kconfig transport tuning to ZPICO_* defines consumed by zpico.c
zephyr_compile_definitions(
    ZPICO_MAX_PUBLISHERS=${CONFIG_NROS_MAX_PUBLISHERS}
    ZPICO_MAX_SUBSCRIBERS=${CONFIG_NROS_MAX_SUBSCRIBERS}
    ZPICO_MAX_QUERYABLES=${CONFIG_NROS_MAX_QUERYABLES}
    ZPICO_MAX_LIVELINESS=${CONFIG_NROS_MAX_LIVELINESS}
    ZPICO_MAX_PENDING_GETS=${CONFIG_NROS_MAX_PENDING_GETS}
    ZPICO_GET_REPLY_BUF_SIZE=${CONFIG_NROS_GET_REPLY_BUF_SIZE}
    ZPICO_GET_POLL_INTERVAL_MS=${CONFIG_NROS_GET_POLL_INTERVAL_MS}
    ZPICO_FRAG_MAX_SIZE=${CONFIG_NROS_FRAG_MAX_SIZE}
    ZPICO_BATCH_UNICAST_SIZE=${CONFIG_NROS_BATCH_UNICAST_SIZE}
)

endfunction()
