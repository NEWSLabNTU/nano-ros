#ifndef NROS_BRIDGE_H
#define NROS_BRIDGE_H

/**
 * @file bridge.h
 * @brief Multi-RMW bridge surface (Phase 128.F.5).
 *
 * Lets a binary that links more than one RMW backend forward raw CDR
 * payloads between Nodes bound to different backends. Single-backend
 * code does not need this header.
 *
 * The functions here are thin C wrappers around the Rust
 * `nros_bridge` crate; the binary must link `libnros_bridge.a` (built
 * with the `nros-bridge` cargo package) in addition to its usual
 * `libnros_c.a` + per-backend static libs.
 *
 * Wire-level loop protection (`bridge_origin` attachment field) is
 * NOT implemented on the C side because the underlying Rust bridge
 * currently uses a payload-hash dedup ring (see
 * `nros_bridge::PubSubBridge` rustdoc). The semantics are identical;
 * pass any non-empty `origin` string to enable the dedup window,
 * pass NULL or "" to disable it.
 */

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ------------------------------------------------------------------ */
/* Multi-session initialisation                                       */
/* ------------------------------------------------------------------ */

/**
 * @brief Spec for one backend session in a bridge-mode binary.
 *
 * Mirrors `nros_node::executor::SessionSpec`. `rmw` MUST match a
 * backend the binary linked in (i.e. one that contributed an entry
 * to `RMW_INIT_ENTRIES`); spelling errors surface as
 * `NROS_RMW_RET_UNKNOWN_BACKEND` at the matching `init_multi` call.
 */
typedef struct {
    const char* rmw;        /**< Canonical backend name, e.g. "zenoh". */
    const char* locator;    /**< Backend-specific locator string. NULL = empty. */
    uint32_t domain_id;     /**< ROS domain id. 0 unless otherwise. */
    const char* node_name;  /**< Session-default node name. NULL = empty. */
    const char* namespace_; /**< Session-default namespace. NULL = "/". */
} nros_session_spec_t;

/**
 * @brief Opaque executor handle returned by `nros_init_multi`.
 *
 * Owned by the runtime; pass to `nros_create_node_on`,
 * `nros_pubsub_bridge_create`, and `nros_fini_multi`.
 */
typedef void* nros_executor_handle_t;

/**
 * @brief Open an executor against multiple RMW backends.
 *
 * `specs[0]` becomes the primary session; `specs[1..specs_len]` open
 * as extras keyed by RMW name. Returns `NROS_RMW_RET_OK` on success
 * and writes the executor handle into `*out`.
 *
 * Failure modes mirror `Executor::open_multi`:
 * - `NROS_RMW_RET_NO_BACKEND` — `specs_len == 0`.
 * - `NROS_RMW_RET_UNKNOWN_BACKEND` — a spec named a backend that
 *   was not linked in.
 * - `NROS_RMW_RET_ERROR` — backend rejected the open.
 */
int32_t nros_init_multi(const nros_session_spec_t* specs, size_t specs_len,
                        nros_executor_handle_t* out);

/** @brief Tear down a `nros_init_multi`-opened executor. */
void nros_fini_multi(nros_executor_handle_t exec);

/* ------------------------------------------------------------------ */
/* Bridge wiring                                                      */
/* ------------------------------------------------------------------ */

/**
 * @brief Opaque pubsub-bridge handle returned by
 * `nros_pubsub_bridge_create`.
 */
typedef void* nros_pubsub_bridge_t;

/**
 * @brief Create a raw pubsub bridge between two backends.
 *
 * Internally calls `Executor::create_node_on(src_node, src_rmw)`,
 * creates a raw subscription on `src_topic`, calls
 * `create_node_on(dst_node, dst_rmw)`, creates a raw publisher on
 * `dst_topic`, and hands them to `nros_bridge::PubSubBridge::new`.
 *
 * `origin` is the source backend's RMW name; pass `NULL` or `""` to
 * skip dedup (single-direction bridges).
 *
 * Returns `NROS_RMW_RET_OK` on success and writes the bridge handle
 * to `*out`.
 */
int32_t nros_pubsub_bridge_create(nros_executor_handle_t exec, const char* src_node,
                                  const char* src_rmw, const char* src_topic, const char* dst_node,
                                  const char* dst_rmw, const char* dst_topic, const char* type_name,
                                  const char* type_hash, const char* origin,
                                  nros_pubsub_bridge_t* out);

/**
 * @brief Drain the source subscription, forwarding each sample to
 * the destination publisher. Returns the number of samples that
 * crossed; samples suppressed by the dedup window are NOT counted
 * here (use `nros_pubsub_bridge_pump_with_stats` for the breakdown).
 */
size_t nros_pubsub_bridge_pump(nros_pubsub_bridge_t bridge);

/** Per-pump counters. */
typedef struct {
    size_t forwarded;
    size_t dropped_echo;
} nros_pump_stats_t;

/** @brief Per-pump statistics variant of `pump`. */
nros_pump_stats_t nros_pubsub_bridge_pump_with_stats(nros_pubsub_bridge_t bridge);

/** @brief Tear down a bridge. */
void nros_pubsub_bridge_destroy(nros_pubsub_bridge_t bridge);

#ifdef __cplusplus
}
#endif

#endif /* NROS_BRIDGE_H */
