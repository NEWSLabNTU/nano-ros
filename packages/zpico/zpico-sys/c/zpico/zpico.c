/**
 * zpico-sys Core Implementation
 *
 * This file implements the zpico API using zenoh-pico.
 * Platform-specific behavior is handled by zenoh-pico's platform layer.
 *
 * zpico provides a simplified C API that hides zenoh-pico's complex
 * ownership types from Rust FFI (avoiding struct size mismatch issues).
 */

#include "zpico.h"
#include <zenoh-pico.h>
#include <zenoh-pico/session/query.h>
#include <zenoh-pico/api/olv_macros.h>
/* Phase 154 — `nros_platform_socket_get_fd` accessor for the
 * `get_session_fd` helper (used by ThreadX-Linux's
 * `select`-driven read-task wakeup path). Replaces the
 * `peer->_socket._fd` field access that no longer compiles
 * once `NROS_PLATFORM_ALIASES` is defined on the vendor build. */
#include <nros/platform_net.h>
#include <stdio.h>
#include <string.h>
#include <stdatomic.h>
#include <time.h>
#if defined(__linux__)
#include <sys/random.h>
#include <unistd.h>
#endif
#ifdef ZENOH_NUTTX
#include <unistd.h>
#endif

#ifdef ZENOH_ZEPHYR
#include <zephyr/kernel.h> // For printk
#include <zephyr/random/random.h>
#if defined(CONFIG_POSIX_MULTI_PROCESS)
#include <zephyr/posix/unistd.h>
#endif
#elif defined(ZENOH_FREERTOS_LWIP)
// On FreeRTOS, route printk to semihosting for debug output.
// Uses SYS_WRITE0 semihosting call (null-terminated string to stdout).
#include <stdio.h>
static void _freertos_printk(const char* fmt, ...) {
    char buf[128];
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(buf, sizeof(buf), fmt, ap);
    va_end(ap);
    // ARM semihosting SYS_WRITE0 (op=0x04): write null-terminated string.
    // ARM-only (the `r0`/`r1` register binds + `bkpt` are invalid elsewhere);
    // guarded so this TU also compiles when zpico-sys is built for a host target
    // (phase-243 — surfaced when the freertos config TU is host-compiled).
#if defined(__arm__) || defined(__thumb__)
    register unsigned r0 __asm__("r0") = 0x04;
    register const char* r1 __asm__("r1") = buf;
    __asm__ volatile("bkpt #0xAB" : : "r"(r0), "r"(r1) : "memory");
#else
    (void)buf;
#endif
}
#define printk(...) _freertos_printk(__VA_ARGS__)
#elif defined(ZENOH_THREADX)
// On ThreadX route printk through printf (Linux sim) or uart_puts (bare-metal)
#include <stdio.h>
#if defined(__linux__)
#include <sys/select.h>
#define printk(...) printf(__VA_ARGS__)
#else
#include "nxd_bsd.h"
extern void uart_puts(const char* s);
static void _threadx_printk(const char* fmt, ...) {
    char buf[128];
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(buf, sizeof(buf), fmt, ap);
    va_end(ap);
    uart_puts(buf);
}
#define printk(...) _threadx_printk(__VA_ARGS__)
#endif
#elif defined(ZPICO_SMOLTCP) || defined(ZPICO_SERIAL)
#define printk(...) // No libc printf on bare-metal
#else
#define printk(...) // No-op on other platforms
#endif

// Internal zenoh-pico headers for socket FD access (select()-based timeout).
// Needed for single-threaded builds and for ThreadX/NSOS, where we deliberately
// drive read + keepalive from zpico_spin_once instead of background tasks.
#if defined(ZENOH_THREADX)
#include "nxd_bsd.h"
#endif
#if !defined(ZPICO_SMOLTCP) && !defined(ZPICO_SERIAL) && !defined(ZENOH_FREERTOS_LWIP) &&          \
    !defined(ZENOH_THREADX) && Z_FEATURE_MULTI_THREAD != 1
#include "zenoh-pico/net/session.h"
#include "zenoh-pico/transport/transport.h"
#include "zenoh-pico/api/olv_macros.h"
#include <sys/select.h>
#elif defined(ZENOH_THREADX)
#include "zenoh-pico/net/session.h"
#include "zenoh-pico/transport/transport.h"
#include "zenoh-pico/api/olv_macros.h"
#endif

// ============================================================================
// Platform-Specific Declarations
// ============================================================================

#ifdef ZPICO_SMOLTCP
// External Rust FFI functions for smoltcp platform
extern int32_t smoltcp_init(void);
extern void smoltcp_cleanup(void);
extern uint64_t smoltcp_clock_now_ms(void);
#endif

// ============================================================================
// Internal Data Structures
// ============================================================================

// Subscriber entry with callback (supports legacy, attachment, and direct-write modes)
typedef struct {
    z_owned_subscriber_t subscriber;
    union {
        ZpicoCallback callback;                   // Legacy callback (payload only)
        ZpicoCallbackWithAttachment callback_ext; // Extended callback (with attachment)
        ZpicoNotifyCallback notify;               // Direct-write notify (len + attachment)
    };
    void* ctx;
    bool active;
    bool with_attachment; // true = use callback_ext, false = use callback
    // Direct-write fields (set when mode == direct_write)
    bool direct_write;      // true = direct-write mode
    uint8_t* buf_ptr;       // Pointer into Rust SUBSCRIBER_BUFFERS[i].data
    size_t buf_capacity;    // Size of the Rust buffer
    const bool* locked_ptr; // Pointer to Rust SUBSCRIBER_BUFFERS[i].locked (AtomicBool)
    // Phase 124.D.3.c — SPSC ring descriptor (set when mode == ring).
    // C is the sole producer, Rust the sole consumer. See
    // `zpico_ring_desc_t` in the header. NULL = not in ring mode.
    bool ring_mode;
    zpico_ring_desc_t* ring;
#if defined(Z_FEATURE_UNSTABLE_API)
    bool zero_copy; // true = zero-copy mode (borrows from zenoh-pico buffer)
    ZpicoZeroCopyCallback zero_copy_cb;
#endif
} subscriber_entry_t;

// Publisher entry
typedef struct {
    z_owned_publisher_t publisher;
    bool active;
} publisher_entry_t;

// Liveliness token entry
typedef struct {
    z_owned_liveliness_token_t token;
    bool active;
} liveliness_entry_t;

// Queryable entry for service servers
typedef struct {
    z_owned_queryable_t queryable;
    ZpicoQueryCallback callback;
    void* ctx;
    bool active;
} queryable_entry_t;

// ZPICO_MAX_PUBLISHERS, ZPICO_MAX_SUBSCRIBERS, ZPICO_MAX_QUERYABLES,
// ZPICO_MAX_LIVELINESS, ZPICO_GET_REPLY_BUF_SIZE, and ZPICO_GET_POLL_INTERVAL_MS
// are provided via -D compiler flags from build.rs (configurable with env vars).
// Defaults are provided here for non-Cargo build paths (e.g., Zephyr CMake).
#ifndef ZPICO_MAX_PUBLISHERS
#define ZPICO_MAX_PUBLISHERS 8
#endif
#ifndef ZPICO_MAX_SUBSCRIBERS
#define ZPICO_MAX_SUBSCRIBERS 8
#endif
#ifndef ZPICO_MAX_QUERYABLES
#define ZPICO_MAX_QUERYABLES 8
#endif
#ifndef ZPICO_MAX_LIVELINESS
#define ZPICO_MAX_LIVELINESS 16
#endif
#ifndef ZPICO_GET_REPLY_BUF_SIZE
#define ZPICO_GET_REPLY_BUF_SIZE 4096
#endif
#ifndef ZPICO_GET_POLL_INTERVAL_MS
#define ZPICO_GET_POLL_INTERVAL_MS 10
#endif
#ifndef ZPICO_MAX_PENDING_GETS
#define ZPICO_MAX_PENDING_GETS 4
#endif

// Static storage for zenoh objects
static z_owned_config_t g_config;
static z_owned_session_t g_session;
static bool g_session_open = false;
static bool g_initialized = false;

static publisher_entry_t g_publishers[ZPICO_MAX_PUBLISHERS];
static subscriber_entry_t g_subscribers[ZPICO_MAX_SUBSCRIBERS];
static liveliness_entry_t g_liveliness[ZPICO_MAX_LIVELINESS];
static queryable_entry_t g_queryables[ZPICO_MAX_QUERYABLES];

// Phase 237 — per-queryable seq-keyed reply slots. A reply can be sent long
// after the query callback returned (an action `get_result` held until the goal
// terminates), and concurrent goals mean several queries may be outstanding at
// once, so a single stored query per queryable would be overwritten. Each slot
// holds one cloned (owned) query; `query_handler` allocates a free slot and
// records its index as the reply seq, `zpico_query_reply(handle, seq)` consumes
// it. Override for servers fielding more concurrent in-flight requests.
#ifndef ZPICO_MAX_PENDING_REPLIES
#define ZPICO_MAX_PENDING_REPLIES 4
#endif
static z_owned_query_t g_stored_queries[ZPICO_MAX_QUERYABLES][ZPICO_MAX_PENDING_REPLIES];
static bool g_stored_query_valid[ZPICO_MAX_QUERYABLES]
                                [ZPICO_MAX_PENDING_REPLIES]; // zero-init = all false
// Index of the slot allocated by the most recent `query_handler` invocation,
// read by `zpico_queryable_take_reply_seq` from inside the (synchronous) user
// callback. -1 if the table was full (the reply is dropped — back-pressure).
static int64_t g_last_reply_seq[ZPICO_MAX_QUERYABLES];
#if defined(ZPICO_SMOLTCP) || defined(ZPICO_SERIAL)
static uint32_t g_session_zid_counter;
#else
static atomic_uint g_session_zid_counter;
#endif

// Context struct for blocking z_get reply (stack-allocated per call)
// ZPICO_GET_REPLY_BUF_SIZE is provided via -D compiler flag from build.rs

typedef struct {
    uint8_t buf[ZPICO_GET_REPLY_BUF_SIZE];
    size_t len;
    /* Phase 127.D — atomic+volatile so fat-LTO doesn't hoist the
     * read in `zpico_get_check` over the write inside
     * `get_reply_handler` (callback fired by zenoh-pico's RX path
     * during `zp_read`, which sits between two polls of the same
     * struct field). Plain volatile alone was not enough under
     * `lto = "fat"` + `opt-level = "s"` on cortex-m3. Use the GCC
     * `__atomic_*` builtins so the access is a real load/store
     * even after whole-program inlining. */
    _Atomic bool received;
    _Atomic bool done;
    /* Phase 108.C.zenoh.4-followup — counts every reply that arrives.
     * Single-response queries (`zpico_get*`) use `received` for "first
     * reply only" semantics; multi-response queries (liveliness) read
     * `reply_count` to learn how many distinct tokens responded. The
     * count is incremented in `get_reply_handler` regardless of
     * payload buffer occupancy. */
    uint32_t reply_count;
#if Z_FEATURE_MULTI_THREAD == 1
    _z_mutex_t mutex;
    _z_condvar_t cond;
#endif
} get_reply_ctx_t;

// Static slots for non-blocking z_get operations
// ZPICO_MAX_PENDING_GETS is provided via -D compiler flag from build.rs,
// with a default fallback above for non-Cargo build paths.
typedef struct {
    get_reply_ctx_t ctx;
    bool in_use;
} pending_get_slot_t;

static pending_get_slot_t g_pending_gets[ZPICO_MAX_PENDING_GETS];

// Phase 127.D — diagnostic counters for reply-dispatch debugging.
// Read via `zpico_get_diag_counters` from Rust.
static volatile uint32_t g_diag_reply_handler_calls = 0;
static volatile uint32_t g_diag_reply_dropper_calls = 0;
static volatile uint32_t g_diag_get_start_calls = 0;
static volatile uint32_t g_diag_get_check_calls = 0;
static volatile uint32_t g_diag_get_check_returns_data = 0;
static volatile uint32_t g_diag_reply_not_ok = 0;
static volatile uint32_t g_diag_reply_to_slice_fail = 0;
static volatile uint32_t g_diag_reply_too_big = 0;
static volatile uint32_t g_diag_reply_already_received = 0;
static volatile uint32_t g_diag_reply_received_set = 0;
static volatile uint32_t g_diag_gck_invalid_arg = 0;
static volatile uint32_t g_diag_gck_not_in_use = 0;
static volatile uint32_t g_diag_gck_too_big = 0;
static volatile uint32_t g_diag_gck_timeout = 0;
static volatile uint32_t g_diag_gck_pending = 0;
static volatile uint32_t g_diag_handler_ctx_addr = 0;
static volatile uint32_t g_diag_check_ctx_addr = 0;
static volatile uint32_t g_diag_start_ctx_addr = 0;

// Reply waker callback — invoked when a pending get slot receives a reply
// or times out, allowing Rust async code to wake the corresponding Future.
typedef void (*zpico_waker_fn)(int32_t slot);
static zpico_waker_fn g_reply_waker = NULL;

// Spin-wake primitive for multi-threaded spin_once().
// Signaled by our callbacks (sample_handler, query_handler, get reply handlers)
// so spin_once() can wake immediately when application data arrives, rather
// than sleeping for the full timeout duration.
#if Z_FEATURE_MULTI_THREAD == 1 && !defined(ZPICO_SMOLTCP)

#if defined(ZENOH_FREERTOS_LWIP)
// FreeRTOS: binary semaphore — lightweight, no mutex needed.
#include <FreeRTOS.h>
#include <semphr.h>
static SemaphoreHandle_t g_spin_sem = NULL;

static inline void _zpico_notify_spin(void) {
    if (g_spin_sem != NULL) {
        xSemaphoreGive(g_spin_sem);
    }
}
#elif defined(ZENOH_NUTTX)
// NuttX: POSIX `sem_t` + `sem_timedwait`. We can't use the pthread
// mutex + condvar pair here — on NuttX the pthread-timed-wait path
// hangs indefinitely inside the kernel's watchdog-backed semaphore
// wait (Phase 55.12 follow-up). POSIX `sem_timedwait` does not share
// that code path and is safe.
#include <semaphore.h>
#include <time.h>
#include <errno.h>
static sem_t g_spin_sem_posix;
static bool g_spin_sem_initialized = false;

