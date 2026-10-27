#ifndef NROS_RMW_XRCE_C_INTERNAL_H
#define NROS_RMW_XRCE_C_INTERNAL_H

/* Shared declarations across vtable.c / session.c / publisher.c /
 * subscriber.c / service.c / transport_custom.c.
 *
 * Phase 115.K.2 — backend state lives on the heap (one
 * `xrce_session_state` per session, malloc'd at `open`, parked in
 * `rmw_session_t::backend_data`). Per-entity state lives in slots
 * inside that struct; entity shells get a pointer to the matching slot
 * via their `backend_data` field. Mirrors the design ground truth in
 * `packages/rmw/xrce/nros-rmw-xrce/src/lib.rs` but without the
 * module-static `XrceSessionState` it relies on.
 */

#include "nros/platform.h"
#include "nros/rmw_entity.h"
#include "nros/rmw_event.h"
#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"

#include <uxr/client/client.h>
#include <uxr/client/core/session/common_create_entities.h>

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include <string.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Issue 0832 — this backend allocates through the platform funnel, never libc.
 *
 * The shim is linked ABOVE the platform layer, so libc's heap is not
 * necessarily the heap the platform owns: on Zephyr, FreeRTOS, ThreadX and
 * NuttX it is a different allocator entirely, and the `unified` tier promises
 * that every allocation reaches `nros_platform_alloc`. A raw `calloc`/`free`
 * here allocates from one heap and, once a board routes the other way, frees
 * to another.
 *
 * The zeroing lives in the helper rather than at the call sites. Every
 * allocation in this backend was `calloc(1, sizeof(X))` on a struct whose
 * fields are then only partly assigned, so the zeroing is load-bearing — and a
 * `memset` that has to be remembered at nine call sites is one that gets
 * forgotten at the tenth.
 */
static inline void* nros_xrce_calloc(size_t count, size_t size) {
    size_t total;
    void* ptr;

    if (count == 0 || size == 0) {
        count = size = 1;
    }
    /* The multiply is the one thing calloc does that alloc cannot, so keep its
       overflow check rather than trusting the product. */
    total = count * size;
    if (total / size != count) {
        return NULL;
    }
    if ((ptr = nros_platform_alloc(total)) != NULL) {
        memset(ptr, 0, total);
    }
    return ptr;
}

static inline void nros_xrce_free(void* ptr) {
    /* `nros_platform_dealloc` is a documented no-op on NULL, matching free(). */
    nros_platform_dealloc(ptr);
}

/* ---- Tunables (must mirror packages/rmw/xrce/nros-rmw-xrce/build.rs
 *      defaults so the C backend behaves the same as the Rust one
 *      under nominal config). ---- */

/* Phase 207.6 — guarded so the xrce-cffi build.rs env knobs
 * (`NROS_XRCE_MAX_SUBSCRIBERS`, `..._MAX_SERVICE_SERVERS`,
 * `..._MAX_SERVICE_CLIENTS`, `..._BUFFER_SIZE`) can shrink the
 * per-session struct on RAM-tight embedded targets. The default
 * `xrce_session_state_t` is ~390 KB; a pub-only bare-metal node can
 * drop it well below 32 KB by setting subscribers/services to 0 and
 * smaller per-entity buffers. */
#ifndef XRCE_MAX_SUBSCRIBERS
#define XRCE_MAX_SUBSCRIBERS 8
#endif
#ifndef XRCE_MAX_SERVICE_SERVERS
#define XRCE_MAX_SERVICE_SERVERS 4
#endif
#ifndef XRCE_MAX_SERVICE_CLIENTS
#define XRCE_MAX_SERVICE_CLIENTS 4
#endif
/* Issue 1808 — how many of the XRCE_MAX_SERVICE_SERVERS slots belong to the
 * BUILTIN family: the six ROS parameter services of every node and the five
 * REP-2002 lifecycle services, which the runtime opens and no `[[endpoint]]`
 * row itemises. The cargo lane derives it beside the total (the same
 * `infra_service_servers` count), and the remaining slots are the
 * application's.
 *
 * The split is what lets the application's request buffer be derived DOWN:
 * those slots hold only the declared servers' requests, while the builtin
 * slots keep a buffer sized for `set_parameters` and lifecycle's `ChangeState`
 * (an unbounded `label`). zenoh made the same split by name in phase-461 W2b.
 *
 * 0 (the default, and every image with no descriptor) is no split: every
 * server shares one buffer size, as before. */
#ifndef XRCE_BUILTIN_SERVICE_SERVERS
#define XRCE_BUILTIN_SERVICE_SERVERS 0
#endif
#if XRCE_BUILTIN_SERVICE_SERVERS > XRCE_MAX_SERVICE_SERVERS
#error                                                                                             \
    "XRCE_BUILTIN_SERVICE_SERVERS is more than XRCE_MAX_SERVICE_SERVERS, the total it is a share of (issue 1808)."
#endif
#define XRCE_USER_SERVICE_SERVERS (XRCE_MAX_SERVICE_SERVERS - XRCE_BUILTIN_SERVICE_SERVERS)
/* Phase 237 — outstanding deferred replies per service server. A reply may be
 * sent long after `take_request` returned (e.g. an action `get_result` held
 * until the goal terminates), so the request's `SampleIdentity` can't live in
 * the single request inbox (a later request overwrites it). Each in-flight
 * reply gets a seq-keyed token slot instead. Sized for the max concurrent
 * goals; override at build time if a server fields more. */
#ifndef XRCE_MAX_PENDING_REPLIES
#define XRCE_MAX_PENDING_REPLIES 4
#endif
/* Phase 237 follow-up — depth of the service-server request inbox ring. The
 * single inbox dropped a request that arrived before the previous one was
 * drained (two clients sending send_goal / get_result in the same spin window —
 * concurrent goals under load). A ring buffers a burst of arrivals so each is
 * read in order. Override on RAM-tight targets (depth 1 = single-inbox). */
#ifndef XRCE_SERVICE_REQUEST_RING_DEPTH
#define XRCE_SERVICE_REQUEST_RING_DEPTH 4
#endif
#ifndef XRCE_BUFFER_SIZE
#define XRCE_BUFFER_SIZE 1024
#endif
/* phase-454 W6.b (RFC-0100 D2/D5) — ONE number served THREE families, and the
 * three have nothing to do with each other.
 *
 * `XRCE_BUFFER_SIZE` sized the subscriber ring entry, the service-server
 * request entry and the service-client reply slot alike, so every family paid
 * for the largest type ANY of them carries. D2's formula is per-pool
 * (`COUNT x SLOTS x SLOT_BYTES`), and `SLOT_BYTES` is a property of the types
 * THAT family receives — the subscriber ring is by far the biggest pool
 * (`XRCE_MAX_SUBSCRIBERS x XRCE_SUBSCRIBER_RING_DEPTH` entries against the
 * service families' 4 and 1), so the shared number is paid ~32x over there and
 * once here.
 *
 * `XRCE_BUFFER_SIZE` stays as the DEFAULT of all three rather than being
 * deleted, for two reasons that are both about not moving bytes nobody asked to
 * move: an image that states nothing derives byte-identically to before this
 * wave, and `NROS_XRCE_BUFFER_SIZE` remains the one lever that raises every
 * receive buffer at once (`large_msg.rs` is built on exactly that, and the
 * backend's own MESSAGE_TOO_LARGE diagnostic names it).
 *
 * All three are derived from declarations by `nros-rmw-xrce-cffi/build.rs`:
 * the subscriber family since phase-454 W6.b, the two service families since
 * issue 1808 (the request buffer only beside the builtin split below). */
#ifndef XRCE_SUBSCRIBER_BUFFER_SIZE
#define XRCE_SUBSCRIBER_BUFFER_SIZE XRCE_BUFFER_SIZE
#endif
/* Issue 1722 — the parameter services are service servers like any other, so
 * their requests land in this buffer too, and a `set_parameters` naming a
 * node's declared parameters outgrows the 1,024-byte default (25 integers with
 * 35-byte names is 2,411 B; measured dropped on main, with the drop logged as
 * InboxOverflow). XRCE_PARAM_REQUEST_BYTES is that request's size: STATED as
 * NROS_PARAM_SERVICE_INBOX_BYTES (the backend-neutral knob nros-node checks), or
 * DERIVED by the cargo lane from the declaration. It only RAISES the default —
 * never below the old floor, which also serves the user services. A STATED
 * buffer below it is refused just after this block.
 *
 * 0 = not priced (nothing stated, nothing declared): no raise, no refusal.
 * The plain `XRCE_BUFFER_SIZE` arm comes FIRST on purpose — it is the default
 * an image that prices nothing compiles, and the one `gen-pool-inventory`
 * publishes for this knob. */
