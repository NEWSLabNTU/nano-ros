// Cyclone DDS session lifecycle.
//
// `session_create` creates a Cyclone participant on the requested
// domain id. The participant entity is stashed in
// `rmw_session_t::backend_data` via a small heap-allocated state
// struct so future per-session resources (publishers, listeners)
// share the same `void*` slot.
//
// Phase 117.4 — domain config is left at Cyclone's default (the
// `CYCLONEDDS_URI` env var, if set; otherwise built-in defaults). A
// raw `ddsi_config` path mirroring autoware-safety-island's static
// peer list lands in 117.6 once pub/sub needs network tuning.

#include "internal.hpp"
#include "user_config.hpp"  // phase-206 W2 — the bringup's own Cyclone XML

#include "cyclone_config.hpp"  // phase-206 W1 — baked baseline + source composition

#include <dds/dds.h>

#include "graph.hpp"  // Phase 177.36 — ros_discovery_info node graph

#include <stdlib.h>
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
    /* Phase 124.B.1 wake path (issue 0889). Written by
     * `session_set_wake_callback` on the runtime thread, read by
     * `on_data_available` on a Cyclone worker thread. */
    void (*wake_cb)(void *){nullptr};
    void *wake_ctx{nullptr};
    dds_listener_t *listener{nullptr};
};

inline SessionState* as_state(rmw_session_t* s) {
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

} // namespace

namespace {

/// Cyclone calls this from its own receive/delivery thread the moment a reader
/// has data. Handing that to the executor is the WHOLE point: without it
/// `spin_once` has no asynchronous wake and its wait degenerates to a blind
/// sleep, so the runtime polls on a timer and mostly misses (measured on the
/// an536 lane: 5,069 takes per reader for 42 deliveries, a 0.8% hit rate).
///
/// Safe to call from a foreign thread by contract: the runtime callback does a
/// flag write plus a condvar signal and nothing else. That is what separates
/// this from the STATUS-event listeners `subscriber.cpp` deliberately declines
/// — those would need a buffer, a lock, and a safe context to deliver into.
void on_data_available(dds_entity_t /*reader*/, void *arg) {
    auto *state = static_cast<SessionState *>(arg);
    if (state == nullptr) {
        return;
    }
    /* Read once: a concurrent clear could otherwise null it between the test
     * and the call. */
    void (*cb)(void *) = state->wake_cb;
    void *ctx = state->wake_ctx;
    if (cb != nullptr) {
        cb(ctx);
    }
}

} // namespace