static inline void _zpico_notify_spin(void) {
    if (g_spin_sem_initialized) {
        // sem_post is async-signal-safe; binary-ish semantics are fine
        // because we only care that spin_once wakes at least once per
        // event, not that every post is counted.
        sem_post(&g_spin_sem_posix);
    }
}
#else
// POSIX/Zephyr: mutex + condvar
static _z_mutex_t g_spin_mutex;
static _z_condvar_t g_spin_cv;
static bool g_spin_cv_initialized = false;

static inline void _zpico_notify_spin(void) {
    if (g_spin_cv_initialized) {
        _z_mutex_lock(&g_spin_mutex);
        _z_condvar_signal(&g_spin_cv);
        _z_mutex_unlock(&g_spin_mutex);
    }
}
#endif // ZENOH_FREERTOS_LWIP / ZENOH_NUTTX

#else
static inline void _zpico_notify_spin(void) {}
#endif

// ============================================================================
// phase-279 (#145) — dedicated tx-flush thread. Flushing from the executor/
// tier threads measured WORSE-or-equal vs no batching: the flush blocks its
// caller on the socket for up to a recv window, stalling the very timers that
// generate the puts. On multi-threaded platforms a dedicated low-duty thread
// absorbs those waits instead; tier threads only ever append to the batch.
// ThreadX deliberately runs no background tasks (read-task starvation — see
// zpico_open), so it keeps the spin-driven flush. Single-threaded platforms
// have no tasks at all.
#if defined(ZPICO_TX_BATCH) && ZPICO_TX_BATCH == 1 && Z_FEATURE_MULTI_THREAD == 1 &&               \
    !defined(ZENOH_THREADX)
#define ZPICO_TX_BATCH_THREAD 1
#else
#define ZPICO_TX_BATCH_THREAD 0
#endif
#ifndef ZPICO_TX_BATCH_FLUSH_MS
#define ZPICO_TX_BATCH_FLUSH_MS 50
#endif

#if ZPICO_TX_BATCH_THREAD == 1
static _z_task_t g_tx_flush_task;
static volatile bool g_tx_flush_run = false;
/* phase-282 W4 (#145) — optional attributes for the flush task, set via
 * zpico_set_flush_task_config() before zpico_open(). Same platform matrix as
 * the read/lease task attrs: FreeRTOS + POSIX-like honour them, others
 * ignore the attr. */
static bool g_flush_task_configured = false;
static z_task_attr_t g_flush_task_attr;
static void* _zpico_tx_flush_task_fn(void* arg) {
    (void)arg;
    while (g_tx_flush_run) {
        zp_batch_flush(z_session_loan(&g_session));
        z_sleep_ms(ZPICO_TX_BATCH_FLUSH_MS);
    }
    return NULL;
}
#endif

// Task Configuration (set before zpico_open)
// ============================================================================

#if Z_FEATURE_MULTI_THREAD == 1
// Optional task attributes for read/lease tasks.  When non-NULL, passed to
// zp_start_read_task() / zp_start_lease_task() instead of NULL (platform default).
static bool g_read_task_configured = false;
static bool g_lease_task_configured = false;
static zp_task_read_options_t g_read_task_opts;
static zp_task_lease_options_t g_lease_task_opts;
static z_task_attr_t g_read_task_attr;
static z_task_attr_t g_lease_task_attr;
#endif

// ============================================================================
// Internal Helper Functions
// ============================================================================

static uint64_t zpico_splitmix64(uint64_t* state) {
    uint64_t z = (*state += UINT64_C(0x9e3779b97f4a7c15));
    z = (z ^ (z >> 30)) * UINT64_C(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)) * UINT64_C(0x94d049bb133111eb);
    return z ^ (z >> 31);
}

#if defined(ZENOH_FREERTOS_LWIP) || defined(ZENOH_THREADX)
static void zpico_mix_clock_bytes(uint64_t* seed, const void* data, size_t len) {
    const uint8_t* bytes = (const uint8_t*)data;
    for (size_t i = 0; i < len; i++) {
        *seed ^= (uint64_t)bytes[i] << ((i % sizeof(uint64_t)) * 8);
        *seed = zpico_splitmix64(seed);
    }
}
#endif

static uint32_t zpico_next_session_zid_counter(void) {
#if defined(ZPICO_SMOLTCP) || defined(ZPICO_SERIAL)
    return g_session_zid_counter++;
#else
    return atomic_fetch_add(&g_session_zid_counter, 1);
#endif
}

static void zpico_fill_session_zid(uint8_t bytes[ZPICO_ZID_SIZE]) {
#if defined(__linux__)
    if (getrandom(bytes, ZPICO_ZID_SIZE, 0) == ZPICO_ZID_SIZE) {
        return;
    }
#endif

#if defined(ZPICO_SMOLTCP) || defined(ZPICO_SERIAL)
    // Bare-metal/QEMU targets seed the platform RNG from board-specific
    // entropy before zpico_open(). Use that RNG directly; clock/address-only
    // fallback data is deterministic across separate QEMU processes and can
    // produce duplicate session ZIDs.
    z_random_fill(bytes, ZPICO_ZID_SIZE);
    return;
#endif

#if defined(ZENOH_ZEPHYR)
    sys_rand_get(bytes, ZPICO_ZID_SIZE);

    uint64_t zephyr_seed = k_cycle_get_64();
    zephyr_seed ^= (uint64_t)(uintptr_t)&g_session;
    zephyr_seed ^= (uint64_t)zpico_next_session_zid_counter();
#if defined(CONFIG_POSIX_MULTI_PROCESS)
    zephyr_seed ^= (uint64_t)getpid() << 32;
#endif
    for (size_t i = 0; i < ZPICO_ZID_SIZE; i += sizeof(uint64_t)) {
        uint64_t word = zpico_splitmix64(&zephyr_seed);
        for (size_t j = 0; j < sizeof(uint64_t) && i + j < ZPICO_ZID_SIZE; j++) {
            bytes[i + j] ^= (uint8_t)(word >> (j * 8));
        }
    }
    return;
#endif

    uint64_t seed = (uint64_t)(uintptr_t)&g_session;
    seed ^= (uint64_t)zpico_next_session_zid_counter();
#if defined(__linux__)
    seed ^= (uint64_t)getpid() << 32;
#endif
#if defined(ZPICO_SMOLTCP)
    seed ^= smoltcp_clock_now_ms() << 1;
    seed ^= z_random_u64();
    seed ^= z_random_u64() << 1;
#elif defined(ZPICO_SERIAL)
    seed ^= z_random_u64();
    seed ^= z_random_u64() << 1;
#elif defined(ZENOH_FREERTOS_LWIP) || defined(ZENOH_THREADX)
    z_clock_t now = z_clock_now();
    zpico_mix_clock_bytes(&seed, &now, sizeof(now));
    seed ^= z_random_u64();
    seed ^= z_random_u64() << 1;
#elif defined(CLOCK_REALTIME) && !defined(ZPICO_SERIAL)
    struct timespec ts;
    if (clock_gettime(CLOCK_REALTIME, &ts) == 0) {
        seed ^= (uint64_t)ts.tv_sec;
        seed ^= (uint64_t)ts.tv_nsec << 1;
    }
#endif

    for (size_t i = 0; i < ZPICO_ZID_SIZE; i += sizeof(uint64_t)) {
        uint64_t word = zpico_splitmix64(&seed);
        memcpy(&bytes[i], &word, sizeof(word));
    }
}

static void zpico_format_session_zid(char out[37], const uint8_t bytes[ZPICO_ZID_SIZE]) {
    static const char hex[] = "0123456789abcdef";
    size_t j = 0;
    for (size_t i = 0; i < ZPICO_ZID_SIZE; i++) {
        if (j == 8 || j == 13 || j == 18 || j == 21) {
            out[j++] = '-';
        }
        out[j++] = hex[bytes[i] >> 4];
        out[j++] = hex[bytes[i] & 0x0f];
    }
    out[j] = '\0';
}

/**
 * Internal callback for queryable that receives queries
 */
static void query_handler(z_loaned_query_t* query, void* arg) {
    int idx = (int)(intptr_t)arg;
    if (idx < 0 || idx >= ZPICO_MAX_QUERYABLES) {
        return;
    }

    queryable_entry_t* entry = &g_queryables[idx];
    if (!entry->active || entry->callback == NULL) {
        return;
    }

    // Get keyexpr
    const z_loaned_keyexpr_t* keyexpr = z_query_keyexpr(query);
    z_view_string_t keyexpr_view;
    z_keyexpr_as_view_string(keyexpr, &keyexpr_view);
    const char* keyexpr_str = z_string_data(z_view_string_loan(&keyexpr_view));
    size_t keyexpr_len = z_string_len(z_view_string_loan(&keyexpr_view));

    // Get payload
    const z_loaned_bytes_t* payload_bytes = z_query_payload(query);
    const uint8_t* payload_data = NULL;
    size_t payload_len = 0;

    z_owned_slice_t payload_slice;
    if (payload_bytes != NULL && z_bytes_len(payload_bytes) > 0) {
        if (z_bytes_to_slice(payload_bytes, &payload_slice) == 0) {
            payload_data = z_slice_data(z_slice_loan(&payload_slice));
            payload_len = z_slice_len(z_slice_loan(&payload_slice));
        }
    }

    // Phase 237 — allocate a free reply slot and clone the query into it so the
    // reply can be sent after this callback returns (deferred get_result) even
    // if more queries land meanwhile. Record the slot index as the reply seq for
    // `zpico_queryable_take_reply_seq` to hand to the (synchronous) callback.
    int64_t reply_seq = -1;
    for (int j = 0; j < ZPICO_MAX_PENDING_REPLIES; ++j) {
        if (!g_stored_query_valid[idx][j]) {
            if (z_query_clone(&g_stored_queries[idx][j], query) == 0) {
                g_stored_query_valid[idx][j] = true;
                reply_seq = j;
            }
            break;
        }
    }
    g_last_reply_seq[idx] = reply_seq;

    // Call user callback
    entry->callback(keyexpr_str, keyexpr_len, payload_data, payload_len, entry->ctx);

    // Clean up slice
    if (payload_data != NULL) {
        z_slice_drop(z_slice_move(&payload_slice));
    }
    _zpico_notify_spin();
}

/**
 * Internal callback that receives zenoh samples and forwards to user callback.
 *
 * Supports three modes:
 * - direct_write: reads payload directly into Rust buffer via z_bytes_reader_read()
 * - with_attachment: copies payload via z_bytes_to_slice() (legacy path)
 * - legacy: copies payload only via z_bytes_to_slice()
 */