#ifndef XRCE_PARAM_REQUEST_BYTES
#define XRCE_PARAM_REQUEST_BYTES 0
#endif
#if XRCE_BUILTIN_SERVICE_SERVERS > 0
/* Issue 1808 — split: the builtin slots take the raise and the refusal, and
 * the application's slots default to the plain XRCE_BUFFER_SIZE (the cargo
 * lane derives them from the declared servers' requests). The builtin buffer
 * keeps XRCE_BUFFER_SIZE as its floor because lifecycle's `ChangeState` has
 * no static bound. */
#ifndef XRCE_BUILTIN_SERVICE_REQUEST_BUFFER_SIZE
#if XRCE_PARAM_REQUEST_BYTES <= XRCE_BUFFER_SIZE
#define XRCE_BUILTIN_SERVICE_REQUEST_BUFFER_SIZE XRCE_BUFFER_SIZE
#else
#define XRCE_BUILTIN_SERVICE_REQUEST_BUFFER_SIZE XRCE_PARAM_REQUEST_BYTES
#endif
#endif
#if XRCE_BUILTIN_SERVICE_REQUEST_BUFFER_SIZE < XRCE_PARAM_REQUEST_BYTES
#error                                                                                             \
    "NROS_XRCE_BUILTIN_SERVICE_REQUEST_BUFFER_SIZE is below XRCE_PARAM_REQUEST_BYTES, the largest set_parameters this image's declared parameters allow. That request would be dropped on arrival. Raise it, or leave it unset to derive (issues 1722, 1808)."
#endif
#ifndef XRCE_SERVICE_REQUEST_BUFFER_SIZE
#define XRCE_SERVICE_REQUEST_BUFFER_SIZE XRCE_BUFFER_SIZE
#endif
#else
#ifndef XRCE_SERVICE_REQUEST_BUFFER_SIZE
#if XRCE_PARAM_REQUEST_BYTES <= XRCE_BUFFER_SIZE
#define XRCE_SERVICE_REQUEST_BUFFER_SIZE XRCE_BUFFER_SIZE
#else
#define XRCE_SERVICE_REQUEST_BUFFER_SIZE XRCE_PARAM_REQUEST_BYTES
#endif
#endif
#if XRCE_SERVICE_REQUEST_BUFFER_SIZE < XRCE_PARAM_REQUEST_BYTES
#error                                                                                             \
    "NROS_XRCE_SERVICE_REQUEST_BUFFER_SIZE is below XRCE_PARAM_REQUEST_BYTES, the largest set_parameters this image's declared parameters allow (NROS_PARAM_SERVICE_INBOX_BYTES, or derived from the contract's params:). That request would be dropped on arrival. Raise it, or leave it unset to derive (issue 1722)."
#endif
/* No split: no builtin slot exists, so this names nothing; it keeps the
 * fragment-window check below one spelling for both shapes. */
#define XRCE_BUILTIN_SERVICE_REQUEST_BUFFER_SIZE XRCE_SERVICE_REQUEST_BUFFER_SIZE
#endif
#ifndef XRCE_SERVICE_REPLY_BUFFER_SIZE
#define XRCE_SERVICE_REPLY_BUFFER_SIZE XRCE_BUFFER_SIZE
#endif
/* Phase 130.4 — bumped default from 4 to 16. Action server callbacks
 * that publish feedback + result + status_array + service replies
 * in a single user-handler invocation could exhaust 4 unACK'd slots
 * before the executor gets a chance to drain ACKs, causing
 * `uxr_buffer_reply` to return UXR_INVALID_REQUEST_ID. Sixteen
 * slots cover a typical action lifecycle (accept + 3-4 feedback
 * + complete + status_array + result) with room to spare. Costs
 * 16 * UXR_CONFIG_CUSTOM_TRANSPORT_MTU = 64 KiB of per-session
 * output stream buffer (was 16 KiB at history=4).
 *
 * Phase 130.6 — tunable via build.rs env override
 * `NROS_XRCE_STREAM_HISTORY=<n>`; defaults to 16 if unset. Tight-
 * RAM RTOS targets can drop to 8 (32 KiB) when the application
 * doesn't run server-side action callbacks that fan out replies.
 * Lower than 4 is rejected — reliable retransmission needs at
 * least 2 unACK'd messages outstanding plus headroom.
 *
 * phase-454 W6.b — the floor below is a PROTOCOL requirement and the
 * default above is a statement about ENDPOINTS, which is why only the
 * second one moves. Read the 130.4 justification again: every message
 * it names (feedback, result, status_array, service replies) is
 * ENDPOINT traffic. An image whose every endpoint declares
 * `reliability = best_effort` fans out none of it on this stream, so
 * `nros-rmw-xrce-cffi/build.rs` derives 4 for it from the sizing
 * descriptor — the RFC-0100 D1 shape, a policy that is stated and a
 * derived fact supplying its default.
 *
 * What it does NOT derive is 0, and that is not conservatism. Both
 * reliable streams stay LIVE on a best-effort-only image: every
 * control message this backend sends rides `st->output_reliable`
 * (participant/topic/publisher/datawriter/datareader CREATE, DELETE,
 * `uxr_buffer_request_data`), and `uxr_buffer_request_data` names
 * `st->input_reliable` as the stream the Agent delivers on, for every
 * reader, whatever its QoS. A declaration about DATA delivery is not a
 * licence to make session setup lossy. The buffer therefore shrinks
 * with the history and does not go away; the block size is
 * `size / history`, so an MTU-sized message still fits at 4. */
#ifndef XRCE_STREAM_HISTORY
#define XRCE_STREAM_HISTORY 16
#endif
#if XRCE_STREAM_HISTORY < 4
#error "XRCE_STREAM_HISTORY must be >= 4 (reliable retransmit headroom)"
#endif
/* Stream buffer sized after the largest MTU compiled in. Embedded
 * (no_POSIX) builds drop the UDP profile, so fall back to the custom
 * transport MTU. */
#if defined(UCLIENT_PROFILE_UDP)
#define XRCE_STREAM_BUFFER_SIZE (UXR_CONFIG_UDP_TRANSPORT_MTU * XRCE_STREAM_HISTORY)
#else
#define XRCE_STREAM_BUFFER_SIZE (UXR_CONFIG_CUSTOM_TRANSPORT_MTU * XRCE_STREAM_HISTORY)
#endif

/* Issue 1722 — a receive buffer is only as large as the stream that fills it.
 *
 * Every session runs over `uxrCustomTransport`, so the MTU this client reports
 * to the Agent is UXR_CONFIG_CUSTOM_TRANSPORT_MTU, and the Agent fragments
 * anything larger at that size. The client reassembles fragments INSIDE its
 * input reliable stream, one fragment per slot, and only within a window of
 * XRCE_STREAM_HISTORY slots (`uxr_receive_reliable_message` refuses a sequence
 * number beyond it). A message needing more fragments than that can never
 * complete, and because the stream is reliable it is retransmitted forever and
 * nothing behind it is delivered either. Measured on a native XRCE image
 * (custom MTU 512, history 4, request buffer stated 4096): a 25-parameter
 * `set_parameters` (~2.4 KB, 5 fragments) got no reply, and a `get_parameters`
 * that had answered before it got none after it. History 16, same image: 25 of
 * 25 set, and the `get_parameters` after still answered.
 *
 * So each receive buffer is held to the window here, at compile time, where
 * every producer of these numbers (env, Kconfig, the cargo lane's derivation,
 * a board rung) arrives. Per fragment: the message header (8, with the client
 * key) and the fragment subheader (4). Per message: the DATA subheader (4),
 * the object/request ids (4), a request's SampleIdentity (24), alignment (3),
 * less the 4-byte CDR header the Agent strips — 32, the worst of the three
 * families. Conservative: it holds the buffer's CAPACITY to the window, not
 * just the messages this image happens to receive. */
#if defined(UCLIENT_PROFILE_UDP) && UXR_CONFIG_UDP_TRANSPORT_MTU < UXR_CONFIG_CUSTOM_TRANSPORT_MTU
#define XRCE_INBOUND_FRAGMENT_BYTES (UXR_CONFIG_UDP_TRANSPORT_MTU - 12)
#else
#define XRCE_INBOUND_FRAGMENT_BYTES (UXR_CONFIG_CUSTOM_TRANSPORT_MTU - 12)
#endif
#define XRCE_INBOUND_FRAGMENTS(bytes)                                                              \
    (((bytes) + 32 + XRCE_INBOUND_FRAGMENT_BYTES - 1) / XRCE_INBOUND_FRAGMENT_BYTES)
