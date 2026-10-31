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
#include "zpico_config_keys.h"
/* The platform ABI: `nros_platform_task_attr_t` for the executor task's
 * attributes, and the multicast / UDP glue in `platform_net.h`. */
#include <nros/platform.h>
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
// Issue 0626 — pthread scheduling attributes for the read/lease tasks.
#include <pthread.h>
#include <sched.h>
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
#elif defined(ZENOH_NUTTX)
// Issue 0870 — NuttX had NO arm here, so it fell to the no-op below and every
// diagnostic in this file compiled away. That is not a platform limitation:
// NuttX has full POSIX stdio and this TU already includes <unistd.h> for it
// above. The cost was real -- two messages that name a fault outright never
// reached the console, including the one whose own comment calls a failed
// liveliness token "a SILENT graph outage ... say so on the console":
//
//     zpico: z_declare_subscriber (ring) failed: %d for '%s'
//     zpico: z_liveliness_declare_token failed: %d for '%s'
//
// so a NuttX failure could only ever be read through the collapsed Rust-side
// return code, which is how 0870 spent three diagnoses on the wrong layer.
#include <stdio.h>
#define printk(...) printf(__VA_ARGS__)
#elif defined(ZPICO_SMOLTCP) || defined(ZPICO_SERIAL)
#define printk(...) // No libc printf on bare-metal
#elif defined(ZENOH_LINUX) || defined(ZENOH_MACOS)
// Same hole issue 0870 found on NuttX, one arm over: the hosted POSIX build
// defines ZENOH_LINUX (or ZENOH_MACOS) and NONE of the arms above, so it fell
// to the no-op below and every diagnostic in this TU compiled away on the one
// platform where a developer is most likely to be reading them. A host has
// full stdio; there is no reason for it to be the quietest target in the tree.
#include <stdio.h>
#define printk(...) printf(__VA_ARGS__)
#else
#define printk(...) // No-op on other platforms
#endif

// ============================================================================
// How this build drives zenoh-pico's executor — RFC-0110 D1, chosen ONCE here
// ============================================================================
//
// zenoh-pico 1.9 replaced the read and lease tasks with one asynchronous
// executor: read, lease, keep-alive, query timeout and reconnect are futures
// in one run queue. Something has to run that queue, and there are two ways:
//
//   ZPICO_DRIVE_THREAD  the executor runs on its own thread (`zp_start_read_task`
//                       starts it). For a platform where a blocked receive is a
//                       wait the scheduler can see: POSIX, NuttX, Zephyr and
//                       FreeRTOS.
//   ZPICO_DRIVE_CALLER  the executor runs inside `zpico_spin_once`, in the
//                       caller's thread, a bounded number of steps per call.
//                       For a build with no second thread (bare-metal smoltcp
//                       and serial, any single-thread build) and for ThreadX,
//                       whose Linux host port blocks a pthread in a host `recv`
//                       the ThreadX scheduler still counts as running
//                       (phase-297 W5).
//
// Every condition that used to spell this choice inline reads the macro, so
// the table in RFC-0110 D1 has exactly one implementation.
#if Z_FEATURE_MULTI_THREAD == 1 && !defined(ZENOH_THREADX) && !defined(ZPICO_SMOLTCP) &&           \
    !defined(ZPICO_SERIAL)
#define ZPICO_DRIVE_THREAD 1
#else
#define ZPICO_DRIVE_CALLER 1
#endif

// Issue 2105 -- THREAD mode's executor checks its timers (lease, keep-alive,
// query timeout) only when the blocking receive returns, so that receive MUST
// time out. Zephyr honours `SO_RCVTIMEO` only under CONFIG_NET_CONTEXT_RCVTIMEO,
// and zenoh-pico's `tcp_zephyr.c` logs the refusal and carries on. Without it
// the receive blocks until data arrives, and every timed future waits for the
// router: measured on mps2_an385, the keep-alive went out at +4 s and +64 s
// and the session reopened. `NROS_RMW_ZENOH` selects the symbol whenever
// sockets exist; this refuses an image that turned it back off.
#if defined(ZENOH_ZEPHYR) && defined(ZPICO_DRIVE_THREAD) && defined(CONFIG_NET_SOCKETS) &&       \
    !defined(CONFIG_NET_CONTEXT_RCVTIMEO)
#error "zenoh on Zephyr needs CONFIG_NET_CONTEXT_RCVTIMEO=y: without it the executor's receive never times out and the lease and keep-alive never run (issue 2105)"
#endif

// Internal zenoh-pico headers. `_zp_send_keep_alive` (RFC-0110 D5) is the one
// keep-alive call every build has: the public `zp_send_keep_alive` is compiled
// out of multi-thread builds.
#if defined(ZENOH_THREADX)
#include "nxd_bsd.h"
#endif
#include "zenoh-pico/net/session.h"
#include "zenoh-pico/transport/transport.h"
#include "zenoh-pico/api/olv_macros.h"

// ============================================================================
// Platform-Specific Declarations
// ============================================================================

#ifdef ZPICO_SMOLTCP
// External Rust FFI functions for smoltcp platform
extern int32_t smoltcp_init(void);
extern void smoltcp_cleanup(void);
extern uint64_t smoltcp_clock_now_ms(void);
// issue 2022 — nros-smoltcp's tickless idle. Returns at once unless the board
// installed a sleep primitive (one with an RX-interrupt wake source).
extern void nros_smoltcp_idle_until_ms(uint64_t deadline_ms);
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
    /* Transient-local history window (`zpico_subscriber_history_query`).
     *
     * While a subscription's one history query is outstanding, the ring has two
     * producers of the same stream: the publisher cache's REPLY and the live
     * SAMPLE, in either order. A latched topic makes the order matter -- a cache
     * reply that lands after a newer live sample would leave the ring's last
     * entry stale until the next change, which for an on-change topic can be
     * never. So each sample in the window is checked against the newest
     * (publisher GID, sequence number) the window has already delivered, and a
     * sample from the same publisher that is not newer is dropped.
     *
     * One pair, not a table: a latched topic has one writer in every case this
     * serves, and a second writer's samples are delivered (different GID) --
     * what is lost is only the cross-writer dedup, never a sample. The window
     * closes when the query's dropper fires. `hist_active` is written by the
     * caller's thread before the query is sent and by the dropper after; the
     * rest only by the ring's producer (the read task), so they need no lock. */
    volatile bool hist_active;
    bool hist_have;
    uint8_t hist_gid[16];
    int64_t hist_seq;
    uint32_t hist_replies;
    uint32_t hist_dropped;
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
    /* Issue 2083 -- the token's keyexpr, heap-held while the token lives, so
     * the session's own graph cache can hold it (zenoh-pico never delivers a
     * session's own tokens to its own liveliness subscriber). NULL when graph
     * discovery is compiled out. */
    char* key;
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
/* Issue 0626 — the transport tasks' priority, on the NORMALISED 0-31 band
 * (0 = least urgent, larger = more urgent; phase-364 W5). 16 keeps the historic
 * behaviour of the one platform that already stated a value: it is what the
 * FreeRTOS board's `zenoh_read_priority` default has always been.
 *
 * This is a default, not a policy. Whether transport should outrank the
 * application's RT tiers is a per-system decision with real costs on both sides
 * (issues 0506 and 0623) — the point of these knobs is that the decision can be
 * MADE, which on Zephyr it previously could not be. */
#ifndef ZPICO_READ_TASK_PRIORITY
#define ZPICO_READ_TASK_PRIORITY 16
#endif
/* RFC-0110 D2 — zenoh-pico 1.10 has no lease task, so its priority knob is
 * retired, and refused rather than ignored. */
#ifdef ZPICO_LEASE_TASK_PRIORITY
#error "ZPICO_LEASE_TASK_PRIORITY is retired: zenoh-pico 1.10 runs the lease on its executor thread (RFC-0110 D2). Set ZPICO_READ_TASK_PRIORITY instead."
#endif
#ifndef ZPICO_GRAPH_CACHE_SIZE
/* Bytes of NUL-separated liveliness keyexprs the graph cache holds.
 *
 * A ROS 2 liveliness keyexpr runs ~140 bytes. Measured against a host running
 * one stock `talker` plus the ROS 2 daemon, the domain carried 222 tokens —
 * ~31 KB — so 8192 held 51 of them and dropped 171. Sized for that measurement
 * rather than for the talker alone, and still a KNOB: an embedded image with a
 * small graph should turn it down. Overflow is COUNTED, never silently
 * truncated —
 * `dropped` is what `zpico_graph_cache_copy` reports, because a graph answer
 * that quietly omits entries is the plausible-wrong-answer failure phase-381
 * exists to avoid. */
#define ZPICO_GRAPH_CACHE_SIZE 65536
#endif
#ifndef ZPICO_GRAPH_DISCOVERY
/* Whether this image keeps a graph cache at all (D9 of the safety-island demo;
 * Kconfig NROS_ZENOH_GRAPH_DISCOVERY, env/cargo ZPICO_GRAPH_DISCOVERY).
 *
 * 1: `zpico_graph_cache_start` declares the standing liveliness SUBSCRIBER on
 * `@ros2_lv/<domain>/**` and the router replays the WHOLE domain's tokens into
 * `graph_cache_t.buf`, then pushes every later change.
 *
 * 0: no subscriber is declared and no cache exists -- the array, the mutex and
 * the subscriber handle are compiled out. The session still DECLARES its own
 * tokens (`zpico_declare_liveliness` is untouched), so a host's
 * `ros2 node list` sees this image exactly as before; what goes is the reverse
 * direction. On a serial or CAN link that direction is the problem: a host
 * running Autoware carries ~136 nodes, and their history burst either stalls
 * the link's read task or exhausts the heap (the safety island measured both,
 * over UART and over TCP with a 1 MiB heap). The graph entry points answer
 * `ZPICO_ERR_CONFIG`, which the Rust side reports as `Unsupported`. */
#define ZPICO_GRAPH_DISCOVERY 1
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

// issue 0348 / phase-328 — the shim is multi-session (handle-passing). Every
// per-session zenoh object that used to be a file-scope static (config, the
// session handle, the lifecycle flags, and the registration tables) now lives
// in `struct zpico_session`, defined below after the reply-slot typedefs. Every
// `zpico_*` entry point takes a leading `zpico_session_t*` handle into the
// compile-time session pool.

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
// `stored_queries` / `stored_query_valid` / `last_reply_seq` are per-session —
// they live in `struct zpico_session` (defined below). The index of the slot
// allocated by the most recent `query_handler` invocation is read by
// `zpico_queryable_take_reply_seq` from inside the (synchronous) user callback;
// -1 if the table was full (the reply is dropped — back-pressure).
#if defined(ZPICO_SMOLTCP) || defined(ZPICO_SERIAL)
static uint32_t g_session_zid_counter;
#else
static atomic_uint g_session_zid_counter;
#endif

// Reply context of one pending get slot (`pending_gets[i].ctx`). Phase-502 W3
// deleted the blocking ~~`zpico_get`~~, the only user of a stack-allocated context
// and of the mutex/condvar pair it carried.
// ZPICO_GET_REPLY_BUF_SIZE is provided via -D compiler flag from build.rs

/* Issue 2136 — room for a reply's rmw attachment: [seq i64][ts i64][gid len
 * u8][gid 16], 33 bytes, the layout rmw_zenoh_cpp writes and our servers echo.
 * A longer (foreign) attachment is not stored, and the reply reports no
 * metadata rather than a truncated one. */
#define ZPICO_REPLY_ATTACHMENT_CAP 33

typedef struct {
    uint8_t buf[ZPICO_GET_REPLY_BUF_SIZE];
    size_t len;
    /* Phase 127.D — atomic+volatile so fat-LTO doesn't hoist the
     * read in `zpico_get_check` over the write inside
     * `get_reply_handler` (callback fired by zenoh-pico's RX path
     * during an executor read step, which sits between two polls of the same
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
    /* phase-381 W1 — GRAPH ENUMERATION.
     *
     * A liveliness reply carries its information in the KEYEXPR, not the
     * payload: `@ros2_lv/<domain>/<zid>/<nid>/<eid>/…/<namespace>/<node>`. The
     * handler used to discard it and keep only `reply_count`, so the shim could
     * answer "does anything match" and never "what matched".
     *
     * When `collect` is set, the handler appends each reply's keyexpr into
     * `buf` as NUL-terminated strings instead of copying a payload there.
     * That buffer is otherwise DEAD on a liveliness query — the payload of a
     * liveliness token is empty, and the pre-W1 comment on
     * `zpico_liveliness_get_start` says so: "the reply handler still copies the
     * (typically empty) liveliness token bytes into the slot's buffer; we never
     * read them". So enumeration costs no new storage, which is what phase-381's
     * deferred bounded-memory decision turned out to be about.
     *
     * `entry_count` counts what was STORED; `reply_count` keeps counting what
     * ARRIVED. The difference is the truncation the caller must be able to see —
     * a graph query that silently drops entries is the plausible-wrong-answer
     * failure this phase exists to avoid. */
    bool collect;
    uint32_t entry_count;
    /* Issue 2136 — the stored reply's metadata, upstream `rmw_take_response`'s
     * `rmw_service_info_t`: its attachment (rmw_zenoh_cpp's layout, which
     * carries the request's sequence number, the replier's send time and the
     * requesting client's gid) and the wall-clock time the reply arrived.
     * `att_len = 0`: the reply had no attachment, or one that does not fit.
     * `recv_ns = 0`: no wall clock. Written before `received` is published,
     * so `zpico_get_check_info` reads them under the same Acquire. */
    uint8_t att[ZPICO_REPLY_ATTACHMENT_CAP];
    size_t att_len;
    int64_t recv_ns;
} get_reply_ctx_t;

/* phase-381 / issue 0903 — the GRAPH CACHE.
 *
 * Enumeration used to be a `z_liveliness_get` per question. That is an INTEREST
 * declaration under the hood (`_z_liveliness_query` sends TOKENS | KEYEXPRS |
 * RESTRICTED | CURRENT), and a token only reaches a get's callback when the
 * router tags its declaration with THAT interest id. Measured against a live
 * `rmw_zenoh_cpp` talker, two sweeps in flight did not both receive replies —
 * whichever started first got tokens and the other got `arrived=0` across 99
 * drains — and a single sweep saw 2-4 of the dozen tokens the talker declares,
 * a different subset each run. Widening the pattern to `**` made it worse.
 *
 * A SUBSCRIBER with `history` is what `rmw_zenoh_cpp` itself uses for its graph
 * cache, and it removes the whole class: one standing declaration, the current
 * tokens delivered once, every later change pushed. No sweeps, so nothing to
 * serialize, nothing to starve, and no per-sweep truncation. */
#if ZPICO_GRAPH_DISCOVERY
typedef struct {
    /* NUL-separated keyexprs, same layout `zpico_entry_at` already walks. */
    uint8_t buf[ZPICO_GRAPH_CACHE_SIZE];
    size_t len;
    uint32_t entry_count;
    /* Tokens that did not fit. Reported, never hidden. */
    uint32_t dropped;
    bool active;
    z_owned_subscriber_t sub;
#if Z_FEATURE_MULTI_THREAD == 1
    _z_mutex_t mutex;
#endif
} graph_cache_t;
#endif /* ZPICO_GRAPH_DISCOVERY */

// Static slots for non-blocking z_get operations
// ZPICO_MAX_PENDING_GETS is provided via -D compiler flag from build.rs,
// with a default fallback above for non-Cargo build paths.
/* Issue 1015 -- these three pools are FIXED C ARRAYS, so zero is not a smaller
 * pool: a zero-length array is a GNU extension that compiles silently, and
 * none of these is the last member of its struct. A derived 0 (an image that
 * declares no service servers derives exactly that) produced a board that
 * transmitted NOTHING -- no panic, no log, core in WFI -- while every gate
 * stayed green, because the value was derived correctly and delivered
 * faithfully. It was simply not a usable size.
 *
 * The derivation floors these at 1 (nros-cli-core entity_inventory.rs). This
 * is the second line, here rather than there because a floor in one producer
 * does not bind another, and this is where the array actually is. */
#if ZPICO_MAX_QUERYABLES < 1
#error "ZPICO_MAX_QUERYABLES must be >= 1: it sizes a C array (issue 1015)"
#endif
#if ZPICO_MAX_LIVELINESS < 1
#error "ZPICO_MAX_LIVELINESS must be >= 1: it sizes a fixed C array (issue 1015)"
#endif
#if ZPICO_MAX_PENDING_GETS < 1
#error "ZPICO_MAX_PENDING_GETS must be >= 1: it sizes a fixed C array (issue 1015)"
#endif
#if ZPICO_MAX_PENDING_REPLIES < 1
#error "ZPICO_MAX_PENDING_REPLIES must be >= 1: it sizes a fixed C array (issue 1015)"
#endif
#if ZPICO_GET_REPLY_BUF_SIZE < 1
#error "ZPICO_GET_REPLY_BUF_SIZE must be >= 1: it sizes a fixed C array (issue 1015)"
#endif
#if ZPICO_GRAPH_CACHE_SIZE < 1
#error "ZPICO_GRAPH_CACHE_SIZE must be >= 1: it sizes a fixed C array (issue 1015)"
#endif
/* A bool, and the preprocessor reads anything else as a number: a stray `2`
 * would compile the cache in while a reader of the define believed otherwise. */
#if ZPICO_GRAPH_DISCOVERY != 0 && ZPICO_GRAPH_DISCOVERY != 1
#error "ZPICO_GRAPH_DISCOVERY must be 0 or 1"
#endif
/* ZPICO_ZID_SIZE is NOT guarded, deliberately. `zpico.h:28` defines it
 * UNCONDITIONALLY -- no `#ifndef` -- so no build can set it to 0 and a
 * `#if ... < 1` there can never fire. Measured: `-DZPICO_ZID_SIZE=0`
 * produces no diagnostic, because the header redefines it first. A
 * guard that cannot fire reads as coverage and is not, which is the
 * shape this whole family of gates exists to remove. Make it settable
 * first if a guard is ever wanted. */
#if ZPICO_MAX_SUBSCRIBERS < 1
#error "ZPICO_MAX_SUBSCRIBERS must be >= 1: it sizes a C array (issue 1015)"
#endif
#if ZPICO_MAX_PUBLISHERS < 1
#error "ZPICO_MAX_PUBLISHERS must be >= 1: it sizes a C array (issue 1015)"
#endif

typedef struct {
    get_reply_ctx_t ctx;
    bool in_use;
} pending_get_slot_t;

// The `pending_gets` slot pool is per-session — see `struct zpico_session` below.

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

/* issue 0902 / phase-455 W2 — the reply-slot refusal count with NO session
 * argument.
 *
 * This is a SECOND reader of the W1 counters, not a replacement, because it
 * answers a DIFFERENT question. `zpico_reply_slot_stats` answers "which server
 * saturated, and how badly" and needs the session handle and the queryable
 * handle to do it. A probe binary holds neither: it owns an `Executor` and a
 * node, and the zenoh `Context` is several layers below the API it was written
 * against. So the question it can actually ask is "did ANY reply slot get
 * refused in this process" — which is exactly the acceptance phase-455 W2
 * needs, and exactly what a process-global counter can carry.
 *
 * Deliberately NOT folded into `zpico_get_diag_counters`: that array's length
 * lives only in its C prototype (Rust binds it as a bare `*mut u32`), so
 * growing it is an undiagnosable overrun for any caller not recompiled with
 * it. A named accessor cannot be got wrong that way. */
static volatile uint32_t g_reply_slot_refusals_total = 0;
static volatile uint32_t g_diag_check_ctx_addr = 0;
static volatile uint32_t g_diag_start_ctx_addr = 0;

// Reply waker callback — invoked when a pending get slot receives a reply
// or times out, allowing Rust async code to wake the corresponding Future.
// Per-session (stored in `struct zpico_session.reply_waker`); set via
// `zpico_set_reply_waker(s, fn)`. phase-328/#376 — carries the owning session's
// pool index alongside the slot so the Rust waker table can be session-scoped
// (two sessions' pending-get slot N must not wake each other's future).
typedef void (*zpico_waker_fn)(int32_t session_index, int32_t slot);

// Graph-change callback — invoked from the liveliness subscriber's sample
// handler when a token PUT or DELETE actually CHANGED the cached token set
// (phase-467 Row 8). Per-session (stored in
// `struct zpico_session.graph_change_cb`), set via
// `zpico_set_graph_change_cb(s, fn)`. Carries the owning session's pool index
// for the same reason `zpico_waker_fn` does: two sessions' graph views must
// not wake each other's guard condition.
//
// An EDGE, with no payload. What changed is answered by re-reading the cache,
// which every graph query already does; buffering a diff is the graph cache a
// small target cannot afford. The callback fires OUTSIDE the cache mutex, so a
// handler that reads the graph cannot deadlock against the sample that woke
// it.
typedef void (*zpico_graph_change_fn)(int32_t session_index);

// ============================================================================
// Session pool (issue 0348 / phase-328 — multi-session handle-passing)
// ============================================================================
//
// Every per-session zenoh object that used to be a file-scope static now lives
// in `struct zpico_session`, held in a compile-time pool (`ZPICO_MAX_SESSIONS`,
// default 1 so single-session targets keep TODAY's footprint plus one `bool`
// per slot). No heap — embedded targets do not assume `malloc`. The diagnostic
// counters and the ZID uniquifier stay process-global (below / above).
//
// The generated header (zpico.h) exposes `zpico_session_t` as an opaque
// 0-length stub and every `zpico_*` prototype takes a `zpico_session_t*`. The
// concrete storage therefore lives under a DISTINCT tag `struct zpico_session`;
// each entry point reinterprets its `zpico_session_t*` handle as a
// `struct zpico_session*`.

// Some struct members need platform types whose headers were previously pulled
// in inside the spin-primitive section; hoist those includes above the struct.
#if Z_FEATURE_MULTI_THREAD == 1 && !defined(ZPICO_SMOLTCP)
#if defined(ZENOH_FREERTOS_LWIP)
#include <FreeRTOS.h>
#include <semphr.h>
#elif defined(ZENOH_NUTTX)
#include <semaphore.h>
#include <errno.h>
#endif
#endif

// phase-279 (#145) — dedicated tx-flush thread selector. Defined here (ahead of
// its previous site) because `struct zpico_session` gates its flush-task
// members on it. On multi-threaded, batching-on, non-ThreadX builds a dedicated
// low-duty thread absorbs the socket waits; ThreadX keeps the spin-driven flush
// (it runs no background tasks — read-task starvation, see zpico_open);
// single-threaded platforms have no tasks at all.
#if defined(ZPICO_TX_BATCH) && ZPICO_TX_BATCH == 1 && Z_FEATURE_MULTI_THREAD == 1 &&               \
    !defined(ZENOH_THREADX)
#define ZPICO_TX_BATCH_THREAD 1
#else
#define ZPICO_TX_BATCH_THREAD 0
#endif
/* issue 1534 — the ports whose `_z_task_init` forwards `task_attributes` to
 * `nros_platform_task_init`, which reads an `nros_platform_task_attr_t`: the
 * flush task must be handed that shape on every one of them (issue 0803 settled
 * it for the read task, issue 0852 for Zephyr's flush task, and Linux, macOS
 * and NuttX were left filling a `pthread_attr_t` the reader decodes as whatever
 * its bytes say). FreeRTOS keeps its own `z_task_attr_t`. */
#if ZPICO_TX_BATCH_THREAD == 1 && (defined(ZENOH_LINUX) || defined(ZENOH_MACOS) ||                 \
                                   defined(ZENOH_NUTTX) || defined(ZENOH_ZEPHYR))
#define ZPICO_FLUSH_NROS_ATTR 1
#else
#define ZPICO_FLUSH_NROS_ATTR 0
#endif
#ifndef ZPICO_TX_BATCH_FLUSH_MS
#define ZPICO_TX_BATCH_FLUSH_MS 50
#endif

#ifndef ZPICO_MAX_SESSIONS
#define ZPICO_MAX_SESSIONS 1 // single-session targets keep TODAY's footprint
#endif

/* Guarded HERE, and AFTER the `#endif` above rather than beside the
 * other extents: `ZPICO_MAX_SESSIONS` is defined further down the file
 * than they are, and an `#if` that precedes a `#define` reads the macro
 * as UNDEFINED — which the preprocessor evaluates as 0, so the guard
 * fires on every build instead of on a zero. It shipped that way and
 * broke `just setup tier2`; the mutation test that missed it drove an
 * EXTRACTED block with `-D` for every knob, so it defined what the real
 * translation unit does not. Inside the `#ifndef` is wrong for the
 * mirror-image reason: a `-D` skips the block and the guard with it. */
#if ZPICO_MAX_SESSIONS < 1
#error "ZPICO_MAX_SESSIONS must be >= 1: it sizes a fixed C array (issue 1015)"
#endif