static void sample_handler(z_loaned_sample_t* sample, void* arg) {
    int idx = (int)(intptr_t)arg;
    if (idx < 0 || idx >= ZPICO_MAX_SUBSCRIBERS) {
        return;
    }

    subscriber_entry_t* entry = &g_subscribers[idx];
    if (!entry->active) {
        return;
    }

    // Get payload
    const z_loaned_bytes_t* payload = z_sample_payload(sample);
    size_t payload_len = z_bytes_len(payload);

#if defined(Z_FEATURE_UNSTABLE_API)
    if (entry->zero_copy) {
        if (entry->zero_copy_cb == NULL) {
            return;
        }
        // Get contiguous view — borrows directly from zenoh-pico's receive buffer
        z_view_slice_t view;
        if (z_bytes_get_contiguous_view(payload, &view) == 0) {
            const uint8_t* data = z_slice_data(z_view_slice_loan(&view));
            size_t len = z_slice_len(z_view_slice_loan(&view));

            // Get attachment (small copy, 33-37 bytes)
            const z_loaned_bytes_t* att = z_sample_attachment(sample);
            if (att != NULL) {
                z_owned_slice_t att_slice;
                if (z_bytes_to_slice(att, &att_slice) == 0) {
                    entry->zero_copy_cb(data, len, z_slice_data(z_slice_loan(&att_slice)),
                                        z_slice_len(z_slice_loan(&att_slice)), entry->ctx);
                    z_slice_drop(z_slice_move(&att_slice));
                } else {
                    entry->zero_copy_cb(data, len, NULL, 0, entry->ctx);
                }
            } else {
                entry->zero_copy_cb(data, len, NULL, 0, entry->ctx);
            }
        }
        _zpico_notify_spin();
        return;
    }
#endif

    if (entry->ring_mode) {
        // Phase 124.D.3.c — SPSC ring producer path. C is the sole
        // writer of `tail`; Rust the sole writer of `head`.
        if (entry->notify == NULL || entry->ring == NULL) {
            return;
        }
        // Drop empty-payload samples — zenoh-pico delivers background
        // probes / liveliness syncs through the regular subscription
        // path with a zero-length payload. Buffering them would let
        // the typed `try_recv()` consume a slot whose CDR header check
        // then fails. Mirrors the legacy single-slot behaviour.
        if (payload_len == 0) {
            return;
        }
        zpico_ring_desc_t* r = entry->ring;

        // Acquire-load head (published by the Rust consumer) and a
        // relaxed-load of our own tail. Ring full when the gap is
        // slot_count — drop the newest message (matches DDS
        // KEEP_LAST overwrite-from-the-front intent loosely; a
        // dropped burst tail is reported via msg-lost accounting on
        // the Rust side from the sequence gap).
        uintptr_t head =
            atomic_load_explicit((const _Atomic uintptr_t*)r->head, memory_order_acquire);
        uintptr_t tail = atomic_load_explicit((_Atomic uintptr_t*)r->tail, memory_order_relaxed);
        if (tail - head >= r->slot_count) {
            // Ring full — drop. Still fire notify(len) so the Rust
            // side can observe the arrival for waker / lost-count.
            entry->notify(payload_len, NULL, 0, entry->ctx);
            _zpico_notify_spin();
            return;
        }

        uintptr_t slot = tail % r->slot_count;
        uint8_t* pay_dst = r->payload_base + slot * r->payload_stride;
        if (payload_len > r->payload_stride) {
            // Slot too small — report overflow via notify, don't
            // advance tail.
            entry->notify(payload_len, NULL, 0, entry->ctx);
            return;
        }
        z_bytes_reader_t reader = z_bytes_get_reader(payload);
        z_bytes_reader_read(&reader, pay_dst, payload_len);
        r->payload_len[slot] = payload_len;

        // Attachment into the parallel per-slot array.
        size_t att_written = 0;
        const z_loaned_bytes_t* attachment = z_sample_attachment(sample);
        if (attachment != NULL && r->att_stride > 0) {
            size_t att_len = z_bytes_len(attachment);
            if (att_len <= r->att_stride) {
                z_bytes_reader_t att_reader = z_bytes_get_reader(attachment);
                att_written =
                    z_bytes_reader_read(&att_reader, r->att_base + slot * r->att_stride, att_len);
            }
        }
        r->att_len[slot] = att_written;

        // Publish the slot: Release store advances tail so the Rust
        // consumer sees the payload + len writes above.
        atomic_store_explicit((_Atomic uintptr_t*)r->tail, tail + 1, memory_order_release);

        // Fire notify for the async waker. Pass NULL attachment —
        // the consumer reads the per-slot attachment array directly.
        entry->notify(payload_len, NULL, 0, entry->ctx);
        _zpico_notify_spin();
        return;
    }

    if (entry->direct_write) {
        // Direct-write mode: read payload directly into Rust static buffer
        if (entry->notify == NULL) {
            return;
        }

        // Check lock (Rust reader is processing)
        if (__atomic_load_n(entry->locked_ptr, __ATOMIC_ACQUIRE)) {
            return;
        }

        if (payload_len > entry->buf_capacity) {
            // Overflow: notify with len so Rust can set overflow flag
            entry->notify(payload_len, NULL, 0, entry->ctx);
            return;
        }

        // Read directly into Rust's static buffer — no malloc
        z_bytes_reader_t reader = z_bytes_get_reader(payload);
        z_bytes_reader_read(&reader, entry->buf_ptr, payload_len);

        // Attachment still uses z_bytes_to_slice (33-37 bytes, negligible)
        const z_loaned_bytes_t* attachment = z_sample_attachment(sample);
        if (attachment != NULL) {
            z_owned_slice_t attachment_slice;
            if (z_bytes_to_slice(attachment, &attachment_slice) == 0) {
                const uint8_t* att_data = z_slice_data(z_slice_loan(&attachment_slice));
                size_t att_len = z_slice_len(z_slice_loan(&attachment_slice));
                entry->notify(payload_len, att_data, att_len, entry->ctx);
                z_slice_drop(z_slice_move(&attachment_slice));
            } else {
                entry->notify(payload_len, NULL, 0, entry->ctx);
            }
        } else {
            entry->notify(payload_len, NULL, 0, entry->ctx);
        }
        _zpico_notify_spin();
        return;
    }

    // Legacy path: copy payload to owned slice (malloc + memcpy)
    z_owned_slice_t payload_slice;
    if (z_bytes_to_slice(payload, &payload_slice) != 0) {
        return; // Failed to get payload
    }

    const uint8_t* data = z_slice_data(z_slice_loan(&payload_slice));
    size_t len = z_slice_len(z_slice_loan(&payload_slice));

    if (entry->with_attachment) {
        // Extended callback with attachment
        if (entry->callback_ext == NULL) {
            z_slice_drop(z_slice_move(&payload_slice));
            return;
        }

        // Get attachment
        const z_loaned_bytes_t* attachment = z_sample_attachment(sample);
        if (attachment != NULL) {
            z_owned_slice_t attachment_slice;
            if (z_bytes_to_slice(attachment, &attachment_slice) == 0) {
                const uint8_t* att_data = z_slice_data(z_slice_loan(&attachment_slice));
                size_t att_len = z_slice_len(z_slice_loan(&attachment_slice));
                entry->callback_ext(data, len, att_data, att_len, entry->ctx);
                z_slice_drop(z_slice_move(&attachment_slice));
            } else {
                // Attachment exists but failed to convert - call with NULL attachment
                entry->callback_ext(data, len, NULL, 0, entry->ctx);
            }
        } else {
            // No attachment
            entry->callback_ext(data, len, NULL, 0, entry->ctx);
        }
    } else {
        // Legacy callback (payload only)
        if (entry->callback != NULL) {
            entry->callback(data, len, entry->ctx);
        }
    }

    z_slice_drop(z_slice_move(&payload_slice));
    _zpico_notify_spin();
}

// ============================================================================
// Session Lifecycle Implementation
// ============================================================================

int32_t zpico_init(const char* locator) {
    return zpico_init_with_config(locator, "client", NULL, 0);
}

int32_t zpico_init_with_config(const char* locator, const char* mode,
                               const zpico_property_t* properties, size_t num_properties) {
    // Initialize storage
    memset(g_publishers, 0, sizeof(g_publishers));
    memset(g_subscribers, 0, sizeof(g_subscribers));
    memset(g_liveliness, 0, sizeof(g_liveliness));
    memset(g_queryables, 0, sizeof(g_queryables));
    memset(g_stored_query_valid, 0, sizeof(g_stored_query_valid));
    for (int i = 0; i < ZPICO_MAX_QUERYABLES; i++) {
        g_last_reply_seq[i] = -1;
        for (int j = 0; j < ZPICO_MAX_PENDING_REPLIES; j++) {
            z_internal_query_null(&g_stored_queries[i][j]);
        }
    }
    g_session_open = false;

#ifdef ZPICO_SMOLTCP
    // Initialize smoltcp platform
    int ret = smoltcp_init();
    if (ret < 0) {
        return ZPICO_ERR_GENERIC;
    }
#endif

    // Initialize zenoh config
    z_config_default(&g_config);

    if (zp_config_insert(z_config_loan_mut(&g_config), Z_CONFIG_MODE_KEY, mode) < 0) {
        return ZPICO_ERR_CONFIG;
    }

    bool has_session_zid = false;
    for (size_t i = 0; i < num_properties; i++) {
        if (properties[i].key != NULL && strcmp(properties[i].key, "session_zid") == 0) {
            has_session_zid = true;
            break;
        }
    }
    if (!has_session_zid) {
        uint8_t zid_bytes[ZPICO_ZID_SIZE];
        char zid[37];
        zpico_fill_session_zid(zid_bytes);
        zpico_format_session_zid(zid, zid_bytes);
        if (zp_config_insert(z_config_loan_mut(&g_config), Z_CONFIG_SESSION_ZID_KEY, zid) < 0) {
            return ZPICO_ERR_CONFIG;
        }
    }

    // Apply additional properties
    for (size_t i = 0; i < num_properties; i++) {
        if (properties[i].key == NULL || properties[i].value == NULL) {
            continue;
        }

        uint8_t config_key;
        if (strcmp(properties[i].key, "multicast_scouting") == 0) {
            config_key = Z_CONFIG_MULTICAST_SCOUTING_KEY;
        } else if (strcmp(properties[i].key, "scouting_timeout_ms") == 0) {
            config_key = Z_CONFIG_SCOUTING_TIMEOUT_KEY;
        } else if (strcmp(properties[i].key, "multicast_locator") == 0) {
            config_key = Z_CONFIG_MULTICAST_LOCATOR_KEY;
        } else if (strcmp(properties[i].key, "listen") == 0) {
            config_key = Z_CONFIG_LISTEN_KEY;
        } else if (strcmp(properties[i].key, "add_timestamp") == 0) {
            config_key = Z_CONFIG_ADD_TIMESTAMP_KEY;
        } else if (strcmp(properties[i].key, "session_zid") == 0) {
            config_key = Z_CONFIG_SESSION_ZID_KEY;
#if Z_FEATURE_LINK_TLS == 1
        } else if (strcmp(properties[i].key, "root_ca_certificate") == 0) {
            config_key = Z_CONFIG_TLS_ROOT_CA_CERTIFICATE_KEY;
        } else if (strcmp(properties[i].key, "root_ca_certificate_base64") == 0) {
            config_key = Z_CONFIG_TLS_ROOT_CA_CERTIFICATE_BASE64_KEY;
        } else if (strcmp(properties[i].key, "verify_name_on_connect") == 0) {
            config_key = Z_CONFIG_TLS_VERIFY_NAME_ON_CONNECT_KEY;
#endif
        } else {
            // Unknown key — silently ignore
            continue;
        }

        if (zp_config_insert(z_config_loan_mut(&g_config), config_key, properties[i].value) < 0) {
            return ZPICO_ERR_CONFIG;
        }
    }

    // Insert connect endpoint after link properties so TLS endpoint parsing can
    // see root CA / verification settings supplied through session config.
    if (locator != NULL) {
        if (zp_config_insert(z_config_loan_mut(&g_config), Z_CONFIG_CONNECT_KEY, locator) < 0) {
            return ZPICO_ERR_CONFIG;
        }
    }

    g_initialized = true;
    return ZPICO_OK;
}

void zpico_set_task_config(uint32_t read_priority, uint32_t read_stack_bytes,
                           uint32_t lease_priority, uint32_t lease_stack_bytes) {
#if Z_FEATURE_MULTI_THREAD == 1
    memset(&g_read_task_attr, 0, sizeof(g_read_task_attr));
    memset(&g_lease_task_attr, 0, sizeof(g_lease_task_attr));

    // Platform-specific field assignment.
    // z_task_attr_t varies by platform — only FreeRTOS and POSIX-like
    // platforms have meaningful fields. ThreadX and generic use void*
    // and zenoh-pico ignores the attr entirely on those platforms.
#if defined(ZENOH_FREERTOS) || defined(ZENOH_FREERTOS_LWIP)
    g_read_task_attr.name = "zpico_read";
    g_read_task_attr.priority = (UBaseType_t)read_priority;
    g_read_task_attr.stack_depth = read_stack_bytes / sizeof(StackType_t);
    g_lease_task_attr.name = "zpico_lease";
    g_lease_task_attr.priority = (UBaseType_t)lease_priority;
    g_lease_task_attr.stack_depth = lease_stack_bytes / sizeof(StackType_t);
    g_read_task_opts.task_attributes = &g_read_task_attr;
    g_lease_task_opts.task_attributes = &g_lease_task_attr;
    g_read_task_configured = true;
    g_lease_task_configured = true;
#elif (defined(ZENOH_LINUX) || defined(ZENOH_MACOS) || defined(__NuttX__) ||                       \
       defined(ZENOH_ZEPHYR)) &&                                                                   \
    !defined(ZENOH_THREADX)
    // POSIX: set stack size via pthread_attr. Priority requires SCHED_FIFO
    // (root privileges); for now only stack size is configurable.
    pthread_attr_init(&g_read_task_attr);
    pthread_attr_setstacksize(&g_read_task_attr, (size_t)read_stack_bytes);
    pthread_attr_init(&g_lease_task_attr);
    pthread_attr_setstacksize(&g_lease_task_attr, (size_t)lease_stack_bytes);
    g_read_task_opts.task_attributes = &g_read_task_attr;
    g_lease_task_opts.task_attributes = &g_lease_task_attr;
    g_read_task_configured = true;
    g_lease_task_configured = true;
    (void)read_priority;
    (void)lease_priority;
#else
    // ThreadX, generic, and other platforms: z_task_attr_t is void* and
    // zenoh-pico ignores it. Config stored for future platform support.
    (void)read_priority;
    (void)read_stack_bytes;
    (void)lease_priority;
    (void)lease_stack_bytes;
#endif
#else
    (void)read_priority;
    (void)read_stack_bytes;
    (void)lease_priority;
    (void)lease_stack_bytes;
#endif
}

void zpico_set_flush_task_config(uint32_t priority, uint32_t stack_bytes) {
#if ZPICO_TX_BATCH_THREAD == 1
    memset(&g_flush_task_attr, 0, sizeof(g_flush_task_attr));
#if defined(ZENOH_FREERTOS) || defined(ZENOH_FREERTOS_LWIP)
    g_flush_task_attr.name = "zpico_flush";
    g_flush_task_attr.priority = (UBaseType_t)priority;
    g_flush_task_attr.stack_depth = stack_bytes / sizeof(StackType_t);
    g_flush_task_configured = true;
#elif (defined(ZENOH_LINUX) || defined(ZENOH_MACOS) || defined(__NuttX__) ||                       \
       defined(ZENOH_ZEPHYR)) &&                                                                   \
    !defined(ZENOH_THREADX)
    /* POSIX: stack size via pthread_attr; priority needs SCHED_FIFO (root),
     * so only the stack size is applied — mirrors zpico_set_task_config. */
    pthread_attr_init(&g_flush_task_attr);
    pthread_attr_setstacksize(&g_flush_task_attr, (size_t)stack_bytes);
    g_flush_task_configured = true;
    (void)priority;
#else
    (void)priority;
    (void)stack_bytes;
#endif
#else
    /* No flush thread in this build (batching off / single-threaded /
     * ThreadX): nothing to configure. */
    (void)priority;
    (void)stack_bytes;
#endif
}

