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
#include <string.h>

#ifdef ZENOH_ZEPHYR
#include <zephyr/kernel.h>  // For printk
#elif defined(ZENOH_FREERTOS_LWIP)
// On FreeRTOS, route printk to semihosting for debug output.
// Uses SYS_WRITE0 semihosting call (null-terminated string to stdout).
#include <stdio.h>
static void _freertos_printk(const char *fmt, ...) {
    char buf[128];
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(buf, sizeof(buf), fmt, ap);
    va_end(ap);
    // ARM semihosting SYS_WRITE0 (op=0x04): write null-terminated string
    register unsigned r0 __asm__("r0") = 0x04;
    register const char *r1 __asm__("r1") = buf;
    __asm__ volatile("bkpt #0xAB" : : "r"(r0), "r"(r1) : "memory");
}
#define printk(...) _freertos_printk(__VA_ARGS__)
#elif defined(ZENOH_THREADX)
// On ThreadX route printk through printf (Linux sim) or uart_puts (bare-metal)
#include <stdio.h>
#if defined(__linux__)
#define printk(...) printf(__VA_ARGS__)
#else
extern void uart_puts(const char *s);
static void _threadx_printk(const char *fmt, ...) {
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
#define printk(...)  // No libc printf on bare-metal
#else
#define printk(...)  // No-op on other platforms
#endif

// Internal zenoh-pico headers for socket FD access (select()-based timeout).
// Only needed for single-threaded builds; multi-threaded uses z_sleep_ms().
#if !defined(ZPICO_SMOLTCP) && !defined(ZPICO_SERIAL) && !defined(ZENOH_FREERTOS_LWIP) && Z_FEATURE_MULTI_THREAD != 1
#include "zenoh-pico/net/session.h"
#include "zenoh-pico/transport/transport.h"
#include "zenoh-pico/api/olv_macros.h"
#include <sys/select.h>
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
        ZpicoCallback callback;                     // Legacy callback (payload only)
        ZpicoCallbackWithAttachment callback_ext;   // Extended callback (with attachment)
        ZpicoNotifyCallback notify;                 // Direct-write notify (len + attachment)
    };
    void *ctx;
    bool active;
    bool with_attachment;  // true = use callback_ext, false = use callback
    // Direct-write fields (set when mode == direct_write)
    bool direct_write;     // true = direct-write mode
    uint8_t *buf_ptr;      // Pointer into Rust SUBSCRIBER_BUFFERS[i].data
    size_t buf_capacity;   // Size of the Rust buffer
    const bool *locked_ptr; // Pointer to Rust SUBSCRIBER_BUFFERS[i].locked (AtomicBool)
#if defined(Z_FEATURE_UNSTABLE_API)
    bool zero_copy;        // true = zero-copy mode (borrows from zenoh-pico buffer)
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
    void *ctx;
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

// Static storage for zenoh objects
static z_owned_config_t g_config;
static z_owned_session_t g_session;
static bool g_session_open = false;
static bool g_initialized = false;

static publisher_entry_t g_publishers[ZPICO_MAX_PUBLISHERS];
static subscriber_entry_t g_subscribers[ZPICO_MAX_SUBSCRIBERS];
static liveliness_entry_t g_liveliness[ZPICO_MAX_LIVELINESS];
static queryable_entry_t g_queryables[ZPICO_MAX_QUERYABLES];

// Per-queryable storage for cloned queries (for later reply)
static z_owned_query_t g_stored_queries[ZPICO_MAX_QUERYABLES];
static bool g_stored_query_valid[ZPICO_MAX_QUERYABLES];  // zero-initialized = all false

// Context struct for blocking z_get reply (stack-allocated per call)
// ZPICO_GET_REPLY_BUF_SIZE is provided via -D compiler flag from build.rs

typedef struct {
    uint8_t buf[ZPICO_GET_REPLY_BUF_SIZE];
    size_t len;
    bool received;
    bool done;
#if Z_FEATURE_MULTI_THREAD == 1
    _z_mutex_t mutex;
    _z_condvar_t cond;
#endif
} get_reply_ctx_t;

