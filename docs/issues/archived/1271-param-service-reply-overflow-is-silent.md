---
id: 1271
title: "A parameter service request or reply that does not fit is dropped
  silently: no reply, no log, and the client only times out"
status: resolved
type: bug
area: core
severity: medium
related: [issue-1270, phase-446, phase-461]
---

## What happens

The parameter services bound their wire messages with fixed caps -- 64
sequence elements and 256-byte strings (`nros-node/src/parameter_services.rs`)
-- and stream each reply into the fixed `PARAM_SERVICE_BUFFER_SIZE` reply
buffer (`nros-rmw/src/traits.rs`). What happens past each cap:

| case | behaviour |
| --- | --- |
| request with more than 64 names, or a string over 256 B | deserialize error, no reply |
| `list_parameters` matching more than 64 names | cut to 64, silently; extra prefixes dropped silently |
| stored value too large for the wire (e.g. a byte array over 64) | `get` answers NOT_SET |
| string over 256 B in a reply | sent as `""`, not truncated |
| reply larger than the buffer | serialize error, `ServiceReplyFailed`, no reply |

The last row is the one that hides: the spin discards the error
(`unwrap_or(0)` and `if let Ok` in `nros-node/src/executor/spin.rs`), so the
ROS 2 client waits out its timeout and nothing on the image says why.

A rough estimate, not measured: `describe_parameters` for about 40 parameters
exceeds the 4,096 B default.

## Why it matters

These are the cases a user reaches first on a real node: `ros2 param dump`
describes everything, and Autoware nodes declare dozens of parameters. A
refusal they can read ("reply too large for PARAM_SERVICE_BUFFER_SIZE") is the
difference between raising a knob and filing a bug about discovery.

## Fix shape

- Log each overflow once per service, naming the cap and the knob.
- Where the service has an error field (`SetParametersResult.reason`), answer
  with it instead of dropping the request.
- Flag truncation in `list_parameters` (and document it) rather than cutting
  the list without a trace.
- Size the reply buffer from the declared parameters (issue 1270) so the
  default case does not overflow at all.

## Evidence

Found by reading; none of the rows above has been triggered on a running
image yet. A test that describes N parameters for N past the threshold would
pin the boundary.

## Progress (with issue 1270)

The first fix-shape item is done. The spin no longer discards the result:
`ParameterServiceServers::process` classifies a request that got no reply
(malformed / past a wire cap, request larger than the buffer, reply larger
than the buffer, transport) and logs it once per service and kind. The log
line names the cap and, for the buffer cases, `NROS_PARAM_SERVICE_BUFFER_SIZE`.
A failure no longer ends the pass for the other services.
`a_reply_past_the_shared_buffer_is_reported_once_and_answered_once_it_fits`
pins the reply-overflow row. The reconcile that attaches services to nodes
logs its failure once too, instead of being dropped every spin (the Cyclone
case, issue 1268).

**phase-461 W4 (2026-10-13).** The transport-inbox case now names the
REQUEST'S LENGTH as well as `NROS_PARAM_SERVICE_INBOX_BYTES`: the zenoh shim
keeps an overflowing request's length in its ring entry and reports it through
`ServiceTrait::refused_request_len`, and the `InboxOverflow` arm reads it.
Below the parameter services, the shim now counts every inbox drop per
queryable (ring full, oversized) and says each episode once
(`ZenohServiceServer::inbox_drop_stats`).

Still open: answering with `SetParametersResult.reason` instead of dropping,
flagging `list_parameters` truncation, and sizing the reply buffer from the
declared parameters (phase-446 W4).

## Resolution (2026-10-13)

The three open items are done. Phase-446 F3 had already derived the reply
buffer from the declared parameters. This change does the other two.

- **A set request past a wire cap is answered, not dropped.**
  - `set_parameters` gives such a parameter its own
    `successful = false` result, with `CAPACITY_EXCEEDED_REASON`, and still
    applies and answers the others. "Past a cap" means a value whose string
    or array is past the message's 256 B / 64 elements, or a name past 256 B.
  - The handler reads the whole value regardless, so the next parameter
    starts in the right place.
  - A request with more than 64 parameters is answered whole.
  - `set_parameters_atomically` refuses such a batch with its reason and
    applies none of it.
  - The two set handlers never needed the generated message's storage: they
    borrow each name and stage one value at a time.
  - The name-list requests (get, describe, types, and list's prefixes) keep
    the cap. Their replies have no field that could carry a reason, so the
    once-per-service log line stays the report there.
- **A cut `list_parameters` is reported.** The reply is still sent, cut to
  64 names and 64 prefixes, and the node logs one warning per service
  (`ParamServiceFailure::ListTruncated`). `ListParametersResult` has no field
  for it. The book's parameter section now states both bounds.

Tests (`parameter_services::tests`), each failing on the previous code:

- `set_parameters_answers_an_over_cap_parameter_and_applies_the_rest`: a
  65-element byte array and a 300-byte name between two good parameters.
- `set_parameters_past_64_is_answered_whole`: 70 parameters.
- `set_parameters_atomically_refuses_an_over_cap_batch_with_a_reason`
- `a_truncated_list_is_answered_and_reported_once`: a 70-slot store, two
  requests, one report.