int32_t zpico_open(void) {
    if (!g_initialized) {
        return ZPICO_ERR_GENERIC;
    }

    z_open_options_t open_opts;
    z_open_options_default(&open_opts);
#if Z_FEATURE_MULTI_THREAD == 1
    open_opts.auto_start_read_task = false;
    open_opts.auto_start_lease_task = false;
#endif
    int open_ret = z_open(&g_session, z_config_move(&g_config), &open_opts);
    if (open_ret < 0) {
        return ZPICO_ERR_SESSION;
    }

#ifdef ZPICO_SERIAL
    // Switch serial reads from blocking (needed for z_open handshake) to
    // non-blocking so zpico_spin_once doesn't block for 5s on idle iterations.
    extern void zpico_serial_set_nonblocking(void);
    zpico_serial_set_nonblocking();
#endif
    {
        z_id_t zid = z_info_zid(z_session_loan(&g_session));
        (void)zid;
    }

    // Start background tasks only in multi-threaded mode. ThreadX/NSOS is an
    // exception: its blocking BSD recv path can keep the read task runnable
    // long enough to starve the lease task, so zpico_spin_once() drives reads
    // and keepalives explicitly for that platform.
#if Z_FEATURE_MULTI_THREAD == 1 && !defined(ZENOH_THREADX)
#if defined(ZENOH_FREERTOS_LWIP)
    g_spin_sem = xSemaphoreCreateBinary();
#elif defined(ZENOH_NUTTX)
    if (sem_init(&g_spin_sem_posix, 0, 0) == 0) {
        g_spin_sem_initialized = true;
    }
#elif !defined(ZPICO_SMOLTCP)
    _z_mutex_init(&g_spin_mutex);
    _z_condvar_init(&g_spin_cv);
    g_spin_cv_initialized = true;
#endif

    const zp_task_read_options_t* read_opts = g_read_task_configured ? &g_read_task_opts : NULL;
    const zp_task_lease_options_t* lease_opts = g_lease_task_configured ? &g_lease_task_opts : NULL;

    if (zp_start_read_task(z_session_loan_mut(&g_session), read_opts) < 0) {
        z_close(z_session_loan_mut(&g_session), NULL);
        return ZPICO_ERR_TASK;
    }

    if (zp_start_lease_task(z_session_loan_mut(&g_session), lease_opts) < 0) {
        zp_stop_read_task(z_session_loan_mut(&g_session));
        z_close(z_session_loan_mut(&g_session), NULL);
        return ZPICO_ERR_TASK;
    }
#endif

#if defined(ZPICO_TX_BATCH) && ZPICO_TX_BATCH == 1
    /* phase-279 (#145) — opt-in tx batching: puts/gets append to the transport
     * write buffer instead of sending; one socket send per flush carries the
     * whole batch. Flush cadence = zpico_spin_once (every executor spin) +
     * zenoh-pico's own implicit flushes (batch-buffer overflow, any transport
     * message — so the lease keepalive bounds batch sit-time even without
     * spins). Express messages (query replies, gets, express publishers)
     * bypass the batch inside zenoh-pico. Compile-time knob, default OFF. */
    zp_batch_start(z_session_loan(&g_session));
#if ZPICO_TX_BATCH_THREAD == 1
    g_tx_flush_run = true;
    if (_z_task_init(&g_tx_flush_task, g_flush_task_configured ? &g_flush_task_attr : NULL,
                     _zpico_tx_flush_task_fn, NULL) != 0) {
        /* No thread → flushes ride only on implicit sends (keepalives bound
         * sit-time to the lease interval). Loud, not fatal. */
        g_tx_flush_run = false;
        printk("zpico: tx-flush task init FAILED — batched puts flush on keepalives only\n");
    }
#endif
#endif

    g_session_open = true;
    return ZPICO_OK;
}

int32_t zpico_is_open(void) {
    return g_session_open ? 1 : 0;
}

/**
 * Phase 124.E.3 — streamed publish.
 *
 * Drives zenoh-pico's `z_bytes_writer` API to assemble the payload
 * chunk-by-chunk inside zenoh's `z_owned_bytes_t` (a refcounted
 * bytes object backed by zenoh-pico's allocator). The caller's
 * `size_cb` reports the total length once, then `chunk_cb` is
 * invoked repeatedly with cursor-into-staging buffers of up to
 * 1 KiB each. Each chunk is appended to the bytes object via
 * `z_bytes_writer_write_all`, then `z_publisher_put` ships the
 * assembled payload.
 *
 * The win over user-side `publish_raw(staging_buffer)`: the chunks
 * land directly in zenoh's allocator-managed `z_owned_bytes_t`
 * rather than first into a caller-owned `[u8; N]` stack array.
 * For a 32 KiB message, that's 32 KiB less stack pressure on the
 * publishing task.
 */
int32_t zpico_publish_streamed(int32_t handle, size_t total_len,
                               void (*chunk_cb)(uint8_t* out_buf, size_t cap, size_t* out_written,
                                                void* user_ctx),
                               void* user_ctx, const uint8_t* attachment, size_t attachment_len) {
    if (handle < 0 || handle >= ZPICO_MAX_PUBLISHERS || !g_publishers[handle].active) {
        return ZPICO_ERR_INVALID;
    }
    if (chunk_cb == NULL) {
        return ZPICO_ERR_INVALID;
    }

    z_owned_bytes_writer_t writer;
    if (z_bytes_writer_empty(&writer) < 0) {
        return ZPICO_ERR_GENERIC;
    }

    /* Chunk buffer is fixed at 1 KiB. Trades publish_streamed
     * memory cost for chunk_cb invocation count. 1 KiB is the
     * smallest aligned size that still gives the caller meaningful
     * batching latitude — at 128 B you'd hit the callback ~250
     * times for a 32 KiB message. */
    uint8_t chunk[1024];
    size_t written_so_far = 0;
    while (written_so_far < total_len) {
        size_t want = total_len - written_so_far;
        if (want > sizeof(chunk)) {
            want = sizeof(chunk);
        }
        size_t actually_written = 0;
        chunk_cb(chunk, want, &actually_written, user_ctx);
        if (actually_written == 0) {
            /* EOF before total_len — abort the writer so we don't
             * publish a truncated payload. */
            z_bytes_writer_drop(z_bytes_writer_move(&writer));
            return ZPICO_ERR_PUBLISH;
        }
        if (actually_written > want) {
            actually_written = want;
        }
        if (z_bytes_writer_write_all(z_bytes_writer_loan_mut(&writer), chunk, actually_written) <
            0) {
            z_bytes_writer_drop(z_bytes_writer_move(&writer));
            return ZPICO_ERR_PUBLISH;
        }
        written_so_far += actually_written;
    }

    z_owned_bytes_t payload;
    z_bytes_writer_finish(z_bytes_writer_move(&writer), &payload);

    z_publisher_put_options_t opts;
    z_publisher_put_options_default(&opts);

    /* ROS interop attachment (sequence number + source timestamp +
     * GID) — same shape `zpico_publish_with_attachment` builds. */
    z_owned_bytes_t attachment_bytes;
    if (attachment != NULL && attachment_len > 0) {
        if (z_bytes_copy_from_buf(&attachment_bytes, attachment, attachment_len) < 0) {
            z_bytes_drop(z_bytes_move(&payload));
            return ZPICO_ERR_PUBLISH;
        }
        opts.attachment = z_bytes_move(&attachment_bytes);
    }

    if (z_publisher_put(z_publisher_loan(&g_publishers[handle].publisher), z_bytes_move(&payload),
                        &opts) < 0) {
        return ZPICO_ERR_PUBLISH;
    }

    return ZPICO_OK;
}

/**
 * Phase 124.F.2 — wire-level "is the agent still reachable?" probe.
 *
 * Issues one `zp_send_keep_alive` against the open session. On
 * zenoh-pico that's the closest match to a true ping primitive:
 * the function returns success when the keep-alive frame fired
 * down the transport layer, and a negative `z_result_t` when the
 * TCP send (or serial / shared-memory equivalent) reports a dead
 * link. Best-effort: a fresh-link silent failure (peer disappeared
 * but the OS hasn't reported the socket as half-closed) will still
 * report OK until the next send-side timeout.
 *
 * Returns ZPICO_OK on success, ZPICO_ERR_SESSION when no session
 * is open, ZPICO_ERR_TIMEOUT when the keep-alive failed (treated
 * as a probe timeout per the 124.F.1 semantics).
 */
int32_t zpico_send_keep_alive(void) {
    if (!g_session_open) {
        return ZPICO_ERR_SESSION;
    }
    zp_send_keep_alive_options_t options;
    zp_send_keep_alive_options_default(&options);
    z_result_t ret = zp_send_keep_alive(z_session_loan(&g_session), &options);
    if (ret < 0) {
        return ZPICO_ERR_TIMEOUT;
    }
    return ZPICO_OK;
}

void zpico_close(void) {
    // Clean up publishers
    for (int i = 0; i < ZPICO_MAX_PUBLISHERS; i++) {
        if (g_publishers[i].active) {
            z_undeclare_publisher(z_publisher_move(&g_publishers[i].publisher));
            g_publishers[i].active = false;
        }
    }

    // Clean up subscribers
    for (int i = 0; i < ZPICO_MAX_SUBSCRIBERS; i++) {
        if (g_subscribers[i].active) {
            z_undeclare_subscriber(z_subscriber_move(&g_subscribers[i].subscriber));
            g_subscribers[i].active = false;
            g_subscribers[i].callback = NULL;
            g_subscribers[i].ctx = NULL;
        }
    }

    // Clean up liveliness tokens
    for (int i = 0; i < ZPICO_MAX_LIVELINESS; i++) {
        if (g_liveliness[i].active) {
            z_liveliness_undeclare_token(z_liveliness_token_move(&g_liveliness[i].token));
            g_liveliness[i].active = false;
        }
    }

    // Clean up queryables
    for (int i = 0; i < ZPICO_MAX_QUERYABLES; i++) {
        if (g_queryables[i].active) {
            z_undeclare_queryable(z_queryable_move(&g_queryables[i].queryable));
            g_queryables[i].active = false;
            g_queryables[i].callback = NULL;
            g_queryables[i].ctx = NULL;
        }
    }

    // Close session
    if (g_session_open) {
#if defined(ZPICO_TX_BATCH) && ZPICO_TX_BATCH == 1
#if ZPICO_TX_BATCH_THREAD == 1
        if (g_tx_flush_run) {
            g_tx_flush_run = false;
            _z_task_join(&g_tx_flush_task);
        }
#endif
        /* phase-279 (#145) — stop batching; zp_batch_stop flushes the remainder. */
        zp_batch_stop(z_session_loan(&g_session));
#endif
#if Z_FEATURE_MULTI_THREAD == 1
        // Stop background tasks (only in multi-threaded mode)
        zp_stop_read_task(z_session_loan_mut(&g_session));
        zp_stop_lease_task(z_session_loan_mut(&g_session));

#if defined(ZENOH_FREERTOS_LWIP)
        if (g_spin_sem != NULL) {
            vSemaphoreDelete(g_spin_sem);
            g_spin_sem = NULL;
        }
#elif defined(ZENOH_NUTTX)
        if (g_spin_sem_initialized) {
            g_spin_sem_initialized = false;
            sem_destroy(&g_spin_sem_posix);
        }
#elif !defined(ZPICO_SMOLTCP)
        g_spin_cv_initialized = false;
        _z_condvar_drop(&g_spin_cv);
        _z_mutex_drop(&g_spin_mutex);
#endif
#endif
        z_close(z_session_loan_mut(&g_session), NULL);
        g_session_open = false;
    }

#ifdef ZPICO_SMOLTCP
    // Cleanup smoltcp platform
    smoltcp_cleanup();
#endif

    g_initialized = false;
}

// ============================================================================
// Publisher Implementation
// ============================================================================

int32_t zpico_declare_publisher(const char* keyexpr) {
    return zpico_declare_publisher_ex(keyexpr, 0);
}

int32_t zpico_declare_publisher_ex(const char* keyexpr, int32_t is_express) {
    if (!g_session_open) {
        return ZPICO_ERR_SESSION;
    }

    // Find free slot
    int idx = -1;
    for (int i = 0; i < ZPICO_MAX_PUBLISHERS; i++) {
        if (!g_publishers[i].active) {
            idx = i;
            break;
        }
    }
    if (idx < 0) {
        return ZPICO_ERR_FULL;
    }

    z_view_keyexpr_t ke;
    int ke_ret = z_view_keyexpr_from_str(&ke, keyexpr);
    if (ke_ret < 0) {
        return ZPICO_ERR_KEYEXPR;
    }

    z_publisher_options_t pub_opts;
    z_publisher_options_default(&pub_opts);
    /* phase-279 (#145) — express publishers bypass tx batching inside zenoh-pico
     * (sent immediately, wire EXPRESS flag). Surfaced from TopicInfo::tx_express
     * through NrosRmwQos; harmless without batching. */
    pub_opts.is_express = (is_express != 0);
#if defined(ZPICO_TX_BATCH) && ZPICO_TX_BATCH == 1
    /* Batching queues puts behind the transport tx mutex; the default DROP
     * congestion control TRY-locks it and silently discards the put whenever a
     * flush is mid-send (waiting on the socket) — measured WORSE than no
     * batching (4.7 vs 8.6 msg/s). BLOCK = append-or-wait: every put lands in
     * the batch and ships with the next flush. Declare-time option (per-put
     * options carry no congestion control in zenoh-pico). */
    pub_opts.congestion_control = Z_CONGESTION_CONTROL_BLOCK;
#endif
    int pub_ret = z_declare_publisher(z_session_loan(&g_session), &g_publishers[idx].publisher,
                                      z_view_keyexpr_loan(&ke), &pub_opts);
    if (pub_ret < 0) {
        printk("zpico: z_declare_publisher failed: %d for '%s'\n", pub_ret, keyexpr);
        return ZPICO_ERR_GENERIC;
    }

    g_publishers[idx].active = true;
    return idx;
}

int32_t zpico_publish(int32_t handle, const uint8_t* data, size_t len) {
    if (handle < 0 || handle >= ZPICO_MAX_PUBLISHERS || !g_publishers[handle].active) {
        return ZPICO_ERR_INVALID;
    }

    z_owned_bytes_t payload;
    int bytes_ret = z_bytes_copy_from_buf(&payload, data, len);
    if (bytes_ret < 0) {
        printk("zpico: z_bytes_copy_from_buf failed: %d\n", bytes_ret);
        return ZPICO_ERR_PUBLISH;
    }

    int put_ret = z_publisher_put(z_publisher_loan(&g_publishers[handle].publisher),
                                  z_bytes_move(&payload), NULL);
    if (put_ret < 0) {
        printk("zpico: z_publisher_put failed: %d\n", put_ret);
        return ZPICO_ERR_PUBLISH;
    }

    return ZPICO_OK;
}

int32_t zpico_undeclare_publisher(int32_t handle) {
    if (handle < 0 || handle >= ZPICO_MAX_PUBLISHERS || !g_publishers[handle].active) {
        return ZPICO_ERR_INVALID;
    }

    z_undeclare_publisher(z_publisher_move(&g_publishers[handle].publisher));
    g_publishers[handle].active = false;
    return ZPICO_OK;
}

// ============================================================================
// Subscriber Implementation
// ============================================================================