struct zpico_session {
#if Z_FEATURE_MULTI_THREAD == 1
    /* issue 1711 -- guards CLAIMING an entity slot (publishers, subscribers,
     * liveliness, queryables). See `zpico_claim_slot`. */
    _z_mutex_t slot_mutex;
    bool slot_mutex_initialized;
    /* issue 1749 -- serializes every call into zenoh-pico's declare path on
     * this session. See `zpico_declare_lock`. */
    _z_mutex_rec_t declare_mutex;
    bool declare_mutex_initialized;
#endif
    // Session handle + config + lifecycle (per handle — issue 0347's
    // single-session guard is gone; each handle is independent).
    z_owned_config_t config;
    z_owned_session_t session;
    bool session_open;
    bool initialized;

    // Registration tables.
    publisher_entry_t publishers[ZPICO_MAX_PUBLISHERS];
    subscriber_entry_t subscribers[ZPICO_MAX_SUBSCRIBERS];
    liveliness_entry_t liveliness[ZPICO_MAX_LIVELINESS];
#if ZPICO_GRAPH_DISCOVERY
    graph_cache_t graph_cache;
#endif
    queryable_entry_t queryables[ZPICO_MAX_QUERYABLES];

    // Phase 237 — per-queryable seq-keyed reply slots. Each slot holds one
    // cloned (owned) query so a reply can be sent after the query callback
    // returned (deferred get_result) even if more queries land meanwhile.
    z_owned_query_t stored_queries[ZPICO_MAX_QUERYABLES][ZPICO_MAX_PENDING_REPLIES];
    bool stored_query_valid[ZPICO_MAX_QUERYABLES][ZPICO_MAX_PENDING_REPLIES];
    int64_t last_reply_seq[ZPICO_MAX_QUERYABLES];

    /* issue 0902 / phase-455 W1 — the reply-slot table was the one bounded
     * resource in this file with no counter. Exhaustion and "no query pending"
     * were the SAME value (`last_reply_seq == -1`), and that IS the defect: a
     * goal was accepted, executed and never answered, while nothing anywhere
     * recorded that a slot had been refused. The tree already has the shape —
     * `zpico_graph_entry_count`'s `out_dropped` and the request ring's own
     * `dropped` both report what a bounded resource discarded.
     *
     * Per QUERYABLE, not per session and not process-global: each server holds
     * its own table, so a summed number cannot say which one saturated. */
    uint32_t reply_slot_refusals[ZPICO_MAX_QUERYABLES];
    bool reply_slot_saturated[ZPICO_MAX_QUERYABLES];
    bool reply_slot_announce[ZPICO_MAX_QUERYABLES];

    /* issue 1332 / phase-455 W2.b — the count of queries the CALLBACK declined,
     * i.e. the ones whose slot `query_handler` reclaims below.
     *
     * The companion to `reply_slot_refusals`, and it answers a different
     * question: refusals say the table ran out, declines say the arm that used
     * to EXHAUST it was reached at all. Without it a `refusals == 0` green is
     * unfalsifiable — issue 1332 measured exactly that, a one-slot table that
     * still refused nothing because no declined query ever arrived, and the
     * lane could not tell that apart from a table being managed correctly. */
    uint32_t reply_slot_declines[ZPICO_MAX_QUERYABLES];

    // Non-blocking z_get slot pool.
    pending_get_slot_t pending_gets[ZPICO_MAX_PENDING_GETS];

    // Reply waker (async service client).
    zpico_waker_fn reply_waker;

    // Graph-change edge (phase-467 Row 8). NULL = nobody is listening, which
    // is the state every image is in until something installs a graph
    // guard condition.
    zpico_graph_change_fn graph_change_cb;

#if defined(ZPICO_TX_BATCH) && ZPICO_TX_BATCH == 1 && ZPICO_TX_BATCH_THREAD == 0
    // phase-279 (#145) — rate-limited batch-flush cadence for platforms WITHOUT
    // the dedicated tx-flush thread (ThreadX + single-threaded). Was a pair of
    // function-local statics in zpico_spin_once.
    z_clock_t last_batch_flush;
    bool batch_flush_init;
#endif

    // Spin-wake primitive for multi-threaded spin_once(). Signaled by our
    // callbacks (sample_handler, query_handler, get reply handlers) so
    // spin_once() wakes immediately when application data arrives, rather than
    // sleeping for the full timeout duration.
#if Z_FEATURE_MULTI_THREAD == 1 && !defined(ZPICO_SMOLTCP)
#if defined(ZENOH_FREERTOS_LWIP)
    // FreeRTOS: binary semaphore — lightweight, no mutex needed.
    SemaphoreHandle_t spin_sem;
#elif defined(ZENOH_NUTTX)
    // NuttX: POSIX `sem_t` + `sem_timedwait`. The pthread mutex + condvar pair
    // hangs indefinitely inside the kernel's watchdog-backed semaphore wait
    // (Phase 55.12 follow-up); POSIX `sem_timedwait` does not share that path.
    sem_t spin_sem_posix;
    bool spin_sem_initialized;
#else
    // POSIX/Zephyr: mutex + condvar
    _z_mutex_t spin_mutex;
    _z_condvar_t spin_cv;
    bool spin_cv_initialized;
#endif
#endif

#if ZPICO_TX_BATCH_THREAD == 1
    // phase-279/282 W4 (#145) — dedicated tx-flush task + its optional
    // attributes (copied from the process-wide defaults at zpico_open).
    _z_task_t tx_flush_task;
    volatile bool tx_flush_run;
    bool flush_task_configured;
    z_task_attr_t flush_task_attr;
#if ZPICO_FLUSH_NROS_ATTR == 1
    /* issue 0852 — the struct the Zephyr port actually reads. `z_task_attr_t`
     * is `pthread_attr_t` here, and `_z_task_init` forwards `task_attributes`
     * to `nros_platform_task_init`, which reads this shape. */
    nros_platform_task_attr_t flush_nros_attr;
#endif
#endif

#if Z_FEATURE_MULTI_THREAD == 1
    // Executor task attributes (copied from the process-wide defaults at
    // zpico_open). When non-configured, NULL is passed (platform default).
    // RFC-0110 D2 — zenoh-pico 1.9 runs read, lease and keep-alive on ONE
    // executor thread, so the old read task's attributes are the executor's
    // and there is no lease task left to configure.
    bool read_task_configured;
    zp_task_read_options_t read_task_opts;
    z_task_attr_t read_task_attr;
    /* issue 0803 — the per-session copy the ALIAS task path reads. See the note
     * on `g_default_read_nros_attr`: `task_attributes` must point at an
     * `nros_platform_task_attr_t`, and `zpico_open` re-points it per session,
     * so the session needs storage of that type too. Fixing only the global
     * left this re-point handing the platform layer the pthread copy again. */
    nros_platform_task_attr_t read_nros_attr;
#endif

#if defined(ZPICO_DRIVE_CALLER)
    // RFC-0110 D3 — set by `_zpico_notify_spin` when a callback delivered
    // application data during this caller's executor steps, so a timed
    // `zpico_spin_once` returns as soon as there is something to dispatch.
    volatile bool spin_delivered;
#endif
};

static struct zpico_session g_sessions[ZPICO_MAX_SESSIONS];

#if ZPICO_GRAPH_DISCOVERY
/* Issue 2083 -- defined with the graph cache, used by the token paths too. */
static void graph_cache_apply(struct zpico_session* s, const char* key, size_t klen, bool remove);
#endif

/* Issue 1749 -- defined beside the slot lock; see there. */
static inline void zpico_declare_lock(struct zpico_session* s);
static inline void zpico_declare_unlock(struct zpico_session* s);
static bool g_session_inuse[ZPICO_MAX_SESSIONS];

// Acquire a free pool slot (zeroed). NULL when the pool is exhausted.
zpico_session_t* zpico_session_acquire(void) {
    for (int i = 0; i < ZPICO_MAX_SESSIONS; i++) {
        if (!g_session_inuse[i]) {
            g_session_inuse[i] = true;
            memset(&g_sessions[i], 0, sizeof(g_sessions[i]));
            return (zpico_session_t*)&g_sessions[i];
        }
    }
    return NULL;
}

// Return a slot to the pool. The pointer is invalid afterwards.
void zpico_session_release(zpico_session_t* session) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (s == NULL || s < g_sessions || s >= g_sessions + ZPICO_MAX_SESSIONS) {
        return;
    }
    g_session_inuse[s - g_sessions] = false;
}

// phase-328/#376 — the session's pool index (0..ZPICO_MAX_SESSIONS), or -1 if
// the handle is not a valid pool slot. The Rust shim uses it to scope its
// process-global service-buffer / reply-waker tables per session.
int32_t zpico_session_index(zpico_session_t* session) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (s == NULL || s < g_sessions || s >= g_sessions + ZPICO_MAX_SESSIONS) {
        return -1;
    }
    return (int32_t)(s - g_sessions);
}

// Callback context packing (issue 0348 / phase-328). zenoh-pico closures carry
// a single `void* ctx`; pack the owning session's pool index and the table slot
// index into it instead of an `int idx` (which no longer identifies a session)
// or a back-pointer into a now-pooled array. At ZPICO_MAX_SESSIONS == 1 the
// session index is 0, so the packed value equals the old `(void*)(intptr_t)idx`.
static inline void* _zpico_pack_ctx(struct zpico_session* s, int slot_idx) {
    uintptr_t session_idx = (uintptr_t)(s - g_sessions);
    return (void*)((session_idx << 16) | (uintptr_t)(unsigned)slot_idx);
}
static inline struct zpico_session* _zpico_unpack_session(void* ctx) {
    return &g_sessions[(uintptr_t)ctx >> 16];
}
static inline int _zpico_unpack_slot(void* ctx) {
    return (int)((uintptr_t)ctx & 0xFFFF);
}

// Spin-wake helpers for multi-threaded spin_once(). Signal the OWNING
// session's wake object (the primitives now live in `struct zpico_session`) so
// spin_once() can wake immediately when application data arrives, rather than
// sleeping for the full timeout duration. Callers have `s` in hand.
#if Z_FEATURE_MULTI_THREAD == 1 && !defined(ZPICO_SMOLTCP)

#if defined(ZENOH_FREERTOS_LWIP)
// FreeRTOS: binary semaphore — lightweight, no mutex needed.
static inline void _zpico_wake_spinner(struct zpico_session* s) {
    if (s->spin_sem != NULL) {
        xSemaphoreGive(s->spin_sem);
    }
}
#elif defined(ZENOH_NUTTX)
// NuttX: POSIX `sem_t` + `sem_timedwait`. We can't use the pthread
// mutex + condvar pair here — on NuttX the pthread-timed-wait path
// hangs indefinitely inside the kernel's watchdog-backed semaphore
// wait (Phase 55.12 follow-up). POSIX `sem_timedwait` does not share
// that code path and is safe.
static inline void _zpico_wake_spinner(struct zpico_session* s) {
    if (s->spin_sem_initialized) {
        // sem_post is async-signal-safe; binary-ish semantics are fine
        // because we only care that spin_once wakes at least once per
        // event, not that every post is counted.
        sem_post(&s->spin_sem_posix);
    }
}
#else
// POSIX/Zephyr: mutex + condvar

static inline void _zpico_wake_spinner(struct zpico_session* s) {
    if (s->spin_cv_initialized) {
        _z_mutex_lock(&s->spin_mutex);
        _z_condvar_signal(&s->spin_cv);
        _z_mutex_unlock(&s->spin_mutex);
    }
}
#endif // ZENOH_FREERTOS_LWIP / ZENOH_NUTTX

#else
static inline void _zpico_wake_spinner(struct zpico_session* s) {
    (void)s;
}
#endif

// Every callback that hands application data to the executor calls this. In a
// CALLER build it also marks the delivery, which is how a timed spin knows its
// executor steps produced work (RFC-0110 D3).
static inline void _zpico_notify_spin(struct zpico_session* s) {
#if defined(ZPICO_DRIVE_CALLER)
    s->spin_delivered = true;
#endif
    _zpico_wake_spinner(s);
}

// ============================================================================
// phase-279 (#145) — dedicated tx-flush thread. Flushing from the executor/
// tier threads measured WORSE-or-equal vs no batching: the flush blocks its
// caller on the socket for up to a recv window, stalling the very timers that
// generate the puts. On multi-threaded platforms a dedicated low-duty thread
// absorbs those waits instead; tier threads only ever append to the batch.
// ThreadX deliberately runs no background tasks (read-task starvation — see
// zpico_open), so it keeps the spin-driven flush. Single-threaded platforms
// have no tasks at all.
//
// The ZPICO_TX_BATCH_THREAD / ZPICO_TX_BATCH_FLUSH_MS selectors are defined
// above, ahead of `struct zpico_session`, which gates its flush members on them.
#if ZPICO_TX_BATCH_THREAD == 1
/* phase-282 W4 (#145) — the flush task runs off the OWNING session (passed as
 * the task `arg`) and flushes that session's transport. */
static void* _zpico_tx_flush_task_fn(void* arg) {
    struct zpico_session* s = (struct zpico_session*)arg;
    while (s->tx_flush_run) {
        zp_batch_flush(z_session_loan(&s->session));
        z_sleep_ms(ZPICO_TX_BATCH_FLUSH_MS);
    }
    return NULL;
}
#endif

// Task Configuration — PROCESS-WIDE DEFAULTS (set before zpico_open, copied
// into the session at open). Boards call these before any session exists, so
// they cannot take a `zpico_session_t*` (documented exception to the handle
// rule — issue 0348 / phase-328).
// ============================================================================

#if Z_FEATURE_MULTI_THREAD == 1
// Optional default attributes for the executor task (RFC-0110 D2: the old read
// task's slot). When configured they are copied into the session at
// zpico_open and passed to zp_start_read_task() instead of NULL (platform
// default).
static bool g_default_read_task_configured = false;
static zp_task_read_options_t g_default_read_task_opts;
/* issue 0803 — the attribute the ALIAS task path reads.
 *
 * On a build whose `_z_task_init` comes from `platform_aliases.c` (everything
 * but ThreadX), that alias forwards `task_attributes` straight to
 * `nros_platform_task_init`, which reads it as `nros_platform_task_attr_t *`.
 * `ZENOH_LINUX` makes zenoh-pico's `unix.h` typedef `z_task_attr_t` to
 * `pthread_attr_t`, so the POSIX arm below used to fill a `pthread_attr_t` and
 * hand the platform layer 56 bytes of a different struct — `stack_bytes`,
 * `priority` and `flags` read out of pthread's opaque bytes. The requested 90
 * became whatever those bytes said, clamped into [1,99]: measured 1, for every
 * requested value, on every arm.
 *
 * `nros_zenoh_generic_platform.h` predicted exactly this — "correct only while
 * the value is always NULL, which it was" — and issue 0765 made it non-NULL.
 * Two headers disagreeing about one type is issue 0135's shape.
 *
 * So the POSIX arm fills THIS, and `task_attributes` points at it. The pointer
 * width is unchanged, so `zp_task_read_options_t`'s layout is untouched; what
 * changes is that the pointee is the struct the reader expects. */
static nros_platform_task_attr_t g_default_read_nros_attr;

static z_task_attr_t g_default_read_task_attr;
#endif

#if ZPICO_TX_BATCH_THREAD == 1
/* phase-282 W4 (#145) — process-wide default attributes for the flush task, set
 * via zpico_set_flush_task_config() before zpico_open(), copied into the session
 * at open. Same platform matrix as the read/lease task attrs: FreeRTOS +
 * POSIX-like honour them, others ignore the attr. */
static bool g_default_flush_task_configured = false;
static z_task_attr_t g_default_flush_task_attr;
#if defined(ZENOH_FREERTOS) || defined(ZENOH_FREERTOS_LWIP)
/* The stack depth of zenoh-pico's FreeRTOS default task attributes
 * (src/system/freertos/system.c) — what the flush task got while it was spawned
 * with a NULL attr. */
#define ZPICO_FREERTOS_DEFAULT_STACK_DEPTH 5120
#endif
#if ZPICO_FLUSH_NROS_ATTR == 1
static nros_platform_task_attr_t g_default_flush_nros_attr;
#endif
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

