// Subscriber path — Phase 117.6 + 117.6.B, receive rewritten by issue 0969.
//
// Entity creation: registry lookup → topic + reader + QoS.
// Data path: dds_takecdr → ddsi_serdata_to_ser into the caller's buffer.
//
// The receive path USED to deserialize the wire CDR into a typed sample and
// then re-serialize it (`dds_take` → `dds_stream_write_sample`), because
// `sertype_min.hpp` recorded the raw-CDR API as blocked on Cyclone exposing
// `dds_writer_lookup_serdatatype`. That blocker is real for the PUBLISH
// direction and does not exist here: `dds_takecdr` takes only a reader entity,
// and the reader already owns its sertype from `dds_create_topic(desc)`. The
// serdata Cyclone hands back is already holding the wire bytes;
// `ddsi_serdata_to_ser` copies them — encapsulation header included, since
// `serdata_default_get_size` counts the `CDRHeader` and `to_ser` starts at
// `&d->hdr` — straight into the caller's buffer. One copy, no typed sample, no
// ostream, no re-encode. This is the shape `rmw_cyclonedds_cpp`'s
// `rmw_take_serialized_message` has always had.
//
// Consequence worth stating, because it is a behaviour change and not only a
// cost one: the caller now sees the WIRE representation rather than one this
// backend re-encoded as XCDR1 in native byte order. That is the point — an
// XCDR2 publisher's bytes now reach `CdrReader::new_with_header`, which
// dispatches on the encapsulation id. It also means a big-endian peer is no
// longer silently normalised to little-endian on the way through. nros-serdes
// decodes little-endian unconditionally, so a BE peer was never supported
// end-to-end anyway; this backend was the only one papering over that, and it
// now behaves like the rest.

#include "internal.hpp"

#include "descriptors.hpp"
#include "nros_sertype.hpp"
#include "qos.hpp"
#include "topic_prefix.hpp"

#include <dds/dds.h>
#include <dds/ddsi/ddsi_serdata.h>
#include <dds/ddsi/ddsi_sertype.h>

#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <new>