int32_t zpico_declare_subscriber(const char* keyexpr, ZpicoCallback callback, void* ctx) {
    if (!g_session_open) {
        return ZPICO_ERR_SESSION;
    }

    // Find free slot
    int idx = -1;
    for (int i = 0; i < ZPICO_MAX_SUBSCRIBERS; i++) {
        if (!g_subscribers[i].active) {
            idx = i;
            break;
        }
    }
    if (idx < 0) {
        return ZPICO_ERR_FULL;
    }

    g_subscribers[idx].callback = callback;
    g_subscribers[idx].ctx = ctx;
    g_subscribers[idx].with_attachment = false; // Legacy mode

    z_view_keyexpr_t ke;
    if (z_view_keyexpr_from_str(&ke, keyexpr) < 0) {
        g_subscribers[idx].callback = NULL;
        g_subscribers[idx].ctx = NULL;
        return ZPICO_ERR_KEYEXPR;
    }

    // Create closure for callback, passing index as context
    z_owned_closure_sample_t closure;
    z_closure_sample(&closure, sample_handler, NULL, (void*)(intptr_t)idx);

    int sub_ret =
        z_declare_subscriber(z_session_loan(&g_session), &g_subscribers[idx].subscriber,
                             z_view_keyexpr_loan(&ke), z_closure_sample_move(&closure), NULL);
    if (sub_ret < 0) {
        printk("zpico: z_declare_subscriber failed: %d for '%s'\n", sub_ret, keyexpr);
        g_subscribers[idx].callback = NULL;
        g_subscribers[idx].ctx = NULL;
        return ZPICO_ERR_GENERIC;
    }

    g_subscribers[idx].active = true;
    return idx;
}

int32_t zpico_declare_subscriber_with_attachment(const char* keyexpr,
                                                 ZpicoCallbackWithAttachment callback, void* ctx) {
    if (!g_session_open) {
        return ZPICO_ERR_SESSION;
    }

    // Find free slot
    int idx = -1;
    for (int i = 0; i < ZPICO_MAX_SUBSCRIBERS; i++) {
        if (!g_subscribers[i].active) {
            idx = i;
            break;
        }
    }
    if (idx < 0) {
        return ZPICO_ERR_FULL;
    }

    g_subscribers[idx].callback_ext = callback;
    g_subscribers[idx].ctx = ctx;
    g_subscribers[idx].with_attachment = true; // Extended mode with attachment

    z_view_keyexpr_t ke;
    if (z_view_keyexpr_from_str(&ke, keyexpr) < 0) {
        g_subscribers[idx].callback_ext = NULL;
        g_subscribers[idx].ctx = NULL;
        return ZPICO_ERR_KEYEXPR;
    }

    // Create closure for callback, passing index as context
    z_owned_closure_sample_t closure;
    z_closure_sample(&closure, sample_handler, NULL, (void*)(intptr_t)idx);

    int sub_ret =
        z_declare_subscriber(z_session_loan(&g_session), &g_subscribers[idx].subscriber,
                             z_view_keyexpr_loan(&ke), z_closure_sample_move(&closure), NULL);
    if (sub_ret < 0) {
        printk("zpico: z_declare_subscriber failed: %d for '%s'\n", sub_ret, keyexpr);
        g_subscribers[idx].callback_ext = NULL;
        g_subscribers[idx].ctx = NULL;
        return ZPICO_ERR_GENERIC;
    }

    g_subscribers[idx].active = true;
    return idx;
}

int32_t zpico_declare_subscriber_direct_write(const char* keyexpr, uint8_t* buf_ptr,
                                              size_t buf_capacity, const bool* locked_ptr,
                                              ZpicoNotifyCallback callback, void* ctx) {
    if (!g_session_open) {
        return ZPICO_ERR_SESSION;
    }

    // Find free slot
    int idx = -1;
    for (int i = 0; i < ZPICO_MAX_SUBSCRIBERS; i++) {
        if (!g_subscribers[i].active) {
            idx = i;
            break;
        }
    }
    if (idx < 0) {
        return ZPICO_ERR_FULL;
    }

    g_subscribers[idx].notify = callback;
    g_subscribers[idx].ctx = ctx;
    g_subscribers[idx].with_attachment = false;
    g_subscribers[idx].direct_write = true;
    g_subscribers[idx].buf_ptr = buf_ptr;
    g_subscribers[idx].buf_capacity = buf_capacity;
    g_subscribers[idx].locked_ptr = locked_ptr;

    z_view_keyexpr_t ke;
    if (z_view_keyexpr_from_str(&ke, keyexpr) < 0) {
        g_subscribers[idx].notify = NULL;
        g_subscribers[idx].ctx = NULL;
        g_subscribers[idx].direct_write = false;
        return ZPICO_ERR_KEYEXPR;
    }

    // Create closure for callback, passing index as context
    z_owned_closure_sample_t closure;
    z_closure_sample(&closure, sample_handler, NULL, (void*)(intptr_t)idx);

    int sub_ret =
        z_declare_subscriber(z_session_loan(&g_session), &g_subscribers[idx].subscriber,
                             z_view_keyexpr_loan(&ke), z_closure_sample_move(&closure), NULL);
    if (sub_ret < 0) {
        printk("zpico: z_declare_subscriber failed: %d for '%s'\n", sub_ret, keyexpr);
        g_subscribers[idx].notify = NULL;
        g_subscribers[idx].ctx = NULL;
        g_subscribers[idx].direct_write = false;
        return ZPICO_ERR_GENERIC;
    }

    g_subscribers[idx].active = true;
    return idx;
}

int32_t zpico_declare_subscriber_ring(const char* keyexpr, zpico_ring_desc_t* desc,
                                      ZpicoNotifyCallback callback, void* ctx) {
    if (!g_session_open) {
        return ZPICO_ERR_SESSION;
    }
    if (desc == NULL || desc->slot_count == 0) {
        return ZPICO_ERR_INVALID;
    }

    int idx = -1;
    for (int i = 0; i < ZPICO_MAX_SUBSCRIBERS; i++) {
        if (!g_subscribers[i].active) {
            idx = i;
            break;
        }
    }
    if (idx < 0) {
        return ZPICO_ERR_FULL;
    }

    g_subscribers[idx].notify = callback;
    g_subscribers[idx].ctx = ctx;
    g_subscribers[idx].with_attachment = false;
    g_subscribers[idx].direct_write = false;
    g_subscribers[idx].ring_mode = true;
    g_subscribers[idx].ring = desc;

    z_view_keyexpr_t ke;
    if (z_view_keyexpr_from_str(&ke, keyexpr) < 0) {
        g_subscribers[idx].notify = NULL;
        g_subscribers[idx].ctx = NULL;
        g_subscribers[idx].ring_mode = false;
        g_subscribers[idx].ring = NULL;
        return ZPICO_ERR_KEYEXPR;
    }

    z_owned_closure_sample_t closure;
    z_closure_sample(&closure, sample_handler, NULL, (void*)(intptr_t)idx);

    int sub_ret =
        z_declare_subscriber(z_session_loan(&g_session), &g_subscribers[idx].subscriber,
                             z_view_keyexpr_loan(&ke), z_closure_sample_move(&closure), NULL);
    if (sub_ret < 0) {
        printk("zpico: z_declare_subscriber (ring) failed: %d for '%s'\n", sub_ret, keyexpr);
        g_subscribers[idx].notify = NULL;
        g_subscribers[idx].ctx = NULL;
        g_subscribers[idx].ring_mode = false;
        g_subscribers[idx].ring = NULL;
        return ZPICO_ERR_GENERIC;
    }

    g_subscribers[idx].active = true;
    return idx;
}

#if defined(Z_FEATURE_UNSTABLE_API)
int32_t zpico_subscribe_zero_copy(const char* keyexpr, ZpicoZeroCopyCallback callback, void* ctx) {
    if (!g_session_open) {
        return ZPICO_ERR_SESSION;
    }

    // Find free slot
    int idx = -1;
    for (int i = 0; i < ZPICO_MAX_SUBSCRIBERS; i++) {
        if (!g_subscribers[i].active) {
            idx = i;
            break;
        }
    }
    if (idx < 0) {
        return ZPICO_ERR_FULL;
    }

    g_subscribers[idx].ctx = ctx;
    g_subscribers[idx].with_attachment = false;
    g_subscribers[idx].direct_write = false;
    g_subscribers[idx].zero_copy = true;
    g_subscribers[idx].zero_copy_cb = callback;

    z_view_keyexpr_t ke;
    if (z_view_keyexpr_from_str(&ke, keyexpr) < 0) {
        g_subscribers[idx].zero_copy = false;
        g_subscribers[idx].zero_copy_cb = NULL;
        g_subscribers[idx].ctx = NULL;
        return ZPICO_ERR_KEYEXPR;
    }

    // Create closure for callback, passing index as context
    z_owned_closure_sample_t closure;
    z_closure_sample(&closure, sample_handler, NULL, (void*)(intptr_t)idx);

    int sub_ret =
        z_declare_subscriber(z_session_loan(&g_session), &g_subscribers[idx].subscriber,
                             z_view_keyexpr_loan(&ke), z_closure_sample_move(&closure), NULL);
    if (sub_ret < 0) {
        printk("zpico: z_declare_subscriber (zero_copy) failed: %d for '%s'\n", sub_ret, keyexpr);
        g_subscribers[idx].zero_copy = false;
        g_subscribers[idx].zero_copy_cb = NULL;
        g_subscribers[idx].ctx = NULL;
        return ZPICO_ERR_GENERIC;
    }

    g_subscribers[idx].active = true;
    return idx;
}
#else
// Stub when unstable API is not enabled — returns error
int32_t zpico_subscribe_zero_copy(const char* keyexpr, ZpicoZeroCopyCallback callback, void* ctx) {
    (void)keyexpr;
    (void)callback;
    (void)ctx;
    return ZPICO_ERR_GENERIC;
}
#endif

int32_t zpico_undeclare_subscriber(int32_t handle) {
    if (handle < 0 || handle >= ZPICO_MAX_SUBSCRIBERS || !g_subscribers[handle].active) {
        return ZPICO_ERR_INVALID;
    }

    z_undeclare_subscriber(z_subscriber_move(&g_subscribers[handle].subscriber));
    g_subscribers[handle].active = false;
    g_subscribers[handle].callback = NULL;
    g_subscribers[handle].ctx = NULL;
    g_subscribers[handle].with_attachment = false;
    return ZPICO_OK;
}

// ============================================================================
// Socket FD Helper (for select()-based timeout)
// ============================================================================

// get_session_fd() is only needed for select()-based paths. Most
// multi-threaded builds use background tasks, but ThreadX/NSOS uses the same
// select + read + keepalive path as single-threaded hosts.
#if !defined(ZPICO_SMOLTCP) && !defined(ZPICO_SERIAL) && !defined(ZENOH_FREERTOS_LWIP) &&          \
    (Z_FEATURE_MULTI_THREAD != 1 || defined(ZENOH_THREADX))
/**
 * Extract the socket file descriptor from the zenoh session.
 *
 * Path: g_session → _z_session_t._tp._transport._unicast._peers → first peer → _socket
 *
 * Returns -1 if the session is not unicast or has no connected peers.
 */
static int get_session_fd(void) {
    _z_session_t* session = _Z_RC_IN_VAL(z_session_loan(&g_session));
    if (session->_tp._type != _Z_TRANSPORT_UNICAST_TYPE) {
        return -1;
    }
    _z_transport_peer_unicast_t* peer =
        _z_transport_peer_unicast_slist_value(session->_tp._transport._unicast._peers);
    if (peer == NULL) {
        return -1;
    }
    // Phase 154 — read the BSD fd via the platform accessor
    // instead of reaching into the per-RTOS `_z_sys_net_socket_t`
    // struct fields. With `NROS_PLATFORM_ALIASES` now defined for
    // the vendor build (so the socket ABI matches the alias TU's
    // 32-byte opaque layout), the `._fd` / `._socket` field names
    // no longer exist at compile time in this TU. Every backend
    // stores `int fd` at offset 0 of its socket struct, so
    // `nros_platform_socket_get_fd` works uniformly across
    // FreeRTOS+lwIP, POSIX, ThreadX, and bare-metal.
    return nros_platform_socket_get_fd(&peer->_socket);
}
#endif

// ============================================================================
// Polling Implementation (zpico_poll — deleted in Phase 77.20; use
// zpico_spin_once() instead, which adds keep-alive handling)
// ============================================================================