#if XRCE_INBOUND_FRAGMENTS(XRCE_SERVICE_REQUEST_BUFFER_SIZE) > XRCE_STREAM_HISTORY
#error                                                                                             \
    "XRCE_SERVICE_REQUEST_BUFFER_SIZE needs more MTU-sized fragments than the reliable stream's window (XRCE_STREAM_HISTORY) can reassemble: a request that large would stall the session. Raise NROS_XRCE_STREAM_HISTORY or NROS_XRCE_CUSTOM_TRANSPORT_MTU, or lower the request buffer / declared parameters (issue 1722)."
#endif
#if XRCE_INBOUND_FRAGMENTS(XRCE_BUILTIN_SERVICE_REQUEST_BUFFER_SIZE) > XRCE_STREAM_HISTORY
#error                                                                                             \
    "XRCE_BUILTIN_SERVICE_REQUEST_BUFFER_SIZE needs more MTU-sized fragments than the reliable stream's window (XRCE_STREAM_HISTORY) can reassemble: a parameter request that large would stall the session. Raise NROS_XRCE_STREAM_HISTORY or NROS_XRCE_CUSTOM_TRANSPORT_MTU, or lower the declared parameters (issues 1722, 1808)."
#endif
#if XRCE_INBOUND_FRAGMENTS(XRCE_SUBSCRIBER_BUFFER_SIZE) > XRCE_STREAM_HISTORY
#error                                                                                             \
    "XRCE_SUBSCRIBER_BUFFER_SIZE needs more MTU-sized fragments than the reliable stream's window (XRCE_STREAM_HISTORY) can reassemble: a sample that large would stall the session. Raise NROS_XRCE_STREAM_HISTORY or NROS_XRCE_CUSTOM_TRANSPORT_MTU, or lower the subscriber buffer (issue 1722)."
#endif
#if XRCE_INBOUND_FRAGMENTS(XRCE_SERVICE_REPLY_BUFFER_SIZE) > XRCE_STREAM_HISTORY
#error                                                                                             \
    "XRCE_SERVICE_REPLY_BUFFER_SIZE needs more MTU-sized fragments than the reliable stream's window (XRCE_STREAM_HISTORY) can reassemble: a reply that large would stall the session. Raise NROS_XRCE_STREAM_HISTORY or NROS_XRCE_CUSTOM_TRANSPORT_MTU, or lower the reply buffer (issue 1722)."
#endif
#define XRCE_CDR_HEADER_LEN 4
#define XRCE_DDS_NAME_BUF_SIZE 128
#define XRCE_PARTICIPANT_NAME_BUF_SIZE 64
#define XRCE_ENTITY_CREATION_TIMEOUT_MS 1000
#define XRCE_SESSION_FLUSH_TIMEOUT_MS 100
#define XRCE_SESSION_CREATION_RETRIES 3

/* Default agent UDP port, matches Micro-XRCE-DDS-Agent's default. */
#define XRCE_DEFAULT_AGENT_PORT 2018

/* Bounded busy-wait for service replies (ms). */
#define XRCE_SERVICE_REPLY_TIMEOUT_MS 50
#define XRCE_SERVICE_REPLY_TOTAL_MS 5000

/* ---- Per-entity slots ----------------------------------------------- */

/* Subscriber slot — N-deep ringbuffer.
 *
 * Phase 160.H.1 — grew from a single-message buffer to N entries so a
 * back-to-back publish burst doesn't silently overwrite unread
 * messages (root cause of `test_xrce_throughput_{100hz,burst}` only
 * receiving 1 of 100 msgs). Each ring entry carries its own
 * `data[XRCE_SUBSCRIBER_BUFFER_SIZE]` + `len`; the topic callback writes to
 * `entries[write_idx]` and advances, `take_serialized` reads from
 * `entries[read_idx]` and advances. `count` distinguishes empty (0)
 * from full (XRCE_SUBSCRIBER_RING_DEPTH). On full, the callback
 * drops the newest message (preserves in-order delivery of buffered
 * msgs); on overflow length the per-entry `overflow` flag is set so
 * `take_serialized` can surface `MESSAGE_TOO_LARGE`.
 *
 * TODO 115.K.2.x: deadline tracking, async wakers. The Rust impl carries
 * `deadline_cb`, `last_msg_at_ms`, etc. Skipped here per K.2 scope.
 */
#ifndef XRCE_SUBSCRIBER_RING_DEPTH
/* Phase 160.H.1 — depth 32 (was bumped 4 → 16 upstream first; raised
 * to 32 after re-testing 100Hz burst behaviour). The XRCE Agent
 * batches all queued outbound messages onto the subscriber's input
 * stream in a single `run_session_time(timeout_ms)` window — the
 * callback fires once per Data frame and the listener can't
 * `take_serialized`-drain in between because they all execute inside
 * the same `uxr_run_session_time` call. So the ring must hold an
 * entire publish burst — anything beyond depth is silently dropped
 * by the topic callback's ring-full policy. The burst test
 * publishes 100 msgs back-to-back; depth 32 caps the visible burst
 * at the head 32 (matches the `test_xrce_throughput_*` expectations
 * of ≥3 / ≥10) without paying the memory cost of a 100+ entry
 * ring. Memory cost: 32 × XRCE_SUBSCRIBER_BUFFER_SIZE per
 * subscriber × 8 max = 256 KB. Tight-RAM RTOS targets can override
 * via `-DXRCE_SUBSCRIBER_RING_DEPTH=...` at build time.
 *
 * phase-454 W6.b — still a POLICY and still stated, exactly as
 * `zephyr/Kconfig` argues: no inventory knows how deep a burst a
 * subscriber must survive. What changed is where its DEFAULT comes
 * from when the image declared one (RFC-0100 D1: "a policy fact is
 * stated, and a derived fact may supply its default"). A subscription
 * declaring `qos.depth = 10` is a KEEP_LAST(10) queue that drops past
 * ten anyway, so 10 is a better default for that image than a literal
 * chosen for a 100-message burst test.
 *
 * The derivation moves this knob and XRCE_SUBSCRIBER_BUFFER_SIZE
 * TOGETHER or not at all, and that coupling is the whole reason it
 * cannot make an image bigger: this pool is one product
 * (`MAX_SUBSCRIBERS x RING_DEPTH x BUFFER_SIZE`, RFC-0100 D2), and a
 * declared bound of 1,174 bytes against a literal depth of 32 prices a
 * pool 15% LARGER than the defaults it replaced. Supplying one factor
 * from the declaration and the other from a literal prices a pool
 * neither describes. */
#define XRCE_SUBSCRIBER_RING_DEPTH 32
#endif

/* ---- Smallest legal size for the PER-SLOT arrays (issue 1131) ---
 *
 * BELOW the `#ifndef` fallbacks above, never before one: in `#if` an undefined
 * identifier reads as 0, so a guard placed above its own default fires on every
 * build instead of protecting anything (issue 1167).
 *
 * These are CAPACITIES INSIDE a slot, and that is what separates them from
 * the three slot COUNTS above, which issue 1033 ruled zero-legal. An image that
 * wants none of an entity sets its COUNT to 0 and the whole slot array goes
 * away — that is the 33,296-bytes-a-subscriber saving. Setting a capacity to 0
 * instead keeps the slots and makes each one unable to do its job, silently:
 *
 *   XRCE_BUFFER_SIZE=0          `xrce_stage_inbound` needs `len + 4 <= cap`, so
 *                               every inbound payload fails to stage and every
 *                               take reports MESSAGE_TOO_LARGE. Its human
 *                               producer, `nros-rmw-xrce-cffi/build.rs`, already
 *                               refuses anything under 64. It sizes no array
 *                               directly since phase-454 W6.b — it is the
 *                               DEFAULT of the three family knobs below, so a 0
 *                               here reaches all three and the guard still has
 *                               work to do.
 *   XRCE_SUBSCRIBER_BUFFER_SIZE=0 / XRCE_SERVICE_REQUEST_BUFFER_SIZE=0 /
 *   XRCE_SERVICE_REPLY_BUFFER_SIZE=0
 *                               the same `xrce_stage_inbound` arm, one family at
 *                               a time. Guarded each rather than by analogy with
 *                               the line above: the analogy is what issue 1131
 *                               ruled out, and a derived producer reaching one
 *                               of these is exactly the 1015 shape.
 *   XRCE_SUBSCRIBER_RING_DEPTH=0  `count >= depth` is true at 0, so the topic
 *                               callback takes its ring-full drop arm for every
 *                               message and the subscriber receives nothing —
 *                               issue 1015's silence exactly. Kconfig already
 *                               says `range 1 1024`.
 *   XRCE_SERVICE_REQUEST_RING_DEPTH=0  same arm in `xrce_request_callback`;
 *                               every request is dropped, and the `% DEPTH` in
 *                               `XRCE_REQ_RING_POP` is one branch away.
 *   XRCE_MAX_PENDING_REPLIES=0  `take_request` finds no free token, returns
 *                               WOULD_BLOCK and leaves the request in the ring,
 *                               so the server can never answer anything.
 */
