#ifndef NROS_RMW_UORB_ABI_HPP
#define NROS_RMW_UORB_ABI_HPP

// Minimal declaration of the uORB ABI surface the backend uses.
//
// Two link paths:
//   1. Inside a PX4 module build (`-DNROS_RMW_UORB_LINK_PX4=ON`),
//      this header is shadowed by including the upstream
//      `<uORB/uORB.h>` from `${PX4_FIRMWARE_DIR}` instead. The
//      declarations below mirror the upstream signatures exactly so
//      symbol-level resolution stays uniform.
//   2. Standalone / host smoke test
//      (`-DNROS_RMW_UORB_LINK_PX4=OFF`, the default): the
//      declarations below are the canonical ABI, and the test
//      driver provides definitions inline (see
//      `tests/register_smoke.cpp`).
//
// The header is C-callable so the runtime can include it from
// `.c` translation units that K.4.4 may add.

#include <cstddef>
#include <cstdint>

#if !defined(NROS_RMW_UORB_USE_PX4_HEADER)

extern "C" {

/** Opaque advertise handle. PX4 defines this as `void *`; mirror. */
typedef void* orb_advert_t;

/** Topic descriptor.
 *
 *  Mirrors PX4 1.16+'s `orb_metadata` (platforms/common/uORB/uORB.h) field for
 *  field — message versioning replaced the pre-1.16 `o_fields` inspector-schema
 *  tail with `message_hash` + `o_id` + `o_queue`. The backend only reads the
 *  `o_name` / `o_size` / `o_size_no_padding` prefix (and real PX4 module builds
 *  shadow this struct with the upstream header anyway), but the full 6-field
 *  layout keeps this standalone fallback ABI-correct if anything ever reads past
 *  the prefix. `orb_id_size_t` is `uint16_t` since v1.15. (Phase 232.4.) */
struct orb_metadata {
    const char* o_name;         /**< Topic name (e.g. "vehicle_status"). */
    uint16_t o_size;            /**< sizeof(message struct). */
    uint16_t o_size_no_padding; /**< Effective payload size (no tail padding). */
    uint32_t message_hash;      /**< Field hash for message-compat checks. */
    uint16_t o_id;              /**< `ORB_ID` enum (`orb_id_size_t`). */
    uint8_t o_queue;            /**< Queue depth. */
};

/** Advertise multi-instance. Returns a non-null advert handle on
 *  success, nullptr on failure. `instance` is filled with the
 *  allocated instance index (uORB picks the next free slot if the
 *  in-pointer is 0, else the caller-requested slot). */
orb_advert_t orb_advertise_multi(const struct orb_metadata* meta, const void* data, int* instance);

/** Publish a message via an advert handle. Returns 0 on success,
 *  negative errno on failure. */
int orb_publish(const struct orb_metadata* meta, orb_advert_t handle, const void* data);

/** Release an advert handle. Returns 0 on success. */
int orb_unadvertise(orb_advert_t handle);

/** Subscribe to a multi-instance topic. Returns a non-negative
 *  subscription handle on success, negative on failure. */
int orb_subscribe_multi(const struct orb_metadata* meta, unsigned instance);

/** Drop a subscription. */
int orb_unsubscribe(int handle);

/** Copy the latest sample. Returns 0 on success. */
int orb_copy(const struct orb_metadata* meta, int handle, void* buffer);

/** Non-destructive availability check. Returns 0 on "data ready",
 *  negative when no fresh sample. */
int orb_check(int handle, bool* updated);

/** Push-wake callback signature. Fires on the broker's workqueue
 *  context when a fresh sample lands. The callback must NOT block —
 *  flip an atomic flag and return. */
typedef void (*nros_orb_callback_t)(void* arg);

/** Register a push-wake callback on a uORB subscription.
 *  Returns 0 on success, negative on failure (e.g. PX4 build was
 *  configured without callback support).
 *
 *  PX4's `SubscriptionCallbackWorkItem` is constructed from
 *  `(meta, instance)`, not the subscription handle that
 *  `orb_subscribe_multi` returns — the broker derives the
 *  subscription internally from the metadata. We mirror that
 *  shape here; the handle parameter is the bookkeeping key the
 *  data plane uses on `unregister_callback`.
 *
 *  The shim defines this function. Two paths:
 *   - PX4 path (NROS_RMW_UORB_USE_PX4_HEADER): wraps
 *     `uORB::SubscriptionCallbackWorkItem`. Ships in a separate
 *     translation unit (px4_callback_glue.cpp) so the C++-only
 *     PX4 class doesn't bleed into the standalone build.
 *   - Standalone path: weak default that returns -1 (unsupported).
 *     The test driver provides a strong override that stashes
 *     (cb, arg) so the test can fire the callback synthetically. */
int nros_orb_register_callback(const struct orb_metadata* meta, uint8_t instance, int handle,
                               nros_orb_callback_t cb, void* arg);

/** Unregister a previously-installed callback by handle. Returns
 *  0 on success. Unknown handle is a no-op (returns 0). */
int nros_orb_unregister_callback(int handle);

} // extern "C"

#else // NROS_RMW_UORB_USE_PX4_HEADER

#include <uORB/uORB.h>

extern "C" {
typedef void (*nros_orb_callback_t)(void* arg);
int nros_orb_register_callback(const struct orb_metadata* meta, uint8_t instance, int handle,
                               nros_orb_callback_t cb, void* arg);
int nros_orb_unregister_callback(int handle);
}

#endif // NROS_RMW_UORB_USE_PX4_HEADER

#endif // NROS_RMW_UORB_ABI_HPP