int32_t zpico_spin_once(uint32_t timeout_ms) {
    if (!g_session_open) {
        return ZPICO_ERR_SESSION;
    }

#if defined(ZPICO_TX_BATCH) && ZPICO_TX_BATCH == 1 && ZPICO_TX_BATCH_THREAD == 0
    /* phase-279 (#145) — rate-limited batch flush (platforms WITHOUT the
     * dedicated tx-flush thread: ThreadX + single-threaded). zenoh-pico's flush HOLDS the
     * transport tx mutex across the whole socket send (fd wait included), so
     * puts cannot append while a flush is in flight — the batch only
     * accumulates BETWEEN flushes. Flushing on every spin (1 ms tiers) ships
     * <=1 message per send and the flushes themselves compete with puts for
     * the send window, which MEASURED WORSE than no batching (4.7-4.9 vs 8.6
     * msg/s on the W1 harness). Flushing at a bounded cadence lets puts pile
     * into the write buffer cheaply and ships them as ONE send per interval.
     * Racy static across tier threads is benign (worst case one extra flush).
     * zenoh-pico's implicit flushes (buffer overflow, any transport message)
     * still bound memory + sit-time independently of this cadence. */
    {
        static z_clock_t g_last_batch_flush;
        static bool g_batch_flush_init = false;
        if (!g_batch_flush_init ||
            z_clock_elapsed_ms(&g_last_batch_flush) >= ZPICO_TX_BATCH_FLUSH_MS) {
            zp_batch_flush(z_session_loan(&g_session));
            g_last_batch_flush = z_clock_now();
            g_batch_flush_init = true;
        }
    }
#endif

#ifdef ZPICO_SMOLTCP
    // smoltcp: poll network and read available data. Uses single_read=true
    // to preserve partial TCP data across calls (non-blocking _z_read_tcp
    // may return fragments). single_read=false resets the zbuf on each call
    // which discards partial messages.
    //
    // Drain behaviour (timeout_ms == 0):
    //   Loop until no data remains. This prevents a race where a keep-alive
    //   is read first, the loop exits, _z_pending_query_process_timeout fires
    //   and removes the pending query, and then the reply — still in the
    //   staging buffer — is read in the next spin but cannot find its matching
    //   pending query (already removed), causing get_check to return TIMEOUT
    //   even though the reply arrived in time.
    //
    // Polling behaviour (timeout_ms > 0):
    //   Stop after the first processed message (original behaviour), and
    //   retry until data arrives or the timeout elapses.
    zp_read_options_t opts;
    opts.single_read = true;
    uint64_t start = smoltcp_clock_now_ms();
    int ret;
    do {
        ret = zp_read(z_session_loan_mut(&g_session), &opts);
        if (ret == 0 && timeout_ms != 0) break; // Processed one message (timeout mode)
        if (ret != 0 && timeout_ms == 0) break; // No data (drain mode)
        // timeout_ms == 0 && ret == 0: data processed, loop to drain next message
        // timeout_ms != 0 && ret != 0: no data yet, keep waiting (while decides)
    } while (ret == 0 || smoltcp_clock_now_ms() - start < timeout_ms);
    // Process query timeouts — in multi-threaded mode the lease task handles
    // this, but on single-threaded bare-metal (smoltcp) there is no lease task.
    // Without this call, timed-out queries are never cleaned up and their
    // dropper callbacks never fire, breaking service/action request flows.
    _z_pending_query_process_timeout(_Z_RC_IN_VAL(z_session_loan_mut(&g_session)));
    zp_send_keep_alive(z_session_loan_mut(&g_session), NULL);
    return ret;

#elif defined(ZPICO_SERIAL)
    // Serial-only bare-metal: poll UART and read available data.
    // Uses single_read=false because the single_read=true path calls
    // _z_unicast_recv_t_msg which returns _Z_ERR_TRANSPORT_RX_FAILED (-99)
    // on no data for datagram links. The single_read=false path uses
    // _z_unicast_client_read which returns _Z_NO_DATA_PROCESSED gracefully.
    // For serial (datagram), each COBS frame is atomic so there's no
    // partial data to preserve across calls (unlike TCP stream).
    z_clock_t start = z_clock_now();
    int ret;
    do {
        ret = zp_read(z_session_loan_mut(&g_session), NULL);
        if (ret == 0) break; // Data processed
        if (timeout_ms == 0) break;
    } while (z_clock_elapsed_ms(&start) < timeout_ms);
    _z_pending_query_process_timeout(_Z_RC_IN_VAL(z_session_loan_mut(&g_session)));
    zp_send_keep_alive(z_session_loan_mut(&g_session), NULL);
    return ret;

#elif defined(ZENOH_THREADX)
    // ThreadX/NSOS: avoid zenoh-pico background read/lease tasks. The read
    // task can block in the host-backed BSD recv path and prevent the lease
    // task from running before the 10s router lease expires. Poll here instead
    // so every executor spin also refreshes the transport keepalive.
    //
    // phase-297 W5 — the timed wait MUST be `z_sleep_ms` (tx_thread_sleep),
    // never a host select(): a host syscall blocks the pthread while the
    // ThreadX scheduler still counts this thread as running, so under strict
    // priority scheduling every other tier thread starves (the multi-tier
    // e2e saw the higher-priority tier's select() wait pin the whole image
    // to one tier — zero publishes from the other). Sleep first (a REAL
    // ThreadX yield — lower-priority tiers run inside it), then poll the fd
    // with a zero timeout and drain only what is already there. Same shape
    // as the other multi-threaded platforms (see the pitfall in
    // platform-implementation-notes.md).
    // phase-297 W5 — single-reader guard: the polled `zp_read` path
    // (`_zp_unicast_read`, single_read=false) does NOT take the transport's
    // `_mutex_rx` (only the background read task does), so two tier threads
    // polling concurrently race on the shared rx zbuf (`_z_zbuf_reset` mid
    // parse) and silently LOSE inbound frames — the multi-tier e2e saw the
    // spawned tier's interest replies/subscriber declares vanish, leaving its
    // write filter closed forever (zero publishes). Only one thread reads at
    // a time; a busy loser skips the read — the reader dispatches every
    // subscriber's data regardless of which tier thread it is.
    static volatile int reading = 0;
    int fd = get_session_fd();
    int ret = ZPICO_ERR_TIMEOUT;
    if (timeout_ms > 0) {
        z_sleep_ms(timeout_ms);
    }
    if (__sync_lock_test_and_set(&reading, 1) != 0) {
        return ZPICO_OK;
    }
    if (fd >= 0) {
#if defined(__linux__)
        fd_set read_fds;
        FD_ZERO(&read_fds);
        FD_SET(fd, &read_fds);
        struct timeval tv;
        tv.tv_sec = 0;
        tv.tv_usec = 0;
        int ready = select(fd + 1, &read_fds, NULL, NULL, &tv);
#else
        nx_bsd_fd_set read_fds;
        NX_BSD_FD_ZERO(&read_fds);
        NX_BSD_FD_SET(fd, &read_fds);
        struct nx_bsd_timeval tv;
        tv.tv_sec = 0;
        tv.tv_usec = 0;
        int ready = nx_bsd_select(fd + 1, &read_fds, NULL, NULL, &tv);
#endif
        if (ready > 0) {
            ret = zp_read(z_session_loan_mut(&g_session), NULL);
        } else if (ready < 0) {
            ret = ready;
        }
    } else {
        ret = zp_read(z_session_loan_mut(&g_session), NULL);
    }
    _z_pending_query_process_timeout(_Z_RC_IN_VAL(z_session_loan_mut(&g_session)));
    zp_send_keep_alive(z_session_loan_mut(&g_session), NULL);
    __sync_lock_release(&reading);
    return ret;

#elif defined(ZENOH_FREERTOS_LWIP)
    // FreeRTOS+lwIP: background read and lease tasks handle data and
    // keep-alives. Wait on a binary semaphore that _zpico_notify_spin()
    // signals when application data arrives (subscriptions, query replies).
    // This gives near-zero latency wake-up without busy-looping.
    if (timeout_ms > 0 && g_spin_sem != NULL) {
        xSemaphoreTake(g_spin_sem, pdMS_TO_TICKS(timeout_ms));
    }
    return 0;

#elif Z_FEATURE_MULTI_THREAD == 1
    // Multi-threaded (Zephyr/POSIX/NuttX): background read and lease tasks
    // handle data and keep-alives. Wait on a wake primitive that our
    // callbacks signal when application data arrives (subscriptions, query
    // replies, service requests). This gives near-zero latency wake-up
    // without busy-looping on select().
    //
    // NuttX note: the pthread-timed-wait path (`pthread_cond_timedwait`)
    // hangs indefinitely inside the kernel's watchdog-backed semaphore
    // wait (Phase 55.12 follow-up), so we can't reuse the condvar path
    // there. POSIX `sem_timedwait` does not share that code path and is
    // safe — it gives us the same early-wake optimisation the other
    // multi-threaded backends enjoy, replacing the old blind
    // `usleep(timeout_ms * 1000)` busy-sleep.
    if (timeout_ms > 0) {
#ifdef ZENOH_NUTTX
        if (g_spin_sem_initialized) {
            struct timespec deadline;
            clock_gettime(CLOCK_REALTIME, &deadline);
            deadline.tv_sec += (time_t)(timeout_ms / 1000);
            deadline.tv_nsec += (long)(timeout_ms % 1000) * 1000000L;
            if (deadline.tv_nsec >= 1000000000L) {
                deadline.tv_sec += 1;
                deadline.tv_nsec -= 1000000000L;
            }
            // EINTR / ETIMEDOUT are both acceptable — the outer executor
            // loop re-checks arena state regardless of why we woke up.
            while (sem_timedwait(&g_spin_sem_posix, &deadline) != 0 && errno == EINTR) {
                // retry on signal
            }
        } else {
            // Fallback if sem_init failed at session open.
            usleep((useconds_t)timeout_ms * 1000);
        }
#else
        z_clock_t deadline = z_clock_now();
        z_clock_advance_ms(&deadline, (unsigned long)timeout_ms);
        _z_mutex_lock(&g_spin_mutex);
        _z_condvar_wait_until(&g_spin_cv, &g_spin_mutex, &deadline);
        _z_mutex_unlock(&g_spin_mutex);
#endif
    }
    return 0;

#else
    // Single-threaded (not smoltcp): use select() then zp_read()
    int fd = get_session_fd();
    if (fd >= 0 && timeout_ms > 0) {
        fd_set read_fds;
        FD_ZERO(&read_fds);
        FD_SET(fd, &read_fds);
        struct timeval tv;
        tv.tv_sec = timeout_ms / 1000;
        tv.tv_usec = (timeout_ms % 1000) * 1000;
        int result = select(fd + 1, &read_fds, NULL, NULL, &tv);
        if (result <= 0) {
            zp_send_keep_alive(z_session_loan_mut(&g_session), NULL);
            return (result == 0) ? ZPICO_ERR_TIMEOUT : result;
        }
    }
    int ret = zp_read(z_session_loan_mut(&g_session), NULL);
    zp_send_keep_alive(z_session_loan_mut(&g_session), NULL);
    return ret;
#endif
}

bool zpico_uses_polling(void) {
    // Returns true if multi-threading is disabled
#if Z_FEATURE_MULTI_THREAD == 0
    return true;
#else
    return false;
#endif
}

// ============================================================================
// ZenohId Implementation
// ============================================================================

int32_t zpico_get_zid(uint8_t* zid_out) {
    if (!g_session_open || zid_out == NULL) {
        return ZPICO_ERR_SESSION;
    }

    z_id_t zid = z_info_zid(z_session_loan(&g_session));
    memcpy(zid_out, zid.id, 16);
    return ZPICO_OK;
}

// ============================================================================
// Liveliness Implementation
// ============================================================================

int32_t zpico_declare_liveliness(const char* keyexpr) {
    if (!g_session_open) {
        return ZPICO_ERR_SESSION;
    }

    // Find free slot
    int idx = -1;
    for (int i = 0; i < ZPICO_MAX_LIVELINESS; i++) {
        if (!g_liveliness[i].active) {
            idx = i;
            break;
        }
    }
    if (idx < 0) {
        return ZPICO_ERR_FULL;
    }

    z_view_keyexpr_t ke;
    if (z_view_keyexpr_from_str(&ke, keyexpr) < 0) {
        return ZPICO_ERR_KEYEXPR;
    }

    int lv_ret = z_liveliness_declare_token(z_session_loan(&g_session), &g_liveliness[idx].token,
                                            z_view_keyexpr_loan(&ke), NULL);
    if (lv_ret < 0) {
        return ZPICO_ERR_GENERIC;
    }

    g_liveliness[idx].active = true;
    return idx;
}

int32_t zpico_undeclare_liveliness(int32_t handle) {
    if (handle < 0 || handle >= ZPICO_MAX_LIVELINESS || !g_liveliness[handle].active) {
        return ZPICO_ERR_INVALID;
    }

    z_liveliness_undeclare_token(z_liveliness_token_move(&g_liveliness[handle].token));
    g_liveliness[handle].active = false;
    return ZPICO_OK;
}

// ============================================================================
// Publish with Attachment Implementation
// ============================================================================

int32_t zpico_publish_with_attachment(int32_t handle, const uint8_t* data, size_t len,
                                      const uint8_t* attachment, size_t attachment_len) {
    if (handle < 0 || handle >= ZPICO_MAX_PUBLISHERS || !g_publishers[handle].active) {
        return ZPICO_ERR_INVALID;
    }

    // Create payload
    z_owned_bytes_t payload;
    if (z_bytes_copy_from_buf(&payload, data, len) < 0) {
        return ZPICO_ERR_PUBLISH;
    }

    // Create put options with attachment
    z_publisher_put_options_t options;
    z_publisher_put_options_default(&options);

    z_owned_bytes_t attachment_bytes;
    if (attachment != NULL && attachment_len > 0) {
        if (z_bytes_copy_from_buf(&attachment_bytes, attachment, attachment_len) < 0) {
            z_bytes_drop(z_bytes_move(&payload));
            return ZPICO_ERR_PUBLISH;
        }
        options.attachment = z_bytes_move(&attachment_bytes);
    }

    if (z_publisher_put(z_publisher_loan(&g_publishers[handle].publisher), z_bytes_move(&payload),
                        &options) < 0) {
        return ZPICO_ERR_PUBLISH;
    }

    return ZPICO_OK;
}

// Phase 99.F — zero-copy publish via z_bytes_from_static_buf.
// Aliases the payload pointer instead of copying. Caller guarantees
// `data` outlives the call (z_publisher_put consumes the alias
// synchronously on posix/embedded transports). Attachment is still
// copied (small, fixed size).
int32_t zpico_publish_with_attachment_aliased(int32_t handle, const uint8_t* data, size_t len,
                                              const uint8_t* attachment, size_t attachment_len) {
    if (handle < 0 || handle >= ZPICO_MAX_PUBLISHERS || !g_publishers[handle].active) {
        return ZPICO_ERR_INVALID;
    }

    // Alias the payload — no copy. zenoh-pico writes directly from
    // the caller-supplied buffer.
    z_owned_bytes_t payload;
    if (z_bytes_from_static_buf(&payload, data, len) < 0) {
        return ZPICO_ERR_PUBLISH;
    }

    z_publisher_put_options_t options;
    z_publisher_put_options_default(&options);

    z_owned_bytes_t attachment_bytes;
    if (attachment != NULL && attachment_len > 0) {
        if (z_bytes_copy_from_buf(&attachment_bytes, attachment, attachment_len) < 0) {
            z_bytes_drop(z_bytes_move(&payload));
            return ZPICO_ERR_PUBLISH;
        }
        options.attachment = z_bytes_move(&attachment_bytes);
    }

    if (z_publisher_put(z_publisher_loan(&g_publishers[handle].publisher), z_bytes_move(&payload),
                        &options) < 0) {
        return ZPICO_ERR_PUBLISH;
    }

    return ZPICO_OK;
}

// ============================================================================
// Queryable Implementation (for ROS 2 Services)
// ============================================================================