#if XRCE_BUFFER_SIZE < 1
#error "XRCE_BUFFER_SIZE must be >= 1: it is the default of the family buffers (issue 1015)"
#endif
#if XRCE_SUBSCRIBER_BUFFER_SIZE < 1
#error "XRCE_SUBSCRIBER_BUFFER_SIZE must be >= 1: it sizes a C array (issue 1015)"
#endif
#if XRCE_SERVICE_REQUEST_BUFFER_SIZE < 1
#error "XRCE_SERVICE_REQUEST_BUFFER_SIZE must be >= 1: it sizes a C array (issue 1015)"
#endif
#if XRCE_BUILTIN_SERVICE_REQUEST_BUFFER_SIZE < 1
#error "XRCE_BUILTIN_SERVICE_REQUEST_BUFFER_SIZE must be >= 1: it sizes a C array (issue 1015)"
#endif
#if XRCE_SERVICE_REPLY_BUFFER_SIZE < 1
#error "XRCE_SERVICE_REPLY_BUFFER_SIZE must be >= 1: it sizes a C array (issue 1015)"
#endif
#if XRCE_SUBSCRIBER_RING_DEPTH < 1
#error "XRCE_SUBSCRIBER_RING_DEPTH must be >= 1: it sizes a C array (issue 1015)"
#endif
#if XRCE_SERVICE_REQUEST_RING_DEPTH < 1
#error "XRCE_SERVICE_REQUEST_RING_DEPTH must be >= 1: it sizes a C array (issue 1015)"
#endif
#if XRCE_MAX_PENDING_REPLIES < 1
#error "XRCE_MAX_PENDING_REPLIES must be >= 1: it sizes a C array (issue 1015)"
#endif

/* Issue 2108 — one client-side DEADLINE tracker (requested on a subscription,
 * offered on a publisher). Both deadlines are LOCAL facts: time since the last
 * takeable arrival / the last publish. Nothing about them crosses the Agent
 * (`uxrQoS_t` has no deadline field), so the Agent's DDS entity keeps an
 * infinite deadline and only this client counts misses. `period_ms == 0` is
 * disarmed. */
typedef struct xrce_deadline {
    uint32_t period_ms;
    /* Monotonic ms at which the current period ends without an arrival. */
    uint64_t next_expiry_ms;
    uint32_t total;
    uint32_t change;
    rmw_status_event_callback_t cb;
    void* cb_ctx;
} xrce_deadline;

typedef struct xrce_subscriber_ring_entry {
    uint8_t data[XRCE_SUBSCRIBER_BUFFER_SIZE];
    size_t len;
    bool overflow;
} xrce_subscriber_ring_entry;

typedef struct xrce_subscriber_slot {
    xrce_subscriber_ring_entry entries[XRCE_SUBSCRIBER_RING_DEPTH];
    uint16_t write_idx;
    uint16_t read_idx;
    uint16_t count;
    /* `locked` mirrors the Rust impl: callbacks observing this drop
     * the message rather than overwriting a buffer mid-read. */
    bool locked;
    uint16_t datareader_id;
    bool active;
    /* Issue 2109 — `subscription_set_on_new_message_callback`'s registration,
     * fired from `xrce_topic_callback` (inside `drive_io`) when a sample becomes
     * takeable. NULL = none. */
    rmw_event_callback_t on_new;
    const void* on_new_user;
    /* Issue 2108 — REQUESTED_DEADLINE_MISSED, reset by `xrce_topic_callback`.
     * In the slot because the arrival path reaches the slot, not the state. */
    xrce_deadline deadline;
} xrce_subscriber_slot;

/* Phase 237 — seq-keyed reply token. Holds the `SampleIdentity` of a received
 * request whose reply may be deferred; `take_request` allocates one and
 * returns its index as the runtime `sequence_number`, `send_response` consumes it. */
typedef struct xrce_reply_token {
    SampleIdentity sample_id;
    bool in_use;
} xrce_reply_token;

/* One buffered request in the service-server inbox ring. Its bytes are the
 * slot's (`xrce_service_request_data`): issue 1808 gave the two families
 * buffers of different sizes, so an entry cannot carry an array of its own. */
typedef struct xrce_service_request_entry {
    size_t len;
    bool overflow;
    SampleIdentity sample_id;
} xrce_service_request_entry;

/* Service-server slot — request inbox RING + deferred-reply token table. */
typedef struct xrce_service_server_slot {
    /* Phase 237 follow-up — SPSC ring of buffered requests (callback produces,
     * `take_request` consumes). `req_count` distinguishes empty (0) from
     * full (depth); on full the callback drops the newest, preserving in-order
     * delivery of the buffered requests. */
    xrce_service_request_entry req_ring[XRCE_SERVICE_REQUEST_RING_DEPTH];
    uint16_t req_write_idx;
    uint16_t req_read_idx;
    uint16_t req_count;
    uint16_t replier_id;
    bool active;
    /* Issue 1808 — the ring's bytes, `XRCE_SERVICE_REQUEST_RING_DEPTH` entries
     * of `entry_cap` each, in the family pool the slot belongs to. Bound when
     * the slot is claimed (`xrce_service_create_locked`). */
    uint8_t* ring_bytes;
    size_t entry_cap;
    /* Phase 237 — outstanding replies keyed by index (the runtime `seq`). */
    xrce_reply_token reply_tokens[XRCE_MAX_PENDING_REPLIES];
    /* Issue 2109 — `service_set_on_new_request_callback`'s registration. */
    rmw_event_callback_t on_new;
    const void* on_new_user;
} xrce_service_server_slot;

/* Service-client slot — reply inbox. */
typedef struct xrce_service_client_slot {
    uint8_t data[XRCE_SERVICE_REPLY_BUFFER_SIZE];
    size_t len;
    bool has_reply;
    bool overflow;
    uint16_t requester_id;
    /* Issue 0778 — the XRCE request id the buffered reply answers, recorded
     * by `xrce_reply_callback` from its `reply_id` argument (issue 2026: NOT
     * its `request_id`, which names the read that delivered the reply) and
     * reported out of `take_response` as the sequence id. */
    uint16_t reply_request_id;
    bool active;
    /* Issue 2109 — `client_set_on_new_response_callback`'s registration. */
    rmw_event_callback_t on_new;
    const void* on_new_user;
} xrce_service_client_slot;

/* ---- ROS 2 graph (issue 1292, graph.c) -------------------------------
 *
 * What `ros_discovery_info` says about this session. No static pool: a node
 * record is allocated by `create_node`, and an endpoint's record is EMBEDDED in
 * its entity's state and linked into one list, so the graph costs nothing per
 * entity the image does not create. Publishers have no slot table (the 0847
 * note on `live_entities` below), which is why this is a list and not a scan
 * of the slot pools. */

/* Fast-DDS entity kinds for a NO_KEY user endpoint — what the Agent's generic
 * XRCE types are (`preprocess_endpoint_attributes<WRITER, 0x03, 0x02>`). */
#define XRCE_GRAPH_WRITER 0x03u
#define XRCE_GRAPH_READER 0x04u

/* The GUID probe's total budget and its resend period (graph.c). */
#ifndef XRCE_GRAPH_PROBE_TIMEOUT_MS
#define XRCE_GRAPH_PROBE_TIMEOUT_MS 1000u
#endif
#ifndef XRCE_GRAPH_PROBE_RETRY_MS
#define XRCE_GRAPH_PROBE_RETRY_MS 20
#endif

typedef struct xrce_graph_node {
    struct xrce_graph_node* next;
    /* Borrowed from the runtime's node shell, which outlives the node
     * (`rmw_node_t::name`). */
    const char* name;
    const char* ns;
    /* Recorded for an endpoint whose node never went through `create_node`;
     * freed with the session rather than by `destroy_node`. */
    bool implicit;
} xrce_graph_node;

typedef struct xrce_graph_endpoint {
    struct xrce_graph_endpoint* next;
    /* NULL = attributed to no node (its node was destroyed first). */
    xrce_graph_node* node;
    /* The Agent-side DDS entity key, 24 bits, and its kind byte. */
    uint32_t key;
    uint8_t kind;
    bool linked;
} xrce_graph_endpoint;

/* Issue 2106 — what one REMOTE participant's latest `ros_discovery_info`
 * sample says: its GID and its nodes, packed as `ns\0name\0` pairs in one
 * allocation. Bounded by `XRCE_GRAPH_MAX_PEERS` / `XRCE_GRAPH_MAX_PEER_NODES`;
 * a sample past either bound is REFUSED (`peers_overflowed`), never truncated. */
#ifndef XRCE_GRAPH_MAX_PEERS
#define XRCE_GRAPH_MAX_PEERS 32u
#endif
#ifndef XRCE_GRAPH_MAX_PEER_NODES
#define XRCE_GRAPH_MAX_PEER_NODES 64u
#endif