namespace nros_rmw_cyclonedds {

namespace {

struct SubState {
    dds_entity_t topic{0};
    dds_entity_t reader{0};
    /// The type this subscription was created against. The data path no longer
    /// reads it — `dds_takecdr` needs only the reader — but it is the
    /// subscription's type identity, and issue 0970 needs it to build the
    /// sertype that replaces `dds_create_topic(desc)`.
    const dds_topic_descriptor_t* desc{nullptr};
    /// Issue 0971 — a `BUFFER_TOO_SMALL` that happened where it could not be
    /// returned.
    ///
    /// `take_sequence` is contractually a COUNT: "partial drains MUST use the
    /// count form, not error-out" (`rmw_vtable.h`). So a batch that stops
    /// because a sample did not fit the caller's slot has nowhere to put the
    /// status, and used to discard it — leaving a partial drain and a drained
    /// reader indistinguishable.
    ///
    /// It is parked here and delivered by the NEXT take instead. That is the
    /// shape `nros-verification`'s `try_recv_post_fix` proves for the single
    /// take — check the flag first, clear it, return the error, take nothing —
    /// moved one call later, because that is where the contract leaves room.
    bool pending_too_small{false};
};

inline SubState* as_state(const rmw_subscription_t* s) {
    return static_cast<SubState*>(s->backend_data);
}

} // namespace

/// `options` is unused, and issue 0958 is why that is a decision rather than an
/// omission.
///
/// `rmw_subscription_options_t::rx_buffer_hint` tells a backend how many bytes a
/// receive buffer needs for this type, so a size-classing backend (zenoh-pico)
/// can pick a class. This backend has no receive buffer to class:
///
///   * the sample arrives in a serdata, sized by the sample and allocated by
///     `nros_sertype.cpp` at the moment it arrives;
///   * the destination is the CALLER's buffer, whose capacity arrives on every
///     `take` and is authoritative there.
///
/// Before issue 0969 there was exactly one candidate consumer: the `dds_ostream`
/// that re-serialised the typed sample grew by `realloc`, and an initial size
/// would have saved those reallocs. That ostream went with the round trip, and
/// with it the last thing a hint could have sized.
///
/// So the field is INAPPLICABLE here rather than unimplemented, and the
/// difference matters to anyone measuring: a Cyclone consumer can set the hint,
/// do everything the sizing campaign asks, and correctly observe nothing change
/// in this backend. What DOES change is the executor's arena, which nano-ros
/// sizes itself from the same bound — measure the arena, not the backend.
///
/// The ABI declares the field advisory and says a backend MAY ignore it
/// (`rmw_entity.h`). What it may not do is ignore it silently, which is the state
/// issue 0958 opened against: the parameter was discarded at a bare
/// `/*options*/` with nothing for a reader to find.
rmw_ret_t subscription_create(const rmw_node_t* node, const rmw_message_type_support_t* type_support,
                                const char* topic_name,
                                 uint32_t /*domain_id*/, const rmw_qos_profile_t* qos,
                                 const rmw_subscription_options_t* /*options — see above*/,
                                 rmw_subscription_t* out) {
    /* phase-406 W1 — one argument in, two locals out, so the body below is
       unchanged. A NULL type support is INVALID_ARGUMENT rather than an
       empty type: the identity is what the entity is keyed on, and one
       created without it matches nothing and reports nothing. */
    if (type_support == nullptr) return NROS_RMW_RET_INVALID_ARGUMENT;
    const char* type_name = type_support->type_name;
    (void)type_name;
    // Phase 376 W5/B1 — the entity is created ON ITS NODE, as upstream does.
    // The node carries the route to its session (our `context`).
    if (node == nullptr) return NROS_RMW_RET_INVALID_ARGUMENT;
    rmw_session_t* session = node->session;
    if (out == nullptr || session == nullptr || topic_name == nullptr || type_name == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    out->backend_data = nullptr;
    out->can_loan_messages = false;

    dds_entity_t pp = session_participant(session);
    if (pp == 0) {
        return NROS_RMW_RET_ERROR;
    }

    char eff_type[256];
    if (!action_topic_type(topic_name, type_name, eff_type, sizeof(eff_type))) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    const dds_topic_descriptor_t* desc = find_descriptor(eff_type);
    if (desc == nullptr) {
        return NROS_RMW_RET_UNSUPPORTED;
    }

    auto* state = new (std::nothrow) SubState();
    if (state == nullptr) {
        return NROS_RMW_RET_BAD_ALLOC;
    }

    // Phase 117.X.2: prepend `rt/` to match `rmw_cyclonedds_cpp`'s
    // wire-level topic naming.
    char prefixed[256];
    if (!topic_prefix::apply(topic_name, "rt", prefixed, sizeof(prefixed))) {
        delete state;
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    // Issue 0970 — register OUR sertype rather than the descriptor's, so the
    // reader's samples are CDR rather than typed C structs. The type name is
    // the descriptor's, so what SEDP advertises is unchanged.
    struct ddsi_sertype* st = create_nros_sertype(desc);
    if (st == nullptr) {
        // Keyed type, or out of memory. Keyed is a real refusal rather than a
        // fallback: see `nros_sertype.hpp`.
        delete state;
        return NROS_RMW_RET_UNSUPPORTED;
    }
    dds_entity_t topic = dds_create_topic_sertype(pp, prefixed, &st, nullptr, nullptr, nullptr);
    if (topic < 0) {
        // Ownership only transfers on success, so this one is still ours.
        ddsi_sertype_unref(st);
        delete state;
        return NROS_RMW_RET_ERROR;
    }
    state->topic = topic;
    state->desc = desc;

    dds_qos_t* dq = (qos != nullptr) ? make_dds_qos(qos) : nullptr;
    dds_entity_t reader = dds_create_reader(pp, topic, dq, nullptr);
    if (dq != nullptr) {
        dds_delete_qos(dq);
    }
    if (reader < 0) {
        (void)dds_delete(topic);
        delete state;
        return NROS_RMW_RET_ERROR;
    }
    state->reader = reader;

    out->backend_data = state;
    graph_track_reader(session_graph(session), reader); // Phase 177.36
    return NROS_RMW_RET_OK;
}

rmw_ret_t subscription_destroy(rmw_subscription_t* subscriber) {
    if (subscriber == nullptr) return NROS_RMW_RET_INVALID_ARGUMENT;
    SubState* state = as_state(subscriber);
    if (state == nullptr) return NROS_RMW_RET_INVALID_ARGUMENT;
    dds_return_t reader_rc = state->reader > 0 ? dds_delete(state->reader) : DDS_RETCODE_OK;
    dds_return_t topic_rc = state->topic > 0 ? dds_delete(state->topic) : DDS_RETCODE_OK;
    delete state;
    subscriber->backend_data = nullptr;
    if (reader_rc < 0 || topic_rc < 0) return NROS_RMW_RET_ERROR;
    return NROS_RMW_RET_OK;
}

rmw_ret_t subscription_take(const rmw_subscription_t* subscriber, rmw_mut_byte_span_t* out,
                                 bool* out_taken) {
    /* phase-406 W2 — by pointer: `capacity` in, `len` out. */
    if (out == nullptr) return NROS_RMW_RET_INVALID_ARGUMENT;
    uint8_t* buf = out->data;
    const size_t buf_len = out->capacity;
    size_t* out_len = &out->len;
    // Phase 376 W3.b/W3.d step A — upstream `rmw_take`'s shape. The parameter
    // is `out_taken`, not upstream's `taken`: this function already has a
    // `dds_return_t taken` holding Cyclone's sample count, and the two would
    // shadow — a name collision the compiler caught, between two things that
    // both legitimately mean "taken".
    if (subscriber == nullptr || buf == nullptr || out_len == nullptr || out_taken == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    SubState* state = as_state(subscriber);
    if (state == nullptr || state->reader <= 0) {
        return NROS_RMW_RET_ERROR;
    }

    // Issue 0971 — deliver a status a previous batch could not return, before
    // taking anything. Taking nothing is the point: the caller has to be able
    // to act on it before it asks for more.
    if (state->pending_too_small) {
        state->pending_too_small = false;
        return NROS_RMW_RET_BUFFER_TOO_SMALL;
    }

    // Skip invalid samples rather than reporting "nothing taken" on the first
    // one, as `rmw_take_ser_int` does. A dispose/unregister notification used
    // to mask any valid sample queued behind it for a whole poll cycle. The
    // loop terminates because each `dds_takecdr` removes what it returns.
    struct ddsi_serdata* d = nullptr;
    dds_sample_info_t si[1];
    for (;;) {
        dds_return_t taken = dds_takecdr(state->reader, &d, 1, si, DDS_ANY_STATE);
        if (taken < 0) {
            return NROS_RMW_RET_ERROR;
        }
        if (taken == 0) {
            // Empty subscription: OK with `taken = false`, not a sentinel.
            *out_taken = false;
            return NROS_RMW_RET_OK;
        }
        if (si[0].valid_data) {
            break;
        }
        ddsi_serdata_unref(d);
        d = nullptr;
    }

    // `ddsi_serdata_size` counts the 4-byte `CDRHeader`, and `to_ser` copies
    // from `&d->hdr`, so this one call delivers header + payload in the wire's
    // own representation. No header is synthesised here any more.
    //
    // 233.6 — the action `goal_id` is a fixed `octet[16]` on both the IDL and
    // the Rust runtime side (ROS 2 `unique_identifier_msgs/UUID`), so the wire
    // bytes already match what the Rust read path expects; the old
    // `insert_goal_id_len_at` adapter was removed with its publisher mirror.
    const uint32_t total = ddsi_serdata_size(d);
    if (buf_len < total) {
        ddsi_serdata_unref(d);
        return NROS_RMW_RET_BUFFER_TOO_SMALL;
    }
    ddsi_serdata_to_ser(d, 0, total, buf);
    ddsi_serdata_unref(d);

    *out_len = static_cast<size_t>(total);
    *out_taken = true;
    return NROS_RMW_RET_OK;
}

// Phase 124.D.3 — native batch take. Cyclone DDS `dds_takecdr` accepts
// (reader, buf, maxs, info, mask) and returns N serdatas in one call; issue
// 0969 replaced the typed `dds_take` + re-serialize body with a copy out of
// each serdata, matching `subscription_take` above.
static int32_t subscription_take_sequence_count(const rmw_subscription_t* subscriber, uint8_t* buf,
                                               size_t per_msg_cap, size_t max_msgs,
                                               size_t* out_lens) {
    if (subscriber == nullptr || buf == nullptr || out_lens == nullptr) {
        return -static_cast<int32_t>(NROS_RMW_RET_INVALID_ARGUMENT);  // issue 0773 — statuses travel NEGATED
    }
    if (per_msg_cap == 0 || max_msgs == 0) {
        return 0;
    }
    SubState* state = as_state(subscriber);
    if (state == nullptr || state->reader <= 0) {
        return -static_cast<int32_t>(NROS_RMW_RET_ERROR);  // issue 0773 — statuses travel NEGATED
    }

    // Issue 0971 — a status parked by an earlier drain goes out before any new
    // take, and this call reports nothing else. Same rule as `subscription_take`
    // above, so a caller that mixes the two entry points hears it either way.
    if (state->pending_too_small) {
        state->pending_too_small = false;
        return -static_cast<int32_t>(NROS_RMW_RET_BUFFER_TOO_SMALL);  // issue 0773 — NEGATED
    }

    // Stack-cap the per-call slot budget; Cyclone happily takes
    // any N but we want to bound the stack alloc. Larger callers
    // can issue multiple sequence-take rounds.
    constexpr size_t kMaxBatch = 32;
    const size_t take_n = max_msgs > kMaxBatch ? kMaxBatch : max_msgs;

    struct ddsi_serdata* ds[kMaxBatch] = {nullptr};
    dds_sample_info_t si[kMaxBatch];

    dds_return_t taken = dds_takecdr(state->reader, ds, take_n, si, DDS_ANY_STATE);
    if (taken < 0) {
        return -static_cast<int32_t>(NROS_RMW_RET_ERROR);  // issue 0773 — statuses travel NEGATED
    }
    if (taken == 0) {
        return 0;
    }

    // Issue 0971 — a sample too large for `per_msg_cap` ends the batch and IS
    // dropped, and the samples already written are still reported. Dropping it
    // is the design rather than an oversight: live zenoh does the same on its
    // single take ("drop the slot so the subscription isn't permanently stuck",
    // `shim/subscriber.rs`), and `nros-verification`'s `try_recv_post_fix` /
    // `no_silent_truncation` fix that in place — the consumer gets the complete
    // message or an explicit error, and no subscription is left stuck holding a
    // sample nobody can take.
    //
    // What was missing is the explicit error, because this function has nowhere
    // to return one: the contract says "partial drains MUST use the count form,
    // not error-out". So the status is parked on the subscription and the next
    // take delivers it. The old body's `err = NROS_RMW_RET_BUFFER_TOO_SMALL`
    // followed by `if (err < 0)` was unreachable — every `nros_rmw_ret_t` is
    // non-negative, which is why the callers negate — AND would have been the
    // wrong fix if reached, trading a silent drop for a contract violation.
    size_t produced = 0;
    for (dds_return_t i = 0; i < taken; ++i) {
        if (!si[i].valid_data) {
            continue;
        }
        const uint32_t total = ddsi_serdata_size(ds[i]);
        if (per_msg_cap < total) {
            state->pending_too_small = true;
            break;
        }
        ddsi_serdata_to_ser(ds[i], 0, total, buf + produced * per_msg_cap);
        out_lens[produced] = total;
        produced++;
    }

    // Release every serdata this call took — including the ones the loop above
    // skipped or never reached after a break. `dds_takecdr` refcounts rather
    // than loans, so this replaces the old single `dds_return_loan`.
    for (dds_return_t i = 0; i < taken; ++i) {
        if (ds[i] != nullptr) {
            ddsi_serdata_unref(ds[i]);
        }
    }

    return static_cast<int32_t>(produced);
}

/* Phase 376 W3.b/W3.d step A — upstream `rmw_take_sequence`'s shape over the
 * unchanged counting body above. A thin adapter for the same reason the service
 * paths got one: the body's partial-drain and loan-return logic is easy to
 * disturb, and only the reporting convention is changing. A count of 0 is a
 * legitimate OK here — an empty reader, not an error. */
rmw_ret_t subscription_take_sequence(const rmw_subscription_t* subscriber, uint8_t* buf,
                                          size_t per_msg_cap, size_t max_msgs, size_t* out_lens,
                                          size_t* taken) {
    if (taken == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    int32_t n = subscription_take_sequence_count(subscriber, buf, per_msg_cap, max_msgs, out_lens);
    if (n < 0) {
        // issue 0773 — the helper returns a COUNT (>= 0) or a negated status.
        // Testing `< 0` only works because of that negation: every
        // `nros_rmw_ret_t` value is non-negative since phase-376 W3.d step B.
        return static_cast<rmw_ret_t>(-n);
    }
    *taken = static_cast<size_t>(n);
    return NROS_RMW_RET_OK;
}

// ---------------------------------------------------------------------------
// Status events (issue 0780)
// ---------------------------------------------------------------------------
//
// POLLED, not listened for. Cyclone offers both, and for this backend polling
// is strictly better:
//
//   * `dds_get_*_status` RESETS the `*_change` counters as it reads them,
//     which is exactly `take` semantics — the event is consumed by the read,
//     so there is nothing to buffer and nothing to bound.
//   * a listener would fire on Cyclone's own worker thread, and this backend
//     has no safe context to hand that to: its `drive_io` is a sleep. That is
//     the fact that made `take_event`'s decline wrong in the first place
//     (issue 0780) — solving it with a listener would have reintroduced the
//     very problem, plus a buffer and a lock.
//
// So `*_event_init` stays NULL here and these two slots carry the surface. A
// caller polls them the way it already polls `has_data`.

namespace {

/// DDS counts are 32-bit; `rmw_liveliness_changed_status_t` is 16-bit. Saturate
/// rather than truncate: a wrapped count reads as a plausible small number and
/// is worse than a pegged one, which is visibly "at least this many".
uint16_t sat_u16(uint32_t v) {
    return v > UINT16_MAX ? UINT16_MAX : static_cast<uint16_t>(v);
}
int16_t sat_i16(int32_t v) {
    if (v > INT16_MAX) return INT16_MAX;
    if (v < INT16_MIN) return INT16_MIN;
    return static_cast<int16_t>(v);
}
/// `rmw_count_status_t::total_count_change` is UNSIGNED; DDS's is signed. These
/// counters only ever grow, so a negative is a Cyclone-side surprise rather
/// than a value to propagate — clamp at 0 and report no event.
uint32_t sat_u32(int32_t v) {
    return v < 0 ? 0u : static_cast<uint32_t>(v);
}

} // namespace

rmw_ret_t subscription_take_event(const rmw_subscription_t* subscription,
                                       rmw_event_type_t kind, rmw_event_payload_t* out,
                                       bool* taken) {
    if (out == nullptr || taken == nullptr) return NROS_RMW_RET_INVALID_ARGUMENT;
    *taken = false;
    if (subscription == nullptr || subscription->backend_data == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    SubState* state = as_state(subscription);
    if (state == nullptr || state->reader <= 0) return NROS_RMW_RET_INVALID_ARGUMENT;

    switch (kind) {
        case NROS_RMW_EVENT_LIVELINESS_CHANGED: {
            dds_liveliness_changed_status_t st{};
            if (dds_get_liveliness_changed_status(state->reader, &st) != DDS_RETCODE_OK) {
                return NROS_RMW_RET_ERROR;
            }
            if (st.alive_count_change == 0 && st.not_alive_count_change == 0) {
                return NROS_RMW_RET_OK;
            }
            out->liveliness_changed.alive_count = sat_u16(st.alive_count);
            out->liveliness_changed.not_alive_count = sat_u16(st.not_alive_count);
            out->liveliness_changed.alive_count_change = sat_i16(st.alive_count_change);
            out->liveliness_changed.not_alive_count_change = sat_i16(st.not_alive_count_change);
            *taken = true;
            return NROS_RMW_RET_OK;
        }
        case NROS_RMW_EVENT_REQUESTED_DEADLINE_MISSED: {
            dds_requested_deadline_missed_status_t st{};
            if (dds_get_requested_deadline_missed_status(state->reader, &st) != DDS_RETCODE_OK) {
                return NROS_RMW_RET_ERROR;
            }
            if (st.total_count_change <= 0) return NROS_RMW_RET_OK;
            out->count.total_count = st.total_count;
            out->count.total_count_change = sat_u32(st.total_count_change);
            *taken = true;
            return NROS_RMW_RET_OK;
        }
        case NROS_RMW_EVENT_MESSAGE_LOST: {
            dds_sample_lost_status_t st{};
            if (dds_get_sample_lost_status(state->reader, &st) != DDS_RETCODE_OK) {
                return NROS_RMW_RET_ERROR;
            }
            if (st.total_count_change <= 0) return NROS_RMW_RET_OK;
            out->count.total_count = st.total_count;
            out->count.total_count_change = sat_u32(st.total_count_change);
            *taken = true;
            return NROS_RMW_RET_OK;
        }
        // Publisher-side kinds on a subscription are a caller error, not an
        // empty poll: answering `taken = false` would let the mistake run
        // forever looking like "no events".
        case NROS_RMW_EVENT_LIVELINESS_LOST:
        case NROS_RMW_EVENT_OFFERED_DEADLINE_MISSED:
        default:
            return NROS_RMW_RET_INVALID_ARGUMENT;
    }
}

rmw_ret_t subscription_has_data(rmw_subscription_t* subscriber, bool* out_has_data) {
    // Phase 376 W3.d step A — flag out, status returned.
    if (out_has_data == nullptr) return NROS_RMW_RET_INVALID_ARGUMENT;
    if (subscriber == nullptr || subscriber->backend_data == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    // Cyclone's DATA_AVAILABLE status is edge-like for our executor use:
    // querying it as a pre-filter can clear/suppress the subsequent take
    // path while samples remain readable. This backend is poll-only, so a
    // conservative "maybe" keeps dispatch correct; try_recv_raw remains the
    // authoritative non-blocking check.
    *out_has_data = true;
    return NROS_RMW_RET_OK;
}

dds_entity_t subscription_reader(const rmw_subscription_t* subscriber) {
    if (subscriber == nullptr || subscriber->backend_data == nullptr) return 0;
    return static_cast<const SubState*>(subscriber->backend_data)->reader;
}

} // namespace nros_rmw_cyclonedds