rmw_ret_t session_set_wake_callback(rmw_session_t* session,
                                    void (*cb)(void*),
                                    void* ctx) {
    if (session == nullptr || session->backend_data == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    auto* state = as_state(session);

    if (cb == nullptr) {
        /* Detach FIRST, then drop the stored pair: after `dds_set_listener`
         * returns with no data_available handler, Cyclone will not call us
         * again, so nothing can observe the half-cleared state. */
        if (state->listener != nullptr) {
            (void)dds_set_listener(state->participant, nullptr);
            dds_delete_listener(state->listener);
            state->listener = nullptr;
        }
        state->wake_cb = nullptr;
        state->wake_ctx = nullptr;
        return NROS_RMW_RET_OK;
    }

    /* Publish the pair BEFORE the listener exists, so the first callback
     * cannot see a null cb. */
    state->wake_ctx = ctx;
    state->wake_cb = cb;

    if (state->listener == nullptr) {
        state->listener = dds_create_listener(state);
        if (state->listener == nullptr) {
            state->wake_cb = nullptr;
            state->wake_ctx = nullptr;
            return NROS_RMW_RET_ERROR;
        }
        /* On the PARTICIPANT, not per reader: DDS propagates an unhandled
         * event up to the parent, so one listener covers every reader this
         * session will ever create, including ones created later. Readers set
         * no data_available handler of their own (subscriber.cpp polls), so
         * nothing is being overridden.
         *
         * reset_on_invoke = false: this is a level signal, not a one-shot. The
         * executor may still be draining an earlier batch when the next one
         * lands, and it must be woken again. */
        dds_lset_data_available_arg(state->listener, on_data_available, state, false);
        if (dds_set_listener(state->participant, state->listener) < 0) {
            dds_delete_listener(state->listener);
            state->listener = nullptr;
            state->wake_cb = nullptr;
            state->wake_ctx = nullptr;
            return NROS_RMW_RET_ERROR;
        }
    }
    return NROS_RMW_RET_OK;
}

rmw_ret_t session_create(const char* /*locator*/, uint8_t /*mode*/, uint32_t domain_id,
                            const char* node_name, const rmw_session_options_t* options,
                            rmw_session_t* out) {
    if (out == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    // phase-206 W3 — a property this backend cannot honour is REFUSED, not
    // dropped. `mode` and `localhost_only` are hints a backend without the
    // concept may ignore; a configuration PROPERTY is not, because a silently
    // dropped one is indistinguishable from one that took effect. Cyclone's
    // own run-time configuration arrives as CYCLONEDDS_URI (see below), not
    // through this list; when the two are joined this becomes a lookup.
    if (options != nullptr && options->property_count != 0) {
        return NROS_RMW_RET_UNSUPPORTED;
    }

    NROS_CYC_TRACE("session_create: domain=%u entering", domain_id);
    auto* state = alloc_session_state();
    if (state == nullptr) {
        NROS_CYC_TRACE("session_create: BAD_ALLOC for SessionState");
        return NROS_RMW_RET_BAD_ALLOC;
    }

// phase-206 W2 — the guard gained `has_user_cyclone_config()`.
//
// The three platforms below create the domain because they have no other way in.
// HOSTED POSIX and plain Zephyr deliberately do not: Cyclone's own loader reads
// `CYCLONEDDS_URI`, which is exactly the ROS 2 experience, and intercepting it
// would take that away.
//
// But a bringup that SHIPS `rmw/cyclonedds.xml` has stated a config for every
// target it builds, and a file that silently applies on the RTOS and not on the
// host is worse than no file: the acceptance for this work item is that the
// same bytes take effect on both. So a baked config — and only a baked config —
// makes the hosted path create the domain too. An image with no bringup config
// keeps the old hosted behaviour byte for byte.
#if defined(NROS_PLATFORM_FREERTOS) || defined(NROS_PLATFORM_THREADX) || defined(CONFIG_BOARD_NATIVE_SIM)
#  define NROS_CYC_COMPOSE_DOMAIN 1
#else
#  define NROS_CYC_COMPOSE_DOMAIN 0
#endif

if (NROS_CYC_COMPOSE_DOMAIN || has_user_cyclone_config()) {
    // Phase 192.4 — honor a user-supplied CYCLONEDDS_URI (inline XML or
    // `file://` ref) so the baked embedded runtime profile (buffer/stack
    // sizes, MaxAutoParticipantIndex, the 127.0.0.1 peer) is overridable
    // without recompiling. FreeRTOS/ThreadX have no env, so `env_lookup`
    // returns null there — and native_sim's picolibc getenv sees no host
    // environment either (issue 0367). The hosted POSIX path below creates
    // the participant directly and already honors CYCLONEDDS_URI via
    // Cyclone's own config loader.
    //
    // Issue 0367 — beside the env var sits the Kconfig knob: a non-empty
    // CONFIG_NROS_CYCLONE_CONFIG_XML (declared in zephyr/Kconfig since
    // phase 117, consumed nowhere until then) is the compile-time override
    // for targets where no environment exists. Kconfig strings can't carry
    // escaped double quotes comfortably — use single-quoted XML attributes
    // (Address='127.0.0.1') in the blob.
#if defined(CONFIG_NROS_CYCLONE_CONFIG_XML)
    constexpr const char* kKconfigCycloneConfig = CONFIG_NROS_CYCLONE_CONFIG_XML;
#else
    constexpr const char* kKconfigCycloneConfig = "";
#endif
    // phase-206 W1 — COMPOSE the three sources; do not choose between them.
    //
    // This was a three-way ternary, so naming ANY override discarded the
    // whole baked baseline — the `<Threads>` stack sizes above all, which
    // exist because a 1 KiB ddsrt default overflows `recv` on the first real
    // ROS payload. "Point me at a different peer" silently meant "and
    // reinstate that overflow".
    //
    // Cyclone splits this string on commas and parses every token into ONE
    // config, later tokens overriding earlier ones — so handing it all three
    // in increasing precedence is both the fix and the whole of it. See
    // `cyclone_config.hpp` for the parser evidence and for the one exception
    // (`<Thread>` is a list element, so it appends rather than overrides).
    //
    // `env_lookup`, not `::getenv` — a cross libc's `<cstdlib>` aliases only
    // a subset of the C names into `std::`, and which subset differs per libc
    // (see `service.cpp`'s `env_u64`).
    // ORDER IS PRECEDENCE, lowest first (see `cyclone_config.hpp`): the baked
    // baseline, then the Kconfig blob, then the BRINGUP's own file, then the
    // environment. The bringup outranks Kconfig because it is the thing the
    // user authored for this system; the environment outranks the bringup
    // because that is how a ROS 2 user overrides a shipped config at run time
    // without rebuilding, and taking that away would be the opposite of this
    // phase's goal.
    const char* frags[4] = {kEmbeddedCycloneConfig, kKconfigCycloneConfig,
                            kUserCycloneConfig, env_lookup("CYCLONEDDS_URI")};
    // Static, not a local: `session_create` runs on the app task, and 4 KiB of
    // stack is a real ask on an RTOS whose Cyclone threads are hand-sized just
    // above. Cyclone copies the string (`ddsrt_strdup` in `ddsi_config_init`),
    // so it need not outlive this call — and session creation is a startup
    // path reached from one thread, which is what makes a shared buffer safe.
    static char cyc_config[kCycloneConfigMax];
    if (!compose_cyclone_config(cyc_config, sizeof(cyc_config), frags, 4)) {
        // Fail loud. A truncated config is unterminated XML, and Cyclone would
        // report it as a parse error against a string the user never wrote.
        NROS_CYC_TRACE("session_create: baked + override config exceeds %u bytes",
                       static_cast<unsigned>(kCycloneConfigMax));
        free_session_state(state);
        return NROS_RMW_RET_ERROR;
    }
    dds_entity_t domain = dds_create_domain(domain_id, cyc_config);
    if (domain < 0 && domain != DDS_RETCODE_PRECONDITION_NOT_MET) {
        free_session_state(state);
        return NROS_RMW_RET_ERROR;
    }
    if (domain > 0) {
        state->domain = domain;
    }
}  // NROS_CYC_COMPOSE_DOMAIN || has_user_cyclone_config()

    /* issue 0808 — honour `localhost_only`, so the field is a real control
     * rather than a carried value nobody reads (which the issue calls an inert
     * slot in a different costume).
     *
     * Cyclone expresses it as a participant QoS property that its RTPS layer
     * reads when choosing interfaces: restricting discovery to the loopback
     * interface. A caller asking for it on a backend that cannot do it must
     * still get a session — the contract for `mode` and for this field alike is
     * IGNORE, not reject — so a failure to set the property is not fatal. */
    dds_qos_t* pp_qos = nullptr;
    if (options != nullptr && options->localhost_only != 0) {
        pp_qos = dds_create_qos();
        if (pp_qos != nullptr) {
            const char* one = "1";
            dds_qset_prop(pp_qos, "__Interface/Networking/Localhost", one);
        }
    }

    NROS_CYC_TRACE("session_create: calling dds_create_participant");
    dds_entity_t pp = dds_create_participant(domain_id, pp_qos, nullptr);
    if (pp_qos != nullptr) {
        dds_delete_qos(pp_qos);
    }
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
GraphState* session_graph(rmw_session_t* session) {
    if (session == nullptr || session->backend_data == nullptr) return nullptr;
    return &as_state(session)->graph;
}

rmw_ret_t session_destroy(rmw_session_t* session) {
    /* Drop the wake path before anything it points at goes away — the ctx
     * belongs to the executor, and Cyclone must stop calling us first. */
    if (session != nullptr && session->backend_data != nullptr) {
        (void)session_set_wake_callback(session, nullptr, nullptr);
    }

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

rmw_ret_t session_drive_io(rmw_session_t* /*session*/, int32_t timeout_ms) {
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

dds_entity_t session_participant(const rmw_session_t* session) {
    if (session == nullptr || session->backend_data == nullptr) {
        return 0;
    }
    return static_cast<const SessionState*>(session->backend_data)->participant;
}

} // namespace nros_rmw_cyclonedds