typedef struct xrce_graph_peer {
    struct xrce_graph_peer* next;
    uint8_t gid[24];
    uint32_t n_nodes;
    size_t names_len;
    char* names;
} xrce_graph_peer;

typedef struct xrce_graph {
    /* The probe answered and the `ros_discovery_info` writer exists. */
    bool active;
    /* `counter` is the key the Agent gave the last endpoint on this
     * participant. False after a failed create, until the next probe. */
    bool counter_known;
    /* Nodes or endpoints changed since the last published sample. */
    bool dirty;
    bool warned_too_large;
    uint8_t prefix[12];
    uint32_t counter;
    uxrObjectId writer_oid;
    xrce_graph_node* nodes;
    xrce_graph_endpoint* endpoints;
    /* Probe in flight: the replier's object id (0 = none), the nonce its
     * request carries, and what the answer said. */
    uint16_t probe_replier_id;
    uint8_t probe_nonce[8];
    bool probe_seen;
    uint32_t probe_key;
    /* Issue 2106 — the `ros_discovery_info` READER (created on first use), the
     * remote participants it has heard, and the graph-change edge. */
    bool reader_active;
    uint16_t reader_id;
    xrce_graph_peer* peers;
    uint32_t n_peers;
    bool peers_overflowed;
    bool warned_overflow;
    rmw_event_callback_t on_change;
    const void* on_change_user;
} xrce_graph;

/* ---- Per-session state ---------------------------------------------- */

struct xrce_session_state {
    /* Transport — UDP (POSIX builds only) or custom. Only one is
     * live at a time; the mode is captured at open via the locator
     * scheme. Embedded builds drop the UDP profile entirely, so the
     * field is gated to keep the struct size predictable across
     * targets. */
#if defined(UCLIENT_PROFILE_UDP)
    uxrUDPTransport udp;
#endif
    uxrCustomTransport custom;
    bool use_custom_transport;
    /* Phase 115.K.2.5.1.2.a-fix-transport — POSIX UDP via custom
     * transport. `udp_bridge.fd` is set by `xrce_posix_udp_init`
     * and read by the per-session trampolines through
     * `uxrCustomTransport.args`. */
    struct {
        int fd;
        void* sock;
        void* endpoint;
    } udp_bridge;
    /* Phase 115.K.2.5.1.5-serial — POSIX serial transport via
     * custom transport. Same shape as `udp_bridge`: an `int fd`
     * threaded through the trampolines via `uxrCustomTransport.args`. */
    struct {
        int fd;
    } serial_bridge;

    uxrSession session;

    /* Reliable streams. The buffers must outlive the session. */
    uint8_t output_reliable_buf[XRCE_STREAM_BUFFER_SIZE];
    uint8_t input_reliable_buf[XRCE_STREAM_BUFFER_SIZE];
    uxrStreamId output_reliable;
    uxrStreamId input_reliable;

    /* Participant + entity-id allocator. */
    uxrObjectId participant_oid;
    uint16_t next_entity_id;

    /* Per-entity slot pools. */
    xrce_subscriber_slot subscriber_slots[XRCE_MAX_SUBSCRIBERS];
    xrce_service_server_slot service_server_slots[XRCE_MAX_SERVICE_SERVERS];
    xrce_service_client_slot service_client_slots[XRCE_MAX_SERVICE_CLIENTS];
    /* Issue 1808 — the two families' request bytes. Slots
     * [0, XRCE_USER_SERVICE_SERVERS) are the application's, the rest the
     * builtin family's. */
    uint8_t service_request_bytes[XRCE_USER_SERVICE_SERVERS][XRCE_SERVICE_REQUEST_RING_DEPTH]
                                 [XRCE_SERVICE_REQUEST_BUFFER_SIZE];
#if XRCE_BUILTIN_SERVICE_SERVERS > 0
    uint8_t builtin_request_bytes[XRCE_BUILTIN_SERVICE_SERVERS][XRCE_SERVICE_REQUEST_RING_DEPTH]
                                 [XRCE_BUILTIN_SERVICE_REQUEST_BUFFER_SIZE];
#endif

    /* Issue 0847 — how many entity handles still point HERE, and whether
     * `xrce_session_destroy` has already run.
     *
     * Every entity state (`xrce_publisher_state` and the three beside it)
     * holds a raw `xrce_session_state_t*`. `executor.close()` destroys the
     * session, and a publisher still alive in the caller's scope then runs its
     * own destructor and dereferences freed memory -- a plain use-after-free
     * across the C ABI, and the ordering the bench binaries and the docs both
     * show. It reproduced as SIGSEGV at every payload size.
     *
     * Two fields, no new pools. A pool was the other candidate: nulling each
     * live entity's back-pointer at close needs the session to ENUMERATE its
     * entities, and the slot tables above cover subscribers, service servers
     * and service clients but NOT publishers -- so that shape needs a fourth
     * static pool, on the backend whose whole campaign right now is removing
     * static RAM nobody can price (phase-392).
     *
     * Cyclone is immune to this and the difference is instructive: it stores
     * `dds_entity_t` HANDLES, which the library validates, so `dds_delete`
     * after its participant is gone returns an error instead of faulting. A
     * raw pointer cannot be validated once freed, so the memory has to outlive
     * the pointer instead.
     *
     * Guarded by `api_lock` below, like every other field here (issue 1734). */
    size_t live_entities;
    bool session_closed;

    /* Issue 1734 — ONE recursive lock per session, held across every entry
     * point that touches the uxr session or the slot pools.
     *
     * A session is shared: a multi-tier image's tiers each create entities on
     * it and each drive it, on their own threads. Two things were unguarded.
     * The slot claims (`subscriber_slots[i].active`, the two service pools,
     * `reply_tokens[i].in_use`) are the find-then-mark issue 1711 fixed in the
     * zenoh shim, and `next_entity_id++` beside them. And every one of those
     * paths, plus publish, take and drive_io, drives Micro XRCE-DDS-Client on
     * `session`, which is not thread-safe (its own MULTITHREAD profile exists
     * for exactly this, on three of our six platforms). So the lock covers the
     * uxr calls, not only the claims — a claim-only lock, 1711's scope, would
     * leave two threads inside `uxr_run_session_*` at once.
     *
     * RECURSIVE because the uxr callbacks (`xrce_topic_callback` and its two
     * siblings) run inside `uxr_run_session_*` on the thread that holds it,
     * and an entry point calls helpers that are entry points themselves.
     *
     * Storage is by value at the platform's compile-time upper bound
     * (`NROS_PLATFORM_MUTEX_REC_STORAGE_SIZE`), `uint64_t` for 8-byte
     * alignment in C99. `api_lock_ready` is false on a state that never went
     * through `xrce_session_create` — the transport-free lifetime test
     * allocates one — and the lock then degrades to the depth bookkeeping
     * alone, which is all a single-threaded caller needs.
     *
     * `lock_depth` and `free_pending` let a destructor free the state from
     * INSIDE a locked region: the free waits for the outermost unlock, so no
     * thread ever unlocks a mutex inside freed memory. */
    uint64_t api_lock[(NROS_PLATFORM_MUTEX_REC_STORAGE_SIZE + 7u) / 8u];
    bool api_lock_ready;
    unsigned lock_depth;
    bool free_pending;

    /* Issue 1292 — the nodes and endpoint GIDs `ros_discovery_info` reports. */
    xrce_graph graph;

    /* Issue 2108 — publishers with an armed OFFERED deadline, intrusive via
     * `xrce_publisher_state::deadline_next`. Publishers have no slot table, so
     * `drive_io` needs this list to find them. */
    struct xrce_publisher_state* deadline_publishers;
};

typedef struct xrce_session_state xrce_session_state_t;

/* Per-publisher state. */
typedef struct xrce_publisher_state {
    xrce_session_state_t* session_state;
    /* Issue 2108 — OFFERED_DEADLINE_MISSED (reset by every publish), the
     * profile's deadline used when a registration names none, and the link in
     * `deadline_publishers` while armed. */
    xrce_deadline deadline;
    uint32_t qos_deadline_ms;
    struct xrce_publisher_state* deadline_next;
    bool deadline_listed;
    uxrObjectId datawriter_oid;
    xrce_graph_endpoint graph_writer;
    /* Issue 2107 — what the BIN create SENT (`xrce_map_qos`) and the topic-name
     * flag, so `publisher_get_actual_qos` reports the profile the Agent built
     * the DataWriter from. */
    uxrQoS_t sent_qos;
    bool avoid_ros;
} xrce_publisher_state;