int32_t zpico_declare_queryable(const char* keyexpr, ZpicoQueryCallback callback, void* ctx) {
    if (!g_session_open) {
        return ZPICO_ERR_SESSION;
    }

    // Find free slot
    int idx = -1;
    for (int i = 0; i < ZPICO_MAX_QUERYABLES; i++) {
        if (!g_queryables[i].active) {
            idx = i;
            break;
        }
    }
    if (idx < 0) {
        return ZPICO_ERR_FULL;
    }

    g_queryables[idx].callback = callback;
    g_queryables[idx].ctx = ctx;

    z_view_keyexpr_t ke;
    if (z_view_keyexpr_from_str(&ke, keyexpr) < 0) {
        g_queryables[idx].callback = NULL;
        g_queryables[idx].ctx = NULL;
        return ZPICO_ERR_KEYEXPR;
    }

    // Create closure for callback
    z_owned_closure_query_t closure;
    z_closure_query(&closure, query_handler, NULL, (void*)(intptr_t)idx);

    // Set complete=true so that queries with Z_QUERY_TARGET_ALL_COMPLETE
    // (used by rmw_zenoh_cpp service clients) match this queryable.
    z_queryable_options_t opts;
    z_queryable_options_default(&opts);
    opts.complete = true;

    int q_ret =
        z_declare_queryable(z_session_loan(&g_session), &g_queryables[idx].queryable,
                            z_view_keyexpr_loan(&ke), z_closure_query_move(&closure), &opts);
    if (q_ret < 0) {
        printk("zpico: z_declare_queryable failed: %d for '%s'\n", q_ret, keyexpr);
        g_queryables[idx].callback = NULL;
        g_queryables[idx].ctx = NULL;
        return ZPICO_ERR_GENERIC;
    }

    g_queryables[idx].active = true;
    return idx;
}

int32_t zpico_undeclare_queryable(int32_t handle) {
    if (handle < 0 || handle >= ZPICO_MAX_QUERYABLES || !g_queryables[handle].active) {
        return ZPICO_ERR_INVALID;
    }

    z_undeclare_queryable(z_queryable_move(&g_queryables[handle].queryable));
    g_queryables[handle].active = false;
    g_queryables[handle].callback = NULL;
    g_queryables[handle].ctx = NULL;
    // Phase 237 — drop any cloned queries still held in this queryable's reply
    // slots (unanswered deferred requests) so they don't leak session refs.
    for (int j = 0; j < ZPICO_MAX_PENDING_REPLIES; j++) {
        if (g_stored_query_valid[handle][j]) {
            z_query_drop(z_query_move(&g_stored_queries[handle][j]));
            g_stored_query_valid[handle][j] = false;
        }
    }
    g_last_reply_seq[handle] = -1;
    return ZPICO_OK;
}

// ============================================================================
// Service Client Implementation (z_get for ROS 2 service calls)
// ============================================================================

/**
 * Internal callback for z_get reply handling
 */
static void get_reply_handler(z_loaned_reply_t* reply, void* ctx) {
    get_reply_ctx_t* rctx = (get_reply_ctx_t*)ctx;

    // Only process successful replies
    if (!z_reply_is_ok(reply)) {
        g_diag_reply_not_ok++;
        return;
    }

    /* Bump the multi-reply count regardless of payload-buffer state.
     * Liveliness queries care about this count; single-response gets
     * ignore it. */
    rctx->reply_count++;

    // Skip if we already have a reply (only take first)
    if (rctx->received) {
        g_diag_reply_already_received++;
        return;
    }

    const z_loaned_sample_t* sample = z_reply_ok(reply);
    const z_loaned_bytes_t* payload = z_sample_payload(sample);

    // Copy payload to reply buffer
    z_owned_slice_t slice;
    if (z_bytes_to_slice(payload, &slice) == 0) {
        const uint8_t* data = z_slice_data(z_slice_loan(&slice));
        size_t len = z_slice_len(z_slice_loan(&slice));

        if (len <= ZPICO_GET_REPLY_BUF_SIZE) {
            memcpy(rctx->buf, data, len);
            rctx->len = len;
            __atomic_store_n(&rctx->received, true, __ATOMIC_SEQ_CST);
            g_diag_reply_received_set++;
        } else {
            g_diag_reply_too_big++;
        }
        z_slice_drop(z_slice_move(&slice));
    } else {
        g_diag_reply_to_slice_fail++;
    }
}

/**
 * Internal callback for z_get completion (dropper)
 */
static void get_reply_dropper(void* ctx) {
    get_reply_ctx_t* rctx = (get_reply_ctx_t*)ctx;
#if Z_FEATURE_MULTI_THREAD == 1
    _z_mutex_lock(&rctx->mutex);
    rctx->done = true;
    _z_condvar_signal(&rctx->cond);
    _z_mutex_unlock(&rctx->mutex);
#else
    rctx->done = true;
#endif
}

int32_t zpico_get(const char* keyexpr, const uint8_t* payload, size_t payload_len,
                  uint8_t* reply_buf, size_t reply_buf_size, uint32_t timeout_ms) {
    if (!g_session_open) {
        return ZPICO_ERR_SESSION;
    }

    // Stack-allocated reply context (safe for concurrent z_get calls)
    get_reply_ctx_t ctx;
    ctx.len = 0;
    ctx.received = false;
    ctx.done = false;
    ctx.reply_count = 0;
#if Z_FEATURE_MULTI_THREAD == 1
    _z_mutex_init(&ctx.mutex);
    _z_condvar_init(&ctx.cond);
#endif

    z_view_keyexpr_t ke;
    if (z_view_keyexpr_from_str(&ke, keyexpr) < 0) {
        return ZPICO_ERR_KEYEXPR;
    }

    // Set up get options
    z_get_options_t opts;
    z_get_options_default(&opts);
#if defined(ZPICO_TX_BATCH) && ZPICO_TX_BATCH == 1
    /* phase-279 (#145) — service/query latency guard: gets bypass the batch
     * (express) so request RTT gains no spin-period latency under batching. */
    opts.is_express = true;
#endif
    opts.target = Z_QUERY_TARGET_ALL;
    opts.timeout_ms = timeout_ms;
    // Use NONE consolidation so the reply callback fires immediately on each
    // partial reply, rather than AUTO (which becomes LATEST and buffers replies
    // until the ReplyFinal arrives). With LATEST, the dropper fires before the
    // reply callback, leaving received=false when the condvar is signalled.
    opts.consolidation.mode = Z_CONSOLIDATION_MODE_NONE;

    // Set payload if provided
    z_owned_bytes_t payload_bytes;
    if (payload != NULL && payload_len > 0) {
        if (z_bytes_copy_from_buf(&payload_bytes, payload, payload_len) < 0) {
            return ZPICO_ERR_GENERIC;
        }
        opts.payload = z_bytes_move(&payload_bytes);
    }

    // Create closure for reply handling with stack context
    z_owned_closure_reply_t callback;
    z_closure(&callback, get_reply_handler, get_reply_dropper, &ctx);

    // Send the query
    if (z_get(z_session_loan(&g_session), z_view_keyexpr_loan(&ke), "", z_move(callback), &opts) <
        0) {
        return ZPICO_ERR_GENERIC;
    }

    // For multi-threaded platforms, wait for reply via background threads
    // For single-threaded platforms, poll until reply or timeout
#if Z_FEATURE_MULTI_THREAD == 0
    // Single-threaded: poll until reply received or timeout.
    //
    // We drive _z_pending_query_process_timeout() on every iteration so that
    // zenoh-pico's own deadline fires the dropper (setting ctx.done = true)
    // while ctx is still live on the stack.  Without this, the wall-clock
    // check below would break out of the loop while a stale pending-query
    // entry still points at ctx, and the dropper would later fire on a
    // dangling pointer, silently corrupting the stack of the next caller.
    //
    // The wall-clock guard (timeout_ms + 2000 ms) is belt-and-suspenders:
    // in normal operation _z_pending_query_process_timeout fires the dropper
    // at opts.timeout_ms (== timeout_ms), so ctx.done becomes true before the
    // guard triggers.
    {
        z_clock_t start = z_clock_now();
        while (!ctx.done) {
            zp_read(z_session_loan_mut(&g_session), NULL);
            zp_send_keep_alive(z_session_loan_mut(&g_session), NULL);
            // Drive zenoh-pico's query timeout so its dropper fires cleanly
            // while ctx is still on the stack.
            _z_pending_query_process_timeout(_Z_RC_IN_VAL(z_session_loan_mut(&g_session)));
            if (ctx.received) {
                break;
            }
            // Safety wall-clock guard (fires 2 s after the zenoh deadline).
            if ((uint32_t)z_clock_elapsed_ms(&start) >= timeout_ms + 2000) {
                break;
            }
        }
    }
#else
    // Multi-threaded: wait for completion via condvar.
    // The dropper callback signals ctx.cond when the reply channel closes.
    //
    // IMPORTANT: Do NOT use _z_condvar_wait_until with a deadline here.
    // Zenoh fires the dropper at opts.timeout_ms — rely on that timeout
    // instead of a parallel OS deadline. Using a separate deadline creates
    // a use-after-free race: if the OS deadline expires first, we drop
    // ctx.mutex/ctx.cond while the background thread's dropper is still
    // pending, causing the dropper to lock/signal freed memory.
    {
        _z_mutex_lock(&ctx.mutex);
        while (!ctx.done) {
            _z_condvar_wait(&ctx.cond, &ctx.mutex);
        }
        _z_mutex_unlock(&ctx.mutex);
    }
    _z_condvar_drop(&ctx.cond);
    _z_mutex_drop(&ctx.mutex);
#endif

    // Check if we got a reply
    if (!ctx.received) {
        return -9; // ZPICO_ERR_TIMEOUT (defined in Rust FFI)
    }

    // Copy reply to output buffer
    if (ctx.len > reply_buf_size) {
        return ZPICO_ERR_FULL; // Buffer too small
    }

    memcpy(reply_buf, ctx.buf, ctx.len);
    return (int32_t)ctx.len;
}

// ============================================================================
// Non-blocking z_get (for async service client)
// ============================================================================

// Reply handler for pending get slots — reuses the same logic as get_reply_handler
static void pending_get_reply_handler(z_loaned_reply_t* reply, void* ctx) {
    g_diag_reply_handler_calls++;
    /* Phase 127.D.2 — keep the address-recording side effect. It
     * defeats whole-program LTO alias analysis that would otherwise
     * prove the closure's `ctx` pointer disjoint from
     * `&g_pending_gets[handle].ctx` (because the slot table is
     * private to this TU and the callback type-erases `ctx` to
     * `void *`). Without this side effect the write to
     * `rctx->received` here was hoisted away from the read in
     * `zpico_get_check`, leaving the polling client spinning on a
     * stale `false`. _Atomic / volatile alone did not change the
     * symptom; recording the address through a non-static observer
     * does. See docs/research/qemu-lan9118-slirp-rx-stall.md.
     *
     * Bumping a counter unconditionally (vs. the "first write only"
     * pattern) keeps the side effect from being scheduled as a
     * one-shot branch that constant-folds away after the first hit. */
    g_diag_handler_ctx_addr = (uint32_t)(uintptr_t)ctx;
    get_reply_handler(reply, ctx);
    _zpico_notify_spin();
    if (g_reply_waker) {
        // ctx is &ps->ctx which is the first field, so ps == (pending_get_slot_t*)ctx
        int32_t slot = (int32_t)((pending_get_slot_t*)ctx - g_pending_gets);
        g_reply_waker(slot);
    }
}

// Dropper for pending get slots — just sets the done flag (no condvar)
static void pending_get_dropper(void* ctx) {
    g_diag_reply_dropper_calls++;
    get_reply_ctx_t* rctx = (get_reply_ctx_t*)ctx;
    __atomic_store_n(&rctx->done, true, __ATOMIC_SEQ_CST);
    _zpico_notify_spin();
    if (g_reply_waker) {
        int32_t slot = (int32_t)((pending_get_slot_t*)ctx - g_pending_gets);
        g_reply_waker(slot);
    }
}

void zpico_get_diag_counters(uint32_t out[18]) {
    out[0] = g_diag_get_start_calls;
    out[1] = g_diag_get_check_calls;
    out[2] = g_diag_get_check_returns_data;
    out[3] = g_diag_reply_handler_calls;
    out[4] = g_diag_reply_dropper_calls;
    out[5] = g_diag_reply_not_ok;
    out[6] = g_diag_reply_already_received;
    out[7] = g_diag_reply_received_set;
    out[8] = g_diag_reply_too_big;
    out[9] = g_diag_reply_to_slice_fail;
    out[10] = g_diag_gck_invalid_arg;
    out[11] = g_diag_gck_not_in_use;
    out[12] = g_diag_gck_too_big;
    out[13] = g_diag_gck_timeout;
    out[14] = g_diag_gck_pending;
    out[15] = g_diag_start_ctx_addr;
    out[16] = g_diag_handler_ctx_addr;
    out[17] = g_diag_check_ctx_addr;
}

int32_t zpico_get_start(const char* keyexpr, const uint8_t* payload, size_t payload_len,
                        uint32_t timeout_ms) {
    return zpico_get_start_with_attachment(keyexpr, payload, payload_len, NULL, 0, timeout_ms);
}

/* Issue 0153 — attachment-carrying variant. rmw_zenoh_cpp's service server
 * REQUIRES the rmw attachment (sequence_number + source_timestamp + gid) on
 * the query: `rmw_service_server_is_available`'s take path
 * (service_take_request) deserializes it and errors the whole take when it
 * is absent, so a nano-ros client request without one reaches the ROS 2
 * server and then dies inside rcl ("service failed to take request") — the
 * client only ever sees Transport(Timeout). nano<->nano services tolerate a
 * missing attachment, which is why this stayed invisible in-tree. */