static void zpico_fill_session_zid(struct zpico_session* s, uint8_t bytes[ZPICO_ZID_SIZE]) {
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
    zephyr_seed ^= (uint64_t)(uintptr_t)&s->session;
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

    uint64_t seed = (uint64_t)(uintptr_t)&s->session;
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
static void _zpico_release_reply_slot(struct zpico_session* s, int32_t handle, int64_t seq);

/* issue 0902 / phase-455 W1 — the PURE half of `query_handler`'s reply-slot
 * allocation, split out for exactly the reason `zpico_entry_at` and
 * `zpico_graph_set_apply` were: it is the piece a test can drive WITHOUT a
 * session, a router and a live peer. Issue 0902 item 3 records why that
 * matters — both leak fixes landed with no regression test, because reaching
 * either arm needs a query delivered to a queryable, and nothing under
 * `packages/testing` referenced this table at all.
 *
 * Picks the first free slot in `valid[0..cap)` and ACCOUNTS for a refusal. It
 * deliberately does not mark the slot taken: the caller does that only once
 * the clone succeeds, which is the one remaining step that can fail.
 *
 * `*refusals` counts allocations refused because every slot was held. It is
 * cumulative for the life of the queryable, so a reader can tell "this server
 * has never run out" (0) from "it ran out" (>0) — the distinction the -1
 * return value cannot carry, and the whole of what made the 20-90 % completion
 * spread unfalsifiable. Saturates at UINT32_MAX rather than wrapping to 0,
 * because wrapping to 0 is indistinguishable from "never happened".
 *
 * `*saturated` latches the state so `*out_announce` is true exactly on the
 * TRANSITION into exhaustion, never on every refused query. phase-444 W6
 * landed the same correction on the Cyclone parameter services, where a
 * permanent failure was reported six times per spin. A successful pick clears
 * the latch, so a table that drains and saturates again announces again: that
 * is a new event, not a repeat of the old one.
 *
 * Returns the slot index, `ZPICO_ERR_FULL` when every slot is held, or
 * `ZPICO_ERR_INVALID` for a NULL argument or `cap == 0`.
 */
int32_t zpico_reply_slot_pick(const bool* valid, uint32_t cap, uint32_t* refusals, bool* saturated,
                              bool* out_announce) {
    if (out_announce != NULL) {
        *out_announce = false;
    }
    if (valid == NULL || refusals == NULL || saturated == NULL || cap == 0) {
        return ZPICO_ERR_INVALID;
    }
    for (uint32_t j = 0; j < cap; ++j) {
        if (!valid[j]) {
            *saturated = false;
            return (int32_t)j;
        }
    }
    if (*refusals != 0xFFFFFFFFu) {
        (*refusals)++;
    }
    if (!*saturated) {
        *saturated = true;
        if (out_announce != NULL) {
            *out_announce = true;
        }
    }
    return ZPICO_ERR_FULL;
}

static void query_handler(z_loaned_query_t* query, void* arg) {
    struct zpico_session* s = _zpico_unpack_session(arg);
    int idx = _zpico_unpack_slot(arg);
    if (idx < 0 || idx >= ZPICO_MAX_QUERYABLES) {
        return;
    }

    queryable_entry_t* entry = &s->queryables[idx];
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
    //
    // issue 0902 / phase-455 W1 — the pick and its accounting are
    // `zpico_reply_slot_pick`, so the refusal is COUNTED and announced ONCE,
    // and so the state machine is reachable from a host test.
    int64_t reply_seq = -1;
    bool announce = false;
    int32_t slot = zpico_reply_slot_pick(
        s->stored_query_valid[idx], (uint32_t)ZPICO_MAX_PENDING_REPLIES,
        &s->reply_slot_refusals[idx], &s->reply_slot_saturated[idx], &announce);
    if (slot >= 0) {
        /* The clone is a refcount increment, not an allocation — issue 0902
         * ruled it out as a failure mode against `refcount.h:70-72` — so a -1
         * reply_seq means "the table was full" and nothing else. That is what
         * keeps the refusal counter's meaning exact, and why a clone failure
         * is not folded into it. */
        if (z_query_clone(&s->stored_queries[idx][slot], query) == 0) {
            s->stored_query_valid[idx][slot] = true;
            reply_seq = slot;
        }
    }
    if (slot == ZPICO_ERR_FULL && g_reply_slot_refusals_total != 0xFFFFFFFFu) {
        /* phase-455 W2 — the same event the per-queryable counter records,
         * summed for a reader that has no session handle. Saturates rather
         * than wrapping, for the reason the per-queryable count does. */
        g_reply_slot_refusals_total++;
    }
    if (announce) {
        /* Latched here, said by the RUST side (`shim/service.rs`). `printk` is
         * a NO-OP on `ZPICO_SMOLTCP` / `ZPICO_SERIAL` — the bare-metal serial
         * board issue 0902 was measured on — so a C-side print would reach
         * nothing on the one target where this was found. `nros_log` reaches
         * every platform (issue 0589). */
        s->reply_slot_announce[idx] = true;
    }
    s->last_reply_seq[idx] = reply_seq;

    // Call user callback
    entry->callback(keyexpr_str, keyexpr_len, payload_data, payload_len, entry->ctx);

    /* issue 0902 — RECLAIM a slot the callback declined.
     *
     * The clone above happens BEFORE the callback runs, and the slot is
     * released in only three places: the `memset` at session open, queryable
     * teardown, and a fully successful `zpico_query_reply`. So a query that is
     * cloned and then dropped by the callback holds its slot for the life of
     * the queryable.
     *
     * The callback drops queries on two ordinary paths, and BOTH are hot:
     * an empty payload — which the Rust side documents as "liveliness probes
     * that zenoh-pico delivers through the same queryable callback as real
     * service requests" — and a full request ring. Neither is an error, and
     * both arrive on an idle link, so the four slots drain with ELAPSED TIME
     * rather than with traffic. Once they are gone `reply_seq` is -1 for
     * every later query, `zpico_query_reply` returns ZPICO_ERR_INVALID, and
     * the caller discards it: a goal is accepted, executed, and never
     * answered. The state is absorbing — nothing re-arms.
     *
     * `zpico_queryable_take_reply_seq` now CLEARS the seq as its name always
     * claimed, so a still-set value here means the callback never asked for
     * it, which means nobody holds this query and nobody will reply to it.
     * Drop it. A callback that DID take the seq keeps its slot for the
     * deferred reply, which is what the mechanism is for. */
    /* issue 1332 / phase-455 W2.b — the DETECTION is its own statement, above
     * and independent of the reclaim.
     *
     * `refusals == 0` is the assertion this arm's fix earns, and on its own it
     * cannot fail for the right reason: a lane where no query is ever declined
     * reports the same zero as a lane where every declined query hands its slot
     * back. Counting the declines separates the two, so a green states that the
     * arm RAN. Keeping the count out of the reclaim's `if` is what lets the
     * reclaim be reverted on its own as a negative control — with the count
     * folded in, a revert would silence the evidence that the control works. */
    const bool declined = (reply_seq >= 0 && s->last_reply_seq[idx] >= 0);
    if (declined && s->reply_slot_declines[idx] != 0xFFFFFFFFu) {
        /* Saturates rather than wrapping, for the reason the refusal count
         * does: a wrapped counter reads as "never happened". */
        s->reply_slot_declines[idx]++;
    }
    if (declined) {
        _zpico_release_reply_slot(s, idx, reply_seq);
        s->last_reply_seq[idx] = -1;
    }

    // Clean up slice
    if (payload_data != NULL) {
        z_slice_drop(z_slice_move(&payload_slice));
    }
    _zpico_notify_spin(s);
}

/* The PURE half of the history-window dedup (phase-473 W2): is the sample
 * whose rmw attachment is `att[0..att_len)` newer than the newest one the window
 * has delivered? The window state is three caller-owned fields, so a test can
 * drive the real rule without a session -- the `zpico_graph_set_apply` shape.
 *
 * The attachment is the layout `RmwAttachment` writes: `int64 seq (LE) | int64
 * timestamp | VLE len (16) | 16-byte GID`, 33 bytes (`safety-e2e` appends a
 * CRC after them). One too short, or not carrying a 16-byte GID, takes no part
 * in the dedup and is admitted: a sample is never dropped on a guess.
 *
 * Returns 1 to admit (and records it as the newest), 0 to drop (a sample from
 * the same publisher at or below the recorded sequence number; `*dropped` is
 * counted), `ZPICO_ERR_INVALID` for a NULL state pointer. */
int32_t zpico_history_admit_apply(bool* have, uint8_t* gid, int64_t* seq, uint32_t* dropped,
                                  const uint8_t* att, size_t att_len) {
    if (have == NULL || gid == NULL || seq == NULL || dropped == NULL) {
        return ZPICO_ERR_INVALID;
    }
    if (att == NULL || att_len < 33 || att[16] != 16) {
        return 1;
    }
    uint64_t v = 0;
    for (int i = 7; i >= 0; i--) {
        v = (v << 8) | att[i];
    }
    int64_t s = (int64_t)v;
    if (*have && memcmp(gid, att + 17, 16) == 0 && s <= *seq) {
        (*dropped)++;
        return 0;
    }
    *have = true;
    memcpy(gid, att + 17, 16);
    *seq = s;
    return 1;
}

/* Whether a sample arriving in a subscription's history window is NEW.
 * Outside a window every sample is. */
static bool _zpico_history_admit(subscriber_entry_t* entry, const z_loaned_bytes_t* attachment) {
    if (!entry->hist_active) {
        return true;
    }
    uint8_t att[33];
    size_t n = 0;
    if (attachment != NULL && z_bytes_len(attachment) >= sizeof(att)) {
        z_bytes_reader_t r = z_bytes_get_reader(attachment);
        n = z_bytes_reader_read(&r, att, sizeof(att));
    }
    return zpico_history_admit_apply(&entry->hist_have, entry->hist_gid, &entry->hist_seq,
                                     &entry->hist_dropped, att, n) != 0;
}

static inline void zpico_slot_lock(struct zpico_session* s);
static inline void zpico_slot_unlock(struct zpico_session* s);

/* Issue 1979 -- what a full ring does with one more sample.
 *
 * A subscription is KEEP_LAST: the newest samples are the ones to keep. This
 * used to refuse the incoming sample instead, so a burst into a slow consumer
 * ended on stale data and the burst's final value never arrived, and the drop
 * was counted nowhere.
 *
 * The consumer reads a slot IN PLACE (an in-place callback runs on the ring's
 * bytes), so the producer may never write the slot the consumer is reading.
 * The consumer says which one that is: it CLAIMS the head slot before reading
 * it (`zpico_ring_claim_head`) and gives it up when it consumes it
 * (`zpico_ring_consume_head`). Both take the session's `slot_mutex`, the lock
 * every producer of the ring already holds, so a claim and a producer's
 * decision are never interleaved. The consumer holds the lock only for the
 * claim itself, never while it reads.
 *
 * On a full ring, under that lock:
 *   - head not claimed: evict the OLDEST sample (advance head), and write the
 *     new one at tail. This is KEEP_LAST.
 *   - head claimed and depth >= 2: the consumer is reading the oldest, so
 *     REPLACE the newest queued sample (tail - 1) in place. The final value
 *     of a burst still arrives; one older sample is lost instead of the
 *     oldest. Tail does not move.
 *   - head claimed and depth 1: the only slot is being read, so the incoming
 *     sample is dropped.
 * Every one of the three counts in `full_drops`.
 *
 * Returns the slot to write, or -1 when the sample is dropped. `*advance` is
 * set when the caller must publish the slot by advancing tail. */
static intptr_t _zpico_ring_reserve(zpico_ring_desc_t* r, bool* advance) {
    uintptr_t head = atomic_load_explicit((_Atomic uintptr_t*)r->head, memory_order_acquire);
    uintptr_t tail = atomic_load_explicit((_Atomic uintptr_t*)r->tail, memory_order_relaxed);
    *advance = true;
    if (tail - head < r->slot_count) {
        return (intptr_t)(tail % r->slot_count);
    }
    uintptr_t drops = atomic_load_explicit((_Atomic uintptr_t*)r->full_drops, memory_order_relaxed);
    atomic_store_explicit((_Atomic uintptr_t*)r->full_drops, drops + 1, memory_order_relaxed);
    uintptr_t claimed = atomic_load_explicit((_Atomic uintptr_t*)r->claimed, memory_order_relaxed);
    if (claimed != head + 1) {
        atomic_store_explicit((_Atomic uintptr_t*)r->head, head + 1, memory_order_release);
        return (intptr_t)(tail % r->slot_count);
    }
    if (r->slot_count >= 2) {
        *advance = false;
        return (intptr_t)((tail - 1) % r->slot_count);
    }
    return -1;
}

/* Publish a slot `_zpico_ring_reserve` returned. Release, so the consumer's
 * acquire load of tail sees the payload and length writes. A replaced slot
 * is already published; its writes became visible under the lock, which the
 * consumer takes before it can claim that slot. */
static void _zpico_ring_commit(zpico_ring_desc_t* r, bool advance) {
    if (advance) {
        uintptr_t tail = atomic_load_explicit((_Atomic uintptr_t*)r->tail, memory_order_relaxed);
        atomic_store_explicit((_Atomic uintptr_t*)r->tail, tail + 1, memory_order_release);
    }
}

static inline void _zpico_ring_lock(zpico_ring_desc_t* r) {
    if (r->lock_session != NULL) {
        zpico_slot_lock((struct zpico_session*)r->lock_session);
    }
}

static inline void _zpico_ring_unlock(zpico_ring_desc_t* r) {
    if (r->lock_session != NULL) {
        zpico_slot_unlock((struct zpico_session*)r->lock_session);
    }
}

ptrdiff_t zpico_ring_claim_head(zpico_ring_desc_t* r) {
    if (r == NULL || r->head == NULL || r->slot_count == 0) {
        return -1;
    }
    _zpico_ring_lock(r);
    uintptr_t head = atomic_load_explicit((_Atomic uintptr_t*)r->head, memory_order_relaxed);
    uintptr_t tail = atomic_load_explicit((_Atomic uintptr_t*)r->tail, memory_order_acquire);
    ptrdiff_t slot = -1;
    if (head != tail) {
        atomic_store_explicit((_Atomic uintptr_t*)r->claimed, head + 1, memory_order_relaxed);
        slot = (ptrdiff_t)(head % r->slot_count);
    }
    _zpico_ring_unlock(r);
    return slot;
}

void zpico_ring_consume_head(zpico_ring_desc_t* r) {
    if (r == NULL || r->head == NULL) {
        return;
    }
    _zpico_ring_lock(r);
    uintptr_t head = atomic_load_explicit((_Atomic uintptr_t*)r->head, memory_order_relaxed);
    uintptr_t tail = atomic_load_explicit((_Atomic uintptr_t*)r->tail, memory_order_acquire);
    if (head != tail) {
        atomic_store_explicit((_Atomic uintptr_t*)r->head, head + 1, memory_order_release);
    }
    atomic_store_explicit((_Atomic uintptr_t*)r->claimed, 0, memory_order_relaxed);
    _zpico_ring_unlock(r);
}

/* Issue 2135 — the slot's receive time, upstream's `received_timestamp`:
 * rmw_zenoh_cpp stamps `system_clock::now()` in the sample callback, which is
 * this producer. Wall clock, the same clock a publisher's attachment
 * `source_timestamp` is read from, so `received - source` is a latency. A
 * platform with no wall clock returns 0, which the consumer reports as "not
 * stamped". */
static void _zpico_ring_stamp(zpico_ring_desc_t* r, intptr_t slot) {
    if (r->recv_ns != NULL) {
        uint64_t ns = nros_platform_time_now_ns();
        r->recv_ns[slot] = ns > (uint64_t)INT64_MAX ? INT64_MAX : (int64_t)ns;
    }
}

int32_t zpico_ring_push_bytes(zpico_ring_desc_t* r, const uint8_t* payload, size_t len) {
    if (r == NULL || r->head == NULL || len == 0) {
        return ZPICO_ERR_INVALID;
    }
    if (len > r->payload_stride) {
        return ZPICO_ERR_INVALID;
    }
    _zpico_ring_lock(r);
    bool advance = false;
    intptr_t slot = _zpico_ring_reserve(r, &advance);
    if (slot < 0) {
        _zpico_ring_unlock(r);
        return ZPICO_ERR_FULL;
    }
    memcpy(r->payload_base + (size_t)slot * r->payload_stride, payload, len);
    r->payload_len[slot] = len;
    r->att_len[slot] = 0;
    _zpico_ring_stamp(r, slot);
    _zpico_ring_commit(r, advance);
    _zpico_ring_unlock(r);
    return 0;
}

/* The ring producer: one sample (live, or a history reply) into the next free
 * slot. Split out of `sample_handler` so the transient-local history reply
 * lands in the SAME ring through the SAME code -- the consumer cannot tell the
 * two apart, which is the point.
 *
 * Issue 1956 -- there is MORE THAN ONE producer, and this comment used to say
 * there was not ("both callers run on the read task"). With
 * Z_FEATURE_LOCAL_SUBSCRIBER a `z_put` delivers to a subscriber on the same
 * session SYNCHRONOUSLY, on the PUBLISHER's task, and zenoh-pico calls the
 * handler outside every session lock. So a tier that publishes a topic another
 * tier subscribes to pushes into that ring while the read task pushes remote
 * samples into it too. The tail update is a load and a later store: a producer
 * preempted between the two stores a STALE tail after the other has advanced
 * it, the consumer has meanwhile drained past it, and `head` is now AHEAD of
 * `tail`. From then on the ring reads as non-empty forever (the consumer
 * re-dispatches its last few slots) and as full to every new sample. That is
 * the shape issue 1956 measured on the rt-eval island under flood, where the
 * gate tier's forwarded command and the flood share
 * `/control/command/control_cmd` and the actuator re-ran one sample ~289,000
 * times.
 *
 * The producers serialize on the session's `slot_mutex`. Every producer of a
 * ring is on that ring's session; under the lock this only copies bytes out of
 * a sample it already holds, so it takes no zenoh-pico lock and cannot invert
 * with one; and reusing it costs no second mutex per session (Zephyr's
 * pthread mutexes come from a fixed pool). Notify runs after the unlock. */

static void _zpico_ring_push(struct zpico_session* s, subscriber_entry_t* entry,
                             const z_loaned_bytes_t* payload, const z_loaned_bytes_t* attachment) {
    size_t payload_len = z_bytes_len(payload);
    if (entry->notify == NULL || entry->ring == NULL) {
        return;
    }
    // Drop empty-payload samples — zenoh-pico delivers background
    // probes / liveliness syncs through the regular subscription
    // path with a zero-length payload. Buffering them would let
    // the typed `take()` consume a slot whose CDR header check
    // then fails. Mirrors the legacy single-slot behaviour.
    if (payload_len == 0) {
        return;
    }
    // The window dedup state `_zpico_history_admit` updates is per entry and
    // shared by the same producers, so it is inside the lock too.
    zpico_slot_lock(s);
    if (!_zpico_history_admit(entry, attachment)) {
        zpico_slot_unlock(s);
        return;
    }
    zpico_ring_desc_t* r = entry->ring;
    if (payload_len > r->payload_stride) {
        zpico_slot_unlock(s);
        // Slot too small — report overflow via notify; nothing is written,
        // and nothing queued is evicted for a sample that cannot be kept.
        entry->notify(payload_len, NULL, 0, entry->ctx);
        return;
    }

    bool advance = false;
    intptr_t slot = _zpico_ring_reserve(r, &advance);
    if (slot < 0) {
        zpico_slot_unlock(s);
        // Dropped (depth 1, slot being read). Still fire notify(len) so the
        // Rust side observes the arrival for its waker.
        entry->notify(payload_len, NULL, 0, entry->ctx);
        _zpico_notify_spin(s);
        return;
    }

    uint8_t* pay_dst = r->payload_base + (size_t)slot * r->payload_stride;
    z_bytes_reader_t reader = z_bytes_get_reader(payload);
    z_bytes_reader_read(&reader, pay_dst, payload_len);
    r->payload_len[slot] = payload_len;

    // Attachment into the parallel per-slot array.
    size_t att_written = 0;
    if (attachment != NULL && r->att_stride > 0) {
        size_t att_len = z_bytes_len(attachment);
        if (att_len <= r->att_stride) {
            z_bytes_reader_t att_reader = z_bytes_get_reader(attachment);
            att_written =
                z_bytes_reader_read(&att_reader, r->att_base + (size_t)slot * r->att_stride, att_len);
        }
    }
    r->att_len[slot] = att_written;
    _zpico_ring_stamp(r, slot);

    _zpico_ring_commit(r, advance);
    zpico_slot_unlock(s);

    // Fire notify for the async waker. Pass NULL attachment —
    // the consumer reads the per-slot attachment array directly.
    entry->notify(payload_len, NULL, 0, entry->ctx);
    _zpico_notify_spin(s);
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
    struct zpico_session* s = _zpico_unpack_session(arg);
    int idx = _zpico_unpack_slot(arg);
    if (idx < 0 || idx >= ZPICO_MAX_SUBSCRIBERS) {
        return;
    }

    subscriber_entry_t* entry = &s->subscribers[idx];
    if (!entry->active) {
        return;
    }

#if defined(__ZEPHYR__)
    /* phase-478 D4 — offer the sample's timestamp to the platform's epoch.
     * A no-op unless the image enables CONFIG_NROS_EPOCH_FROM_ZENOH, which is
     * decided in the platform port: this file sees no CONFIG_* (zpico-sys's
     * build script forwards none), so it calls unconditionally. A strong
     * reference, like the `nros_platform_task_attr_init` calls below: the
     * Zephyr port is always linked beside this shim, and a weak-undefined
     * call does not relocate on RISC-V (weak-symbols-allowlist.txt). */
    {
        extern void nros_zephyr_epoch_observe_ntp64(uint64_t ntp64);
        const z_timestamp_t* ts = z_sample_timestamp(sample);
        if (ts != NULL) {
            nros_zephyr_epoch_observe_ntp64(z_timestamp_ntp64_time(ts));
        }
    }
#endif

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
        _zpico_notify_spin(s);
        return;
    }
#endif

    if (entry->ring_mode) {
        _zpico_ring_push(s, entry, payload, z_sample_attachment(sample));
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
        _zpico_notify_spin(s);
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
    _zpico_notify_spin(s);
}

// ============================================================================
// Session Lifecycle Implementation
// ============================================================================

int32_t zpico_init(zpico_session_t* session, const char* locator) {
    return zpico_init_with_config(session, locator, "client", NULL, 0);
}

int32_t zpico_init_with_config(zpico_session_t* session, const char* locator, const char* mode,
                               const zpico_property_t* properties, size_t num_properties) {
    struct zpico_session* s = (struct zpico_session*)session;
    /* issue 0348 / phase-328 — each handle is an INDEPENDENT session drawn from
       the pool, so issue 0347's "refuse a second init while a session is open"
       guard is gone: a second `zpico_session_acquire()` + init operates on its
       own slot and cannot clobber the first session's registrations. The
       per-handle `initialized` / `session_open` lifecycle is unchanged (a FAILED
       `zpico_open()` leaves `s->session_open == false` and re-inits normally —
       issue #64's esp32-c3 backoff depends on that). */

    // Initialize storage
    memset(s->publishers, 0, sizeof(s->publishers));
    memset(s->subscribers, 0, sizeof(s->subscribers));
    memset(s->liveliness, 0, sizeof(s->liveliness));
    memset(s->queryables, 0, sizeof(s->queryables));
    memset(s->stored_query_valid, 0, sizeof(s->stored_query_valid));
    for (int i = 0; i < ZPICO_MAX_QUERYABLES; i++) {
        s->last_reply_seq[i] = -1;
        // issue 0902 / phase-455 W1 — the refusal counters belong to the table
        // they describe, so they are re-armed with it.
        s->reply_slot_refusals[i] = 0;
        s->reply_slot_saturated[i] = false;
        s->reply_slot_announce[i] = false;
        s->reply_slot_declines[i] = 0;
        for (int j = 0; j < ZPICO_MAX_PENDING_REPLIES; j++) {
            z_internal_query_null(&s->stored_queries[i][j]);
        }
    }
    s->session_open = false;

#ifdef ZPICO_SMOLTCP
    // Initialize smoltcp platform
    int ret = smoltcp_init();
    if (ret < 0) {
        return ZPICO_ERR_GENERIC;
    }
#endif

    // Initialize zenoh config
    z_config_default(&s->config);

    if (zp_config_insert(z_config_loan_mut(&s->config), Z_CONFIG_MODE_KEY, mode) < 0) {
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
        zpico_fill_session_zid(s, zid_bytes);
        zpico_format_session_zid(zid, zid_bytes);
        if (zp_config_insert(z_config_loan_mut(&s->config), Z_CONFIG_SESSION_ZID_KEY, zid) < 0) {
            return ZPICO_ERR_CONFIG;
        }
    }

    /* Apply additional properties.
     *
     * The name -> `Z_CONFIG_*_KEY` map is DERIVED from zenoh-pico's own
     * config.h (`scripts/gen-zpico-config-keys.py` -> `zpico_config_keys.h`),
     * not hand-written here. The hand-written version covered 10 of the 23
     * keys upstream defines; a closed list drifts on every upstream bump and
     * nothing notices, because the old `else` branch dropped an unmapped key
     * without a word.
     *
     * Three refusals, all loud, all `ZPICO_ERR_CONFIG` so the session does not
     * open with a configuration the caller believes it asked for:
     *   - a NULL key or value,
     *   - a name no `Z_CONFIG_*_KEY` matches (the only place a typo can be
     *     caught: zenoh-pico's keys are a bare numbered enum with no schema
     *     validation of its own),
     *   - a TLS key on a build without `Z_FEATURE_LINK_TLS`, which would
     *     otherwise be accepted into the config map and never read.
     *
     * Two of the 23 keys also arrive as dedicated arguments, and the ORDER
     * around this loop decides what that means. `mode` is inserted above, and
     * `zp_config_insert` REPLACES for every key but one — so a `mode` property
     * wins over the `mode` argument. `connect` is inserted below (after the
     * link properties, so TLS endpoint parsing can see the root CA), and it is
     * the one key upstream inserts with `_z_str_intmap_insert_push` — so a
     * `connect` property ADDS an endpoint rather than being overwritten by the
     * locator, which is how a caller states more than one. */
    for (size_t i = 0; i < num_properties; i++) {
        if (properties[i].key == NULL || properties[i].value == NULL) {
            printk("[zpico] session config: property %u has a NULL %s\n", (unsigned)i,
                   properties[i].key == NULL ? "key" : "value");
            return ZPICO_ERR_CONFIG;
        }

        const zpico_config_key_entry* entry = NULL;
        for (size_t k = 0; k < ZPICO_CONFIG_KEY_COUNT; k++) {
            if (strcmp(properties[i].key, ZPICO_CONFIG_KEYS[k].name) == 0) {
                entry = &ZPICO_CONFIG_KEYS[k];
                break;
            }
        }
        if (entry == NULL) {
            printk("[zpico] session config: unknown key '%s' — zenoh-pico defines no such "
                   "run-time option (see zpico_config_keys.h for the %u accepted names)\n",
                   properties[i].key, (unsigned)ZPICO_CONFIG_KEY_COUNT);
            return ZPICO_ERR_CONFIG;
        }
#if Z_FEATURE_LINK_TLS != 1
        if (entry->needs_tls) {
            printk("[zpico] session config: key '%s' needs a zenoh-pico built with "
                   "Z_FEATURE_LINK_TLS; this build has none\n",
                   properties[i].key);
            return ZPICO_ERR_CONFIG;
        }
#endif

        if (zp_config_insert(z_config_loan_mut(&s->config), entry->key, properties[i].value) < 0) {
            printk("[zpico] session config: zenoh-pico refused key '%s'\n", properties[i].key);
            return ZPICO_ERR_CONFIG;
        }
    }

    // Insert connect endpoint after link properties so TLS endpoint parsing can
    // see root CA / verification settings supplied through session config.
    if (locator != NULL) {
        if (zp_config_insert(z_config_loan_mut(&s->config), Z_CONFIG_CONNECT_KEY, locator) < 0) {
            return ZPICO_ERR_CONFIG;
        }
    }

    s->initialized = true;
    return ZPICO_OK;
}

/* Issue 0626 follow-up — `sched_get_priority_{min,max}` come from Zephyr's
 * `lib/posix/options/sched.c`, compiled only under
 * CONFIG_POSIX_PRIORITY_SCHEDULING. native_sim images enable it, cortex-m ones
 * do not, so gating on ZENOH_ZEPHYR alone left every cortex-m link with
 * `undefined reference to sched_get_priority_min` and took the whole zephyr
 * fixture family down with it.
 *
 * Guarded rather than reimplemented: mapping onto whatever the policy REPORTS
 * is the point of the band — the range is a build-configuration property
 * (CONFIG_NUM_PREEMPT_PRIORITIES), not a constant — and an image without the
 * POSIX scheduling option has no range to map onto. Such an image keeps the
 * pre-0626 behaviour (transport tasks at the default priority) and opts in with
 * `CONFIG_POSIX_PRIORITY_SCHEDULING=y`. */
#if defined(ZENOH_ZEPHYR) && defined(CONFIG_POSIX_PRIORITY_SCHEDULING)
/* Issue 0626 — apply a NORMALISED 0-31 priority (0 = least urgent, larger =
 * more urgent) to a pthread attr, in the one place that knows how.
 *
 * The band is phase-364 W5's, chosen so a single authored number does not mean
 * "run me first" on one kernel and "run me last" on another. It is mapped onto
 * the SCHED_RR range for THIS build rather than an assumed one — that range is
 * a Zephyr build-configuration property (`CONFIG_NUM_PREEMPT_PRIORITIES`), not
 * a constant. See the body for why it is computed rather than queried.
 *
 * SCHED_RR rather than SCHED_FIFO: the transport tasks are not the only
 * runnable work at their level, and a FIFO thread that never blocks would keep
 * the CPU indefinitely. Round-robin preserves the priority ORDER, which is what
 * is being asked for, without turning a busy transport into a starvation
 * source. */
/* issue 0736 — NuttX's raw-priority sibling of `zpico_posix_set_priority`.
 *
 * Separate function on purpose. The Zephyr helper takes a NORMALISED 0-31
 * priority and maps it across `CONFIG_NUM_PREEMPT_PRIORITIES`; NuttX tier
 * priorities are authored RAW (`[tiers.*.nuttx] priority = 110`) and go
 * verbatim into `pthread_attr_setschedparam`, which is the vocabulary issue
 * 0623 settled on after a normalised scale silently inverted a tier against
 * the transport band. Two scales through one function is how that happened;
 * this keeps NuttX in the units its callers already write.
 *
 * `sched_get_priority_{min,max}` are safe to call here — the NuttX board's own
 * tier spawn already uses them (`nuttx_clamp_priority`), unlike Zephyr where
 * they sit behind an experimental Kconfig.
 *
 * SCHED_FIFO, not SCHED_RR: the tiers are FIFO, and mixing policies across one
 * priority ORDER only matters among equals. The read task blocks in `recvfrom`
 * with a timeout, so it is not the never-yielding thread the Zephyr helper's
 * round-robin choice guards against.
 *
 * PTHREAD_EXPLICIT_SCHED is load-bearing, for the same reason it is in the
 * Zephyr arm and in the board's tier spawn: the default is
 * PTHREAD_INHERIT_SCHED, under which the policy and param below are IGNORED and
 * the thread silently takes the creator's priority. That silent inheritance is
 * the defect this is fixing — do not reintroduce it one layer down.
 *
 * A raw 0 means "unset": keep whatever the port would have done. */
/* issue 0765 — the guard is not NuttX-only any more.
 *
 * Linux and macOS speak the same raw SCHED_FIFO priorities NuttX does, and the
 * hosted "may not be allowed to request it" concern is about the RESULT, not
 * about the vocabulary: the call either succeeds or returns EPERM, and both are
 * reportable. Writing a second, Linux-flavoured copy of this would be the
 * two-spellings failure issue 0623 already charged the tree for. */

/* issue 0852 — `zpico_posix_set_priority` DELETED, not left beside the new path.
 *
 * It mapped a private 0-31 band onto SCHED_RR and wrote it into a
 * `pthread_attr_t` that no Zephyr code path ever read. Two things made keeping
 * it actively harmful rather than merely dead: a second normalised scale in the
 * same scheduler is issue 0623 verbatim, and
 * `scripts/lib/priority_plan.py:resolve_zephyr_plan` MODELLED THIS FUNCTION —
 * so `check-tier-priority-plan-image` certified a band no image ever had while
 * `realtime-c`'s `system.toml` authored tier values against that fiction.
 * The band now has exactly one spelling: `NROS_PLATFORM_PRIORITY_MAX`, mapped
 * by `nros_zephyr_native_priority`. */
#endif /* ZENOH_ZEPHYR */

/* issue 0765 — OUTSIDE the ZENOH_ZEPHYR block below.
 *
 * This first landed inside `#if defined(ZENOH_ZEPHYR) && ...`, which meant it
 * compiled ONLY on Zephyr while its callers are the NuttX and Linux/macOS arms
 * of `zpico_set_task_config` — the two platforms where it therefore did not
 * exist. A helper defined under a guard narrower than its call sites is a
 * link-time hole with a compile-time smell; keep it beside the function that
 * uses it. */
/* Guarded on the macros THIS BUILD defines (`nros-zpico-build`'s
 * `build.define(...)`), not on compiler/OS macros.
 *
 * An earlier revision of this comment claimed `ZENOH_LINUX` was "not yet
 * defined at this point" because it came from a header further down. That was
 * wrong: these are `-D` flags and are set for the whole TU. The real reason the
 * first attempt failed to compile was that this helper sat INSIDE the
 * `ZENOH_ZEPHYR` block above — a helper guarded more narrowly than its call
 * sites (issue 0775). Recorded because the wrong explanation was plausible and
 * would have sent the next reader to the include order. */
#if defined(ZENOH_NUTTX) || defined(ZENOH_LINUX) || defined(ZENOH_MACOS)
/* issue 0765 — is SCHED_FIFO actually permitted for this process?
 *
 * This must be asked BEFORE the policy goes into a pthread ATTRIBUTE, because
 * an attribute is not best-effort the way a post-create `pthread_setschedparam`
 * is: with `PTHREAD_EXPLICIT_SCHED` set, `pthread_create` itself returns EPERM
 * and the thread is never created. Measured the hard way — setting the
 * transport band on an unprivileged host made the zenoh session fail to open
 * at all (`RMW session open failed — Backend("rmw_ret error")`), turning a
 * scheduling preference into a total outage.
 *
 * Probed by asking on THIS thread and putting it back, once per process. A
 * request that is refused changes nothing; one that succeeds is undone
 * immediately, so the probe cannot leave the caller on a policy it did not
 * choose. */
static int zpico_posix_rt_permitted(void) {
    static int cached = -1;
    if (cached >= 0) {
        return cached;
    }
    int old_policy;
    struct sched_param old_param;
    if (pthread_getschedparam(pthread_self(), &old_policy, &old_param) != 0) {
        cached = 0;
        return cached;
    }
    struct sched_param probe;
    memset(&probe, 0, sizeof(probe));
    probe.sched_priority = sched_get_priority_min(SCHED_FIFO);
    if (pthread_setschedparam(pthread_self(), SCHED_FIFO, &probe) == 0) {
        (void)pthread_setschedparam(pthread_self(), old_policy, &old_param);
        cached = 1;
    } else {
        cached = 0;
    }
    return cached;
}

static void zpico_posix_fifo_set_priority(pthread_attr_t* attr, uint32_t raw) {
    if (raw == 0u) {
        return;
    }
    if (!zpico_posix_rt_permitted()) {
        /* Leave the attribute alone: the tasks inherit, exactly as before this
         * arm existed. The tier path has already printed the one line naming
         * `setcap cap_sys_nice+ep`, so saying it again per task would be noise
         * about a fact the operator has been told. */
        return;
    }
    int lo = sched_get_priority_min(SCHED_FIFO);
    int hi = sched_get_priority_max(SCHED_FIFO);
    int p = (int)raw;
    if (p < lo) {
        p = lo;
    }
    if (p > hi) {
        p = hi;
    }
    struct sched_param sp;
    sp.sched_priority = p;
    (void)pthread_attr_setschedpolicy(attr, SCHED_FIFO);
    (void)pthread_attr_setinheritsched(attr, PTHREAD_EXPLICIT_SCHED);
    (void)pthread_attr_setschedparam(attr, &sp);
}
#endif

void zpico_set_task_config(uint32_t read_priority, uint32_t read_stack_bytes) {
#if Z_FEATURE_MULTI_THREAD == 1
    /* RFC-0110 D2 — zenoh-pico 1.9 runs the read path, the lease and keep-alive
     * sends, the reconnect and every user callback on ONE executor thread, so
     * the read task's attributes are the executor's. The lease task and the two
     * arguments that configured it are gone (RFC-0110 step 6). */
    memset(&g_default_read_task_attr, 0, sizeof(g_default_read_task_attr));

    // Platform-specific field assignment.
    // z_task_attr_t varies by platform — only FreeRTOS and POSIX-like
    // platforms have meaningful fields. ThreadX and generic use void*
    // and zenoh-pico ignores the attr entirely on those platforms.
#if defined(ZENOH_FREERTOS) || defined(ZENOH_FREERTOS_LWIP)
    g_default_read_task_attr.name = "zpico_read";
    g_default_read_task_attr.priority = (UBaseType_t)read_priority;
    g_default_read_task_attr.stack_depth = read_stack_bytes / sizeof(StackType_t);
    g_default_read_task_opts.task_attributes = &g_default_read_task_attr;
    g_default_read_task_configured = true;
/* issue 0775 — `ZENOH_NUTTX`, not `__NuttX__`.
 *
 * `__NuttX__` is a NuttX HEADER macro and this TU never pulls the header, so
 * this condition has never been true on NuttX. Measured from the build's own
 * flags: the NuttX lane compiles with `-DZENOH_NUTTX -DZENOH_GENERIC` and NO
 * `-DZENOH_LINUX`, so every arm below keyed on that list fell through to the
 * generic one — which discards the attributes these arms exist to set. */
#elif (defined(ZENOH_LINUX) || defined(ZENOH_MACOS) || defined(ZENOH_NUTTX) ||                     \
       defined(__NuttX__) || defined(ZENOH_ZEPHYR)) &&                                             \
    !defined(ZENOH_THREADX)
    // POSIX: stack size via pthread_attr. A ZERO means "leave the port's
    // default alone" (the convention `nros_platform_task_attr_t.stack_bytes`
    // already uses), so a caller that only wants to state a PRIORITY does not
    // have to invent a stack size to get one.
    pthread_attr_init(&g_default_read_task_attr);
    if (read_stack_bytes != 0u) {
        pthread_attr_setstacksize(&g_default_read_task_attr, (size_t)read_stack_bytes);
    }
#if defined(ZENOH_ZEPHYR)
    // Issue 0626 — priority IS settable here, and used not to be set.
    //
    // The comment this replaces said "Priority requires SCHED_FIFO (root
    // privileges)". That is true of Linux and false of Zephyr, which has no
    // privilege model: a pthread priority is just a thread priority. The
    // sentence was written for the POSIX hosts that share this branch and then
    // applied to an RTOS, so the transport tasks on every Zephyr image ran at
    // whatever the default happened to be, with no way to state otherwise —
    // which made issue 0506's question (what preempts the RT tiers?)
    // unanswerable on this platform.
    //
    // PTHREAD_EXPLICIT_SCHED is load-bearing: the default is
    // PTHREAD_INHERIT_SCHED, under which the policy and param set below are
    // IGNORED and the new thread silently takes the creator's. A scheduling
    // attribute that is quietly dropped is the failure this issue is about, so
    // it must not be reintroduced one layer down.
    /* issue 0852 — fill the struct the Zephyr port actually READS.
     *
     * This used to call `zpico_posix_set_priority` on `g_default_read_task_attr`,
     * a `pthread_attr_t`. Nothing ever read it. `_z_task_init` on this port
     * (`zephyr/nros_zenoh_zephyr_system.c`) forwards `task_attributes` to
     * `nros_platform_task_init`, which reads an `nros_platform_task_attr_t` —
     * and `zephyr.h` typedefs `z_task_attr_t` to `pthread_attr_t`, so the two
     * are different types and the cast read past the end of a 12-byte object.
     * The bytes there are zero, so the transport was born at band 0: SCHED_RR
     * 0, k_thread 14 — the LEAST urgent preemptible slot, under an executor at
     * 0. Before that "fix" it merely tied with the executor.
     *
     * This is issue 0803 one platform over: that issue fixed exactly this cast
     * for NuttX/Linux/macOS and left Zephyr off the guard list — the one
     * platform whose `z_task_attr_t` genuinely IS a `pthread_attr_t`, so the
     * only one where the bug was guaranteed rather than incidental.
     *
     * BAND, not `NROS_PLATFORM_PRIORITY_RAW`: `nros_zephyr_native_priority`
     * maps `0..NROS_PLATFORM_PRIORITY_MAX` onto whatever `SCHED_RR` reports for
     * THIS image, which is the point of the normalised band. The old 0-31
     * scale is gone rather than left beside this one — two normalised scales
     * meeting in one scheduler is issue 0623 verbatim, and a 16 authored
     * against 0-31 maps to 0 through the 255-wide one, i.e. the bottom slot
     * again. */
    nros_platform_task_attr_init(&g_default_read_nros_attr);
    if (read_priority != 0u) {
        g_default_read_nros_attr.priority = (int32_t)read_priority;
    }
    g_default_read_nros_attr.name = "zpico_exec";
    if (read_stack_bytes != 0u) {
        g_default_read_nros_attr.stack_bytes = read_stack_bytes;
    }
#else
    // issue 0765 / 0775 — Linux, macOS AND NuttX place the transport here.
    //
    // NuttX had its own `#elif defined(__NuttX__)` arm for a while. It never
    // compiled: `__NuttX__` is a NuttX HEADER macro and this TU never pulls the
    // header, while `nros-zpico-build` defines `ZENOH_NUTTX` AND `ZENOH_LINUX`
    // for that target — so NuttX has always fallen through to this branch,
    // which used to discard the priority. The arm is merged away rather than
    // re-guarded: all three ports want the identical raw-SCHED_FIFO call, and
    // a separate arm that agrees with this one is just somewhere for them to
    // drift apart (issue 0775).
    //
    // This arm used to discard the priority, on the grounds that RT scheduling
    // "needs a policy this process may not be allowed to request". True, and
    // not a reason to refuse to ASK: `setcap cap_sys_nice+ep` is exactly how
    // play_launch does this without root, and where the capability is absent
    // the attribute is simply ignored at spawn — the same best-effort contract
    // every other port here already has.
    //
    // Refusing to ask had a cost that only appeared once TIER priorities became
    // real (issue 0765): the tiers run SCHED_FIFO while the read and lease
    // tasks stay on SCHED_OTHER, and a SCHED_FIFO thread outranks every
    // SCHED_OTHER thread unconditionally. So the app preempted the link it
    // publishes over, with no way for an operator to state otherwise — issue
    // 0623's inversion, on the one platform that could not express the fix.
    /* issue 0803 — fill the struct the alias reader expects, not a
     * `pthread_attr_t`. `NROS_PLATFORM_PRIORITY_RAW` because this is a RAW
     * SCHED_FIFO value from the board's reserved band, not a value on the
     * normalised cross-RTOS band — the same vocabulary issue 0623 settled on
     * after a normalised scale silently inverted a tier against the transport. */
    nros_platform_task_attr_init(&g_default_read_nros_attr);
    if (read_priority != 0u) {
        g_default_read_nros_attr.priority = NROS_PLATFORM_PRIORITY_RAW(read_priority);
    }
    if (read_stack_bytes != 0u) {
        g_default_read_nros_attr.stack_bytes = read_stack_bytes;
    }
#endif
    g_default_read_task_opts.task_attributes = (z_task_attr_t*)&g_default_read_nros_attr;
    g_default_read_task_configured = true;
#elif defined(ZENOH_THREADX)
    // Issue 0626 — ThreadX carries the platform ABI's attributes now.
    //
    // This used to share the `#else` arm below, whose comment ("z_task_attr_t
    // is void* and zenoh-pico ignores it") was accurate when written and is
    // what made the priority unstatable here: `_z_task_init` in
    // `c/platform/threadx/task.c` discarded the attr and gave every zenoh task
    // the single compile-time `Z_TASK_PRIORITY`.
    //
    // The priority stays on the NORMALISED band; `task.c` inverts it, because
    // ThreadX counts 0 as the HIGHEST priority and the band counts larger as
    // more urgent. Doing the inversion at the spawn site keeps every caller in
    // one vocabulary (phase-364 W5) — that is the whole point of the band.
    //
    // Stacks are not forwarded: the ThreadX `_z_task_t` embeds its stack at the
    // compile-time `Z_TASK_STACK_SIZE`, so there is no larger region to point
    // at, and `task.c` documents the same refusal at the other end.
    nros_platform_task_attr_init(&g_default_read_task_attr);
    g_default_read_task_attr.name = "zpico_read";
    g_default_read_task_attr.priority = (int32_t)read_priority;
    g_default_read_task_opts.task_attributes = &g_default_read_task_attr;
    g_default_read_task_configured = true;
    (void)read_stack_bytes;
#else
    // Bare-metal and other single-threaded platforms: `z_task_attr_t` is still
    // `void *` there (see `c/platform/bare-metal/platform.h`) and no task is
    // ever created, so there is nothing to configure.
    (void)read_priority;
    (void)read_stack_bytes;
#endif
#else
    (void)read_priority;
    (void)read_stack_bytes;
#endif
}