// Static slots for non-blocking z_get operations
// ZPICO_MAX_PENDING_GETS is provided via -D compiler flag from build.rs
typedef struct {
    get_reply_ctx_t ctx;
    bool in_use;
} pending_get_slot_t;

static pending_get_slot_t g_pending_gets[ZPICO_MAX_PENDING_GETS];

// Reply waker callback — invoked when a pending get slot receives a reply
// or times out, allowing Rust async code to wake the corresponding Future.
typedef void (*zpico_waker_fn)(int32_t slot);
static zpico_waker_fn g_reply_waker = NULL;

// Condition variable for multi-threaded spin_once().
// Signaled by our callbacks (sample_handler, query_handler, get reply handlers)
// so spin_once() can wake immediately when application data arrives, rather
// than sleeping for the full timeout duration.
#if Z_FEATURE_MULTI_THREAD == 1 && !defined(ZPICO_SMOLTCP) && !defined(ZENOH_FREERTOS_LWIP)
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
#else
static inline void _zpico_notify_spin(void) {}
#endif

// ============================================================================
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

/**
 * Internal callback for queryable that receives queries
 */
static void query_handler(z_loaned_query_t *query, void *arg) {
    int idx = (int)(intptr_t)arg;
    if (idx < 0 || idx >= ZPICO_MAX_QUERYABLES) {
        return;
    }

    queryable_entry_t *entry = &g_queryables[idx];
    if (!entry->active || entry->callback == NULL) {
        return;
    }

    // Get keyexpr
    const z_loaned_keyexpr_t *keyexpr = z_query_keyexpr(query);
    z_view_string_t keyexpr_view;
    z_keyexpr_as_view_string(keyexpr, &keyexpr_view);
    const char *keyexpr_str = z_string_data(z_view_string_loan(&keyexpr_view));
    size_t keyexpr_len = z_string_len(z_view_string_loan(&keyexpr_view));

    // Get payload
    const z_loaned_bytes_t *payload_bytes = z_query_payload(query);
    const uint8_t *payload_data = NULL;
    size_t payload_len = 0;

    z_owned_slice_t payload_slice;
    if (payload_bytes != NULL && z_bytes_len(payload_bytes) > 0) {
        if (z_bytes_to_slice(payload_bytes, &payload_slice) == 0) {
            payload_data = z_slice_data(z_slice_loan(&payload_slice));
            payload_len = z_slice_len(z_slice_loan(&payload_slice));
        }
    }

    // Drop any previously stored query for this queryable
    if (g_stored_query_valid[idx]) {
        z_query_drop(z_query_move(&g_stored_queries[idx]));
        g_stored_query_valid[idx] = false;
    }

    // Clone the query for later reply (after callback returns)
    if (z_query_clone(&g_stored_queries[idx], query) == 0) {
        g_stored_query_valid[idx] = true;
    }

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
static void sample_handler(z_loaned_sample_t *sample, void *arg) {
    int idx = (int)(intptr_t)arg;
    if (idx < 0 || idx >= ZPICO_MAX_SUBSCRIBERS) {
        return;
    }

    subscriber_entry_t *entry = &g_subscribers[idx];
    if (!entry->active) {
        return;
    }

    // Get payload
    const z_loaned_bytes_t *payload = z_sample_payload(sample);
    size_t payload_len = z_bytes_len(payload);

#if defined(Z_FEATURE_UNSTABLE_API)
    if (entry->zero_copy) {
        if (entry->zero_copy_cb == NULL) {
            return;
        }
        // Get contiguous view — borrows directly from zenoh-pico's receive buffer
        z_view_slice_t view;
        if (z_bytes_get_contiguous_view(payload, &view) == 0) {
            const uint8_t *data = z_slice_data(z_view_slice_loan(&view));
            size_t len = z_slice_len(z_view_slice_loan(&view));

            // Get attachment (small copy, 33-37 bytes)
            const z_loaned_bytes_t *att = z_sample_attachment(sample);
            if (att != NULL) {
                z_owned_slice_t att_slice;
                if (z_bytes_to_slice(att, &att_slice) == 0) {
                    entry->zero_copy_cb(data, len,
                        z_slice_data(z_slice_loan(&att_slice)),
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
        const z_loaned_bytes_t *attachment = z_sample_attachment(sample);
        if (attachment != NULL) {
            z_owned_slice_t attachment_slice;
            if (z_bytes_to_slice(attachment, &attachment_slice) == 0) {
                const uint8_t *att_data = z_slice_data(z_slice_loan(&attachment_slice));
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
        return;  // Failed to get payload
    }

    const uint8_t *data = z_slice_data(z_slice_loan(&payload_slice));
    size_t len = z_slice_len(z_slice_loan(&payload_slice));

    if (entry->with_attachment) {
        // Extended callback with attachment
        if (entry->callback_ext == NULL) {
            z_slice_drop(z_slice_move(&payload_slice));
            return;
        }

        // Get attachment
        const z_loaned_bytes_t *attachment = z_sample_attachment(sample);
        if (attachment != NULL) {
            z_owned_slice_t attachment_slice;
            if (z_bytes_to_slice(attachment, &attachment_slice) == 0) {
                const uint8_t *att_data = z_slice_data(z_slice_loan(&attachment_slice));
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

int32_t zpico_init(const char *locator) {
    return zpico_init_with_config(locator, "client", NULL, 0);
}

int32_t zpico_init_with_config(const char *locator,
                                     const char *mode,
                                     const zpico_property_t *properties,
                                     size_t num_properties) {
    // Initialize storage
    memset(g_publishers, 0, sizeof(g_publishers));
    memset(g_subscribers, 0, sizeof(g_subscribers));
    memset(g_liveliness, 0, sizeof(g_liveliness));
    memset(g_queryables, 0, sizeof(g_queryables));
    memset(g_stored_query_valid, 0, sizeof(g_stored_query_valid));
    for (int i = 0; i < ZPICO_MAX_QUERYABLES; i++) {
        z_internal_query_null(&g_stored_queries[i]);
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

    if (locator != NULL) {
        if (zp_config_insert(z_config_loan_mut(&g_config), Z_CONFIG_CONNECT_KEY, locator) < 0) {
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
#elif (defined(ZENOH_LINUX) || defined(ZENOH_MACOS) || defined(__NuttX__) || \
       defined(ZENOH_ZEPHYR)) && !defined(ZENOH_THREADX)
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

int32_t zpico_open(void) {
    if (!g_initialized) {
        return ZPICO_ERR_GENERIC;
    }

    int open_ret = z_open(&g_session, z_config_move(&g_config), NULL);
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
        printk("zpico: session opened, zid=");
        (void)zid;  // printk may be no-op on bare-metal
    }

    // Start background tasks only in multi-threaded mode
    // In single-threaded mode (Z_FEATURE_MULTI_THREAD=0), polling is done
    // explicitly via zpico_poll() / zpico_spin_once()
#if Z_FEATURE_MULTI_THREAD == 1
#if !defined(ZPICO_SMOLTCP) && !defined(ZENOH_FREERTOS_LWIP)
    _z_mutex_init(&g_spin_mutex);
    _z_condvar_init(&g_spin_cv);
    g_spin_cv_initialized = true;
#endif

    const zp_task_read_options_t *read_opts =
        g_read_task_configured ? &g_read_task_opts : NULL;
    const zp_task_lease_options_t *lease_opts =
        g_lease_task_configured ? &g_lease_task_opts : NULL;

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

    g_session_open = true;
    printk("zpico: session opened successfully\n");
    return ZPICO_OK;
}

int32_t zpico_is_open(void) {
    return g_session_open ? 1 : 0;
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
#if Z_FEATURE_MULTI_THREAD == 1
        // Stop background tasks (only in multi-threaded mode)
        zp_stop_read_task(z_session_loan_mut(&g_session));
        zp_stop_lease_task(z_session_loan_mut(&g_session));

#if !defined(ZPICO_SMOLTCP) && !defined(ZENOH_FREERTOS_LWIP)
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

int32_t zpico_declare_publisher(const char *keyexpr) {
    printk("zpico: declare_pub: session_open=%d max=%d keyexpr=%s\n",
           (int)g_session_open, ZPICO_MAX_PUBLISHERS, keyexpr ? keyexpr : "(null)");
    if (!g_session_open) {
        printk("zpico: declare_pub: SESSION NOT OPEN\n");
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
        printk("zpico: declare_pub: ALL %d SLOTS FULL\n", ZPICO_MAX_PUBLISHERS);
        for (int i = 0; i < ZPICO_MAX_PUBLISHERS; i++) {
            printk("  slot[%d].active=%d\n", i, (int)g_publishers[i].active);
        }
        return ZPICO_ERR_FULL;
    }
    printk("zpico: declare_pub: using slot %d\n", idx);

    z_view_keyexpr_t ke;
    int ke_ret = z_view_keyexpr_from_str(&ke, keyexpr);
    if (ke_ret < 0) {
        printk("zpico: z_view_keyexpr_from_str failed: %d for '%s'\n", ke_ret, keyexpr);
        return ZPICO_ERR_KEYEXPR;
    }

    int pub_ret = z_declare_publisher(z_session_loan(&g_session), &g_publishers[idx].publisher,
                                      z_view_keyexpr_loan(&ke), NULL);
    if (pub_ret < 0) {
        printk("zpico: z_declare_publisher failed: %d for '%s'\n", pub_ret, keyexpr);
        return ZPICO_ERR_GENERIC;
    }

    g_publishers[idx].active = true;
    return idx;
}

int32_t zpico_publish(int32_t handle, const uint8_t *data, size_t len) {
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

int32_t zpico_declare_subscriber(const char *keyexpr,
                                       ZpicoCallback callback,
                                       void *ctx) {
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
    g_subscribers[idx].with_attachment = false;  // Legacy mode

    z_view_keyexpr_t ke;
    if (z_view_keyexpr_from_str(&ke, keyexpr) < 0) {
        g_subscribers[idx].callback = NULL;
        g_subscribers[idx].ctx = NULL;
        return ZPICO_ERR_KEYEXPR;
    }

    // Create closure for callback, passing index as context
    z_owned_closure_sample_t closure;
    z_closure_sample(&closure, sample_handler, NULL, (void *)(intptr_t)idx);

    int sub_ret = z_declare_subscriber(z_session_loan(&g_session), &g_subscribers[idx].subscriber,
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

int32_t zpico_declare_subscriber_with_attachment(const char *keyexpr,
                                                       ZpicoCallbackWithAttachment callback,
                                                       void *ctx) {
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
    g_subscribers[idx].with_attachment = true;  // Extended mode with attachment

    z_view_keyexpr_t ke;
    if (z_view_keyexpr_from_str(&ke, keyexpr) < 0) {
        g_subscribers[idx].callback_ext = NULL;
        g_subscribers[idx].ctx = NULL;
        return ZPICO_ERR_KEYEXPR;
    }

    // Create closure for callback, passing index as context
    z_owned_closure_sample_t closure;
    z_closure_sample(&closure, sample_handler, NULL, (void *)(intptr_t)idx);

    int sub_ret = z_declare_subscriber(z_session_loan(&g_session), &g_subscribers[idx].subscriber,
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

int32_t zpico_declare_subscriber_direct_write(const char *keyexpr,
                                                     uint8_t *buf_ptr,
                                                     size_t buf_capacity,
                                                     const bool *locked_ptr,
                                                     ZpicoNotifyCallback callback,
                                                     void *ctx) {
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
    z_closure_sample(&closure, sample_handler, NULL, (void *)(intptr_t)idx);

    int sub_ret = z_declare_subscriber(z_session_loan(&g_session), &g_subscribers[idx].subscriber,
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

#if defined(Z_FEATURE_UNSTABLE_API)
int32_t zpico_subscribe_zero_copy(const char *keyexpr,
                                        ZpicoZeroCopyCallback callback,
                                        void *ctx) {
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
    z_closure_sample(&closure, sample_handler, NULL, (void *)(intptr_t)idx);

    int sub_ret = z_declare_subscriber(z_session_loan(&g_session), &g_subscribers[idx].subscriber,
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
int32_t zpico_subscribe_zero_copy(const char *keyexpr,
                                        ZpicoZeroCopyCallback callback,
                                        void *ctx) {
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

// get_session_fd() is only needed for single-threaded select()-based paths.
// Multi-threaded builds use z_sleep_ms(); FreeRTOS uses vTaskDelay(); smoltcp
// uses its own clock loop.
#if !defined(ZPICO_SMOLTCP) && !defined(ZPICO_SERIAL) && !defined(ZENOH_FREERTOS_LWIP) && Z_FEATURE_MULTI_THREAD != 1
/**
 * Extract the socket file descriptor from the zenoh session.
 *
 * Path: g_session → _z_session_t._tp._transport._unicast._peers → first peer → _socket
 *
 * Returns -1 if the session is not unicast or has no connected peers.
 */
static int get_session_fd(void) {
    _z_session_t *session = _Z_RC_IN_VAL(z_session_loan(&g_session));
    if (session->_tp._type != _Z_TRANSPORT_UNICAST_TYPE) {
        return -1;
    }
    _z_transport_peer_unicast_t *peer =
        _z_transport_peer_unicast_slist_value(session->_tp._transport._unicast._peers);
    if (peer == NULL) {
        return -1;
    }
    // FreeRTOS+lwIP uses _socket (int), POSIX/bare-metal uses _fd
#if defined(ZENOH_FREERTOS_LWIP) || defined(ZENOH_FREERTOS_PLUS_TCP)
    return peer->_socket._socket;
#else
    return peer->_socket._fd;
#endif
}
#endif

// ============================================================================
// Polling Implementation
// ============================================================================

int32_t zpico_poll(uint32_t timeout_ms) {
    if (!g_session_open) {
        return ZPICO_ERR_SESSION;
    }

#ifdef ZPICO_SMOLTCP
    // smoltcp: use single_read=true to avoid the _z_zbuf_reset() in the
    // single_read=false path, which discards remaining data when multiple
    // zenoh messages arrive in a single TCP read (e.g., keep-alive + reply).
    // With single_read=true, the zbuf accumulates data across calls and
    // processes one complete message per call.
    zp_read_options_t opts;
    opts.single_read = true;
    uint64_t start = smoltcp_clock_now_ms();
    int ret;
    do {
        ret = zp_read(z_session_loan_mut(&g_session), &opts);
        if (ret == 0) break;  // Data processed
        if (timeout_ms == 0) break;
    } while (smoltcp_clock_now_ms() - start < timeout_ms);
    return ret;

#elif defined(ZENOH_FREERTOS_LWIP)
    // FreeRTOS+lwIP: background read task handles data. Use vTaskDelay()
    // instead of select() to yield CPU time. lwIP's select() can interact
    // poorly with the background read task calling recv() on the same socket.
    extern void vTaskDelay(unsigned long);
    if (timeout_ms > 0) {
        vTaskDelay(timeout_ms);
    }
    return 0;

#elif Z_FEATURE_MULTI_THREAD == 1
    // Multi-threaded (Zephyr/POSIX): background read task handles data.
    // Wait on condvar — see zpico_spin_once() for the rationale.
    if (timeout_ms > 0) {
        z_clock_t deadline = z_clock_now();
        z_clock_advance_ms(&deadline, (unsigned long)timeout_ms);
        _z_mutex_lock(&g_spin_mutex);
        _z_condvar_wait_until(&g_spin_cv, &g_spin_mutex, &deadline);
        _z_mutex_unlock(&g_spin_mutex);
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
            return (result == 0) ? ZPICO_ERR_TIMEOUT : result;
        }
    }
    return zp_read(z_session_loan_mut(&g_session), NULL);
#endif
}

int32_t zpico_spin_once(uint32_t timeout_ms) {
    if (!g_session_open) {
        return ZPICO_ERR_SESSION;
    }

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
        if (ret == 0 && timeout_ms != 0) break;  // Processed one message (timeout mode)
        if (ret != 0 && timeout_ms == 0) break;  // No data (drain mode)
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
        if (ret == 0) break;  // Data processed
        if (timeout_ms == 0) break;
    } while (z_clock_elapsed_ms(&start) < timeout_ms);
    _z_pending_query_process_timeout(_Z_RC_IN_VAL(z_session_loan_mut(&g_session)));
    zp_send_keep_alive(z_session_loan_mut(&g_session), NULL);
    return ret;

#elif defined(ZENOH_FREERTOS_LWIP)
    // FreeRTOS+lwIP: background read and lease tasks handle data and
    // keep-alives respectively. Just yield CPU time with vTaskDelay()
    // to let lower-priority tasks (network poll) run. Do NOT call
    // zp_send_keep_alive() here — the lease task handles it, and
    // sending from the app task contends on the TX mutex which can
    // block indefinitely when the background tasks hold it.
    extern void vTaskDelay(unsigned long);
    if (timeout_ms > 0) {
        vTaskDelay(timeout_ms);
    }
    return 0;

#elif Z_FEATURE_MULTI_THREAD == 1
    // Multi-threaded (Zephyr/POSIX): background read and lease tasks handle
    // data and keep-alives. Wait on a condvar that our callbacks signal when
    // application data arrives (subscriptions, query replies, service requests).
    // This gives near-zero latency wake-up without busy-looping on select().
    if (timeout_ms > 0) {
        z_clock_t deadline = z_clock_now();
        z_clock_advance_ms(&deadline, (unsigned long)timeout_ms);
        _z_mutex_lock(&g_spin_mutex);
        _z_condvar_wait_until(&g_spin_cv, &g_spin_mutex, &deadline);
        _z_mutex_unlock(&g_spin_mutex);
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

int32_t zpico_get_zid(uint8_t *zid_out) {
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

int32_t zpico_declare_liveliness(const char *keyexpr) {
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

    int lv_ret = z_liveliness_declare_token(z_session_loan(&g_session),
                                            &g_liveliness[idx].token,
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

int32_t zpico_publish_with_attachment(int32_t handle,
                                            const uint8_t *data, size_t len,
                                            const uint8_t *attachment, size_t attachment_len) {
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

    if (z_publisher_put(z_publisher_loan(&g_publishers[handle].publisher),
                        z_bytes_move(&payload), &options) < 0) {
        return ZPICO_ERR_PUBLISH;
    }

    return ZPICO_OK;
}

// ============================================================================
// Queryable Implementation (for ROS 2 Services)
// ============================================================================

int32_t zpico_declare_queryable(const char *keyexpr,
                                      ZpicoQueryCallback callback,
                                      void *ctx) {
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
    z_closure_query(&closure, query_handler, NULL, (void *)(intptr_t)idx);

    printk("zpico: declaring queryable[%d] keyexpr='%s'\n", idx, keyexpr);

    // Set complete=true so that queries with Z_QUERY_TARGET_ALL_COMPLETE
    // (used by rmw_zenoh_cpp service clients) match this queryable.
    z_queryable_options_t opts;
    z_queryable_options_default(&opts);
    opts.complete = true;

    int q_ret = z_declare_queryable(z_session_loan(&g_session), &g_queryables[idx].queryable,
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
    return ZPICO_OK;
}

// ============================================================================
// Service Client Implementation (z_get for ROS 2 service calls)
// ============================================================================

/**
 * Internal callback for z_get reply handling
 */
static void get_reply_handler(z_loaned_reply_t *reply, void *ctx) {
    get_reply_ctx_t *rctx = (get_reply_ctx_t *)ctx;

    // Only process successful replies
    if (!z_reply_is_ok(reply)) {
        return;
    }

    // Skip if we already have a reply (only take first)
    if (rctx->received) {
        return;
    }

    const z_loaned_sample_t *sample = z_reply_ok(reply);
    const z_loaned_bytes_t *payload = z_sample_payload(sample);

    // Copy payload to reply buffer
    z_owned_slice_t slice;
    if (z_bytes_to_slice(payload, &slice) == 0) {
        const uint8_t *data = z_slice_data(z_slice_loan(&slice));
        size_t len = z_slice_len(z_slice_loan(&slice));

        if (len <= ZPICO_GET_REPLY_BUF_SIZE) {
            memcpy(rctx->buf, data, len);
            rctx->len = len;
            rctx->received = true;
        }
        z_slice_drop(z_slice_move(&slice));
    }
}

/**
 * Internal callback for z_get completion (dropper)
 */
static void get_reply_dropper(void *ctx) {
    get_reply_ctx_t *rctx = (get_reply_ctx_t *)ctx;
#if Z_FEATURE_MULTI_THREAD == 1
    _z_mutex_lock(&rctx->mutex);
    rctx->done = true;
    _z_condvar_signal(&rctx->cond);
    _z_mutex_unlock(&rctx->mutex);
#else
    rctx->done = true;
#endif
}

int32_t zpico_get(const char *keyexpr,
                       const uint8_t *payload, size_t payload_len,
                       uint8_t *reply_buf, size_t reply_buf_size,
                       uint32_t timeout_ms) {
    if (!g_session_open) {
        return ZPICO_ERR_SESSION;
    }

    // Stack-allocated reply context (safe for concurrent z_get calls)
    get_reply_ctx_t ctx;
    ctx.len = 0;
    ctx.received = false;
    ctx.done = false;
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
    if (z_get(z_session_loan(&g_session), z_view_keyexpr_loan(&ke), "",
              z_move(callback), &opts) < 0) {
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
        return -9;  // ZPICO_ERR_TIMEOUT (defined in Rust FFI)
    }

    // Copy reply to output buffer
    if (ctx.len > reply_buf_size) {
        return ZPICO_ERR_FULL;  // Buffer too small
    }

    memcpy(reply_buf, ctx.buf, ctx.len);
    return (int32_t)ctx.len;
}

// ============================================================================
// Non-blocking z_get (for async service client)
// ============================================================================

// Reply handler for pending get slots — reuses the same logic as get_reply_handler
static void pending_get_reply_handler(z_loaned_reply_t *reply, void *ctx) {
    get_reply_handler(reply, ctx);
    _zpico_notify_spin();
    if (g_reply_waker) {
        // ctx is &ps->ctx which is the first field, so ps == (pending_get_slot_t*)ctx
        int32_t slot = (int32_t)((pending_get_slot_t *)ctx - g_pending_gets);
        g_reply_waker(slot);
    }
}

// Dropper for pending get slots — just sets the done flag (no condvar)
static void pending_get_dropper(void *ctx) {
    get_reply_ctx_t *rctx = (get_reply_ctx_t *)ctx;
    rctx->done = true;
    _zpico_notify_spin();
    if (g_reply_waker) {
        int32_t slot = (int32_t)((pending_get_slot_t *)ctx - g_pending_gets);
        g_reply_waker(slot);
    }
}

int32_t zpico_get_start(const char *keyexpr,
                              const uint8_t *payload, size_t payload_len,
                              uint32_t timeout_ms) {
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
    pending_get_slot_t *ps = &g_pending_gets[slot];
    ps->ctx.len = 0;
    ps->ctx.received = false;
    ps->ctx.done = false;
    ps->in_use = true;

    z_view_keyexpr_t ke;
    z_view_keyexpr_from_str(&ke, keyexpr);

    z_get_options_t opts;
    z_get_options_default(&opts);
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

    z_owned_closure_reply_t callback;
    z_closure(&callback, pending_get_reply_handler, pending_get_dropper, &ps->ctx);

    z_result_t zret = z_get(z_session_loan(&g_session), z_view_keyexpr_loan(&ke), "",
                             z_move(callback), &opts);
    if (zret < 0) {
        ps->in_use = false;
        return ZPICO_ERR_GENERIC;
    }

    return slot;
}

int32_t zpico_get_check(int32_t handle,
                              uint8_t *reply_buf, size_t reply_buf_size) {
    if (handle < 0 || handle >= ZPICO_MAX_PENDING_GETS) {
        return ZPICO_ERR_INVALID;
    }

    pending_get_slot_t *ps = &g_pending_gets[handle];
    if (!ps->in_use) {
        return ZPICO_ERR_INVALID;
    }

    if (ps->ctx.received) {
        // Reply arrived — copy data
        if (ps->ctx.len > reply_buf_size) {
            // Only release slot if dropper has also fired
            if (ps->ctx.done) {
                ps->in_use = false;
            }
            return ZPICO_ERR_FULL;
        }
        memcpy(reply_buf, ps->ctx.buf, ps->ctx.len);
        int32_t len = (int32_t)ps->ctx.len;
        // Only release slot if dropper has also fired; otherwise the old
        // z_get's dropper callback still references this slot and would
        // corrupt it if the slot were reused before the dropper fires.
        if (ps->ctx.done) {
            ps->in_use = false;
        }
        return len;
    }

    if (ps->ctx.done) {
        // Dropper fired without a reply — timeout
        ps->in_use = false;
        return -9;  // ZPICO_ERR_TIMEOUT
    }

    // Not yet — still pending
    return 0;
}

void zpico_set_reply_waker(zpico_waker_fn fn) {
    g_reply_waker = fn;
}

// ============================================================================
// Query Reply Implementation (for service servers)
// ============================================================================

int32_t zpico_query_reply(int32_t queryable_handle,
                                const char *keyexpr,
                                const uint8_t *data, size_t len,
                                const uint8_t *attachment, size_t attachment_len) {
    if (queryable_handle < 0 || queryable_handle >= ZPICO_MAX_QUERYABLES) {
        return ZPICO_ERR_INVALID;
    }
    if (!g_stored_query_valid[queryable_handle]) {
        return ZPICO_ERR_INVALID;
    }

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
        const z_loaned_bytes_t *query_att = z_query_attachment(
            z_query_loan(&g_stored_queries[queryable_handle]));
        if (query_att != NULL && z_bytes_len(query_att) > 0) {
            z_owned_slice_t att_slice;
            if (z_bytes_to_slice(query_att, &att_slice) == 0) {
                if (z_bytes_copy_from_buf(&attachment_bytes,
                        z_slice_data(z_slice_loan(&att_slice)),
                        z_slice_len(z_slice_loan(&att_slice))) == 0) {
                    options.attachment = z_bytes_move(&attachment_bytes);
                }
                z_slice_drop(z_slice_move(&att_slice));
            }
        }
    }

    // Reply using the stored (cloned) query for this queryable
    if (z_query_reply(z_query_loan(&g_stored_queries[queryable_handle]),
                      z_view_keyexpr_loan(&ke),
                      z_bytes_move(&payload), &options) < 0) {
        return ZPICO_ERR_GENERIC;
    }

    // Drop the stored query after reply
    z_query_drop(z_query_move(&g_stored_queries[queryable_handle]));
    g_stored_query_valid[queryable_handle] = false;

    return ZPICO_OK;
}

// ============================================================================
// Clock helpers (for FFI reentrancy guard timeout decomposition)
// ============================================================================

// Opaque buffer size: z_clock_t is uint64_t (8 bytes) on bare-metal,
// struct { uint64_t tv_sec; uint64_t tv_nsec; } (16 bytes) on ThreadX/POSIX.
_Static_assert(sizeof(z_clock_t) <= 16, "z_clock_t must fit in 16 bytes");

void zpico_clock_start(uint8_t *clock_buf) {
    z_clock_t now = z_clock_now();
    memcpy(clock_buf, &now, sizeof(z_clock_t));
}

unsigned long zpico_clock_elapsed_ms_since(uint8_t *clock_buf) {
    return z_clock_elapsed_ms((z_clock_t *)clock_buf);
}
