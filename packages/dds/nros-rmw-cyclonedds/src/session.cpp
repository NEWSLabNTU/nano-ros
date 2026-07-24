// Cyclone DDS session lifecycle.
//
// `session_create` creates a Cyclone participant on the requested
// domain id. The participant entity is stashed in
// `nros_rmw_session_t::backend_data` via a small heap-allocated state
// struct so future per-session resources (publishers, listeners)
// share the same `void*` slot.
//
// Phase 117.4 — domain config is left at Cyclone's default (the
// `CYCLONEDDS_URI` env var, if set; otherwise built-in defaults). A
// raw `ddsi_config` path mirroring autoware-safety-island's static
// peer list lands in 117.6 once pub/sub needs network tuning.

#include "internal.hpp"

#include <dds/dds.h>

#include "graph.hpp"  // Phase 177.36 — ros_discovery_info node graph

#include <cstdlib>
#include <cstring>
#include <new>

#if defined(NROS_PLATFORM_FREERTOS)
#include <FreeRTOS.h>
#include <task.h>
#elif defined(NROS_PLATFORM_THREADX)
#include <nros/platform.h>
#elif !defined(__ZEPHYR__) && !defined(NROS_PLATFORM_THREADX)
#include <ctime> // nanosleep / timespec (POSIX spin-loop pacing)
#endif

#ifdef __ZEPHYR__
#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
LOG_MODULE_DECLARE(cyclonedds, LOG_LEVEL_INF);
#define NROS_CYC_TRACE(...) LOG_INF(__VA_ARGS__)
#else
#define NROS_CYC_TRACE(...) ((void)0)
#endif