void zpico_set_flush_task_config(uint32_t priority, uint32_t stack_bytes) {
#if ZPICO_TX_BATCH_THREAD == 1
    memset(&g_default_flush_task_attr, 0, sizeof(g_default_flush_task_attr));
#if defined(ZENOH_FREERTOS) || defined(ZENOH_FREERTOS_LWIP)
    g_default_flush_task_attr.name = "zpico_flush";
    g_default_flush_task_attr.priority = (UBaseType_t)priority;
    /* issue 1534 — 0 means "the port default", as on every other port: the
     * 5120 words zenoh-pico's FreeRTOS `system.c` gives a task spawned with a
     * NULL attr (its default task attributes). A literal 0 would ask
     * xTaskCreate for a zero-word stack. */
    g_default_flush_task_attr.stack_depth =
        (stack_bytes != 0u) ? (stack_bytes / sizeof(StackType_t)) : ZPICO_FREERTOS_DEFAULT_STACK_DEPTH;
    g_default_flush_task_configured = true;
/* issue 0775 — `ZENOH_NUTTX`, not `__NuttX__`.
 *
 * `__NuttX__` is a NuttX HEADER macro and this TU never pulls the header, so
 * this condition has never been true on NuttX. Measured from the build's own
 * flags: the NuttX lane compiles with `-DZENOH_NUTTX -DZENOH_GENERIC` and NO
 * `-DZENOH_LINUX`, so every arm below keyed on that list fell through to the
 * generic one — which discards the attributes these arms exist to set. */
#elif (defined(ZENOH_LINUX) || defined(ZENOH_MACOS) || defined(ZENOH_NUTTX) ||                     \
       defined(__NuttX__) || defined(ZENOH_ZEPHYR)) &&                                             \
    !defined(ZENOH_THREADX)
#if ZPICO_FLUSH_NROS_ATTR == 1
    /* issue 0852 — the flush task is the THIRD transport task and it had the
     * same defect as read and lease: it filled a `pthread_attr_t` that the
     * Zephyr port never reads, and discarded the priority outright ("needs
     * SCHED_FIFO (root)" — true of Linux, meaningless on an RTOS with no
     * privilege model). Fixed in the same commit as its two siblings, because
     * fixing one transport task and leaving the third is how this class keeps
     * coming back. Band, not RAW — see the read/lease arm. */
    nros_platform_task_attr_init(&g_default_flush_nros_attr);
    if (priority != 0u) {
        /* The same vocabulary the read setter uses on this port (issue 0623):
         * Zephyr's normalised band, a RAW SCHED_FIFO value on Linux, macOS and
         * NuttX (issue 0803). Issue 1534 moved those three onto this arm; until
         * then the "needs SCHED_FIFO (root)" arm below discarded the priority
         * and handed the port a `pthread_attr_t`. */
#if defined(ZENOH_ZEPHYR)
        g_default_flush_nros_attr.priority = (int32_t)priority;
#else
        g_default_flush_nros_attr.priority = NROS_PLATFORM_PRIORITY_RAW(priority);
#endif
    }
    g_default_flush_nros_attr.name = "zpico_flush";
    if (stack_bytes != 0u) {
        g_default_flush_nros_attr.stack_bytes = stack_bytes;
    }
    g_default_flush_task_configured = true;
#else
    (void)priority;
    (void)stack_bytes;
#endif
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

#if ZPICO_TX_BATCH_THREAD == 1
/* issue 1534 — the flush task's default: the READ task's urgency (priority,
 * core, flags), its own name, the port's default stack. Called by zpico_open
 * when a read task was configured and no flush task was. */
static void zpico_flush_task_follows_read(void) {
#if ZPICO_FLUSH_NROS_ATTR == 1
    nros_platform_task_attr_init(&g_default_flush_nros_attr);
    g_default_flush_nros_attr.priority = g_default_read_nros_attr.priority;
    g_default_flush_nros_attr.core = g_default_read_nros_attr.core;
    g_default_flush_nros_attr.flags = g_default_read_nros_attr.flags;
    g_default_flush_nros_attr.name = "zpico_flush";
    g_default_flush_task_configured = true;
#elif defined(ZENOH_FREERTOS) || defined(ZENOH_FREERTOS_LWIP)
    zpico_set_flush_task_config((uint32_t)g_default_read_task_attr.priority, 0u);
#endif
}
#endif

// ============================================================================
// Executor stack peak — RFC-0110 D2's "measured, not summed"
// ============================================================================
//
// In THREAD mode one thread runs zenoh-pico's whole session: the read path,
// the lease and keep-alive sends, the reconnect (a blocking connect, handshake
// and declaration replay) and every user callback. Its stack used to be two
// stacks (read + lease) sized separately; RFC-0110 D2 says the one that
// replaces them is sized from a MEASURED high-water mark, the way issue 1146
// sized `app_stack_bytes`, and that every image prints its own peak so the
// next person reads the number instead of copying it.
//
// The measurement runs ON the executor thread, as one more future: it samples
// its own thread's high-water mark, and once the value has held still for a
// while it prints one line and finishes. No board needs a handle to the
// thread, so no board needs plumbing. It costs one `Z_RUNTIME_MAX_TASKS` slot
// for at most ZPICO_EXEC_STACK_MAX_SAMPLES seconds after the first open; the
// measured floor is 5 (open + a reconnect), so 8 still has room for it.
//
// Where the platform cannot answer (POSIX hosts, Zephyr without
// CONFIG_INIT_STACKS + CONFIG_THREAD_STACK_INFO, NuttX without stack
// coloration) nothing is spawned and nothing is printed.
#if defined(ZPICO_DRIVE_THREAD) &&                                                                  \
    (defined(ZENOH_FREERTOS_LWIP) ||                                                               \
     (defined(ZENOH_ZEPHYR) && defined(CONFIG_INIT_STACKS) && defined(CONFIG_THREAD_STACK_INFO)) || \
     (defined(ZENOH_NUTTX) && defined(CONFIG_STACK_COLORATION)))
#define ZPICO_EXEC_STACK_REPORT 1
#if defined(ZENOH_FREERTOS_LWIP)
#include <task.h>
#elif defined(ZENOH_NUTTX)
#include <nuttx/arch.h>
#include <nuttx/sched.h>
#endif

#define ZPICO_EXEC_STACK_FIRST_MS 4000u
#define ZPICO_EXEC_STACK_PERIOD_MS 1000u
#define ZPICO_EXEC_STACK_STABLE_SAMPLES 10u
#define ZPICO_EXEC_STACK_MAX_SAMPLES 45u

static struct {
    bool spawned;
    uint32_t samples;
    uint32_t stable;
    size_t prev_used;
} g_exec_stack;

/* (used, size) of the CALLING thread's stack; size 0 when it is not known. */
static void _zpico_exec_stack_sample(size_t* used, size_t* size) {
#if defined(ZENOH_FREERTOS_LWIP)
    *size = g_default_read_task_configured
                ? (size_t)g_default_read_task_attr.stack_depth * sizeof(StackType_t)
                : 0u;
    size_t unused = (size_t)uxTaskGetStackHighWaterMark(NULL) * sizeof(StackType_t);
    *used = (*size > unused) ? *size - unused : 0u;
    if (*size == 0u) {
        *used = unused; /* reported as headroom below */
    }
#elif defined(ZENOH_ZEPHYR)
    size_t unused = 0;
    *size = k_current_get()->stack_info.size;
    *used = (k_thread_stack_space_get(k_current_get(), &unused) == 0 && *size > unused)
                ? *size - unused
                : 0u;
#elif defined(ZENOH_NUTTX)
    struct tcb_s* tcb = nxsched_self();
    *size = tcb->adj_stack_size;
    *used = up_check_tcbstack(tcb, tcb->adj_stack_size);
#endif
}

static _z_fut_fn_result_t _zpico_exec_stack_fn(void* arg, _z_executor_t* executor) {
    (void)arg;
    (void)executor;
    if (g_exec_stack.samples++ == 0u) {
        return _z_fut_fn_result_wake_up_after(ZPICO_EXEC_STACK_FIRST_MS);
    }
    size_t used = 0;
    size_t size = 0;
    _zpico_exec_stack_sample(&used, &size);
    /* The high-water mark only grows, so a value that has held for a while is
     * the answer; a fixed delay would be a guess a slow router under-reports. */
    if (used == g_exec_stack.prev_used) {
        g_exec_stack.stable++;
    } else {
        g_exec_stack.stable = 0;
        g_exec_stack.prev_used = used;
    }
    if (g_exec_stack.stable < ZPICO_EXEC_STACK_STABLE_SAMPLES &&
        g_exec_stack.samples < ZPICO_EXEC_STACK_MAX_SAMPLES) {
        return _z_fut_fn_result_wake_up_after(ZPICO_EXEC_STACK_PERIOD_MS);
    }
    if (size != 0u) {
        printk("nros: zenoh executor stack peak %lu of %lu bytes (%lu free) (RFC-0110 D2)\n",
               (unsigned long)used, (unsigned long)size, (unsigned long)(size - used));
    } else {
        printk("nros: zenoh executor stack peak: %lu bytes never touched, size not stated "
               "(RFC-0110 D2)\n",
               (unsigned long)used);
    }
    return _z_fut_fn_result_ready();
}

/* Once per process: the first session's executor carries the measurement. */
static void _zpico_exec_stack_spawn(struct zpico_session* s) {
    if (g_exec_stack.spawned) {
        return;
    }
    _z_fut_t f = _z_fut_null();
    f._fut_fn = _zpico_exec_stack_fn;
    f._fut_arg = NULL;
    if (!_z_fut_handle_is_null(
            _z_runtime_spawn(&_Z_RC_IN_VAL(z_session_loan(&s->session))->_runtime, &f))) {
        g_exec_stack.spawned = true;
    }
}
#endif

int32_t zpico_open(zpico_session_t* session) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (!s->initialized) {
        return ZPICO_ERR_GENERIC;
    }

#if Z_FEATURE_MULTI_THREAD == 1
    /* issue 0348 / phase-328 — copy the process-wide task-spawn DEFAULTS (set by
     * the board via zpico_set_task_config before any session existed) into this
     * session, and re-point the opts at the session's OWN attr copy. */
#if defined(ZENOH_ZEPHYR) || defined(ZENOH_THREADX)
    /* Issue 0626 — neither Zephyr nor ThreadX has a board-side caller for
     * `zpico_set_task_config` (the FreeRTOS board is its only one in the tree),
     * so before this the transport tasks got NULL attrs and an unstatable
     * priority. Apply the compile-time default here when nothing set one
     * explicitly; a board that DOES call the setter first wins, because that
     * sets `configured`.
     *
     * Stacks are passed as 0 deliberately: this path is stating a priority.
     * Zephyr reads 0 as "leave the port default alone", and ThreadX ignores the
     * field outright (its stack is embedded in `_z_task_t`). */
    if (!g_default_read_task_configured) {
        zpico_set_task_config(ZPICO_READ_TASK_PRIORITY, 0u);
    }
#endif
#if ZPICO_TX_BATCH_THREAD == 1
    /* Issue 1534 -- the tx-flush task is the third transport task and had no
     * default at all: spawned with a NULL attr, it landed at the platform
     * default -- k_thread 0 on Zephyr, ABOVE the read task; configMAX_PRIORITIES
     * / 2 on FreeRTOS, wherever that falls against the board's read priority.
     * On a serial link it then preempted the RX drain for as long as it had
     * work, and the ISR's ring overflowed (measured on an S32K344 at 921,600
     * baud: the read task READY, state 0x80, while the flush thread ran and the
     * ring sat at 1024/1024). The flush task has no business outranking the
     * task that receives, so it takes the READ task's urgency -- whatever the
     * read task was given, by a board or by the default above: equal priority,
     * no preemption either way, and each yields when it blocks. A board that
     * calls zpico_set_flush_task_config first still wins.
     *
     * This was a Zephyr-only `zpico_set_flush_task_config(
     * ZPICO_READ_TASK_PRIORITY)`, which was right only while no Zephyr board
     * states a read priority, and left every other port's flush task where the
     * platform default put it. */
    if (!g_default_flush_task_configured && g_default_read_task_configured) {
        zpico_flush_task_follows_read();
    }
#endif
    s->read_task_configured = g_default_read_task_configured;
    s->read_task_attr = g_default_read_task_attr;
    s->read_task_opts = g_default_read_task_opts;
#if defined(ZENOH_NUTTX) || defined(ZENOH_LINUX) || defined(ZENOH_MACOS) || defined(ZENOH_ZEPHYR)
    /* issue 0852 — ZENOH_ZEPHYR joins this list. It was the platform the cast
     * below was guaranteed to break on, and it was the one left out. */
    /* issue 0803 — point at the per-session `nros_platform_task_attr_t`, which
     * is what `platform_aliases.c`'s `_z_task_init` forwards to
     * `nros_platform_task_init`. Pointing at `read_task_attr` handed it a
     * `pthread_attr_t`, whose bytes at the `priority` offset are 0 whatever was
     * requested — decoded as band value 0, the LEAST urgent RT priority, so the
     * transport landed at SCHED_FIFO 1 below every tier and 90/40/5 all
     * produced identical kernel tables. */
    s->read_nros_attr = g_default_read_nros_attr;
    if (s->read_task_configured) {
        s->read_task_opts.task_attributes = (z_task_attr_t*)&s->read_nros_attr;
    }