int32_t zpico_get_start_with_attachment(const char* keyexpr, const uint8_t* payload,
                                        size_t payload_len, const uint8_t* attachment,
                                        size_t attachment_len, uint32_t timeout_ms) {
    g_diag_get_start_calls++;
    if (!g_session_open) {
        return ZPICO_ERR_SESSION;
    }

    // Find a free slot, cleaning up zombie slots first.
    // A zombie slot has in_use=true but ctx.done=true, meaning the reply was
    // delivered by get_check but the dropper hadn't fired yet at that time.
    // Now that the dropper has fired (done=true), the slot can be reclaimed.
    int32_t slot = -1;
    for (int32_t i = 0; i < ZPICO_MAX_PENDING_GETS; i++) {
        if (g_pending_gets[i].in_use && g_pending_gets[i].ctx.done) {
            g_pending_gets[i].in_use = false;
        }
        if (!g_pending_gets[i].in_use) {
            slot = i;
            break;
        }
    }
    if (slot < 0) {
        return ZPICO_ERR_FULL;
    }

    // Initialize slot context
    pending_get_slot_t* ps = &g_pending_gets[slot];
    ps->ctx.len = 0;
    ps->ctx.received = false;
    ps->ctx.done = false;
    ps->ctx.reply_count = 0;
    ps->in_use = true;

    z_view_keyexpr_t ke;
    z_view_keyexpr_from_str(&ke, keyexpr);

    z_get_options_t opts;
    z_get_options_default(&opts);
#if defined(ZPICO_TX_BATCH) && ZPICO_TX_BATCH == 1
    /* phase-279 (#145) — service/query latency guard: gets bypass the batch
     * (express) so request RTT gains no spin-period latency under batching. */
    opts.is_express = true;
#endif
    opts.target = Z_QUERY_TARGET_ALL;
    opts.timeout_ms = (uint64_t)timeout_ms;
    // Use NONE consolidation so the reply callback fires immediately on each
    // partial reply, rather than AUTO (which becomes LATEST and buffers replies
    // until the ReplyFinal arrives). With LATEST, a race between partial and
    // final delivery could leave received=false when get_check is polled.
    opts.consolidation.mode = Z_CONSOLIDATION_MODE_NONE;

    // Declare payload_bytes in the same scope as opts and z_get() so it stays
    // alive until z_get() consumes it (z_move takes the address).
    z_owned_bytes_t payload_bytes;
    if (payload != NULL && payload_len > 0) {
        z_bytes_copy_from_buf(&payload_bytes, payload, payload_len);
        opts.payload = z_move(payload_bytes);
    }
    // Same lifetime rule for the rmw attachment (issue 0153).
    z_owned_bytes_t attachment_bytes;
    if (attachment != NULL && attachment_len > 0) {
        z_bytes_copy_from_buf(&attachment_bytes, attachment, attachment_len);
        opts.attachment = z_move(attachment_bytes);
    }

    z_owned_closure_reply_t callback;
    z_closure(&callback, pending_get_reply_handler, pending_get_dropper, &ps->ctx);
    /* Same aliasing-defeat trick. */
    g_diag_start_ctx_addr = (uint32_t)(uintptr_t)&ps->ctx;

    z_result_t zret =
        z_get(z_session_loan(&g_session), z_view_keyexpr_loan(&ke), "", z_move(callback), &opts);
    if (zret < 0) {
        ps->in_use = false;
        return ZPICO_ERR_GENERIC;
    }

    return slot;
}

/* Non-blocking liveliness query.
 *
 * Used by `Client::wait_for_service` (and the action-client equivalent) to
 * implement rclcpp-style server discovery: issue a `z_liveliness_get` against
 * the server's wildcarded liveliness keyexpr, then poll `zpico_liveliness_get_check`
 * until either at least one matching token reports back or the dropper fires
 * empty-handed.
 *
 * Reuses the same `g_pending_gets` slot pool as `zpico_get_start` — a slot is
 * just a (received_flag, dropper_done_flag, payload_buf) triple, agnostic to
 * whether the caller will read the payload. The reply handler still copies
 * the (typically empty) liveliness token bytes into the slot's buffer; we
 * never read them. Only `received` matters.
 *
 * Returns the slot handle on success, ZPICO_ERR_* on failure.
 */
int32_t zpico_liveliness_get_start(const char* keyexpr, uint32_t timeout_ms) {
    if (!g_session_open) {
        return ZPICO_ERR_SESSION;
    }

    int32_t slot = -1;
    for (int32_t i = 0; i < ZPICO_MAX_PENDING_GETS; i++) {
        if (g_pending_gets[i].in_use && g_pending_gets[i].ctx.done) {
            g_pending_gets[i].in_use = false;
        }
        if (!g_pending_gets[i].in_use) {
            slot = i;
            break;
        }
    }
    if (slot < 0) {
        return ZPICO_ERR_FULL;
    }

    pending_get_slot_t* ps = &g_pending_gets[slot];
    ps->ctx.len = 0;
    ps->ctx.received = false;
    ps->ctx.done = false;
    ps->ctx.reply_count = 0;
    ps->in_use = true;

    z_view_keyexpr_t ke;
    if (z_view_keyexpr_from_str(&ke, keyexpr) < 0) {
        ps->in_use = false;
        return ZPICO_ERR_KEYEXPR;
    }

    z_liveliness_get_options_t opts;
    z_liveliness_get_options_default(&opts);
    opts.timeout_ms = (uint64_t)timeout_ms;

    z_owned_closure_reply_t callback;
    z_closure(&callback, pending_get_reply_handler, pending_get_dropper, &ps->ctx);

    z_result_t zret = z_liveliness_get(z_session_loan(&g_session), z_view_keyexpr_loan(&ke),
                                       z_move(callback), &opts);
    if (zret < 0) {
        ps->in_use = false;
        return ZPICO_ERR_GENERIC;
    }
    return slot;
}

/* Check status of a pending liveliness query.
 *
 * Unlike `zpico_get_check`, the caller doesn't care about the reply payload —
 * just whether *any* matching liveliness token responded. Liveliness tokens
 * carry an empty (0-byte) payload, which `zpico_get_check` would otherwise
 * report as "still pending" (its return value `0` is overloaded).
 *
 * Returns:
 *   1 — at least one token reply seen, server is discoverable.
 *   0 — query still in flight, no replies yet.
 *  -9 — dropper fired with no replies (timeout, no matching server).
 *   ZPICO_ERR_INVALID — handle out of range or slot not in use.
 */
/* Phase 108.C.zenoh.4-followup — count of liveliness-token replies
 * received on this slot. Returns 0 while the query is still in
 * flight, ZPICO_ERR_INVALID for bad handles. After the dropper has
 * fired (i.e. `zpico_liveliness_get_check` would return 1 or -9),
 * the count is final and accurate up to the timeout. Used by the
 * subscriber-side `LivelinessChanged` bridge to report
 * `alive_count > 1` when more than one publisher matches the
 * wildcard liveliness keyexpr.
 *
 * The count is left intact on read; only `zpico_liveliness_get_check`
 * releases the slot, so callers should pair `count → check`.
 */
int32_t zpico_liveliness_get_count(int32_t handle) {
    if (handle < 0 || handle >= ZPICO_MAX_PENDING_GETS) {
        return ZPICO_ERR_INVALID;
    }
    pending_get_slot_t* ps = &g_pending_gets[handle];
    if (!ps->in_use) {
        return ZPICO_ERR_INVALID;
    }
    /* Cap to int32 max — wildcards in practice match a handful of
     * tokens, never billions. */
    uint32_t c = ps->ctx.reply_count;
    return c > (uint32_t)INT32_MAX ? INT32_MAX : (int32_t)c;
}

int32_t zpico_liveliness_get_check(int32_t handle) {
    if (handle < 0 || handle >= ZPICO_MAX_PENDING_GETS) {
        return ZPICO_ERR_INVALID;
    }

    pending_get_slot_t* ps = &g_pending_gets[handle];
    if (!ps->in_use) {
        return ZPICO_ERR_INVALID;
    }
    /* Same aliasing-defeat trick as `zpico_get_check`; see comment
     * in `pending_get_reply_handler`. */
    g_diag_check_ctx_addr = (uint32_t)(uintptr_t)&ps->ctx;

    if (__atomic_load_n(&ps->ctx.received, __ATOMIC_SEQ_CST)) {
        if (__atomic_load_n(&ps->ctx.done, __ATOMIC_SEQ_CST)) {
            ps->in_use = false;
        }
        return 1;
    }
    if (__atomic_load_n(&ps->ctx.done, __ATOMIC_SEQ_CST)) {
        ps->in_use = false;
        return -9; /* ZPICO_ERR_TIMEOUT */
    }
    return 0;
}

int32_t zpico_get_check(int32_t handle, uint8_t* reply_buf, size_t reply_buf_size) {
    g_diag_get_check_calls++;
    if (handle < 0 || handle >= ZPICO_MAX_PENDING_GETS) {
        g_diag_gck_invalid_arg++;
        return ZPICO_ERR_INVALID;
    }

    pending_get_slot_t* ps = &g_pending_gets[handle];
    if (!ps->in_use) {
        g_diag_gck_not_in_use++;
        return ZPICO_ERR_INVALID;
    }

    /* Same aliasing-defeat trick as in `pending_get_reply_handler`. */
    g_diag_check_ctx_addr = (uint32_t)(uintptr_t)&ps->ctx;
    bool received_snap = __atomic_load_n(&ps->ctx.received, __ATOMIC_SEQ_CST);
    bool done_snap = __atomic_load_n(&ps->ctx.done, __ATOMIC_SEQ_CST);
    if (received_snap) {
        g_diag_get_check_returns_data++;
        // Reply arrived — copy data
        if (ps->ctx.len > reply_buf_size) {
            g_diag_gck_too_big++;
            // Only release slot if dropper has also fired
            if (done_snap) {
                ps->in_use = false;
            }
            return ZPICO_ERR_FULL;
        }
        memcpy(reply_buf, ps->ctx.buf, ps->ctx.len);
        int32_t len = (int32_t)ps->ctx.len;
        // Only release slot if dropper has also fired; otherwise the old
        // z_get's dropper callback still references this slot and would
        // corrupt it if the slot were reused before the dropper fires.
        if (done_snap) {
            ps->in_use = false;
        }
        return len;
    }

    if (done_snap) {
        g_diag_gck_timeout++;
        // Dropper fired without a reply — timeout
        ps->in_use = false;
        return -9; // ZPICO_ERR_TIMEOUT
    }

    g_diag_gck_pending++;
    // Not yet — still pending
    return 0;
}

void zpico_set_reply_waker(zpico_waker_fn fn) {
    g_reply_waker = fn;
}

// ============================================================================
// Query Reply Implementation (for service servers)
// ============================================================================

int32_t zpico_query_reply(int32_t queryable_handle, int64_t reply_seq, const char* keyexpr,
                          const uint8_t* data, size_t len, const uint8_t* attachment,
                          size_t attachment_len) {
    if (queryable_handle < 0 || queryable_handle >= ZPICO_MAX_QUERYABLES) {
        return ZPICO_ERR_INVALID;
    }
    // Phase 237 — `reply_seq` selects the cloned query captured by
    // `query_handler` (the slot index it recorded). The reply may arrive long
    // after the query callback returned (deferred get_result).
    if (reply_seq < 0 || reply_seq >= ZPICO_MAX_PENDING_REPLIES ||
        !g_stored_query_valid[queryable_handle][reply_seq]) {
        return ZPICO_ERR_INVALID;
    }
    z_owned_query_t* stored_query = &g_stored_queries[queryable_handle][reply_seq];

    z_view_keyexpr_t ke;
    if (z_view_keyexpr_from_str(&ke, keyexpr) < 0) {
        return ZPICO_ERR_KEYEXPR;
    }

    // Create payload
    z_owned_bytes_t payload;
    if (z_bytes_copy_from_buf(&payload, data, len) < 0) {
        return ZPICO_ERR_GENERIC;
    }

    // Create reply options with attachment
    z_query_reply_options_t options;
    z_query_reply_options_default(&options);
#if defined(ZPICO_TX_BATCH) && ZPICO_TX_BATCH == 1
    /* phase-279 (#145) — replies bypass the batch (express): service RTT must
     * not wait for the next spin flush when batching is on. */
    options.is_express = true;
#endif

    z_owned_bytes_t attachment_bytes;
    if (attachment != NULL && attachment_len > 0) {
        // Use explicitly provided attachment
        if (z_bytes_copy_from_buf(&attachment_bytes, attachment, attachment_len) < 0) {
            z_bytes_drop(z_bytes_move(&payload));
            return ZPICO_ERR_GENERIC;
        }
        options.attachment = z_bytes_move(&attachment_bytes);
    } else {
        // Echo back the original query's attachment (required by rmw_zenoh_cpp
        // which expects sequence_number, source_timestamp, gid in the reply).
        const z_loaned_bytes_t* query_att = z_query_attachment(z_query_loan(stored_query));
        if (query_att != NULL && z_bytes_len(query_att) > 0) {
            z_owned_slice_t att_slice;
            if (z_bytes_to_slice(query_att, &att_slice) == 0) {
                if (z_bytes_copy_from_buf(&attachment_bytes, z_slice_data(z_slice_loan(&att_slice)),
                                          z_slice_len(z_slice_loan(&att_slice))) == 0) {
                    options.attachment = z_bytes_move(&attachment_bytes);
                }
                z_slice_drop(z_slice_move(&att_slice));
            }
        }
    }

    // Reply using the cloned query held in this reply slot.
    if (z_query_reply(z_query_loan(stored_query), z_view_keyexpr_loan(&ke), z_bytes_move(&payload),
                      &options) < 0) {
        return ZPICO_ERR_GENERIC;
    }

    // Drop the cloned query + free the slot after reply.
    z_query_drop(z_query_move(stored_query));
    g_stored_query_valid[queryable_handle][reply_seq] = false;

    return ZPICO_OK;
}

// Phase 237 — return the reply-slot index allocated by the most recent
// `query_handler` for this queryable (the deferred-reply seq). Must be called
// from inside the synchronous query callback; -1 if the reply table was full.
int64_t zpico_queryable_take_reply_seq(int32_t queryable_handle) {
    if (queryable_handle < 0 || queryable_handle >= ZPICO_MAX_QUERYABLES) {
        return -1;
    }
    return g_last_reply_seq[queryable_handle];
}

// ============================================================================
// Clock helpers (for FFI reentrancy guard timeout decomposition)
// ============================================================================

// Opaque buffer size: z_clock_t is uint64_t (8 bytes) on bare-metal,
// struct { uint64_t tv_sec; uint64_t tv_nsec; } (16 bytes) on ThreadX/POSIX.
_Static_assert(sizeof(z_clock_t) <= 16, "z_clock_t must fit in 16 bytes");

void zpico_clock_start(uint8_t* clock_buf) {
    z_clock_t now = z_clock_now();
    memcpy(clock_buf, &now, sizeof(z_clock_t));
}

unsigned long zpico_clock_elapsed_ms_since(uint8_t* clock_buf) {
    return z_clock_elapsed_ms((z_clock_t*)clock_buf);
}