namespace nros_rmw_cyclonedds {

namespace {

struct SessionState {
    dds_entity_t domain{0};
    dds_entity_t participant{0};
    GraphState graph{};  // Phase 177.36 — ros_discovery_info publisher state
};

inline SessionState* as_state(nros_rmw_session_t* s) {
    return static_cast<SessionState*>(s->backend_data);
}

SessionState* alloc_session_state() {
#if defined(NROS_PLATFORM_THREADX)
    void* mem = nros_platform_alloc(sizeof(SessionState));
    if (mem == nullptr) {
        return nullptr;
    }
    auto* state = static_cast<SessionState*>(mem);
    state->domain = 0;
    state->participant = 0;
    return state;
#else
    return new (std::nothrow) SessionState();
#endif
}

void free_session_state(SessionState* state) {
    if (state == nullptr) {
        return;
    }
#if defined(NROS_PLATFORM_THREADX)
    nros_platform_dealloc(state);
#else
    delete state;
#endif
}

#if defined(NROS_PLATFORM_FREERTOS) || defined(NROS_PLATFORM_THREADX) || defined(CONFIG_BOARD_NATIVE_SIM)
constexpr const char* kEmbeddedCycloneConfig =
    "<CycloneDDS>"
    "<Domain Id=\"any\">"
    "<General>"
#if defined(NROS_PLATFORM_THREADX)
    // Phase 177.26 — SPDP multicast discovery over NetX Duo. NetX enables
    // IGMPv2 (`nx_igmp_enable`) and virtio-net accepts all multicast on RX;
    // peers discover via the default DDSI multicast group, data unicast.
    "<AllowMulticast>spdp</AllowMulticast>"
#elif defined(CONFIG_BOARD_NATIVE_SIM)
    // Phase 180 — native_sim (NSOS). Multicast breaks cyclone's select-based
    // socket waitset here (the multicast RX fd select()s as failed), so
    // disable it and discover via unicast SPDP to 127.0.0.1 (Peers, below).
    "<AllowMulticast>false</AllowMulticast>"
#endif
    "</General>"
#if defined(CONFIG_BOARD_NATIVE_SIM)
    // Unicast SPDP to localhost (numeric IP — NSOS getaddrinfo can't resolve
    // the name). Widen the participant-index scan so the talker reaches the
    // listener even when host-port collisions bump it to a higher index.
    "<Discovery>"
    "<ParticipantIndex>auto</ParticipantIndex>"
    "<MaxAutoParticipantIndex>20</MaxAutoParticipantIndex>"
    "<Peers><Peer Address=\"127.0.0.1\"/></Peers>"
    "</Discovery>"
#endif
    "<Sizing>"
    "<ReceiveBufferSize>64 KiB</ReceiveBufferSize>"
    "<ReceiveBufferChunkSize>16 KiB</ReceiveBufferChunkSize>"
    "</Sizing>"
    "<Threads>"
    "<Thread Name=\"dq.builtins\">"
    "<StackSize>64 KiB</StackSize>"
    "</Thread>"
    "</Threads>"
    "</Domain>"
    "</CycloneDDS>";
#endif

} // namespace

nros_rmw_ret_t session_create(const char* /*locator*/, uint8_t /*mode*/, uint32_t domain_id,
                            const char* node_name, nros_rmw_session_t* out) {
    if (out == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }

    NROS_CYC_TRACE("session_create: domain=%u entering", domain_id);
    auto* state = alloc_session_state();
    if (state == nullptr) {
        NROS_CYC_TRACE("session_create: BAD_ALLOC for SessionState");
        return NROS_RMW_RET_BAD_ALLOC;
    }

#if defined(NROS_PLATFORM_FREERTOS) || defined(NROS_PLATFORM_THREADX) || defined(CONFIG_BOARD_NATIVE_SIM)
    // Phase 192.4 — honor a user-supplied CYCLONEDDS_URI (inline XML or
    // `file://` ref) so the baked embedded runtime profile (buffer/stack
    // sizes, MaxAutoParticipantIndex, the 127.0.0.1 peer) is overridable
    // without recompiling. Falls back to the built-in profile when unset
    // (FreeRTOS/ThreadX have no env, so getenv returns null there). The
    // hosted POSIX path below creates the participant directly and already
    // honors CYCLONEDDS_URI via Cyclone's own config loader.
    const char* user_uri = std::getenv("CYCLONEDDS_URI");
    const char* cyc_config =
        (user_uri != nullptr && user_uri[0] != '\0') ? user_uri : kEmbeddedCycloneConfig;
    dds_entity_t domain = dds_create_domain(domain_id, cyc_config);
    if (domain < 0 && domain != DDS_RETCODE_PRECONDITION_NOT_MET) {
        free_session_state(state);
        return NROS_RMW_RET_ERROR;
    }
    if (domain > 0) {
        state->domain = domain;
    }
#endif

    NROS_CYC_TRACE("session_create: calling dds_create_participant");
    dds_entity_t pp = dds_create_participant(domain_id, nullptr, nullptr);
    NROS_CYC_TRACE("session_create: dds_create_participant returned %d", (int)pp);
    if (pp < 0) {
        if (state->domain > 0) {
            (void)dds_delete(state->domain);
        }
        free_session_state(state);
        return NROS_RMW_RET_ERROR;
    }
    state->participant = pp;
    out->backend_data = state;

    // Phase 177.36 — stand up the ros_discovery_info graph publisher so stock
    // ROS 2 sees this participant as a node. Best-effort: if the descriptor /
    // writer can't be created the graph stays inactive and interop degrades to
    // endpoint-only (pre-177.36) behaviour.
    graph_init(&state->graph, pp, node_name, "/");
    return NROS_RMW_RET_OK;
}

// Phase 177.36 — expose the per-session graph so the endpoint-create paths
// (publisher/subscriber/service) can register their reader/writer GIDs.
GraphState* session_graph(nros_rmw_session_t* session) {
    if (session == nullptr || session->backend_data == nullptr) return nullptr;
    return &as_state(session)->graph;
}

nros_rmw_ret_t session_destroy(nros_rmw_session_t* session) {
    if (session == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    SessionState* state = as_state(session);
    if (state == nullptr) {
        return NROS_RMW_RET_OK; // already closed / never opened
    }
    if (state->participant > 0) {
        // dds_delete on the participant cascades to every child
        // entity (writers, readers, topics) it owns.
        (void)dds_delete(state->participant);
    }
    if (state->domain > 0) {
        (void)dds_delete(state->domain);
    }
    free_session_state(state);
    session->backend_data = nullptr;
    return NROS_RMW_RET_OK;
}

nros_rmw_ret_t session_drive_io(nros_rmw_session_t* /*session*/, int32_t timeout_ms) {
    // Cyclone owns its own RX threads internally — `drive_io` has
    // nothing to pump. Listener trampolines (Phase 117.6) wake the
    // runtime's `Activator` directly from inside Cyclone's worker.
    //
    // Phase 11W.10 — the executor spin loop calls drive_io as its
    // "wait up to timeout_ms for events" primitive. As a poll-only
    // backend with no async-wake callback, an instant return makes
    // `spin_once` free-run: the no_std Zephyr executor credits
    // `timeout_ms` to timers every call (no clock_us_fn), so a 1 Hz
    // timer fires hundreds of times/second and the writer-history
    // cache grows until the heap is exhausted. Sleep for timeout_ms
    // so the loop paces to real time, the credited delta matches
    // wall-clock, and the thread yields to the native_sim scheduler.
    // Cyclone's own RX threads keep delivering in parallel.
    //
    // The same pacing is required on hosted POSIX. With no async-wake
    // callback the executor's `spin_once` free-runs here; an instant
    // return makes it iterate sub-microsecond, and the runtime credits
    // timers by `elapsed.as_micros()`, which truncates each sub-µs
    // iteration to 0 — so wall-clock timers never accumulate and never
    // fire. Sleeping `timeout_ms` paces the loop to real time exactly
    // like the Zephyr branch.
#if defined(__ZEPHYR__)
    if (timeout_ms > 0) {
        (void)k_msleep(timeout_ms);
    }
#elif defined(NROS_PLATFORM_FREERTOS)
    if (timeout_ms > 0) {
        vTaskDelay(pdMS_TO_TICKS(timeout_ms));
    }
#elif defined(NROS_PLATFORM_THREADX)
    if (timeout_ms > 0) {
        platform_sleep_ms(static_cast<uint32_t>(timeout_ms));
    }
#else
    if (timeout_ms > 0) {
        struct timespec ts;
        ts.tv_sec = timeout_ms / 1000;
        ts.tv_nsec = static_cast<long>(timeout_ms % 1000) * 1000000L;
        (void)nanosleep(&ts, nullptr);
    }
#endif
    return NROS_RMW_RET_OK;
}

dds_entity_t session_participant(const nros_rmw_session_t* session) {
    if (session == nullptr || session->backend_data == nullptr) {
        return 0;
    }
    return static_cast<const SessionState*>(session->backend_data)->participant;
}

} // namespace nros_rmw_cyclonedds
