---
id: 1271
title: "A parameter service request or reply that does not fit is dropped
  silently: no reply, no log, and the client only times out"
status: open
type: bug
area: core
severity: medium
related: [issue-1270]
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