#else
    if (s->read_task_configured) {
        s->read_task_opts.task_attributes = &s->read_task_attr;
    }
#endif
#endif
#if ZPICO_TX_BATCH_THREAD == 1
    s->flush_task_configured = g_default_flush_task_configured;
    s->flush_task_attr = g_default_flush_task_attr;
#if ZPICO_FLUSH_NROS_ATTR == 1
    s->flush_nros_attr = g_default_flush_nros_attr;
#endif
#endif

    z_open_options_t open_opts;
    z_open_options_default(&open_opts);
#if Z_FEATURE_MULTI_THREAD == 1
    /* The executor thread is started below, once the spin-wake primitive its
     * callbacks signal exists (THREAD), or never (CALLER — RFC-0110 D4). */
    open_opts.auto_start_read_task = false;
#endif
    int open_ret = z_open(&s->session, z_config_move(&s->config), &open_opts);
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
        z_id_t zid = z_info_zid(z_session_loan(&s->session));
        (void)zid;
    }

    // RFC-0110 D2 — THREAD mode starts zenoh-pico's executor thread, which runs
    // read, lease, keep-alive, query timeout and reconnect. CALLER mode
    // (ThreadX, bare metal) starts nothing: zpico_spin_once() runs the executor.
#if defined(ZPICO_DRIVE_THREAD)
#if defined(ZENOH_FREERTOS_LWIP)
    s->spin_sem = xSemaphoreCreateBinary();
#elif defined(ZENOH_NUTTX)
    if (sem_init(&s->spin_sem_posix, 0, 0) == 0) {
        s->spin_sem_initialized = true;
    }
#elif !defined(ZPICO_SMOLTCP)
    _z_mutex_init(&s->spin_mutex);
    _z_condvar_init(&s->spin_cv);
    s->spin_cv_initialized = true;
#endif

    const zp_task_read_options_t* read_opts = s->read_task_configured ? &s->read_task_opts : NULL;

    if (zp_start_read_task(z_session_loan_mut(&s->session), read_opts) < 0) {
        z_close(z_session_loan_mut(&s->session), NULL);
        return ZPICO_ERR_TASK;
    }
#if defined(ZPICO_EXEC_STACK_REPORT)
    _zpico_exec_stack_spawn(s);
#endif
#endif

#if defined(ZPICO_TX_BATCH) && ZPICO_TX_BATCH == 1
    /* phase-279 (#145) — opt-in tx batching: puts/gets append to the transport
     * write buffer instead of sending; one socket send per flush carries the
     * whole batch. Flush cadence = zpico_spin_once (every executor spin) +
     * zenoh-pico's own implicit flushes (batch-buffer overflow, any transport
     * message — so the lease keepalive bounds batch sit-time even without
     * spins). Express messages (query replies, gets, express publishers)
     * bypass the batch inside zenoh-pico. Compile-time knob, default OFF. */
    zp_batch_start(z_session_loan(&s->session));
#if ZPICO_TX_BATCH_THREAD == 1
    s->tx_flush_run = true;
#if ZPICO_FLUSH_NROS_ATTR == 1
    /* issue 0852 — hand the port the struct it reads, as read and lease do. */
    z_task_attr_t* flush_attr =
        s->flush_task_configured ? (z_task_attr_t*)&s->flush_nros_attr : NULL;
#else
    z_task_attr_t* flush_attr = s->flush_task_configured ? &s->flush_task_attr : NULL;
#endif
    if (_z_task_init(&s->tx_flush_task, flush_attr, _zpico_tx_flush_task_fn, s) != 0) {
        /* No thread → flushes ride only on implicit sends (keepalives bound
         * sit-time to the lease interval). Loud, not fatal. */
        s->tx_flush_run = false;
        printk("zpico: tx-flush task init FAILED — batched puts flush on keepalives only\n");
    }
#endif
#endif

#if Z_FEATURE_MULTI_THREAD == 1
    if (!s->slot_mutex_initialized && _z_mutex_init(&s->slot_mutex) == 0) {
        s->slot_mutex_initialized = true;
    }
    if (!s->declare_mutex_initialized && _z_mutex_rec_init(&s->declare_mutex) == 0) {
        s->declare_mutex_initialized = true;
    }
#endif
    s->session_open = true;
    return ZPICO_OK;
}

int32_t zpico_is_open(zpico_session_t* session) {
    struct zpico_session* s = (struct zpico_session*)session;
    return s->session_open ? 1 : 0;
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
int32_t zpico_publish_streamed(zpico_session_t* session, int32_t handle, size_t total_len,
                               void (*chunk_cb)(uint8_t* out_buf, size_t cap, size_t* out_written,
                                                void* user_ctx),
                               void* user_ctx, const uint8_t* attachment, size_t attachment_len) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (handle < 0 || handle >= ZPICO_MAX_PUBLISHERS || !s->publishers[handle].active) {
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

    if (z_publisher_put(z_publisher_loan(&s->publishers[handle].publisher), z_bytes_move(&payload),
                        &opts) < 0) {
        return ZPICO_ERR_PUBLISH;
    }

    return ZPICO_OK;
}

/**
 * Phase 124.F.2 — wire-level "is the agent still reachable?" probe.
 *
 * Issues one `_zp_send_keep_alive` against the open session. On
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
int32_t zpico_send_keep_alive(zpico_session_t* session) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (!s->session_open) {
        return ZPICO_ERR_SESSION;
    }
    /* RFC-0110 D5 — the public `zp_send_keep_alive` is compiled out of
     * multi-thread builds since zenoh-pico 1.9; the internal call every build
     * has sends one keep-alive through the transport's TX path. */
    z_result_t ret = _zp_send_keep_alive(_Z_RC_IN_VAL(z_session_loan(&s->session)));
    if (ret < 0) {
        return ZPICO_ERR_TIMEOUT;
    }
    return ZPICO_OK;
}

void zpico_close(zpico_session_t* session) {
    struct zpico_session* s = (struct zpico_session*)session;
    // Clean up publishers
    zpico_declare_lock(s);
    for (int i = 0; i < ZPICO_MAX_PUBLISHERS; i++) {
        if (s->publishers[i].active) {
            z_undeclare_publisher(z_publisher_move(&s->publishers[i].publisher));
            s->publishers[i].active = false;
        }
    }

    // Clean up subscribers
    for (int i = 0; i < ZPICO_MAX_SUBSCRIBERS; i++) {
        if (s->subscribers[i].active) {
            z_undeclare_subscriber(z_subscriber_move(&s->subscribers[i].subscriber));
            s->subscribers[i].active = false;
            s->subscribers[i].callback = NULL;
            s->subscribers[i].ctx = NULL;
        }
    }

    // Clean up liveliness tokens
    for (int i = 0; i < ZPICO_MAX_LIVELINESS; i++) {
        if (s->liveliness[i].active) {
            z_liveliness_undeclare_token(z_liveliness_token_move(&s->liveliness[i].token));
            s->liveliness[i].active = false;
        }
        /* Issue 2083 -- the key a live token kept for the session's own graph. */
        if (s->liveliness[i].key != NULL) {
            z_free(s->liveliness[i].key);
            s->liveliness[i].key = NULL;
        }
    }

    // Clean up queryables
    for (int i = 0; i < ZPICO_MAX_QUERYABLES; i++) {
        if (s->queryables[i].active) {
            z_undeclare_queryable(z_queryable_move(&s->queryables[i].queryable));
            s->queryables[i].active = false;
            s->queryables[i].callback = NULL;
            s->queryables[i].ctx = NULL;
        }
    }
    zpico_declare_unlock(s);

    // Close session
    if (s->session_open) {
#if defined(ZPICO_TX_BATCH) && ZPICO_TX_BATCH == 1
#if ZPICO_TX_BATCH_THREAD == 1
        if (s->tx_flush_run) {
            s->tx_flush_run = false;
            _z_task_join(&s->tx_flush_task);
        }
#endif
        /* phase-279 (#145) — stop batching; zp_batch_stop flushes the remainder. */
        zp_batch_stop(z_session_loan(&s->session));
#endif
#if defined(ZPICO_DRIVE_THREAD)
        // Stop the executor thread (RFC-0110 D2; `z_close` would stop it too).
        zp_stop_read_task(z_session_loan_mut(&s->session));

#if defined(ZENOH_FREERTOS_LWIP)
        if (s->spin_sem != NULL) {
            vSemaphoreDelete(s->spin_sem);
            s->spin_sem = NULL;
        }
#elif defined(ZENOH_NUTTX)
        if (s->spin_sem_initialized) {
            s->spin_sem_initialized = false;
            sem_destroy(&s->spin_sem_posix);
        }
#elif !defined(ZPICO_SMOLTCP)
        s->spin_cv_initialized = false;
        _z_condvar_drop(&s->spin_cv);
        _z_mutex_drop(&s->spin_mutex);
#endif
#endif
        z_close(z_session_loan_mut(&s->session), NULL);
        s->session_open = false;
#if Z_FEATURE_MULTI_THREAD == 1
        if (s->slot_mutex_initialized) {
            s->slot_mutex_initialized = false;
            _z_mutex_drop(&s->slot_mutex);
        }
        if (s->declare_mutex_initialized) {
            s->declare_mutex_initialized = false;
            _z_mutex_rec_drop(&s->declare_mutex);
        }
#endif
    }

#ifdef ZPICO_SMOLTCP
    // Cleanup smoltcp platform
    smoltcp_cleanup();
#endif

    s->initialized = false;
}

// ============================================================================
// Entity slot claim -- issue 1711
// ============================================================================
//
// Every declare used to FIND a free slot (`!active`), declare into it, and only
// then set `active = true`. With one zenoh session shared by several tier
// threads (RFC-0015 Model 1), two concurrent declares found the SAME slot and
// both wrote into it: measured under ASan on `derived-tiers-cpp`, one thread's
// failure path (`_z_undeclare_publisher`) freed the keyexpr string the other
// thread's declaration had just allocated in that slot, while the other was
// still comparing against it -- a heap-use-after-free that surfaced as a
// SIGSEGV in `malloc` roughly one boot in four.
//
// `zpico_claim_slot` finds and MARKS the slot under `slot_mutex`, so the slot
// is the caller's before the declare starts; the declare itself runs outside
// the lock (a subscriber callback on the read task may declare an entity, and
// zenoh-pico takes its own session mutex inside `z_declare_*`). A declare that
// then fails gives the slot back with `zpico_release_slot`. Without
// `Z_FEATURE_MULTI_THREAD` there is no second thread and no lock.
static inline void zpico_slot_lock(struct zpico_session* s) {
#if Z_FEATURE_MULTI_THREAD == 1
    if (s->slot_mutex_initialized) {
        _z_mutex_lock(&s->slot_mutex);
    }
#else
    (void)s;
#endif
}

static inline void zpico_slot_unlock(struct zpico_session* s) {
#if Z_FEATURE_MULTI_THREAD == 1
    if (s->slot_mutex_initialized) {
        _z_mutex_unlock(&s->slot_mutex);
    }
#else
    (void)s;
#endif
}

// Issue 1749 -- zenoh-pico's declare path is not thread-safe on one session.
// `_z_get_entity_id` is `zn->_entity_id++`, and `_z_cache_declaration` /
// `_z_prune_declaration` (Z_FEATURE_AUTO_RECONNECT) push onto and filter the
// session's `_declaration_cache` list with no lock. Two threads declaring or
// undeclaring at once tear that list: issue 1711's own regression test
// SIGSEGVed 3 of 100 runs inside `_z_prune_declaration`, and helgrind saw the
// push race between two tier setups (issue 1733).
//
// Every call into that path from this file -- `z_declare_*`, `z_undeclare_*`,
// the liveliness token/subscriber pair, `z_get` and `z_liveliness_get` (both
// take an entity id) -- runs under this per-session lock. RECURSIVE, because a
// declare can deliver synchronously to a local subscriber or queryable whose
// callback is ours. Lock order is this lock, then zenoh-pico's own mutexes;
// nothing here takes it from inside a zenoh-pico callback, so the reverse
// order never occurs. Without `Z_FEATURE_MULTI_THREAD` there is no second
// thread and no lock.
//
// The lease task's reconnect replay (`_z_reopen`) walks the same list on
// zenoh-pico's own thread, which this lock cannot reach; the zenoh-pico fork
// now guards the cache itself with the session's `_mutex_transport` and makes
// the entity-id counter atomic (issue 1749). This lock is KEPT anyway: it is
// one uncontended take per declare, and it still serializes the rest of
// zenoh-pico's declare path (registration lists, write filters, interests)
// against concurrent tier setups, which nobody has audited lock by lock.
// Order: this lock, then `_mutex_transport` -- the replay never takes this one.
static inline void zpico_declare_lock(struct zpico_session* s) {
#if Z_FEATURE_MULTI_THREAD == 1
    if (s->declare_mutex_initialized) {
        _z_mutex_rec_lock(&s->declare_mutex);
    }
#else
    (void)s;
#endif
}

static inline void zpico_declare_unlock(struct zpico_session* s) {
#if Z_FEATURE_MULTI_THREAD == 1
    if (s->declare_mutex_initialized) {
        _z_mutex_rec_unlock(&s->declare_mutex);
    }
#else
    (void)s;
#endif
}

/* Claim the first slot whose `active` flag is clear and set it, atomically with
 * respect to every other claim. `first_active` is `&table[0].active`, `stride`
 * is `sizeof(table[0])`. Returns the index, or -1 when the table is full. */
static int zpico_claim_slot_impl(struct zpico_session* s, bool* first_active, size_t stride,
                                 int count) {
    int idx = -1;
    zpico_slot_lock(s);
    for (int i = 0; i < count; i++) {
        bool* active = (bool*)((uint8_t*)first_active + (size_t)i * stride);
        if (!*active) {
            *active = true;
            idx = i;
            break;
        }
    }
    zpico_slot_unlock(s);
    return idx;
}

static void zpico_release_slot_impl(struct zpico_session* s, bool* active) {
    zpico_slot_lock(s);
    *active = false;
    zpico_slot_unlock(s);
}

#define zpico_claim_slot(s, table, count)                                                          \
    zpico_claim_slot_impl((s), &(s)->table[0].active, sizeof((s)->table[0]), (count))
#define zpico_release_slot(s, table, idx) zpico_release_slot_impl((s), &(s)->table[(idx)].active)

// ============================================================================
// Publisher Implementation
// ============================================================================

int32_t zpico_declare_publisher(zpico_session_t* session, const char* keyexpr) {
    /* RELIABLE, the ROS default QoS. */
    return zpico_declare_publisher_ex(session, keyexpr, 0, 1);
}

int32_t zpico_declare_publisher_ex(zpico_session_t* session, const char* keyexpr,
                                   int32_t is_express, int32_t is_reliable) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (!s->session_open) {
        return ZPICO_ERR_SESSION;
    }

    // Find free slot
    int idx = zpico_claim_slot(s, publishers, ZPICO_MAX_PUBLISHERS);
    if (idx < 0) {
        return ZPICO_ERR_FULL;
    }

    z_view_keyexpr_t ke;
    int ke_ret = z_view_keyexpr_from_str(&ke, keyexpr);
    if (ke_ret < 0) {
        zpico_release_slot(s, publishers, idx);
        return ZPICO_ERR_KEYEXPR;
    }

    z_publisher_options_t pub_opts;
    z_publisher_options_default(&pub_opts);
    /* phase-279 (#145) — express publishers bypass tx batching inside zenoh-pico
     * (sent immediately, wire EXPRESS flag). Surfaced from TopicInfo::tx_express
     * through NrosRmwQos; harmless without batching. */
    pub_opts.is_express = (is_express != 0);
    /* Issue 2001 — what a put does when ANOTHER thread holds the transport TX
     * mutex. Data sends serialise on the session's transport lock first
     * (`_z_send_n_msg`, issue 0899), so that other thread is the lease task
     * sending a keep-alive, which takes the TX mutex directly. zenoh-pico's
     * default DROP only TRY-locks it and discards the put, which is not
     * zenoh's DROP (that drops on a full send queue). A RELIABLE publisher
     * therefore WAITS (BLOCK), for as long as the keep-alive's write takes.
     * BEST_EFFORT keeps DROP: losing a sample is what it allows, and since
     * issue 2005 the fork reports that drop as success. */
    if (is_reliable != 0) {
        pub_opts.congestion_control = Z_CONGESTION_CONTROL_BLOCK;
    }
#if defined(ZPICO_TX_BATCH) && ZPICO_TX_BATCH == 1
    /* Batching queues puts behind the transport tx mutex; the default DROP
     * congestion control TRY-locks it and silently discards the put whenever a
     * flush is mid-send (waiting on the socket) — measured WORSE than no
     * batching (4.7 vs 8.6 msg/s). BLOCK = append-or-wait: every put lands in
     * the batch and ships with the next flush. Declare-time option (per-put
     * options carry no congestion control in zenoh-pico). */
    pub_opts.congestion_control = Z_CONGESTION_CONTROL_BLOCK;
#endif
    zpico_declare_lock(s);
    int pub_ret = z_declare_publisher(z_session_loan(&s->session), &s->publishers[idx].publisher,
                                      z_view_keyexpr_loan(&ke), &pub_opts);
    zpico_declare_unlock(s);
    if (pub_ret < 0) {
        printk("zpico: z_declare_publisher failed: %d for '%s'\n", pub_ret, keyexpr);
        zpico_release_slot(s, publishers, idx);
        return ZPICO_ERR_GENERIC;
    }

    /* Claimed by zpico_claim_slot (issue 1711); set again for the reader. */
    s->publishers[idx].active = true;
    return idx;
}

int32_t zpico_publish(zpico_session_t* session, int32_t handle, const uint8_t* data, size_t len) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (handle < 0 || handle >= ZPICO_MAX_PUBLISHERS || !s->publishers[handle].active) {
        return ZPICO_ERR_INVALID;
    }

    z_owned_bytes_t payload;
    int bytes_ret = z_bytes_copy_from_buf(&payload, data, len);
    if (bytes_ret < 0) {
        printk("zpico: z_bytes_copy_from_buf failed: %d\n", bytes_ret);
        return ZPICO_ERR_PUBLISH;
    }

    int put_ret = z_publisher_put(z_publisher_loan(&s->publishers[handle].publisher),
                                  z_bytes_move(&payload), NULL);
    if (put_ret < 0) {
        printk("zpico: z_publisher_put failed: %d\n", put_ret);
        return ZPICO_ERR_PUBLISH;
    }

    return ZPICO_OK;
}

int32_t zpico_undeclare_publisher(zpico_session_t* session, int32_t handle) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (handle < 0 || handle >= ZPICO_MAX_PUBLISHERS || !s->publishers[handle].active) {
        return ZPICO_ERR_INVALID;
    }

    zpico_declare_lock(s);
    z_undeclare_publisher(z_publisher_move(&s->publishers[handle].publisher));
    zpico_declare_unlock(s);
    zpico_release_slot(s, publishers, handle);
    return ZPICO_OK;
}

// ============================================================================
// Subscriber Implementation
// ============================================================================

int32_t zpico_declare_subscriber(zpico_session_t* session, const char* keyexpr,
                                 ZpicoCallback callback, void* ctx) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (!s->session_open) {
        return ZPICO_ERR_SESSION;
    }

    // Find free slot
    int idx = zpico_claim_slot(s, subscribers, ZPICO_MAX_SUBSCRIBERS);
    if (idx < 0) {
        return ZPICO_ERR_FULL;
    }

    s->subscribers[idx].callback = callback;
    s->subscribers[idx].ctx = ctx;
    s->subscribers[idx].with_attachment = false; // Legacy mode

    z_view_keyexpr_t ke;
    if (z_view_keyexpr_from_str(&ke, keyexpr) < 0) {
        s->subscribers[idx].callback = NULL;
        s->subscribers[idx].ctx = NULL;
        zpico_release_slot(s, subscribers, idx);
        return ZPICO_ERR_KEYEXPR;
    }

    // Create closure for callback, passing index as context
    z_owned_closure_sample_t closure;
    z_closure_sample(&closure, sample_handler, NULL, _zpico_pack_ctx(s, idx));

    zpico_declare_lock(s);
    int sub_ret =
        z_declare_subscriber(z_session_loan(&s->session), &s->subscribers[idx].subscriber,
                             z_view_keyexpr_loan(&ke), z_closure_sample_move(&closure), NULL);
    zpico_declare_unlock(s);
    if (sub_ret < 0) {
        printk("zpico: z_declare_subscriber failed: %d for '%s'\n", sub_ret, keyexpr);
        s->subscribers[idx].callback = NULL;
        s->subscribers[idx].ctx = NULL;
        zpico_release_slot(s, subscribers, idx);
        return ZPICO_ERR_GENERIC;
    }

    /* Claimed by zpico_claim_slot (issue 1711); set again for the reader. */
    s->subscribers[idx].active = true;
    return idx;
}

int32_t zpico_declare_subscriber_with_attachment(zpico_session_t* session, const char* keyexpr,
                                                 ZpicoCallbackWithAttachment callback, void* ctx) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (!s->session_open) {
        return ZPICO_ERR_SESSION;
    }

    // Find free slot
    int idx = zpico_claim_slot(s, subscribers, ZPICO_MAX_SUBSCRIBERS);
    if (idx < 0) {
        return ZPICO_ERR_FULL;
    }

    s->subscribers[idx].callback_ext = callback;
    s->subscribers[idx].ctx = ctx;
    s->subscribers[idx].with_attachment = true; // Extended mode with attachment

    z_view_keyexpr_t ke;
    if (z_view_keyexpr_from_str(&ke, keyexpr) < 0) {
        s->subscribers[idx].callback_ext = NULL;
        s->subscribers[idx].ctx = NULL;
        zpico_release_slot(s, subscribers, idx);
        return ZPICO_ERR_KEYEXPR;
    }

    // Create closure for callback, passing index as context
    z_owned_closure_sample_t closure;
    z_closure_sample(&closure, sample_handler, NULL, _zpico_pack_ctx(s, idx));

    zpico_declare_lock(s);
    int sub_ret =
        z_declare_subscriber(z_session_loan(&s->session), &s->subscribers[idx].subscriber,
                             z_view_keyexpr_loan(&ke), z_closure_sample_move(&closure), NULL);
    zpico_declare_unlock(s);
    if (sub_ret < 0) {
        printk("zpico: z_declare_subscriber failed: %d for '%s'\n", sub_ret, keyexpr);
        s->subscribers[idx].callback_ext = NULL;
        s->subscribers[idx].ctx = NULL;
        zpico_release_slot(s, subscribers, idx);
        return ZPICO_ERR_GENERIC;
    }

    /* Claimed by zpico_claim_slot (issue 1711); set again for the reader. */
    s->subscribers[idx].active = true;
    return idx;
}

int32_t zpico_declare_subscriber_direct_write(zpico_session_t* session, const char* keyexpr,
                                              uint8_t* buf_ptr, size_t buf_capacity,
                                              const bool* locked_ptr, ZpicoNotifyCallback callback,
                                              void* ctx) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (!s->session_open) {
        return ZPICO_ERR_SESSION;
    }

    // Find free slot
    int idx = zpico_claim_slot(s, subscribers, ZPICO_MAX_SUBSCRIBERS);
    if (idx < 0) {
        return ZPICO_ERR_FULL;
    }

    s->subscribers[idx].notify = callback;
    s->subscribers[idx].ctx = ctx;
    s->subscribers[idx].with_attachment = false;
    s->subscribers[idx].direct_write = true;
    s->subscribers[idx].buf_ptr = buf_ptr;
    s->subscribers[idx].buf_capacity = buf_capacity;
    s->subscribers[idx].locked_ptr = locked_ptr;

    z_view_keyexpr_t ke;
    if (z_view_keyexpr_from_str(&ke, keyexpr) < 0) {
        s->subscribers[idx].notify = NULL;
        s->subscribers[idx].ctx = NULL;
        s->subscribers[idx].direct_write = false;
        zpico_release_slot(s, subscribers, idx);
        return ZPICO_ERR_KEYEXPR;
    }

    // Create closure for callback, passing index as context
    z_owned_closure_sample_t closure;
    z_closure_sample(&closure, sample_handler, NULL, _zpico_pack_ctx(s, idx));

    zpico_declare_lock(s);
    int sub_ret =
        z_declare_subscriber(z_session_loan(&s->session), &s->subscribers[idx].subscriber,
                             z_view_keyexpr_loan(&ke), z_closure_sample_move(&closure), NULL);
    zpico_declare_unlock(s);
    if (sub_ret < 0) {
        printk("zpico: z_declare_subscriber failed: %d for '%s'\n", sub_ret, keyexpr);
        s->subscribers[idx].notify = NULL;
        s->subscribers[idx].ctx = NULL;
        s->subscribers[idx].direct_write = false;
        zpico_release_slot(s, subscribers, idx);
        return ZPICO_ERR_GENERIC;
    }

    /* Claimed by zpico_claim_slot (issue 1711); set again for the reader. */
    s->subscribers[idx].active = true;
    return idx;
}

