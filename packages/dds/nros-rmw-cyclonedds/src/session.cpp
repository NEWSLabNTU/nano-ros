// Cyclone DDS session lifecycle.
//
// `session_open` creates a Cyclone participant on the requested
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
};

inline SessionState *as_state(nros_rmw_session_t *s) {
    return static_cast<SessionState *>(s->backend_data);
}

SessionState *alloc_session_state() {
#if defined(NROS_PLATFORM_THREADX)
    void *mem = nros_platform_alloc(sizeof(SessionState));
    if (mem == nullptr) {
        return nullptr;
    }
    auto *state = static_cast<SessionState *>(mem);
    state->domain = 0;
    state->participant = 0;
    return state;
#else
    return new (std::nothrow) SessionState();
#endif
}

void free_session_state(SessionState *state) {
    if (state == nullptr) {
        return;
    }
#if defined(NROS_PLATFORM_THREADX)
    nros_platform_dealloc(state);
#else
    delete state;
#endif
}

#if defined(NROS_PLATFORM_FREERTOS) || defined(NROS_PLATFORM_THREADX)
constexpr const char *kEmbeddedCycloneConfig =
    "<CycloneDDS>"
    "<Domain Id=\"any\">"
    "<General>"
#if defined(NROS_PLATFORM_THREADX)
    "<AllowMulticast>false</AllowMulticast>"
#endif
    "</General>"
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

nros_rmw_ret_t session_open(const char * /*locator*/, uint8_t /*mode*/,
                            uint32_t domain_id, const char * /*node_name*/,
                            nros_rmw_session_t *out) {
    if (out == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }

    NROS_CYC_TRACE("session_open: domain=%u entering", domain_id);
    auto *state = alloc_session_state();
    if (state == nullptr) {
        NROS_CYC_TRACE("session_open: BAD_ALLOC for SessionState");
        return NROS_RMW_RET_BAD_ALLOC;
    }

#if defined(NROS_PLATFORM_FREERTOS) || defined(NROS_PLATFORM_THREADX)
    dds_entity_t domain = dds_create_domain(domain_id, kEmbeddedCycloneConfig);
    if (domain < 0 && domain != DDS_RETCODE_PRECONDITION_NOT_MET) {
        free_session_state(state);
        return NROS_RMW_RET_ERROR;
    }
    if (domain > 0) {
        state->domain = domain;
    }
#endif

    NROS_CYC_TRACE("session_open: calling dds_create_participant");
    dds_entity_t pp = dds_create_participant(domain_id, nullptr, nullptr);
    NROS_CYC_TRACE("session_open: dds_create_participant returned %d", (int)pp);
    if (pp < 0) {
        if (state->domain > 0) {
            (void) dds_delete(state->domain);
        }
        free_session_state(state);
        return NROS_RMW_RET_ERROR;
    }
    state->participant = pp;
    out->backend_data  = state;
    return NROS_RMW_RET_OK;
}

nros_rmw_ret_t session_close(nros_rmw_session_t *session) {
    if (session == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    SessionState *state = as_state(session);
    if (state == nullptr) {
        return NROS_RMW_RET_OK;  // already closed / never opened
    }
    if (state->participant > 0) {
        // dds_delete on the participant cascades to every child
        // entity (writers, readers, topics) it owns.
        (void) dds_delete(state->participant);
    }
    if (state->domain > 0) {
        (void) dds_delete(state->domain);
    }
    free_session_state(state);
    session->backend_data = nullptr;
    return NROS_RMW_RET_OK;
}

nros_rmw_ret_t session_drive_io(nros_rmw_session_t * /*session*/,
                                int32_t timeout_ms) {
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
        (void) k_msleep(timeout_ms);
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
        (void) nanosleep(&ts, nullptr);
    }
#endif
    return NROS_RMW_RET_OK;
}

dds_entity_t session_participant(const nros_rmw_session_t *session) {
    if (session == nullptr || session->backend_data == nullptr) {
        return 0;
    }
    return static_cast<const SessionState *>(session->backend_data)->participant;
}

} // namespace nros_rmw_cyclonedds
