---
id: 778
title: "A client cannot learn the id of the request it just sent, so two
  in-flight calls cannot be told apart — and each backend papers over it
  differently"
status: open
type: bug
area: rmw
related: [phase-376, issue-0745]
---

## Problem

Upstream's `rmw_send_request(client, request, int64_t *sequence_id)` hands the
caller the id it assigned. Our slot drops that out-parameter:

```c
rmw_ret_t (*send_request)(rmw_client_t *client, const uint8_t *data, size_t len);
```

Every backend COMPUTES a sequence and then throws it away:

* cyclonedds builds a `RequestId{guid, seq}` (`service.cpp:1123-1132`)
* zenoh does `request_seq.fetch_add(1, Ordering::Relaxed) + 1` into the rmw
  attachment (`shim/service.rs:731`)
* xrce reads `uxr_buffer_request`'s id only to test it for validity
  (`service.c:409`)

Our own ABI has the vocabulary on the SERVER side — `take_request` carries
`int64_t *seq_out`, and `send_response` takes the sequence back — and withdraws
it on the client side. The asymmetry is the tell: the id exists, it is just not
returned to the one caller who needs it to correlate.

## Why this is a correctness gap, not an ergonomic one

With no id, a client with two calls outstanding cannot match a reply to a
request. Both backends therefore invented a policy, and neither is safe in
general:

* **cyclonedds ABANDONS the first request when a second is sent**
  (`service.cpp:1115-1120`): "Mirror that abandon here so a slow first request
  doesn't wedge every later call". A slow first call is silently discarded.
* **zenoh takes FIRST REPLY WINS**, justified in a comment as "queryable is
  idempotent at the application layer" (`shim/service.rs:859-864`). The ABI
  cannot enforce that, and it is false for the calls this path actually carries:
  an action's `send_goal` is not idempotent, and neither is `SetParameters`.

So the failure is silent misdelivery or a silent drop, chosen per backend,
depending on which one an image links. That is worse than either policy on its
own, because the same application code behaves differently on different
transports.

## Adjacent, same slot family

The `int64_t` in `take_request` / `send_response` is **not a sequence number on
cyclonedds** — it is an index into a 32-entry table (`service.cpp:869-895`),
released only by `send_reply` (`:988`). A request taken and never answered leaks
a slot permanently. Worth fixing in the same pass, since it is the same
identifier crossing the same seam with two different meanings.

## Direction

Restore upstream's out-parameter:

```c
rmw_ret_t (*send_request)(rmw_client_t *client, const uint8_t *data, size_t len,
                          int64_t *sequence_id);
```

That is exact parity with upstream (modulo the bytes-not-typesupport deviation
this ABI already declares everywhere), and it lets `take_response` report which
request a reply belongs to — which is the actual fix, since the client also
needs the id on the way back.

Then both backend policies can be deleted rather than reasoned about: cyclone
stops abandoning, zenoh stops assuming idempotence.

## Provenance

Found by the phase-376 W5 audit, which was checking whether declared ABI
deviations were TRUE. This one was recorded as a deviation — "ours is a
fire-and-forget publish whose reply is matched by `take_response`" — and is
better described as a gap wearing a deviation's clothes: nothing matches the
reply, because there is nothing to match it BY.

## Landed 2026-08-25: the ABI carries the id, both ways

```c
rmw_ret_t (*send_request)(const rmw_client_t *client,
    const uint8_t *request, size_t req_len, int64_t *sequence_id);

rmw_ret_t (*take_response)(const rmw_client_t *client,
    uint8_t *reply_buf, size_t reply_buf_len,
    int64_t *seq_out, size_t *out_len, bool *taken);
```

Both halves, because handing the id out at send time is useless if it does not
come back. The Rust `ClientTrait` matches: `send_request_raw` returns `i64` and
`try_recv_reply_raw` returns `Option<(usize, i64)>`.

Each backend now reports the id it was already computing:

* **cyclonedds** — the `RequestId{guid, seq}` it builds, and `take_response`
  reports `got_id.seq`. The match against the pending id already existed; it
  simply had nowhere to report the answer.
* **zenoh** — the `fetch_add` it puts in the rmw attachment. `pending_handles`
  became `(handle, seq)` pairs, and **first-reply-wins is gone**: a reply
  retires only the generations sharing its sequence id, so a second in-flight
  request no longer disappears when the first one answers.
* **xrce** — `uxr_buffer_request`'s id, which was read only to test it for
  validity. `xrce_reply_callback` had the same discard one layer down
  (`(void)request_id;`) and now records it on the slot.

## Still open, and now explicit rather than silent

**cyclonedds holds ONE outstanding request.** `ClientState` has a single
`pending_seq` and a single staging buffer, so sending a second request while
the first is unanswered still abandons the first. Making it genuinely
multi-outstanding means a pending TABLE, mirroring the server side's `slots`.

What changed is that the caller can now SEE it: every send returns its id, so
the reply that arrives names the request it belongs to and the abandoned one is
identifiable rather than merely absent.

**The user-facing APIs discard the id.** Every caller in the tree keeps one
call in flight (`in_flight_flag` on the raw handle, one arena entry per client,
the blocking C/C++ paths), so each drops the id at a named site with a comment
rather than by omission. Exposing it is a follow-up; the ABI no longer prevents
it.

**The adjacent finding in this issue is untouched:** `take_request` /
`send_response`'s `int64_t` is a slot INDEX on cyclonedds, not a sequence
number, and a request taken but never answered leaks a slot permanently.