/* phase-412 -- which exit `zpico_declare_subscriber_ring` took, and the raw
 * zenoh-pico code when the declare itself is what failed.
 *
 * The return value cannot carry this. Five exits collapse onto four ZPICO_ERR_*
 * codes, and ZPICO_ERR_GENERIC in particular DISCARDS `sub_ret`, the only
 * number saying what zenoh-pico objected to. The printk beside it carries both
 * -- to the console, which is not wired on the MR-CANHUBK344. On the one board
 * that needed the message it is written and unreadable.
 *
 * Globals rather than an out-param, so the ABI does not move: the Rust caller
 * reads them immediately after a failed call, on a path that already failed.
 *
 * Numbering is stable and append-only. 0 after a call means NO exit stamped,
 * which is itself a finding when the caller reports an error it believes came
 * from here.
 */
int32_t zpico_last_sub_declare_exit = 0;
int32_t zpico_last_sub_declare_ret = 0;

int32_t zpico_declare_subscriber_ring(zpico_session_t* session, const char* keyexpr,
                                      zpico_ring_desc_t* desc, ZpicoNotifyCallback callback,
                                      void* ctx) {
    struct zpico_session* s = (struct zpico_session*)session;
    /* Reset at entry: a value left by an earlier call must not be read as
     * this one's. 0 therefore means "entered but stamped no exit". */
    zpico_last_sub_declare_exit = 0;
    zpico_last_sub_declare_ret = 0;
    if (!s->session_open) {
        zpico_last_sub_declare_exit = 1;
        return ZPICO_ERR_SESSION;
    }
    if (desc == NULL || desc->slot_count == 0) {
        zpico_last_sub_declare_exit = 2;
        return ZPICO_ERR_INVALID;
    }

    int idx = zpico_claim_slot(s, subscribers, ZPICO_MAX_SUBSCRIBERS);
    if (idx < 0) {
        zpico_last_sub_declare_exit = 3;
        return ZPICO_ERR_FULL;
    }

    s->subscribers[idx].notify = callback;
    s->subscribers[idx].ctx = ctx;
    s->subscribers[idx].with_attachment = false;
    s->subscribers[idx].direct_write = false;
    s->subscribers[idx].ring_mode = true;
    s->subscribers[idx].ring = desc;
    /* Issue 1979 -- the consumer's claim takes this session's slot lock. */
    desc->lock_session = s;
    s->subscribers[idx].hist_active = false;
    s->subscribers[idx].hist_have = false;
    s->subscribers[idx].hist_replies = 0;
    s->subscribers[idx].hist_dropped = 0;

    z_view_keyexpr_t ke;
    if (z_view_keyexpr_from_str(&ke, keyexpr) < 0) {
        s->subscribers[idx].notify = NULL;
        s->subscribers[idx].ctx = NULL;
        s->subscribers[idx].ring_mode = false;
        s->subscribers[idx].ring = NULL;
        zpico_last_sub_declare_exit = 4;
        zpico_release_slot(s, subscribers, idx);
        return ZPICO_ERR_KEYEXPR;
    }

    z_owned_closure_sample_t closure;
    z_closure_sample(&closure, sample_handler, NULL, _zpico_pack_ctx(s, idx));

    zpico_declare_lock(s);
    int sub_ret =
        z_declare_subscriber(z_session_loan(&s->session), &s->subscribers[idx].subscriber,
                             z_view_keyexpr_loan(&ke), z_closure_sample_move(&closure), NULL);
    zpico_declare_unlock(s);
    if (sub_ret < 0) {
        printk("zpico: z_declare_subscriber (ring) failed: %d for '%s'\n", sub_ret, keyexpr);
        s->subscribers[idx].notify = NULL;
        s->subscribers[idx].ctx = NULL;
        s->subscribers[idx].ring_mode = false;
        s->subscribers[idx].ring = NULL;
        zpico_last_sub_declare_exit = 5;
        zpico_last_sub_declare_ret = (int32_t)sub_ret;
        zpico_release_slot(s, subscribers, idx);
        return ZPICO_ERR_GENERIC;
    }

    /* Claimed by zpico_claim_slot (issue 1711); set again for the reader. */
    s->subscribers[idx].active = true;
    zpico_last_sub_declare_exit = 6;
    return idx;
}

/* Reply handler of a transient-local history query: each cached sample goes
 * into the subscription's ring exactly as a live one would, through the same
 * producer and the same window dedup. An error reply (a peer's queryable
 * refusing) carries no sample and is ignored. */
static void history_reply_handler(z_loaned_reply_t* reply, void* arg) {
    struct zpico_session* s = _zpico_unpack_session(arg);
    int idx = _zpico_unpack_slot(arg);
    if (idx < 0 || idx >= ZPICO_MAX_SUBSCRIBERS) {
        return;
    }
    subscriber_entry_t* entry = &s->subscribers[idx];
    if (!entry->active || !entry->ring_mode || !z_reply_is_ok(reply)) {
        return;
    }
    const z_loaned_sample_t* sample = z_reply_ok(reply);
    entry->hist_replies++;
    _zpico_ring_push(s, entry, z_sample_payload(sample), z_sample_attachment(sample));
}

/* The query is complete (every queryable answered, or the timeout fired):
 * close the window, so later samples skip the dedup. */
static void history_reply_dropper(void* arg) {
    struct zpico_session* s = _zpico_unpack_session(arg);
    int idx = _zpico_unpack_slot(arg);
    if (idx < 0 || idx >= ZPICO_MAX_SUBSCRIBERS) {
        return;
    }
    s->subscribers[idx].hist_active = false;
    _zpico_notify_spin(s);
}

/* The SUBSCRIBER half of transient-local durability: one history query, at
 * creation, on `keyexpr` -- the caller passes `<subscription keyexpr>/@adv/**`,
 * the global query `rmw_zenoh_cpp`'s advanced subscriber issues, which
 * intersects every advanced publisher's cache queryable
 * (`<topic keyexpr>/@adv/pub/<zid>/<eid>/_`, served by nano-ros's own TL
 * publishers too). Replies land in the subscription's ring beside live
 * samples; see `hist_active` for the ordering rule.
 *
 * Costs no slot of the pending-get table: the context is the subscriber's own
 * index, and zenoh-pico holds the pending query until its dropper runs.
 * Returns 0 once the query is sent, ZPICO_ERR_* otherwise. */
int32_t zpico_subscriber_history_query(zpico_session_t* session, int32_t handle,
                                       const char* keyexpr, uint32_t timeout_ms) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (s == NULL || keyexpr == NULL) {
        return ZPICO_ERR_INVALID;
    }
    if (!s->session_open) {
        return ZPICO_ERR_SESSION;
    }
    if (handle < 0 || handle >= ZPICO_MAX_SUBSCRIBERS || !s->subscribers[handle].active ||
        !s->subscribers[handle].ring_mode) {
        return ZPICO_ERR_INVALID;
    }
    subscriber_entry_t* entry = &s->subscribers[handle];
    if (entry->hist_active) {
        return 0; /* one window per subscription; the first query serves it */
    }
    z_view_keyexpr_t ke;
    if (z_view_keyexpr_from_str(&ke, keyexpr) < 0) {
        return ZPICO_ERR_KEYEXPR;
    }
    z_get_options_t opts;
    z_get_options_default(&opts);
    opts.target = Z_QUERY_TARGET_ALL;
    opts.timeout_ms = (uint64_t)timeout_ms;
    /* Every cache's reply, as it arrives: consolidation would keep one reply
     * per key, and all replies here carry the TOPIC key. */
    opts.consolidation.mode = Z_CONSOLIDATION_MODE_NONE;
    /* A cache replies on the TOPIC key, which does not intersect the query
     * key `<topic>/@adv/**`; without `_anyke` the publisher's session refuses
     * to send that reply and answers only the final one. Measured on the
     * safety island against a stock rclpy latched publisher: the query was
     * routed, the final reply came back 0.5 ms later, and no sample. This is
     * what `rmw_zenoh_cpp`'s advanced subscriber sets too
     * (`accept_replies(ReplyKeyExpr::Any)`). */
    opts.accept_replies = Z_REPLY_KEYEXPR_ANY;
    entry->hist_have = false;
    entry->hist_replies = 0;
    entry->hist_dropped = 0;
    entry->hist_active = true;
    z_owned_closure_reply_t callback;
    z_closure(&callback, history_reply_handler, history_reply_dropper, _zpico_pack_ctx(s, handle));
    zpico_declare_lock(s);
    z_result_t hist_ret =
        z_get(z_session_loan(&s->session), z_view_keyexpr_loan(&ke), "", z_move(callback), &opts);
    zpico_declare_unlock(s);
    if (hist_ret < 0) {
        entry->hist_active = false;
        return ZPICO_ERR_GENERIC;
    }
    return 0;
}

/* What the history window did, for a test or a probe: `out_replies` cached
 * samples arrived, `out_dropped` samples (either source) were older than one
 * already delivered. Returns 1 while the window is open, 0 once closed. */
int32_t zpico_subscriber_history_stats(zpico_session_t* session, int32_t handle,
                                       uint32_t* out_replies, uint32_t* out_dropped) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (s == NULL || handle < 0 || handle >= ZPICO_MAX_SUBSCRIBERS) {
        return ZPICO_ERR_INVALID;
    }
    subscriber_entry_t* entry = &s->subscribers[handle];
    if (out_replies != NULL) {
        *out_replies = entry->hist_replies;
    }
    if (out_dropped != NULL) {
        *out_dropped = entry->hist_dropped;
    }
    return entry->hist_active ? 1 : 0;
}

#if defined(Z_FEATURE_UNSTABLE_API)
int32_t zpico_subscribe_zero_copy(zpico_session_t* session, const char* keyexpr,
                                  ZpicoZeroCopyCallback callback, void* ctx) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (!s->session_open) {
        return ZPICO_ERR_SESSION;
    }

    // Find free slot
    int idx = zpico_claim_slot(s, subscribers, ZPICO_MAX_SUBSCRIBERS);
    if (idx < 0) {
        return ZPICO_ERR_FULL;
    }

    s->subscribers[idx].ctx = ctx;
    s->subscribers[idx].with_attachment = false;
    s->subscribers[idx].direct_write = false;
    s->subscribers[idx].zero_copy = true;
    s->subscribers[idx].zero_copy_cb = callback;

    z_view_keyexpr_t ke;
    if (z_view_keyexpr_from_str(&ke, keyexpr) < 0) {
        s->subscribers[idx].zero_copy = false;
        s->subscribers[idx].zero_copy_cb = NULL;
        s->subscribers[idx].ctx = NULL;
        zpico_release_slot(s, subscribers, idx);
        return ZPICO_ERR_KEYEXPR;
    }

    // Create closure for callback, passing index as context
    z_owned_closure_sample_t closure;
    z_closure_sample(&closure, sample_handler, NULL, _zpico_pack_ctx(s, idx));

    zpico_declare_lock(s);
    int sub_ret =
        z_declare_subscriber(z_session_loan(&s->session), &s->subscribers[idx].subscriber,
                             z_view_keyexpr_loan(&ke), z_closure_sample_move(&closure), NULL);
    zpico_declare_unlock(s);
    if (sub_ret < 0) {
        printk("zpico: z_declare_subscriber (zero_copy) failed: %d for '%s'\n", sub_ret, keyexpr);
        s->subscribers[idx].zero_copy = false;
        s->subscribers[idx].zero_copy_cb = NULL;
        s->subscribers[idx].ctx = NULL;
        zpico_release_slot(s, subscribers, idx);
        return ZPICO_ERR_GENERIC;
    }

    /* Claimed by zpico_claim_slot (issue 1711); set again for the reader. */
    s->subscribers[idx].active = true;
    return idx;
}
#else
// Stub when unstable API is not enabled — returns error
int32_t zpico_subscribe_zero_copy(zpico_session_t* session, const char* keyexpr,
                                  ZpicoZeroCopyCallback callback, void* ctx) {
    (void)session;
    (void)keyexpr;
    (void)callback;
    (void)ctx;
    return ZPICO_ERR_GENERIC;
}
#endif

int32_t zpico_undeclare_subscriber(zpico_session_t* session, int32_t handle) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (handle < 0 || handle >= ZPICO_MAX_SUBSCRIBERS || !s->subscribers[handle].active) {
        return ZPICO_ERR_INVALID;
    }

    zpico_declare_lock(s);
    z_undeclare_subscriber(z_subscriber_move(&s->subscribers[handle].subscriber));
    zpico_declare_unlock(s);
    zpico_release_slot(s, subscribers, handle);
    s->subscribers[handle].callback = NULL;
    s->subscribers[handle].ctx = NULL;
    s->subscribers[handle].with_attachment = false;
    return ZPICO_OK;
}

// ============================================================================
// CALLER-mode executor driving — RFC-0110 D3/D4
// ============================================================================

#if defined(ZPICO_DRIVE_CALLER)
// The most executor steps one `zpico_spin_once` runs. One step runs one ready
// future (a read, a lease check, a keep-alive, a query timeout), so a flood of
// inbound frames cannot hold the caller for longer than this many of them; the
// rest wait for the next spin. The caller-side twin of the read future's own
// frame budget (fork commit `00f8e2f0`).
#ifndef ZPICO_SPIN_BUDGET
#define ZPICO_SPIN_BUDGET 64
#endif

// Run ready executor futures until none is ready or the budget is spent.
// `zp_spin_once` returns false when the executor has nothing ready (an idle
// read reports itself sleeping because `Z_RUNTIME_IDLE_READ_TASK_SLEEP` is 1 in
// a CALLER build), so a quiet link costs one step. On ThreadX (RFC-0110 D4) the
// fork's multi-thread `zp_spin_once` takes the executor's lock with a TRY-lock:
// a tier that loses the race returns at once, because the winner is draining —
// the single-reader rule of issue #247, now enforced by zenoh-pico's own lock.
static void _zpico_run_executor(struct zpico_session* s) {
    for (uint32_t steps = 0; steps < ZPICO_SPIN_BUDGET; steps++) {
        if (!zp_spin_once(z_session_loan(&s->session))) {
            break;
        }
    }
}
#endif

// ============================================================================
// Polling Implementation (zpico_poll — deleted in Phase 77.20; use
// zpico_spin_once() instead, which adds keep-alive handling)
// ============================================================================

int32_t zpico_spin_once(zpico_session_t* session, uint32_t timeout_ms) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (!s->session_open) {
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
     * Racy across tier threads sharing this session is benign (worst case one
     * extra flush). zenoh-pico's implicit flushes (buffer overflow, any
     * transport message) still bound memory + sit-time independently of this
     * cadence. The cadence state is per-session (was function-local statics). */
    {
        if (!s->batch_flush_init ||
            z_clock_elapsed_ms(&s->last_batch_flush) >= ZPICO_TX_BATCH_FLUSH_MS) {
            zp_batch_flush(z_session_loan(&s->session));
            s->last_batch_flush = z_clock_now();
            s->batch_flush_init = true;
        }
    }
#endif

#if defined(ZPICO_DRIVE_CALLER)
    // RFC-0110 D3/D4 — run zenoh-pico's executor here, in the caller's thread.
    // Read, lease, keep-alive, query timeout and reconnect are all futures in
    // its queue, so this one loop replaces the polled `zp_read`, the hand-run
    // `_z_pending_query_process_timeout` and the per-spin keep-alive the old
    // arms made. Mixing the polled read with the executor is not an option: it
    // resets the buffer the read future compacts (issue #247's frame loss).
    //
    // issue 1512 / issue 0387's class — a QUIET tick returns 0, never an error:
    // `rclc_executor_spin_period` gives up after SPIN_ERROR_TOLERANCE failed
    // rounds, and an idle spin is not a failure. Only a session zenoh-pico has
    // closed is reported.
    s->spin_delivered = false;
#if defined(ZENOH_THREADX)
    // phase-297 W5 — the timed wait MUST be `z_sleep_ms` (tx_thread_sleep),
    // never a host select(): a host syscall blocks the pthread while the ThreadX
    // scheduler still counts this thread as running, so under strict priority
    // scheduling every other tier thread starves. Sleep first (a REAL ThreadX
    // yield — lower-priority tiers run inside it), then drain what is already
    // there; the socket shim's reads do not block (RFC-0110 D4).
    if (timeout_ms > 0) {
        z_sleep_ms(timeout_ms);
    }
    _zpico_run_executor(s);
#else
    // Bare metal (smoltcp, serial) and any other single-thread build. With
    // `timeout_ms == 0`, drain once. Otherwise keep driving until a callback
    // delivered application data or the deadline passes, idling between rounds.
    //
    // issue 2022 — on smoltcp the idle is `nros_smoltcp_idle_until_ms`, which
    // sleeps to the deadline (narrowed by smoltcp's own timers) on a board that
    // a frame can wake, and is a no-op on one with no wake source.
#ifdef ZPICO_SMOLTCP
    uint64_t start = smoltcp_clock_now_ms();
#else
    z_clock_t start = z_clock_now();
#endif
    for (;;) {
        _zpico_run_executor(s);
        if (s->spin_delivered || timeout_ms == 0) {
            break;
        }
#ifdef ZPICO_SMOLTCP
        if (smoltcp_clock_now_ms() - start >= timeout_ms) {
            break;
        }
        nros_smoltcp_idle_until_ms(start + timeout_ms);
#else
        if (z_clock_elapsed_ms(&start) >= timeout_ms) {
            break;
        }
        z_sleep_ms(1);
#endif
    }
#endif
    if (z_session_is_closed(z_session_loan(&s->session))) {
        return ZPICO_ERR_SESSION;
    }
    return 0;

#elif defined(ZENOH_FREERTOS_LWIP)
    // FreeRTOS+lwIP: the executor thread handles data and keep-alives
    // (RFC-0110 D2). Wait on a binary semaphore that _zpico_notify_spin()
    // signals when application data arrives (subscriptions, query replies).
    // This gives near-zero latency wake-up without busy-looping.
    if (timeout_ms > 0 && s->spin_sem != NULL) {
        xSemaphoreTake(s->spin_sem, pdMS_TO_TICKS(timeout_ms));
    }
    return 0;

#else
    // THREAD mode (Zephyr/POSIX/NuttX): the executor thread handles data and
    // keep-alives (RFC-0110 D2). Wait on a wake primitive that our
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
        if (s->spin_sem_initialized) {
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
            while (sem_timedwait(&s->spin_sem_posix, &deadline) != 0 && errno == EINTR) {
                // retry on signal
            }
        } else {
            // Fallback if sem_init failed at session open.
            usleep((useconds_t)timeout_ms * 1000);
        }
#else
        z_clock_t deadline = z_clock_now();
        z_clock_advance_ms(&deadline, (unsigned long)timeout_ms);
        _z_mutex_lock(&s->spin_mutex);
        _z_condvar_wait_until(&s->spin_cv, &s->spin_mutex, &deadline);
        _z_mutex_unlock(&s->spin_mutex);
#endif
    }
    return 0;

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

// RFC-0113 D6 — the RFC-0110 D1 choice, reported. In THREAD mode zenoh-pico's
// own executor thread runs the lease and keep-alive, so the caller owes it no
// deadline; in CALLER mode those futures run only inside `zpico_spin_once`.
bool zpico_drives_on_thread(void) {
#if defined(ZPICO_DRIVE_THREAD)
    return true;
#else
    return false;
#endif
}

// ============================================================================
// ZenohId Implementation
// ============================================================================

int32_t zpico_get_zid(zpico_session_t* session, uint8_t* zid_out) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (!s->session_open || zid_out == NULL) {
        return ZPICO_ERR_SESSION;
    }

    z_id_t zid = z_info_zid(z_session_loan(&s->session));
    memcpy(zid_out, zid.id, 16);
    return ZPICO_OK;
}

// ============================================================================
// Liveliness Implementation
// ============================================================================

int32_t zpico_declare_liveliness(zpico_session_t* session, const char* keyexpr) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (!s->session_open) {
        return ZPICO_ERR_SESSION;
    }

    // Find free slot
    int idx = zpico_claim_slot(s, liveliness, ZPICO_MAX_LIVELINESS);
    if (idx < 0) {
        /* phase-412 — issue 0283 gave the declare-failure arm below a printk
         * because "a failed token is a SILENT graph outage (the ROS 2 tools see
         * nothing)". THIS arm produces the identical outcome and was left mute,
         * so an image that ran out of slots looked like one that never tried.
         * It names the knob because the count is not derivable from anything
         * the caller can see. */
        printk("zpico: liveliness pool exhausted (NROS_ZENOH_MAX_LIVELINESS=%d) for '%s'\n"
               "       every node takes one token and every publisher, subscriber,\n"
               "       service server and client takes one more; this entity works\n"
               "       but is invisible to ROS 2 tooling\n",
               ZPICO_MAX_LIVELINESS, keyexpr);
        return ZPICO_ERR_FULL;
    }

    z_view_keyexpr_t ke;
    if (z_view_keyexpr_from_str(&ke, keyexpr) < 0) {
        zpico_release_slot(s, liveliness, idx);
        return ZPICO_ERR_KEYEXPR;
    }

    zpico_declare_lock(s);
    int lv_ret = z_liveliness_declare_token(z_session_loan(&s->session), &s->liveliness[idx].token,
                                            z_view_keyexpr_loan(&ke), NULL);
    zpico_declare_unlock(s);
    if (lv_ret < 0) {
        /* issue 0283 — a failed token is a SILENT graph outage (the ROS 2
         * tools see nothing) — say so on the console like the publisher /
         * subscriber declare paths do. */
        printk("zpico: z_liveliness_declare_token failed: %d for '%s'\n", lv_ret, keyexpr);
        zpico_release_slot(s, liveliness, idx);
        return ZPICO_ERR_GENERIC;
    }

    /* Claimed by zpico_claim_slot (issue 1711); set again for the reader. */
    s->liveliness[idx].active = true;
#if ZPICO_GRAPH_DISCOVERY
    /* Issue 2083 -- zenoh-pico delivers this token to the router and to every
     * OTHER session, never to this session's own liveliness subscriber, so
     * without this an image's own servers are missing from its own graph: a
     * client in the same process waits forever for a server it hosts. The key
     * is kept so the cache can be seeded when it starts later than the token,
     * and emptied when the token goes. A failed copy only costs this image its
     * view of itself, which is what it had before. */
    size_t klen = strlen(keyexpr);
    s->liveliness[idx].key = (char*)z_malloc(klen + 1);
    if (s->liveliness[idx].key != NULL) {
        memcpy(s->liveliness[idx].key, keyexpr, klen + 1);
        graph_cache_apply(s, s->liveliness[idx].key, klen, false);
    }
#endif
    return idx;
}

int32_t zpico_undeclare_liveliness(zpico_session_t* session, int32_t handle) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (handle < 0 || handle >= ZPICO_MAX_LIVELINESS || !s->liveliness[handle].active) {
        return ZPICO_ERR_INVALID;
    }

    zpico_declare_lock(s);
    z_liveliness_undeclare_token(z_liveliness_token_move(&s->liveliness[handle].token));
    zpico_declare_unlock(s);
#if ZPICO_GRAPH_DISCOVERY
    /* Issue 2083 -- and leave this session's own graph with it. */
    if (s->liveliness[handle].key != NULL) {
        graph_cache_apply(s, s->liveliness[handle].key, strlen(s->liveliness[handle].key), true);
        z_free(s->liveliness[handle].key);
        s->liveliness[handle].key = NULL;
    }
#endif
    zpico_release_slot(s, liveliness, handle);
    return ZPICO_OK;
}

// ============================================================================
// Publish with Attachment Implementation
// ============================================================================