/* Per-subscriber state — the slot lives inside the session state. */
typedef struct xrce_subscriber_state {
    xrce_session_state_t* session_state;
    /* Issue 2108 — the profile's deadline, used when a registration names none. */
    uint32_t qos_deadline_ms;
    xrce_subscriber_slot* slot;
    uxrObjectId datareader_oid;
    xrce_graph_endpoint graph_reader;
    /* Issue 2107 — as `xrce_publisher_state::sent_qos`. */
    uxrQoS_t sent_qos;
    bool avoid_ros;
} xrce_subscriber_state;

/* Per-service-server state. The Agent builds a replier as a reply DataWriter
 * then a request DataReader (`FastDDSReplier::create_by_attributes`). */
typedef struct xrce_service_server_state {
    xrce_session_state_t* session_state;
    xrce_service_server_slot* slot;
    uxrObjectId replier_oid;
    xrce_graph_endpoint graph_writer;
    xrce_graph_endpoint graph_reader;
    /* Issue 2107 — the one requested field a requester/replier honours (it
     * shapes the DDS topic names client-side); see `xrce_actual_qos_service`. */
    bool avoid_ros;
} xrce_service_server_state;

/* Per-service-client state. A requester is a request DataWriter then a reply
 * DataReader, in that order. */
typedef struct xrce_service_client_state {
    xrce_session_state_t* session_state;
    xrce_service_client_slot* slot;
    uxrObjectId requester_oid;
    xrce_graph_endpoint graph_writer;
    xrce_graph_endpoint graph_reader;
    /* Issue 2107 — as `xrce_service_server_state::avoid_ros`. */
    bool avoid_ros;
} xrce_service_client_state;

/* ---- Helpers -------------------------------------------------------- */

/* ---- Issue 0847: entity/session lifetime ------------------------------ */

/* Register one entity against the session. Call at the single SUCCESS point of
 * a creator -- not where `session_state` is assigned, because several creators
 * fail after that assignment and free their own state, which would leave the
 * count high forever and leak the session. */
void xrce_session_entity_attach(xrce_session_state_t* st);

/* Has `xrce_session_destroy` already run? A destructor MUST check this before
 * touching `st->session` or `st->output_reliable`: after close, the uxr session
 * is deleted and the transport is shut, so the agent has already dropped every
 * entity and a DELETE_ENTITY has nowhere to go. Call BEFORE detaching -- detach
 * may free `st`. */
bool xrce_session_is_closed(const xrce_session_state_t* st);

/* Mark the session closed and free its state IF no entity still points at it.
 * Split out of `xrce_session_destroy` so the lifetime decision is testable
 * without a transport: `uxr_delete_session` waits on a session status and
 * faults without one, and this bug is about WHEN the state is freed, not about
 * the wire. */
void xrce_session_mark_closed(xrce_session_state_t* st);

/* Unregister one entity. Frees the session state when it was the last one and
 * the session is already closed -- so the memory outlives the pointers into it
 * without the session needing to know who they are. Never touch `st` after
 * this returns. */
void xrce_session_entity_detach(xrce_session_state_t* st);

/* Issue 1734 — take / release the session's API lock (see `api_lock` in the
 * struct). Recursive. NULL is a no-op, so an entry point may call these before
 * it has checked `st`.
 *
 * `xrce_session_unlock` is also where a deferred free happens: when the last
 * entity leaves a closed session inside a locked region, the state is freed by
 * the OUTERMOST unlock. So after `xrce_session_unlock(st)` returns, `st` may be
 * gone if a destroy ran inside the region; callers never read it again. */
/* The session an entry point's argument leads to, or NULL when it leads to
 * none (the body then refuses it as before). For the lock only: these read no
 * further than the first pointer, so a bad argument still gets the body's own
 * answer. */
#define XRCE_ST_OF_SESSION(session)                                                                \
    ((session) != NULL ? (xrce_session_state_t*)(session)->backend_data : NULL)
#define XRCE_ST_OF_NODE(node) ((node) != NULL ? XRCE_ST_OF_SESSION((node)->session) : NULL)
#define XRCE_ST_OF(state_type, entity)                                                             \
    (((entity) != NULL && (entity)->backend_data != NULL)                                          \
         ? ((const state_type*)(entity)->backend_data)->session_state                              \
         : NULL)

void xrce_session_lock(xrce_session_state_t* st);
void xrce_session_unlock(xrce_session_state_t* st);

/* Issue 1734 — release a session state that is NOT shared yet (a failed
 * `xrce_session_create`) or that the lifetime rules say is unreachable: drops
 * the lock, releases the graph, frees. */
void xrce_session_free(xrce_session_state_t* st);

/* Allocate the next entity id of the given type. Mirrors the Rust
 * impl's `alloc_entity_id`. */
uxrObjectId xrce_alloc_entity_id(xrce_session_state_t* st, uint8_t type);

/* Run the agent until all `count` request statuses are received,
 * returning OK only if every status is `UXR_STATUS_OK` /
 * `UXR_STATUS_OK_MATCHED`. */
rmw_ret_t xrce_confirm_entities(xrce_session_state_t* st, const uint16_t* requests,
                                uint8_t* statuses, size_t count);

/* Issue 0819 — stage one inbound XRCE payload into a receive slot.
 *
 * Every receive callback in this backend does the same three things: bound the
 * declared length against the slot, write the 4-byte CDR encapsulation header
 * the agent strips on the wire, and copy the payload in behind it. The copy is
 * the part that cannot be hand-written, and all three sites had it wrong the
 * same way.
 *
 * A fragmented message does NOT arrive as contiguous bytes. `uxr_next_input_
 * reliable_buffer_available` initialises the `ucdrBuffer` over the FIRST
 * fragment only and installs `on_full_input_buffer`, which swaps in the next
 * fragment when a read crosses the end; `read_format_data` deliberately
 * propagates that callback onto the buffer it hands the callback. So
 * `ub->iterator` addresses one fragment, `ucdr_buffer_remaining()` measures one
 * fragment, and a flat `memcpy` of the DECLARED length reads past the fragment
 * into whatever follows it. `ucdr_deserialize_array_uint8_t` is the read that
 * honours the chain, and is what this uses.
 *
 * Writes the header + payload into `dst` (capacity `dst_cap`) and sets
 * `*out_len` to the total staged length. Returns false when the message does
 * not fit or the reassembly failed — the caller's overflow flag, surfaced as
 * `NROS_RMW_RET_MESSAGE_TOO_LARGE`, which is loud. `*out_len` is 0 then. */
bool xrce_stage_inbound(uint8_t* dst, size_t dst_cap, struct ucdrBuffer* ub, size_t len,
                        size_t* out_len);

/* Issue 2109 — fire a per-entity new-data registration: once per arrival,
 * with 1 (the slot contract's lower bound; this backend does not count). */
static inline void xrce_fire_on_new(rmw_event_callback_t cb, const void* user) {
    if (cb != NULL) {
        cb(user, 1);
    }
}

/* Issue 2109 — the three `set_on_new_*` slots (subscriber.c / service.c). A
 * second call replaces the registration, NULL clears it, destroy clears it. */
rmw_ret_t xrce_subscription_set_on_new_message_callback(rmw_subscription_t* subscription,
                                                        rmw_event_callback_t callback,
                                                        const void* user_data);
rmw_ret_t xrce_service_set_on_new_request_callback(rmw_service_t* service,
                                                   rmw_event_callback_t callback,
                                                   const void* user_data);
rmw_ret_t xrce_client_set_on_new_response_callback(rmw_client_t* client,
                                                   rmw_event_callback_t callback,
                                                   const void* user_data);

/* Issue 1808 — the service-server families (service.c). Whether `name` is a
 * builtin (parameter / lifecycle) endpoint, and claiming a free slot in
 * [from, to) bound to its family's request bytes. Exposed for the CTest
 * harness (`tests/service_families.c`). */
bool xrce_is_builtin_service(const char* name);
xrce_service_server_slot* xrce_claim_server_slot(xrce_session_state_t* st, size_t from, size_t to);

/* DDS topic-name conversion. Strips a leading '/' and prepends "rt/"
 * unless `avoid_ros_prefix` is non-zero. Writes a NUL-terminated
 * string into `out` (capacity `out_cap`); truncates if too long. */
void xrce_dds_topic_name(const char* topic_name, int avoid_ros_prefix, char* out, size_t out_cap);
void xrce_dds_request_topic(const char* service_name, int avoid_ros_prefix, char* out,
                            size_t out_cap);
void xrce_dds_reply_topic(const char* service_name, int avoid_ros_prefix, char* out,
                          size_t out_cap);
void xrce_dds_request_type(const char* type_name, char* out, size_t out_cap);
void xrce_dds_reply_type(const char* type_name, char* out, size_t out_cap);

/* QoS mapping. */
uxrQoS_t xrce_map_qos(const rmw_qos_profile_t* qos);