int32_t zpico_publish_with_attachment(zpico_session_t* session, int32_t handle, const uint8_t* data,
                                      size_t len, const uint8_t* attachment,
                                      size_t attachment_len) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (handle < 0 || handle >= ZPICO_MAX_PUBLISHERS || !s->publishers[handle].active) {
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

    if (z_publisher_put(z_publisher_loan(&s->publishers[handle].publisher), z_bytes_move(&payload),
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
int32_t zpico_publish_with_attachment_aliased(zpico_session_t* session, int32_t handle,
                                              const uint8_t* data, size_t len,
                                              const uint8_t* attachment, size_t attachment_len) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (handle < 0 || handle >= ZPICO_MAX_PUBLISHERS || !s->publishers[handle].active) {
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

    if (z_publisher_put(z_publisher_loan(&s->publishers[handle].publisher), z_bytes_move(&payload),
                        &options) < 0) {
        return ZPICO_ERR_PUBLISH;
    }

    return ZPICO_OK;
}

// ============================================================================
// Queryable Implementation (for ROS 2 Services)
// ============================================================================

int32_t zpico_declare_queryable(zpico_session_t* session, const char* keyexpr,
                                ZpicoQueryCallback callback, void* ctx) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (!s->session_open) {
        return ZPICO_ERR_SESSION;
    }

    // Find free slot
    int idx = zpico_claim_slot(s, queryables, ZPICO_MAX_QUERYABLES);
    if (idx < 0) {
        return ZPICO_ERR_FULL;
    }

    s->queryables[idx].callback = callback;
    s->queryables[idx].ctx = ctx;

    z_view_keyexpr_t ke;
    if (z_view_keyexpr_from_str(&ke, keyexpr) < 0) {
        s->queryables[idx].callback = NULL;
        s->queryables[idx].ctx = NULL;
        zpico_release_slot(s, queryables, idx);
        return ZPICO_ERR_KEYEXPR;
    }

    // Create closure for callback
    z_owned_closure_query_t closure;
    z_closure_query(&closure, query_handler, NULL, _zpico_pack_ctx(s, idx));

    // Set complete=true so that queries with Z_QUERY_TARGET_ALL_COMPLETE
    // (used by rmw_zenoh_cpp service clients) match this queryable.
    z_queryable_options_t opts;
    z_queryable_options_default(&opts);
    opts.complete = true;

    zpico_declare_lock(s);
    int q_ret =
        z_declare_queryable(z_session_loan(&s->session), &s->queryables[idx].queryable,
                            z_view_keyexpr_loan(&ke), z_closure_query_move(&closure), &opts);
    zpico_declare_unlock(s);
    if (q_ret < 0) {
        printk("zpico: z_declare_queryable failed: %d for '%s'\n", q_ret, keyexpr);
        s->queryables[idx].callback = NULL;
        s->queryables[idx].ctx = NULL;
        zpico_release_slot(s, queryables, idx);
        return ZPICO_ERR_GENERIC;
    }

    /* Claimed by zpico_claim_slot (issue 1711); set again for the reader. */
    s->queryables[idx].active = true;
    return idx;
}

int32_t zpico_undeclare_queryable(zpico_session_t* session, int32_t handle) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (handle < 0 || handle >= ZPICO_MAX_QUERYABLES || !s->queryables[handle].active) {
        return ZPICO_ERR_INVALID;
    }

    zpico_declare_lock(s);
    z_undeclare_queryable(z_queryable_move(&s->queryables[handle].queryable));
    zpico_declare_unlock(s);
    zpico_release_slot(s, queryables, handle);
    s->queryables[handle].callback = NULL;
    s->queryables[handle].ctx = NULL;
    // Phase 237 — drop any cloned queries still held in this queryable's reply
    // slots (unanswered deferred requests) so they don't leak session refs.
    for (int j = 0; j < ZPICO_MAX_PENDING_REPLIES; j++) {
        if (s->stored_query_valid[handle][j]) {
            z_query_drop(z_query_move(&s->stored_queries[handle][j]));
            s->stored_query_valid[handle][j] = false;
        }
    }
    s->last_reply_seq[handle] = -1;
    // issue 0902 / phase-455 W1 — the counters describe THIS queryable's
    // table, and the handle is reused by the next server declared into the
    // slot; leaving them set would credit one server's exhaustion to another.
    s->reply_slot_refusals[handle] = 0;
    s->reply_slot_saturated[handle] = false;
    s->reply_slot_announce[handle] = false;
    s->reply_slot_declines[handle] = 0;
    return ZPICO_OK;
}

/* issue 0902 / phase-455 W1 — read the reply-slot table of one queryable.
 *
 * Shaped after `zpico_graph_entry_count`, which is the file's existing answer
 * to "a bounded resource must report what it discarded": the live count is the
 * return value, the discard count is reported beside it rather than folded in.
 *
 * A new accessor rather than two more entries in `zpico_get_diag_counters`:
 * those 18 counters are file-scope `static volatile uint32_t`, i.e. PROCESS-
 * global and all 18 on the client-side `zpico_get*` path, while this table is
 * per-session AND per-queryable — a summed number could not name the server
 * that saturated. The array also binds in Rust as a bare `*mut u32` whose
 * length lives only in the C prototype, so growing `out[18]` is an
 * undiagnosable overrun for any caller not recompiled with it.
 *
 * Returns the number of slots currently HELD (cloned queries awaiting a
 * reply), or `ZPICO_ERR_INVALID` for a bad session or handle.
 * `out_refusals` receives the cumulative count of allocations refused because
 * the table was full — zero means "this server has never run out", which is
 * precisely what `last_reply_seq == -1` could not say.
 * `out_capacity` receives `ZPICO_MAX_PENDING_REPLIES` as this TU sees it, so a
 * caller reads the effective capacity instead of assuming a `-D` arrived.
 */
int32_t zpico_reply_slot_stats(zpico_session_t* session, int32_t queryable_handle,
                               uint32_t* out_refusals, uint32_t* out_capacity) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (s == NULL || queryable_handle < 0 || queryable_handle >= ZPICO_MAX_QUERYABLES) {
        return ZPICO_ERR_INVALID;
    }
    if (out_refusals != NULL) {
        *out_refusals = s->reply_slot_refusals[queryable_handle];
    }
    if (out_capacity != NULL) {
        *out_capacity = (uint32_t)ZPICO_MAX_PENDING_REPLIES;
    }
    int32_t held = 0;
    for (int j = 0; j < ZPICO_MAX_PENDING_REPLIES; j++) {
        if (s->stored_query_valid[queryable_handle][j]) {
            held++;
        }
    }
    return held;
}

/* issue 1332 / phase-455 W2.b — how many queries this queryable's callback
 * DECLINED, i.e. how many times the reclaim arm above ran.
 *
 * A separate accessor rather than a fifth out-parameter on
 * `zpico_reply_slot_stats`: that function landed in W1 and is already bound,
 * mirrored and called, and a signature change on a live symbol is the silent
 * half of the hand-mirror class (`check-platform-abi-mirror`'s reason for
 * existing). Shaped like its sibling — a count as the return value,
 * `ZPICO_ERR_INVALID` for a bad session or handle.
 *
 * NOT the same as "arrived and went unanswered": a query REFUSED a slot never
 * gets a reply seq, so the decline arm cannot see it and it lands in
 * `out_refusals` instead. `declines + refusals` is the arrival count; measured
 * on a two-slot table with the reclaim removed, four queries read
 * `declines=2 refusals=2`.
 *
 * What it is FOR: a declined query is the population issue 0902's leak fed on,
 * and the native lane has none of it by construction (issue 1332 — graph
 * discovery is a liveliness SUBSCRIBER since phase-381, and both action client
 * paths are single-in-flight). So a test that means to control the leak has to
 * state that declined queries ARRIVED, or its `refusals == 0` is a green that
 * cannot fail.
 */
int32_t zpico_reply_slot_declines(zpico_session_t* session, int32_t queryable_handle) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (s == NULL || queryable_handle < 0 || queryable_handle >= ZPICO_MAX_QUERYABLES) {
        return ZPICO_ERR_INVALID;
    }
    return (int32_t)s->reply_slot_declines[queryable_handle];
}

/* issue 0902 / phase-455 W1 — take the pending "this table just saturated"
 * announcement, clearing it.
 *
 * A take-and-clear, like `zpico_queryable_take_reply_seq`, so the once-ness
 * lives in exactly ONE place: `zpico_reply_slot_pick` decides when a
 * transition happened, and whoever asks first gets it. A second latch on the
 * Rust side would be a second spelling of the same rule, which is how a
 * permanent failure ends up reported per spin again.
 *
 * Returns 1 when an announcement was pending, 0 when not, `ZPICO_ERR_INVALID`
 * for a bad session or handle. Deliberately separate from
 * `zpico_reply_slot_stats`: a stats read must not consume the event, or the
 * probe that samples the counter silences the log line.
 */
/* issue 0902 / phase-455 W2 — every reply-slot refusal in this process, summed.
 *
 * The companion to `zpico_reply_slot_stats` for a caller that holds no session:
 * "did any server here ever run out of reply slots?" Zero is the statement the
 * W2 completion probe asserts, and it is a statement — not the absence of one —
 * because a refusal is counted whether or not anything later reads it.
 *
 * NEVER reset. A counter a caller can zero is one a caller can zero by
 * accident, and the only consumer wants a monotone "has it happened yet".
 */
uint32_t zpico_reply_slot_refusals_total(void) {
    return g_reply_slot_refusals_total;
}

int32_t zpico_reply_slot_take_announcement(zpico_session_t* session, int32_t queryable_handle) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (s == NULL || queryable_handle < 0 || queryable_handle >= ZPICO_MAX_QUERYABLES) {
        return ZPICO_ERR_INVALID;
    }
    bool pending = s->reply_slot_announce[queryable_handle];
    s->reply_slot_announce[queryable_handle] = false;
    return pending ? 1 : 0;
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

    /* phase-381 W1 — collection mode: the KEYEXPR is the payload.
     *
     * Appended NUL-terminated into the same `buf` a single-response get uses
     * for its payload; the two modes never share a slot, because `collect` is
     * set once at start and a slot serves one query. A keyexpr that does not
     * fit is DROPPED rather than truncated — half a keyexpr parses as a
     * different, plausible entity, which is worse than a missing one — and the
     * caller sees it as `reply_count > entry_count`. */
    if (rctx->collect) {
        const z_loaned_sample_t* ksample = z_reply_ok(reply);
        const z_loaned_keyexpr_t* ke = z_sample_keyexpr(ksample);
        z_view_string_t kstr;
        z_keyexpr_as_view_string(ke, &kstr);
        const char* kdata = z_string_data(z_view_string_loan(&kstr));
        size_t klen = z_string_len(z_view_string_loan(&kstr));
        /* +1 for the NUL that separates entries. */
        if (klen > 0 && rctx->len + klen + 1 <= ZPICO_GET_REPLY_BUF_SIZE) {
            memcpy(rctx->buf + rctx->len, kdata, klen);
            rctx->len += klen;
            rctx->buf[rctx->len++] = '\0';
            rctx->entry_count++;
        }
        /* `received` marks "this slot has something to read", which for a
         * collecting query is true from the first stored entry. */
        if (rctx->entry_count > 0) {
            __atomic_store_n(&rctx->received, true, __ATOMIC_SEQ_CST);
        }
        return;
    }

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
            /* Issue 2136 — the reply's metadata, before `received` publishes
             * it. The receive stamp is what rmw_zenoh_cpp's reply object
             * records: wall clock, in the reply callback, which is this one. */
            {
                uint64_t ns = nros_platform_time_now_ns();
                rctx->recv_ns = ns > (uint64_t)INT64_MAX ? INT64_MAX : (int64_t)ns;
            }
            rctx->att_len = 0;
            const z_loaned_bytes_t* att = z_sample_attachment(sample);
            if (att != NULL) {
                z_owned_slice_t att_slice;
                if (z_bytes_to_slice(att, &att_slice) == 0) {
                    size_t alen = z_slice_len(z_slice_loan(&att_slice));
                    if (alen <= ZPICO_REPLY_ATTACHMENT_CAP) {
                        memcpy(rctx->att, z_slice_data(z_slice_loan(&att_slice)), alen);
                        rctx->att_len = alen;
                    }
                    z_slice_drop(z_slice_move(&att_slice));
                }
            }
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

// ============================================================================
// Non-blocking z_get (for async service client)
// ============================================================================

// Reply handler for pending get slots — reuses the same logic as get_reply_handler
static void pending_get_reply_handler(z_loaned_reply_t* reply, void* arg) {
    g_diag_reply_handler_calls++;
    struct zpico_session* s = _zpico_unpack_session(arg);
    int slot = _zpico_unpack_slot(arg);
    get_reply_ctx_t* rctx = &s->pending_gets[slot].ctx;
    /* Phase 127.D.2 — keep the address-recording side effect. It
     * defeats whole-program LTO alias analysis that would otherwise
     * prove the closure's `ctx` pointer disjoint from
     * `&s->pending_gets[handle].ctx` (because the slot table is
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
    /* Record the REAL reply-context address (not the packed handle): a volatile
     * store of an address derived from the pooled `g_sessions` array keeps the
     * write to `rctx->received` in `get_reply_handler` from being hoisted over
     * the read in `zpico_get_check`. */
    g_diag_handler_ctx_addr = (uint32_t)(uintptr_t)rctx;
    get_reply_handler(reply, rctx);
    _zpico_notify_spin(s);
    if (s->reply_waker) {
        s->reply_waker((int32_t)(s - g_sessions), slot);
    }
}

// Dropper for pending get slots — just sets the done flag (no condvar)
static void pending_get_dropper(void* arg) {
    g_diag_reply_dropper_calls++;
    struct zpico_session* s = _zpico_unpack_session(arg);
    int slot = _zpico_unpack_slot(arg);
    get_reply_ctx_t* rctx = &s->pending_gets[slot].ctx;
    __atomic_store_n(&rctx->done, true, __ATOMIC_SEQ_CST);
    _zpico_notify_spin(s);
    if (s->reply_waker) {
        s->reply_waker((int32_t)(s - g_sessions), slot);
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

int32_t zpico_get_start(zpico_session_t* session, const char* keyexpr, const uint8_t* payload,
                        size_t payload_len, uint32_t timeout_ms) {
    return zpico_get_start_with_attachment(session, keyexpr, payload, payload_len, NULL, 0,
                                           timeout_ms);
}

/* Issue 0153 — attachment-carrying variant. rmw_zenoh_cpp's service server
 * REQUIRES the rmw attachment (sequence_number + source_timestamp + gid) on
 * the query: `rmw_service_server_is_available`'s take path
 * (service_take_request) deserializes it and errors the whole take when it
 * is absent, so a nano-ros client request without one reaches the ROS 2
 * server and then dies inside rcl ("service failed to take request") — the
 * client only ever sees Transport(Timeout). nano<->nano services tolerate a
 * missing attachment, which is why this stayed invisible in-tree. */
int32_t zpico_get_start_with_attachment(zpico_session_t* session, const char* keyexpr,
                                        const uint8_t* payload, size_t payload_len,
                                        const uint8_t* attachment, size_t attachment_len,
                                        uint32_t timeout_ms) {
    struct zpico_session* s = (struct zpico_session*)session;
    g_diag_get_start_calls++;
    if (!s->session_open) {
        return ZPICO_ERR_SESSION;
    }

    // Find a free slot, cleaning up zombie slots first.
    // A zombie slot has in_use=true but ctx.done=true, meaning the reply was
    // delivered by get_check but the dropper hadn't fired yet at that time.
    // Now that the dropper has fired (done=true), the slot can be reclaimed.
    int32_t slot = -1;
    for (int32_t i = 0; i < ZPICO_MAX_PENDING_GETS; i++) {
        if (s->pending_gets[i].in_use && s->pending_gets[i].ctx.done) {
            s->pending_gets[i].in_use = false;
        }
        if (!s->pending_gets[i].in_use) {
            slot = i;
            break;
        }
    }
    if (slot < 0) {
        return ZPICO_ERR_FULL;
    }

    // Initialize slot context
    pending_get_slot_t* ps = &s->pending_gets[slot];
    ps->ctx.len = 0;
    ps->ctx.received = false;
    ps->ctx.done = false;
    ps->ctx.reply_count = 0;
    ps->ctx.att_len = 0; /* issue 2136 */
    ps->ctx.recv_ns = 0;
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
    z_closure(&callback, pending_get_reply_handler, pending_get_dropper, _zpico_pack_ctx(s, slot));
    /* Same aliasing-defeat trick. */
    g_diag_start_ctx_addr = (uint32_t)(uintptr_t)&ps->ctx;

    zpico_declare_lock(s);
    z_result_t zret =
        z_get(z_session_loan(&s->session), z_view_keyexpr_loan(&ke), "", z_move(callback), &opts);
    zpico_declare_unlock(s);
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
 * Reuses the same `s->pending_gets` slot pool as `zpico_get_start` — a slot is
 * just a (received_flag, dropper_done_flag, payload_buf) triple, agnostic to
 * whether the caller will read the payload. The reply handler still copies
 * the (typically empty) liveliness token bytes into the slot's buffer; we
 * never read them. Only `received` matters.
 *
 * Returns the slot handle on success, ZPICO_ERR_* on failure.
 */
int32_t zpico_liveliness_get_start(zpico_session_t* session, const char* keyexpr,
                                   uint32_t timeout_ms) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (!s->session_open) {
        return ZPICO_ERR_SESSION;
    }

    int32_t slot = -1;
    for (int32_t i = 0; i < ZPICO_MAX_PENDING_GETS; i++) {
        if (s->pending_gets[i].in_use && s->pending_gets[i].ctx.done) {
            s->pending_gets[i].in_use = false;
        }
        if (!s->pending_gets[i].in_use) {
            slot = i;
            break;
        }
    }
    if (slot < 0) {
        return ZPICO_ERR_FULL;
    }

    pending_get_slot_t* ps = &s->pending_gets[slot];
    ps->ctx.len = 0;
    ps->ctx.received = false;
    ps->ctx.done = false;
    ps->ctx.reply_count = 0;
    ps->ctx.collect = false;
    ps->ctx.entry_count = 0;
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
    z_closure(&callback, pending_get_reply_handler, pending_get_dropper, _zpico_pack_ctx(s, slot));

    zpico_declare_lock(s);
    z_result_t zret = z_liveliness_get(z_session_loan(&s->session), z_view_keyexpr_loan(&ke),
                                       z_move(callback), &opts);
    zpico_declare_unlock(s);
    if (zret < 0) {
        ps->in_use = false;
        return ZPICO_ERR_GENERIC;
    }
    return slot;
}

/* phase-381 W1 — a liveliness query that KEEPS its replies' keyexprs.
 *
 * `zpico_liveliness_get_start` answers "does anything match". Reading the ROS
 * graph needs "WHAT matched": a liveliness reply's information is entirely in
 * its keyexpr (`@ros2_lv/<domain>/<zid>/<nid>/<eid>/…/<namespace>/<node>`), and
 * the handler used to discard it.
 *
 * Same slot pool, same start/poll shape, same dropper. The only difference is
 * that the handler appends each reply's keyexpr into the slot's buffer instead
 * of copying a payload into it — that buffer is dead weight on a liveliness
 * query, whose token payload is empty, so enumeration adds no storage.
 *
 * Poll with `zpico_liveliness_get_check` exactly as for the counting form, then
 * read entries with `zpico_liveliness_entry_count` / `zpico_liveliness_entry`.
 *
 * Returns the slot handle on success, ZPICO_ERR_* on failure.
 */
int32_t zpico_liveliness_collect_start(zpico_session_t* session, const char* keyexpr,
                                       uint32_t timeout_ms) {
    /* Deliberately NOT `zpico_liveliness_get_start` + set the flag.
     *
     * That was the first shape and it LOST EVERY REPLY. `collect` has to be
     * true before the query goes on the wire: zenoh-pico's RX task runs the
     * reply handler as soon as replies arrive, and against a local router with
     * latched liveliness tokens they arrive within microseconds — reliably
     * inside the window between `z_liveliness_get` returning and the caller
     * setting the flag. Replies in that window take the SINGLE-RESPONSE path,
     * which sets `received`, copies a payload nobody reads, and leaves
     * `entry_count` at zero.
     *
     * Measured against a live `rmw_zenoh_cpp` talker (issue 0903): the entity
     * enumeration returned zero every time while node enumeration worked, and
     * widening the wildcard to `@ros2_lv/<domain>/**` changed nothing — because
     * the pattern was never the problem.
     *
     * The comment this replaces called the race "worst case the first reply is
     * counted but not stored". That was wrong in two ways: it is not the first
     * reply, it is all of them, and a multi-threaded RX task is the DEFAULT on
     * POSIX rather than an edge case. */
    struct zpico_session* s = (struct zpico_session*)session;
    if (!s->session_open) {
        return ZPICO_ERR_SESSION;
    }

    int32_t slot = -1;
    for (int32_t i = 0; i < ZPICO_MAX_PENDING_GETS; i++) {
        if (s->pending_gets[i].in_use && s->pending_gets[i].ctx.done) {
            s->pending_gets[i].in_use = false;
        }
        if (!s->pending_gets[i].in_use) {
            slot = i;
            break;
        }
    }
    if (slot < 0) {
        return ZPICO_ERR_FULL;
    }

    pending_get_slot_t* ps = &s->pending_gets[slot];
    ps->ctx.len = 0;
    ps->ctx.received = false;
    ps->ctx.done = false;
    ps->ctx.reply_count = 0;
    ps->ctx.entry_count = 0;
    ps->ctx.collect = true; /* BEFORE the query, which is the whole point */
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
    z_closure(&callback, pending_get_reply_handler, pending_get_dropper, _zpico_pack_ctx(s, slot));

    zpico_declare_lock(s);
    z_result_t zret = z_liveliness_get(z_session_loan(&s->session), z_view_keyexpr_loan(&ke),
                                       z_move(callback), &opts);
    zpico_declare_unlock(s);
    if (zret < 0) {
        ps->in_use = false;
        return ZPICO_ERR_GENERIC;
    }
    return slot;
}

/* How many keyexprs this slot STORED.
 *
 * Distinct from `zpico_liveliness_get_count`, which reports how many replies
 * ARRIVED. The two differ exactly when the buffer could not hold an entry, so
 * a caller that wants to know whether the enumeration is complete compares
 * them — this is the truncation signal, and it is why entries are dropped whole
 * rather than truncated.
 *
 * Returns ZPICO_ERR_INVALID for a bad handle or a slot not in use.
 */
/* Has the collecting sweep FINISHED — dropper fired, no more replies coming?
 *
 * `zpico_liveliness_get_check` cannot answer this. It returns 1 as soon as ONE
 * reply has arrived, which is the right signal for "is the service up?" (its
 * only caller before phase-381) and the wrong one for enumeration: a caller
 * that restarts its query on that truncates every sweep to whatever landed in
 * the poll window, and never sees the rest of the graph.
 *
 * Measured against a live `rmw_zenoh_cpp` talker: collecting until `get_check`
 * went non-zero captured 2 tokens of the dozen it declares. This is the
 * distinction that fixes that.
 *
 * Returns 1 finished, 0 still collecting, ZPICO_ERR_INVALID for a bad handle.
 */
int32_t zpico_liveliness_collect_done(zpico_session_t* session, int32_t handle) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (handle < 0 || handle >= ZPICO_MAX_PENDING_GETS) {
        return ZPICO_ERR_INVALID;
    }
    pending_get_slot_t* ps = &s->pending_gets[handle];
    if (!ps->in_use) {
        return ZPICO_ERR_INVALID;
    }
    return __atomic_load_n(&ps->ctx.done, __ATOMIC_SEQ_CST) ? 1 : 0;
}

int32_t zpico_liveliness_entry_count(zpico_session_t* session, int32_t handle) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (handle < 0 || handle >= ZPICO_MAX_PENDING_GETS) {
        return ZPICO_ERR_INVALID;
    }
    if (!s->pending_gets[handle].in_use) {
        return ZPICO_ERR_INVALID;
    }
    return (int32_t)s->pending_gets[handle].ctx.entry_count;
}

/* Copy stored keyexpr `index` into `out`, NUL-terminated.
 *
 * Returns the number of bytes written excluding the NUL, ZPICO_ERR_INVALID for
 * a bad handle/index, or ZPICO_ERR_BUFFER when `cap` cannot hold the entry plus
 * its NUL. A short buffer is an ERROR rather than a truncation for the same
 * reason the handler drops rather than truncates: half a keyexpr names a
 * different, plausible entity.
 */
int32_t zpico_graph_set_apply(uint8_t* buf, size_t cap, size_t* len, uint32_t* count,
                              uint32_t* dropped, const char* key, size_t klen, bool remove);

#if ZPICO_GRAPH_DISCOVERY
/* Apply one token change to the session's graph cache and fire the change
 * edge when the set moved. The ONE apply: the liveliness subscriber's samples
 * and this session's own tokens (issue 2083) both come through here. */
static void graph_cache_apply(struct zpico_session* s, const char* key, size_t klen, bool remove) {
    if (s == NULL || !s->graph_cache.active || key == NULL || klen == 0) {
        return;
    }
    graph_cache_t* c = &s->graph_cache;
#if Z_FEATURE_MULTI_THREAD == 1
    _z_mutex_lock(&c->mutex);
#endif
    /* phase-467 Row 8 — the return value is the CHANGE EDGE. `zpico_graph_set_apply`
     * returns 1 exactly when the token set is now different: a PUT that
     * inserted, or a DELETE that removed. A duplicate PUT, a DELETE of
     * something absent and a PUT that did not fit all return 0, so a no-op
     * never reaches the guard condition. */
    int32_t changed = zpico_graph_set_apply(c->buf, sizeof(c->buf), &c->len, &c->entry_count,
                                            &c->dropped, key, klen, remove);
#if Z_FEATURE_MULTI_THREAD == 1
    _z_mutex_unlock(&c->mutex);
#endif
    /* OUTSIDE the lock: the handler this wakes reads the graph, and reading
     * the graph takes this same mutex. */
    if (changed == 1 && s->graph_change_cb != NULL) {
        s->graph_change_cb((int32_t)(s - g_sessions));
    }
}

/* Sample handler for the liveliness subscriber.
 *
 * A liveliness sample carries its meaning in the KEYEXPR and its KIND: PUT is
 * "this token exists", DELETE is "it is gone". The payload is empty, as it is
 * for the query form. */
static void graph_cache_sample_handler(z_loaned_sample_t* sample, void* arg) {
    struct zpico_session* s = (struct zpico_session*)arg;
    if (s == NULL || !s->graph_cache.active) {
        return;
    }
    z_view_string_t ks;
    z_keyexpr_as_view_string(z_sample_keyexpr(sample), &ks);
    const char* key = z_string_data(z_view_string_loan(&ks));
    size_t klen = z_string_len(z_view_string_loan(&ks));
    graph_cache_apply(s, key, klen, z_sample_kind(sample) == Z_SAMPLE_KIND_DELETE);
}

#endif /* ZPICO_GRAPH_DISCOVERY */

/* The PURE half of `graph_cache_sample_handler`: apply one liveliness sample
 * to a NUL-separated token SET held in a caller buffer (phase-428 W13).
 *
 * `remove == false` is a PUT ("this token exists"): inserted once, a duplicate
 * is a no-op. `remove == true` is a DELETE ("it is gone"): the entry is
 * removed and the run closed up so `count` keeps indexing it; an absent key is
 * a no-op. A PUT that does not fit is COUNTED in `*dropped`, never truncated —
 * half a keyexpr names a different, plausible entity.
 *
 * Split out like `zpico_entry_at` so the set semantics — the thing
 * `service_is_ready` now answers from — can be tested against the real C code
 * without a zenoh session. Returns 1 if the set changed, 0 if it did not,
 * `ZPICO_ERR_INVALID` for a NULL argument.
 */
int32_t zpico_graph_set_apply(uint8_t* buf, size_t cap, size_t* len, uint32_t* count,
                              uint32_t* dropped, const char* key, size_t klen, bool remove) {
    if (buf == NULL || len == NULL || count == NULL || dropped == NULL || key == NULL ||
        klen == 0) {
        return ZPICO_ERR_INVALID;
    }
    /* Find `key` in the run: the PUT and DELETE arms share one notion of
     * identity, the whole keyexpr. */
    size_t at = SIZE_MAX;
    size_t off = 0;
    while (off < *len) {
        const char* e = (const char*)buf + off;
        size_t n = strlen(e);
        if (n == klen && memcmp(e, key, klen) == 0) {
            at = off;
            break;
        }
        off += n + 1;
    }
    if (remove) {
        if (at == SIZE_MAX) {
            return 0;
        }
        /* Close the gap so the run stays dense and `count` keeps indexing it. */
        size_t n = strlen((const char*)buf + at) + 1;
        memmove(buf + at, buf + at + n, *len - at - n);
        *len -= n;
        (*count)--;
        return 1;
    }
    if (at != SIZE_MAX) {
        return 0;
    }
    if (*len + klen + 1 > cap) {
        (*dropped)++;
        return 0;
    }
    memcpy(buf + *len, key, klen);
    buf[*len + klen] = '\0';
    *len += klen + 1;
    (*count)++;
    return 1;
}

#if ZPICO_GRAPH_DISCOVERY
/* Start the standing graph cache on `keyexpr`.
 *
 * `history = true` is the whole point: the subscriber is delivered the tokens
 * that ALREADY exist, then every later change. Without it the cache would only
 * ever see nodes that appear after us, which is the same blind spot the query
 * form had for a different reason.
 *
 * Idempotent — a second call while active is a no-op, so every graph entry
 * point can call it without coordinating.
 */
int32_t zpico_graph_cache_start(zpico_session_t* session, const char* keyexpr) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (s == NULL || keyexpr == NULL) {
        return ZPICO_ERR_INVALID;
    }
    if (s->graph_cache.active) {
        return 0;
    }
    graph_cache_t* c = &s->graph_cache;
    c->len = 0;
    c->entry_count = 0;
    c->dropped = 0;
#if Z_FEATURE_MULTI_THREAD == 1
    if (_z_mutex_init(&c->mutex) != _Z_RES_OK) {
        return ZPICO_ERR_INVALID;
    }
#endif
    z_view_keyexpr_t ke;
    if (z_view_keyexpr_from_str(&ke, keyexpr) != _Z_RES_OK) {
#if Z_FEATURE_MULTI_THREAD == 1
        _z_mutex_drop(&c->mutex);
#endif
        return ZPICO_ERR_INVALID;
    }
    z_owned_closure_sample_t closure;
    z_closure_sample(&closure, graph_cache_sample_handler, NULL, s);
    z_liveliness_subscriber_options_t opts;
    z_liveliness_subscriber_options_default(&opts);
    opts.history = true;
    /* Set active BEFORE declaring: the handler can fire from the RX path the
     * moment the declaration lands, and a flag set afterwards drops exactly the
     * history burst this call exists to collect. That ordering was defect 4 in
     * the query form (issue 0903); it is not repeated here. */
    c->active = true;
    zpico_declare_lock(s);
    z_result_t ret = z_liveliness_declare_subscriber(z_session_loan(&s->session), &c->sub,
                                                     z_view_keyexpr_loan(&ke),
                                                     z_closure_sample_move(&closure), &opts);
    zpico_declare_unlock(s);
    if (ret != _Z_RES_OK) {
        c->active = false;
#if Z_FEATURE_MULTI_THREAD == 1
        _z_mutex_drop(&c->mutex);
#endif
        return ZPICO_ERR_INVALID;
    }
    /* Issue 2083 -- `history` replays the tokens zenoh-pico knows as REMOTE;
     * this session's own were declared before the cache existed and are never
     * replayed, so seed them here. Matching `keyexpr` is not re-checked: every
     * token this shim declares is a ROS 2 liveliness token under the same
     * `@ros2_lv/**` root the cache subscribes to. */
    for (int i = 0; i < ZPICO_MAX_LIVELINESS; i++) {
        if (s->liveliness[i].active && s->liveliness[i].key != NULL) {
            graph_cache_apply(s, s->liveliness[i].key, strlen(s->liveliness[i].key), false);
        }
    }
    return 0;
}

/* Stop the cache and release the subscriber. Idempotent. */
int32_t zpico_graph_cache_stop(zpico_session_t* session) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (s == NULL) {
        return ZPICO_ERR_INVALID;
    }
    if (!s->graph_cache.active) {
        return 0;
    }
    s->graph_cache.active = false;
    zpico_declare_lock(s);
    z_undeclare_subscriber(z_subscriber_move(&s->graph_cache.sub));
    zpico_declare_unlock(s);
#if Z_FEATURE_MULTI_THREAD == 1
    _z_mutex_drop(&s->graph_cache.mutex);
#endif
    return 0;
}

#else  /* !ZPICO_GRAPH_DISCOVERY */

/* Graph discovery is compiled out: there is no cache to start, so say so with
 * the one code that means "this image was configured without it" rather than
 * `ZPICO_ERR_INVALID`, which a caller would read as its own mistake. The Rust
 * side latches on it and stops asking. No subscriber is declared, so the
 * router never replays the domain's liveliness history toward this session. */
int32_t zpico_graph_cache_start(zpico_session_t* session, const char* keyexpr) {
    (void)keyexpr;
    return session == NULL ? ZPICO_ERR_INVALID : ZPICO_ERR_CONFIG;
}

/* Nothing was started, so there is nothing to stop. Idempotent, like the real
 * one, so a teardown path need not know which build it is in. */
int32_t zpico_graph_cache_stop(zpico_session_t* session) {
    return session == NULL ? ZPICO_ERR_INVALID : 0;
}
#endif /* ZPICO_GRAPH_DISCOVERY */

int32_t zpico_entry_at(const uint8_t* buf, size_t len, uint32_t count, uint32_t index, char* out,
                       size_t cap);

#if ZPICO_GRAPH_DISCOVERY
/* How many tokens the cache currently holds, and how many did not fit.
 *
 * `out_dropped` is reported beside the count rather than folded into it,
 * because a graph answer that silently omits entries is the plausible-wrong-
 * answer failure this path exists to avoid.
 */
int32_t zpico_graph_entry_count(zpico_session_t* session, uint32_t* out_dropped) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (s == NULL || !s->graph_cache.active) {
        return ZPICO_ERR_INVALID;
    }
    graph_cache_t* c = &s->graph_cache;
#if Z_FEATURE_MULTI_THREAD == 1
    _z_mutex_lock(&c->mutex);
#endif
    uint32_t n = c->entry_count;
    if (out_dropped != NULL) {
        *out_dropped = c->dropped;
    }
#if Z_FEATURE_MULTI_THREAD == 1
    _z_mutex_unlock(&c->mutex);
#endif
    return (int32_t)n;
}

/* Copy cached keyexpr `index` into `out`, NUL-terminated.
 *
 * ONE entry per call, under the lock, rather than a bulk snapshot: the cache is
 * sized for a real ROS graph (tens of KB) and the callers are embedded, where a
 * buffer that size cannot live on the stack. It also keeps the capacity a fact
 * about the C side alone — a bulk copy forced the Rust caller to know
 * `ZPICO_GRAPH_CACHE_SIZE`, and two constants that must agree eventually do not.
 *
 * A token may appear or vanish between calls; that is inherent to enumerating a
 * LIVE graph and is why callers poll to convergence.
 */
int32_t zpico_graph_entry_at(zpico_session_t* session, uint32_t index, char* out, size_t cap) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (s == NULL || out == NULL || !s->graph_cache.active) {
        return ZPICO_ERR_INVALID;
    }
    graph_cache_t* c = &s->graph_cache;
#if Z_FEATURE_MULTI_THREAD == 1
    _z_mutex_lock(&c->mutex);
#endif
    int32_t ret = zpico_entry_at(c->buf, c->len, c->entry_count, index, out, cap);
#if Z_FEATURE_MULTI_THREAD == 1
    _z_mutex_unlock(&c->mutex);
#endif
    return ret;
}

#else  /* !ZPICO_GRAPH_DISCOVERY */

/* No cache: "cannot say", never "empty". A zero count here would tell
 * `service_is_ready` that no server exists, which is a wrong answer; an error
 * makes it `Unsupported`, which every wait loop already reads as keep waiting. */
int32_t zpico_graph_entry_count(zpico_session_t* session, uint32_t* out_dropped) {
    if (out_dropped != NULL) {
        *out_dropped = 0;
    }
    return session == NULL ? ZPICO_ERR_INVALID : ZPICO_ERR_CONFIG;
}

int32_t zpico_graph_entry_at(zpico_session_t* session, uint32_t index, char* out, size_t cap) {
    (void)index;
    (void)out;
    (void)cap;
    return session == NULL ? ZPICO_ERR_INVALID : ZPICO_ERR_CONFIG;
}
#endif /* ZPICO_GRAPH_DISCOVERY */

int32_t zpico_entry_at(const uint8_t* buf, size_t len, uint32_t count, uint32_t index, char* out,
                       size_t cap) {
    if (buf == NULL || out == NULL || index >= count) {
        return ZPICO_ERR_INVALID;
    }
    /* Walk the NUL-separated run. `count` bounds the loop, so this cannot run
     * off the end of what the handler wrote. */
    const char* p = (const char*)buf;
    const char* end = (const char*)buf + len;
    for (uint32_t i = 0; i < index; i++) {
        while (p < end && *p != '\0') {
            p++;
        }
        if (p < end) {
            p++;
        }
    }
    size_t n = 0;
    while (p + n < end && p[n] != '\0') {
        n++;
    }
    if (n + 1 > cap) {
        return ZPICO_ERR_BUFFER;
    }
    memcpy(out, p, n);
    out[n] = '\0';
    return (int32_t)n;
}

int32_t zpico_liveliness_entry(zpico_session_t* session, int32_t handle, uint32_t index, char* out,
                               size_t cap) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (handle < 0 || handle >= ZPICO_MAX_PENDING_GETS) {
        return ZPICO_ERR_INVALID;
    }
    pending_get_slot_t* ps = &s->pending_gets[handle];
    if (!ps->in_use) {
        return ZPICO_ERR_INVALID;
    }
    return zpico_entry_at(ps->ctx.buf, ps->ctx.len, ps->ctx.entry_count, index, out, cap);
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
int32_t zpico_liveliness_get_count(zpico_session_t* session, int32_t handle) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (handle < 0 || handle >= ZPICO_MAX_PENDING_GETS) {
        return ZPICO_ERR_INVALID;
    }
    pending_get_slot_t* ps = &s->pending_gets[handle];
    if (!ps->in_use) {
        return ZPICO_ERR_INVALID;
    }
    /* Cap to int32 max — wildcards in practice match a handful of
     * tokens, never billions. */
    uint32_t c = ps->ctx.reply_count;
    return c > (uint32_t)INT32_MAX ? INT32_MAX : (int32_t)c;
}

int32_t zpico_liveliness_get_check(zpico_session_t* session, int32_t handle) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (handle < 0 || handle >= ZPICO_MAX_PENDING_GETS) {
        return ZPICO_ERR_INVALID;
    }

    pending_get_slot_t* ps = &s->pending_gets[handle];
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

int32_t zpico_get_check(zpico_session_t* session, int32_t handle, uint8_t* reply_buf,
                        size_t reply_buf_size) {
    return zpico_get_check_info(session, handle, reply_buf, reply_buf_size, NULL, 0, NULL, NULL);
}

/* Issue 2136 — `zpico_get_check`, plus the reply's attachment and receive time,
 * copied out in the same step that consumes the reply (the slot may be released
 * by it). `att_out` may be NULL (then nothing is copied and `*att_len_out`
 * reads 0); `att_len_out` and `recv_ns_out`, when non-NULL, are written only
 * on a positive return. An attachment larger than `att_cap` reads as none. */
int32_t zpico_get_check_info(zpico_session_t* session, int32_t handle, uint8_t* reply_buf,
                             size_t reply_buf_size, uint8_t* att_out, size_t att_cap,
                             size_t* att_len_out, int64_t* recv_ns_out) {
    struct zpico_session* s = (struct zpico_session*)session;
    g_diag_get_check_calls++;
    if (handle < 0 || handle >= ZPICO_MAX_PENDING_GETS) {
        g_diag_gck_invalid_arg++;
        return ZPICO_ERR_INVALID;
    }

    pending_get_slot_t* ps = &s->pending_gets[handle];
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
        if (att_len_out != NULL) {
            size_t alen = ps->ctx.att_len;
            if (att_out != NULL && alen > 0 && alen <= att_cap) {
                memcpy(att_out, ps->ctx.att, alen);
                *att_len_out = alen;
            } else {
                *att_len_out = 0;
            }
        }
        if (recv_ns_out != NULL) {
            *recv_ns_out = ps->ctx.recv_ns;
        }
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

/* Issue 1632 — the length of the reply a pending get RECEIVED, for the caller
 * that `zpico_get_check` refused with `ZPICO_ERR_FULL` because the reply did not
 * fit its buffer. That refusal says only THAT it did not fit; this says how big
 * it was, so the client can report the size a refused take needed.
 *
 * `ZPICO_ERR_INVALID` when the handle is out of range or no reply was received.
 * Read-only: the slot's state is `zpico_get_check`'s to change. */
int32_t zpico_get_reply_len(zpico_session_t* session, int32_t handle) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (s == NULL || handle < 0 || handle >= ZPICO_MAX_PENDING_GETS) {
        return ZPICO_ERR_INVALID;
    }
    pending_get_slot_t* ps = &s->pending_gets[handle];
    if (!__atomic_load_n(&ps->ctx.received, __ATOMIC_SEQ_CST)) {
        return ZPICO_ERR_INVALID;
    }
    return (int32_t)ps->ctx.len;
}

void zpico_set_reply_waker(zpico_session_t* session, zpico_waker_fn fn) {
    struct zpico_session* s = (struct zpico_session*)session;
    s->reply_waker = fn;
}

/* phase-467 Row 8 — install (or clear, with NULL) the graph-change edge.
 *
 * Idempotent and last-writer-wins, like `zpico_set_reply_waker`. Clearing is
 * the CAPABILITY PROBE the runtime makes before it consumes an executor handle
 * slot, so it must be cheap and must not fail. */
void zpico_set_graph_change_cb(zpico_session_t* session, zpico_graph_change_fn fn) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (s == NULL) {
        return;
    }
    s->graph_change_cb = fn;
}

// ============================================================================
// Query Reply Implementation (for service servers)
// ============================================================================

/* issue 0902 — release a reply slot on EVERY exit, not only the successful one.
 *
 * `query_handler` clones the query into the slot before the callback runs, and
 * the slot is freed in three places: the session-open `memset`, queryable
 * teardown, and a successful reply. So each error return below used to strand
 * its slot for the life of the queryable, and four exhausted slots make the
 * server answer nothing for ever.
 *
 * Freeing here is correct because NO CALLER CAN RETRY a seq -- checked, not
 * assumed: every `send_response` site either `?`-propagates or maps to
 * `ServiceReplyFailed` (`action_core.rs:373,410,822,895,1014,1037`), and the
 * deferred-reply flush has already `swap_remove`d the pending entry before it
 * looks at the result (`action_core.rs:688,695`). A retry has nothing to retry
 * WITH. Holding the slot would preserve a query nobody will ever answer. */
static void _zpico_release_reply_slot(struct zpico_session* s, int32_t handle, int64_t seq) {
    if (seq < 0 || seq >= ZPICO_MAX_PENDING_REPLIES) {
        return;
    }
    if (!s->stored_query_valid[handle][seq]) {
        return;
    }
    z_query_drop(z_query_move(&s->stored_queries[handle][seq]));
    s->stored_query_valid[handle][seq] = false;
}

/* Issue 1709 -- one reply, and whether a SUCCESSFUL one ends the query.
 *
 * A zenoh query may be answered any number of times until it is dropped; the
 * drop is what finalises it for the querier. `zpico_query_reply` has always
 * dropped after one reply, which is right for a service and wrong for a
 * transient-local publisher's history cache, which owes a late joiner every
 * retained sample. `release_on_success = false` keeps the cloned query (and
 * its reply slot) for the next reply; the LAST reply of a series must be a
 * plain `zpico_query_reply`, which releases as it always did. A FAILED reply
 * releases in both cases: there is no state worth keeping a slot for, and a
 * caller that stopped would otherwise leak it (issue 0902). */
static int32_t _zpico_query_reply_impl(zpico_session_t* session, int32_t queryable_handle,
                                       int64_t reply_seq, const char* keyexpr, const uint8_t* data,
                                       size_t len, const uint8_t* attachment, size_t attachment_len,
                                       bool release_on_success);

int32_t zpico_query_reply(zpico_session_t* session, int32_t queryable_handle, int64_t reply_seq,
                          const char* keyexpr, const uint8_t* data, size_t len,
                          const uint8_t* attachment, size_t attachment_len) {
    return _zpico_query_reply_impl(session, queryable_handle, reply_seq, keyexpr, data, len,
                                   attachment, attachment_len, true);
}

int32_t zpico_query_reply_keep(zpico_session_t* session, int32_t queryable_handle,
                               int64_t reply_seq, const char* keyexpr, const uint8_t* data,
                               size_t len, const uint8_t* attachment, size_t attachment_len) {
    return _zpico_query_reply_impl(session, queryable_handle, reply_seq, keyexpr, data, len,
                                   attachment, attachment_len, false);
}

static int32_t _zpico_query_reply_impl(zpico_session_t* session, int32_t queryable_handle,
                                       int64_t reply_seq, const char* keyexpr, const uint8_t* data,
                                       size_t len, const uint8_t* attachment, size_t attachment_len,
                                       bool release_on_success) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (queryable_handle < 0 || queryable_handle >= ZPICO_MAX_QUERYABLES) {
        return ZPICO_ERR_INVALID;
    }
    // Phase 237 — `reply_seq` selects the cloned query captured by
    // `query_handler` (the slot index it recorded). The reply may arrive long
    // after the query callback returned (deferred get_result).
    if (reply_seq < 0 || reply_seq >= ZPICO_MAX_PENDING_REPLIES ||
        !s->stored_query_valid[queryable_handle][reply_seq]) {
        return ZPICO_ERR_INVALID;
    }
    z_owned_query_t* stored_query = &s->stored_queries[queryable_handle][reply_seq];

    z_view_keyexpr_t ke;
    if (z_view_keyexpr_from_str(&ke, keyexpr) < 0) {
        _zpico_release_reply_slot(s, queryable_handle, reply_seq);
        return ZPICO_ERR_KEYEXPR;
    }

    // Create payload
    z_owned_bytes_t payload;
    if (z_bytes_copy_from_buf(&payload, data, len) < 0) {
        _zpico_release_reply_slot(s, queryable_handle, reply_seq);
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
            _zpico_release_reply_slot(s, queryable_handle, reply_seq);
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
                const uint8_t* att_data = z_slice_data(z_slice_loan(&att_slice));
                size_t att_len = z_slice_len(z_slice_loan(&att_slice));
                /* Issue 2136 — the reply's `source_timestamp` is the REPLIER's
                 * send time, as rmw_zenoh_cpp's server writes it
                 * (`send_response`), not the client's request time an echo
                 * would carry. In the rmw layout ([seq i64][ts i64][gid len]
                 * [gid]) restamp bytes 8..16, little-endian, with this server's
                 * wall clock; a platform with no wall clock keeps the echo. */
                uint8_t restamped[ZPICO_REPLY_ATTACHMENT_CAP];
                uint64_t now_ns = nros_platform_time_now_ns();
                if (att_len == ZPICO_REPLY_ATTACHMENT_CAP && now_ns != 0) {
                    memcpy(restamped, att_data, att_len);
                    if (now_ns > (uint64_t)INT64_MAX) {
                        now_ns = (uint64_t)INT64_MAX;
                    }
                    for (int b = 0; b < 8; b++) {
                        restamped[8 + b] = (uint8_t)(now_ns >> (8 * b));
                    }
                    att_data = restamped;
                }
                if (z_bytes_copy_from_buf(&attachment_bytes, att_data, att_len) == 0) {
                    options.attachment = z_bytes_move(&attachment_bytes);
                }
                z_slice_drop(z_slice_move(&att_slice));
            }
        }
    }

    // Reply using the cloned query held in this reply slot.
    if (z_query_reply(z_query_loan(stored_query), z_view_keyexpr_loan(&ke), z_bytes_move(&payload),
                      &options) < 0) {
        _zpico_release_reply_slot(s, queryable_handle, reply_seq);
        return ZPICO_ERR_GENERIC;
    }

    // Drop the cloned query + free the slot after reply -- unless the caller
    // has more replies to send on it (issue 1709, `zpico_query_reply_keep`).
    if (release_on_success) {
        _zpico_release_reply_slot(s, queryable_handle, reply_seq);
    }

    return ZPICO_OK;
}

// Phase 237 — return the reply-slot index allocated by the most recent
// `query_handler` for this queryable (the deferred-reply seq). Must be called
// from inside the synchronous query callback; -1 if the reply table was full.
int64_t zpico_queryable_take_reply_seq(zpico_session_t* session, int32_t queryable_handle) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (queryable_handle < 0 || queryable_handle >= ZPICO_MAX_QUERYABLES) {
        return -1;
    }
    /* issue 0902 — a real TAKE. The name always said so; the function used to
     * be a pure read, which left the caller's decision invisible to
     * `query_handler` and made a declined query indistinguishable from a
     * deferred one. Clearing it is the claim: whatever is still set when the
     * callback returns belongs to nobody. */
    int64_t seq = s->last_reply_seq[queryable_handle];
    s->last_reply_seq[queryable_handle] = -1;
    return seq;
}

/* Issue 1942 — copy the attachment of the request held in reply slot
 * `reply_seq` into `out`, for the server's per-request `ServiceInfo`.
 *
 * The slot is the query `query_handler` cloned for the deferred reply, so it is
 * readable from the consumer side, any time between the take and the reply —
 * no state is added to the session. A stock `rmw_zenoh_cpp` client and ours
 * both attach `int64 seq | int64 source_timestamp | u8 len | gid[16]`.
 *
 * Returns the attachment's FULL length (0 when the query carried none), of
 * which `min(len, out_cap)` bytes were copied, or `ZPICO_ERR_INVALID` for a
 * bad handle or a slot holding no query. */
int32_t zpico_queryable_request_attachment(zpico_session_t* session, int32_t queryable_handle,
                                           int64_t reply_seq, uint8_t* out, size_t out_cap) {
    struct zpico_session* s = (struct zpico_session*)session;
    if (queryable_handle < 0 || queryable_handle >= ZPICO_MAX_QUERYABLES) {
        return ZPICO_ERR_INVALID;
    }
    if (reply_seq < 0 || reply_seq >= ZPICO_MAX_PENDING_REPLIES ||
        !s->stored_query_valid[queryable_handle][reply_seq]) {
        return ZPICO_ERR_INVALID;
    }
    const z_loaned_query_t* query = z_query_loan(&s->stored_queries[queryable_handle][reply_seq]);
    const z_loaned_bytes_t* attachment = z_query_attachment(query);
    if (attachment == NULL) {
        return 0;
    }
    size_t len = z_bytes_len(attachment);
    if (out != NULL && out_cap > 0 && len > 0) {
        z_bytes_reader_t r = z_bytes_get_reader(attachment);
        (void)z_bytes_reader_read(&r, out, len < out_cap ? len : out_cap);
    }
    return len > (size_t)INT32_MAX ? INT32_MAX : (int32_t)len;
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