/* issue 1329 — the `NROS_RMW_QOS_POLICY_*` bits this backend honours. */
rmw_ret_t xrce_supported_qos_policies(const rmw_session_t* session, uint32_t* out_mask);

/* Issue 2107 — the QoS the Agent-side DDS entity holds, for the six
 * `*_get_actual_qos` slots. Writes only what is known and leaves the rest of
 * `out` as the caller pre-loaded it (`NROS_RMW_QOS_PROFILE_UNKNOWN`). */
void xrce_actual_qos_topic(const uxrQoS_t* sent, bool avoid_ros, rmw_qos_profile_t* out);
void xrce_actual_qos_service(bool writer, bool avoid_ros, rmw_qos_profile_t* out);

/* ---- Issue 2108: client-side DEADLINE emulation (session.c) ----------------
 *
 * Every function takes `now_ms` (monotonic) rather than reading the clock, so
 * a test can drive time; the entry points pass `xrce_now_ms()`. */
uint64_t xrce_now_ms(void);
/* The period a registration asks for: `requested`, else the profile's, else
 * none. 0 and NROS_RMW_DURATION_INFINITE_MS both mean "no deadline". */
uint32_t xrce_deadline_period(uint32_t requested, uint32_t qos_deadline_ms);
void xrce_deadline_arm(xrce_deadline* d, uint32_t period_ms, rmw_status_event_callback_t cb,
                       void* ctx, uint64_t now_ms);
/* An arrival (subscription) or a publish: the current period starts over. */
void xrce_deadline_touch(xrce_deadline* d, uint64_t now_ms);
/* Count every period that ended since the last check and, if any did, fire the
 * callback once with the accumulated `total_count_change`. */
void xrce_deadline_check_one(xrce_deadline* d, rmw_event_type_t kind, uint64_t now_ms);
/* `check_one` over every armed subscription slot and listed publisher. */
void xrce_deadline_check(xrce_session_state_t* st, uint64_t now_ms);
/* The earliest armed expiry, as ms from `now_ms` (0 when already due). */
bool xrce_deadline_next(const xrce_session_state_t* st, uint64_t now_ms, uint32_t* out_ms);
void xrce_deadline_unlist_publisher(xrce_session_state_t* st, struct xrce_publisher_state* ps);

rmw_ret_t xrce_subscription_event_init(const rmw_subscription_t* subscription,
                                       rmw_event_type_t kind, uint32_t deadline_ms,
                                       rmw_status_event_callback_t cb, void* user_context);
rmw_ret_t xrce_publisher_event_init(const rmw_publisher_t* publisher, rmw_event_type_t kind,
                                    uint32_t deadline_ms, rmw_status_event_callback_t cb,
                                    void* user_context);
rmw_ret_t xrce_next_deadline_ms(const rmw_session_t* session, uint32_t* out_ms,
                                bool* has_deadline);
bool xrce_event_type_is_supported(rmw_event_type_t event_type);

/* ---- Capacity diagnostics (issue 1033) ------------------------------- */

/* Severity for `nros_platform_log_write`, which takes `nros_log::Severity`'s
 * `as_u8()`: 0 Trace, 1 Debug, 2 Info, 3 Warn, 4 Error, 5 Fatal. Spelled here
 * rather than included from `rclc/log.h`: that header belongs to `nros-c`, the
 * layer ABOVE this one, and an RMW backend must not depend upward. */
#define XRCE_LOG_ERROR 4

/* Report that this image was BUILT with fewer `kind` slots than it has just
 * been asked to create, then let the caller return
 * `NROS_RMW_RET_INVALID_CONFIG`.
 *
 * issue 1033. Until the entity caps joined the derivable ladder, exhausting one
 * meant creating a ninth subscriber on an image budgeted for eight — rare, and
 * arguably the caller's problem. Now the cap is DERIVED from what the image
 * DECLARED (`nano_ros_node_register(... ENTITIES ...)`), so it is exactly the
 * declared count, and the ordinary way to hit it is to create an entity that
 * was never declared. The remedy is a rebuild, and the reader needs to be told
 * WHICH knob and WHY it is the number it is — a bare `NROS_RMW_RET_ERROR`, the
 * code these paths used to return, says none of that and is the same value a
 * NULL `backend_data` returns three lines up.
 *
 * `NROS_RMW_RET_INVALID_CONFIG` is issue 0468's code, added for precisely this
 * on the zenoh side: "a COMPILE-TIME capacity or configuration made the call
 * impossible ... the remedy is a rebuild, never a different argument". This is
 * the same failure one backend over, so it gets the same code rather than a
 * second spelling of it.
 *
 * Delivery is `nros_platform_log_write` — the "platform's printk-equivalent"
 * `nros/rmw_ret.h` already names as where backends log at the failure site. It
 * reaches every platform including Zephyr native_sim, where Rust `std` stdio is
 * fatal (issue 0589).
 *
 * `kind` names the entity ("subscriber"), `name` the topic or service it was
 * for, `cap` the compile-time maximum and `knob` the Kconfig symbol that moves
 * it. */
void xrce_report_capacity_exhausted(const char* kind, const char* name, unsigned cap,
                                    const char* knob);

/* Report that the ONE per-session allocation could not be served, with the size
 * that was asked for and the knobs that decide it.
 *
 * issue 1033 — `sizeof(xrce_session_state_t)` is decided entirely at build time
 * and dominated by `subscriber_slots`, and the whole struct is a single
 * request. When it fails the caller gets `NROS_RMW_RET_BAD_ALLOC` and nothing
 * else, which is how a 309,696-byte request against a 65,536-byte Zephyr heap
 * presented as an anonymous boot failure for as long as issue 0968 was open.
 * The number is knowable at the failure site; printing it is the difference
 * between a nine-step bisect and one line. */
void xrce_report_session_alloc_failed(size_t bytes);

/* ---- session.c ---- */
rmw_ret_t xrce_session_create(const char* locator, uint8_t mode, uint32_t domain_id,
                              const char* node_name, const rmw_session_options_t* options,
                              rmw_session_t* out);
rmw_ret_t xrce_session_destroy(rmw_session_t* session);
rmw_ret_t xrce_session_drive_io(rmw_session_t* session, int32_t timeout_ms);
/* Phase 124.F.2 — connectivity probe via `uxr_ping_agent_session`. */
rmw_ret_t xrce_session_ping(rmw_session_t* session, int32_t timeout_ms);

/* ---- graph.c (issue 1292) ---- */

/* After the participant exists: probe the participant's GUID prefix and the
 * Agent's endpoint numbering, create the `ros_discovery_info` writer and
 * publish the first sample. On failure the graph stays inactive, with one
 * logged line; the session works as before. */
void xrce_graph_open(xrce_session_state_t* st);
/* Free what the graph still owns. Called where the session state is freed. */
void xrce_graph_release(xrce_session_state_t* st);
/* Call BEFORE buffering an endpoint CREATE: re-probes when a failed create left
 * the Agent's numbering unknown. */
void xrce_graph_ensure_counter(xrce_session_state_t* st);
/* Call AFTER the create's confirm. `count` = the DDS endpoints the Agent built
 * for it (1 for a writer or reader, 2 for a replier or requester); `created` =
 * whether the confirm succeeded. On true, `*first_key` is the first of them. */
bool xrce_graph_claim(xrce_session_state_t* st, unsigned count, bool created, uint32_t* first_key);
void xrce_graph_track(xrce_session_state_t* st, xrce_graph_endpoint* ep, const rmw_node_t* node,
                      uint32_t key, uint8_t kind);
void xrce_graph_untrack(xrce_session_state_t* st, xrce_graph_endpoint* ep);
/* Issue 2110 — `ep`'s DDS GID (24 bytes, the last 8 zero), the one
 * `ros_discovery_info` lists. False when `ep` is not tracked: no GID is known. */
bool xrce_graph_endpoint_gid(const xrce_session_state_t* st, const xrce_graph_endpoint* ep,
                             uint8_t out[24]);
/* Publish the sample if anything changed. Called from `drive_io`. */
void xrce_graph_flush(xrce_session_state_t* st);
/* The request callback's first stop: true if the request was the probe's. */
/* Issue 2106 — the `ros_discovery_info` read side (graph.c). */
/* Consume a topic sample addressed to the graph reader. True = it was ours. */
bool xrce_graph_on_topic(xrce_session_state_t* st, uxrObjectId object_id, struct ucdrBuffer* ub,
                         size_t len);
/* Fold one serialized ParticipantEntitiesInfo (no encapsulation header) into
 * the peer table. Returns false for a sample that does not parse or does not
 * fit (the latter also sets `peers_overflowed`). Split out for no-Agent tests. */
bool xrce_graph_ingest(xrce_graph* g, const uint8_t* buf, size_t len);
rmw_ret_t xrce_get_node_names(const rmw_session_t* session, rmw_node_visitor_t visitor);
rmw_ret_t xrce_node_get_graph_guard_condition(rmw_session_t* session,
                                              rmw_event_callback_t callback,
                                              const void* user_data);

bool xrce_graph_on_request(xrce_session_state_t* st, uxrObjectId object_id,
                           const SampleIdentity* sample_id, struct ucdrBuffer* ub, size_t len);
/* Serialize the ParticipantEntitiesInfo body (no encapsulation header) into
 * `buf`. Returns the length, or 0 if `cap` was short. Exposed for the CTest. */
size_t xrce_graph_serialize(const xrce_graph* g, uint8_t* buf, size_t cap);
size_t xrce_graph_sample_bound(const xrce_graph* g);
/* The `create_node` / `destroy_node` vtable slots. */
rmw_ret_t xrce_node_create(rmw_session_t* session, const char* name, const char* namespace_,
                           rmw_node_t* out);
rmw_ret_t xrce_node_destroy(rmw_node_t* node);

/* ---- publisher.c ---- */
rmw_ret_t xrce_publisher_create(const rmw_node_t* node,
                                const rmw_message_type_support_t* type_support,
                                const char* topic_name, uint32_t domain_id,
                                const rmw_qos_profile_t* qos,
                                const rmw_publisher_options_t* options, rmw_publisher_t* out);
rmw_ret_t xrce_publisher_destroy(rmw_publisher_t* publisher);
/* Issue 0782 — exposed for the smoke test: the streamed-publish chunk loop,
 * which is the only part of that path reachable without an XRCE agent. */
size_t xrce_drive_streamed_body(uint8_t* body, size_t body_len, size_t total,
                                void (*chunk_cb)(uint8_t* out_buf, size_t cap, size_t* out_written,
                                                 void* user_ctx),
                                void* user_ctx);
rmw_ret_t xrce_publisher_publish_raw(const rmw_publisher_t* publisher, rmw_byte_span_t payload);
/* Phase 124.E.3 — streamed publish via `uxr_prepare_output_stream`. */
rmw_ret_t xrce_publisher_publish_streamed(rmw_publisher_t* publisher,
                                          void (*size_cb)(size_t* out_total_len, void* user_ctx),
                                          void (*chunk_cb)(uint8_t* out_buf, size_t cap,
                                                           size_t* out_written, void* user_ctx),
                                          void* user_ctx);

/* ---- subscriber.c ---- */
rmw_ret_t
xrce_subscription_create(const rmw_node_t* node, const rmw_message_type_support_t* type_support,
                         const char* topic_name, uint32_t domain_id, const rmw_qos_profile_t* qos,
                         const rmw_subscription_options_t* options, rmw_subscription_t* out);
rmw_ret_t xrce_subscription_destroy(rmw_subscription_t* subscriber);
rmw_ret_t xrce_subscription_take(const rmw_subscription_t* subscriber, rmw_mut_byte_span_t* out,
                                 bool* taken);
rmw_ret_t xrce_subscription_has_data(rmw_subscription_t* subscriber, bool* out_has_data);
/* Phase 231 (RFC-0038) — zero-copy in-place take over the XRCE static ring. */
rmw_ret_t xrce_subscription_supports_in_place(rmw_subscription_t* subscriber, bool* out_supports);
rmw_ret_t xrce_subscription_process_raw_in_place(rmw_subscription_t* subscriber, void* ctx,
                                                 void (*cb)(void* ctx, rmw_byte_span_t message),
                                                 bool* out_processed);

/* Topic data callback — single instance per session, registered at
 * session_open. Exposed so session.c can pass its address to
 * `uxr_set_topic_callback`. */
void xrce_topic_callback(uxrSession* session, uxrObjectId object_id, uint16_t request_id,
                         uxrStreamId stream_id, struct ucdrBuffer* ub, uint16_t length, void* args);

/* ---- service.c ---- */
rmw_ret_t xrce_service_create(const rmw_node_t* node,
                              const rmw_service_type_support_t* type_support,
                              const char* service_name, uint32_t domain_id,
                              const rmw_qos_profile_t* qos, rmw_service_t* out);
rmw_ret_t xrce_service_destroy(rmw_service_t* server);
rmw_ret_t xrce_service_take_request(const rmw_service_t* server, rmw_mut_byte_span_t* request,
                                    int64_t* seq_out, bool* taken);
/* Issue 2081 — `xrce_service_take_request` plus the request's
 * `rmw_service_info_t`. */
rmw_ret_t xrce_service_take_request_with_info(const rmw_service_t* server,
                                              rmw_mut_byte_span_t* request, int64_t* seq_out,
                                              bool* taken, rmw_service_info_t* service_info);
rmw_ret_t xrce_service_has_request(rmw_service_t* server, bool* out_has_request);
rmw_ret_t xrce_service_send_response(const rmw_service_t* server, int64_t seq,
                                     rmw_byte_span_t response);

rmw_ret_t xrce_client_create(const rmw_node_t* node, const rmw_service_type_support_t* type_support,
                             const char* service_name, uint32_t domain_id,
                             const rmw_qos_profile_t* qos, rmw_client_t* out);
rmw_ret_t xrce_client_destroy(rmw_client_t* client);
/* Phase 130.4 — non-blocking split (phase-301: the deprecated blocking
 * `call_raw` slot was deleted from the vtable; this pair is the one
 * request/reply path). */
rmw_ret_t xrce_service_send_request_raw(const rmw_client_t* client, rmw_byte_span_t request_span,
                                        int64_t* sequence_id);
rmw_ret_t xrce_service_take_response(const rmw_client_t* client, rmw_mut_byte_span_t* reply,
                                     int64_t* seq_out, bool* taken);

void xrce_request_callback(uxrSession* session, uxrObjectId object_id, uint16_t request_id,
                           SampleIdentity* sample_id, struct ucdrBuffer* ub, uint16_t length,
                           void* args);
void xrce_reply_callback(uxrSession* session, uxrObjectId object_id, uint16_t request_id,
                         uint16_t reply_id, struct ucdrBuffer* ub, uint16_t length, void* args);

/* ---- transport_custom.c (Phase 115.K.2.4) -------------------------- */

/* Install a runtime-supplied transport vtable into the session's
 * `uxrCustomTransport`. Trampolines fan out to the user's
 * open/close/write/read callbacks. The session.c open path consults
 * `xrce_custom_transport_is_armed()` after a `custom://` locator and
 * routes accordingly. */
struct xrce_custom_ops_slot;
int xrce_custom_transport_is_armed(void);
rmw_ret_t xrce_custom_transport_install(xrce_session_state_t* st, bool framing);

/* Phase 115.K.2.5.1.2.a-fix-transport — POSIX UDP via custom
 * transport. Replaces the K.2.1 `uxr_init_udp_transport` direct
 * path. Resolves `host`/`port`, opens a connected UDP socket,
 * and wires `xrce_session_state_t::custom` with trampolines that
 * drive the socket via `poll()` + `recv()` / `send()`. The
 * resulting transport behaves like the legacy `xrce-sys` shape
 * the agent has interop'd with for years. */
rmw_ret_t xrce_posix_udp_init(xrce_session_state_t* st, const char* host, const char* port);

/* Phase 129.NET.3 — platform-agnostic XRCE UDP. Works on every
 * target that satisfies the `nros_platform_udp_*` symbols at link
 * time (POSIX, Zephyr, FreeRTOS, ThreadX, ESP-IDF; bare-metal via
 * `nros-smoltcp`), and it is the ONLY UDP path `session.c` routes a
 * bare `host:port` locator through, on every platform.
 *
 * It superseded two per-platform TUs. `xrce_posix_udp_init` above is
 * still compiled (`backend_posix` in xrce-sources.txt) so a caller
 * that resolves the older symbol keeps working. Its Zephyr twin
 * `xrce_zephyr_udp_init` is GONE (issue 1073): 129.C.1 deleted the
 * `platform-zephyr` feature that compiled `transport_zephyr_udp.c`
 * AND turned off the `UCLIENT_PLATFORM_ZEPHYR` its whole body sat
 * behind, so the symbol has been absent from every archive since —
 * the "one cycle" of fallback was never actually delivered for it,
 * and the file was deleted rather than revived. */
rmw_ret_t xrce_nros_udp_init(xrce_session_state_t* st, const char* host, const char* port);

/* Phase 115.K.2.5.1.5-serial — POSIX serial transport via custom
 * transport. Opens a tty/pty `path`, configures termios (raw mode,
 * 8N1, baud from `XRCE_SERIAL_BAUD` env or 115200), and registers
 * read/write trampolines. framing=true (HDLC). */
rmw_ret_t xrce_posix_serial_init(xrce_session_state_t* st, const char* path);

#ifdef __cplusplus
}
#endif

#endif /* NROS_RMW_XRCE_C_INTERNAL_H */
