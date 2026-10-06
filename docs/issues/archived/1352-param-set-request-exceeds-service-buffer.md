---
id: 1352
title: "A set_parameters request for a node's declared parameters does not fit
  the service buffer, and is dropped with no build-time warning"
status: resolved
type: bug
area: rmw, zenoh, params, sizing
severity: high
related: [issue-1270, issue-1271, issue-1722, phase-461, phase-480]
---

## What happens

`nros_rmw_zenoh`'s service-server inbox gives every queryable the same inbox:

```rust
pub(super) struct ServiceRequestSlot {
    pub(super) data: [u8; SERVICE_BUFFER_SIZE],   // one flat size for every service
    ...
}
pub(super) struct ServiceBuffer {
    pub(super) ring: [ServiceRequestSlot; SERVICE_REQUEST_RING_DEPTH],  // 4
    ...
}
```

`SERVICE_BUFFER_SIZE` is one number for every service an image serves, and the
parameter services are services like any other. But their request sizes are a
function of how many parameters a node declares, and two of the six outgrow the
buffer well before anything reports it.

At `CONFIG_NROS_SERVICE_BUFFER_SIZE=1024` with 25 declared parameters and
35-byte names (both DERIVED from the contract by phase-446 W4, so both are the
values the image actually runs):

| service | request bytes | fits 1024 |
| --- | --- | --- |
| `get_parameters` | 1004 | yes |
| `get_parameter_types` | 1004 | yes |
| `describe_parameters` | 1004 | yes |
| `list_parameters` | 1016 | yes |
| `set_parameters` | **2408** | **no** |
| `set_parameters_atomically` | **2408** | **no** |

A `set_parameters` carrying the node's declared parameters is 2.35x the slot it
must land in. The callback sets `ServiceRequestSlot::overflow` and the request
is dropped: the caller sees a service that answers `get_parameters` and
silently ignores `set_parameters`.

Nothing fails at build time. `SERVICE_BUFFER_SIZE` is checked against nothing,
and the sizes above are never computed.

## The arithmetic

CDR, from the `rcl_interfaces` definitions and this image's capacities
(`MAX_STRING_VALUE_LEN=0`, `MAX_ARRAY_LEN=0`, `MAX_BYTE_ARRAY_LEN=0`, which
collapse every `ParameterValue` variant arm to its empty-sequence header):

```
string(n)        = 4 + n + 1
ParameterValue   = 56 B     (type, bool, int64, float64, string, 5 empty seqs)
Parameter        = 96 B     (string name + ParameterValue, 8-aligned)

get/describe/types  = 4 + 25 x 40         = 1004
list_parameters     = align8(1004) + 8    = 1016
set_parameters      = 4 + 25 x 96         = 2408
```

The four `string[] names` requests are near the limit too: at 35-byte names
they need 1004 of 1024, so a name one byte longer, or a 26th parameter, drops
those as well. The buffer is not comfortably sized for any of them; two are
simply already past it.

## Why it also costs a lot of RAM

The same flat sizing runs the other way for the four that DO fit. Every
queryable gets `4 x 1024` of ring whatever it carries, and since issue 1270 the
parameter family is 6 queryables per node.

Measured on Autoware Safety Island (4 nodes, 26 queryables of which 24 are
parameter services), MR-CANHUBK344, `CONFIG_SRAM_SIZE=320 KiB`:

```
.bss  nros_rmw_zenoh::shim::service::SE...   115,128 B   (26 x 4,428, exact)
per slot                                       4,428 B   = 4 x 1024 + 332

image with the parameter services   RAM overflowed by 61,608 B  (link fails)
image without them (MAX_QUERYABLES=2)  RAM 281,192 B / 320 KB, 85.81%, links
measured cost of the 24 slots                108,096 B
```

against 8,844 B of actual per-node request payload summed over all six
services. The image does not fit its board because of inbox it cannot use.

Ring depth is most of that: `SERVICE_REQUEST_RING_DEPTH = 4` is documented for
"a burst of queries delivered in one read-task batch -- concurrent goals under
load". Parameter services are configuration traffic, not action goals.

## What would fix both

Size the inbox per service from the request type and the store's capacities,
the way phase-446 F3 already sizes `PARAM_SERVICE_BUFFER_SIZE` from the
contract's `params:`, and let the parameter family's ring depth differ from the
action path's. Sized per type at depth 1 the same four nodes need 43,344 B
rather than 106,272 B -- 62,928 B less, which is more than the overflow above --
and `set_parameters` gets a 2,408-byte slot instead of being dropped.

Sizing per type at depth 4 would be WORSE than today (37,336 B per node), so
the depth is the half that makes it pay; doing either alone misses.

## Where the fix is planned

[phase-461](../../roadmap/phase-461-service-inbox-per-family.md) owns this issue:
per-family inboxes (W1), the parameter family sized by phase-446 F3's request
bound at depth 1 in nros-node (W2), a build-time assert that the slot holds
the largest declared `set_parameters` (W2), and a counted, once-logged inbox
drop (W4). One correction from that planning: the 25-parameter figures above
are the executor's store capacity (`NROS_MAX_PARAMETERS`), not any node's
declaration -- the island's worst node declares 8 and its largest well-formed
`set_parameters` is 669 B, which fits 1,024. The 2,408 B request is a client
naming 25 parameters at a node that declares 8; it is still dropped silently,
which is the defect, but `ros2 param load` of the node's own file is not the
trigger. The RAM half is unchanged and is the board blocker.

## Reproducing

Declare more than ~10 parameters on a node in the contract's `params:`, build
for any target, and call `set_parameters` with the full set. The request is
dropped. Raising `CONFIG_NROS_SERVICE_BUFFER_SIZE` to 2408 fixes the drop and
costs every OTHER queryable in the image the same increase, which is the
trade this issue exists to remove.

## Resolution

Resolved 2026-10-06 (phase-480 W5), on top of phase-461 W1-W3, which gave the
parameter family its own zenoh inbox, derived from the contract's declared
parameters.

**Measured on main before this change.** The image was the `features`
workspace's `native_rust_params` (native, zenoh). The client was a stock humble
`ros2 service call /param_talker/set_parameters` naming 25 parameters with
35-byte names, about 2,411 B. "Lands" means a reply came back with 25
`SetParametersResult`s. "Dropped" means no reply at all, and the CLI had to be
`SIGKILL`ed because it ignores `SIGTERM` while it waits.

| what the contract declares | builtin slot | result |
| --- | ---: | --- |
| 25 integers in `params:` | 2,444 B | lands |
| 25, one of them a string | 1,024 B | dropped |
| no `params:` (no descriptor) | user ring, 1,024 B | dropped |

Every drop logged "larger than the 4096-byte (or 9176-byte) request buffer.
Raise NROS_PARAM_SERVICE_BUFFER_SIZE". That buffer is the executor's, and it was
never the limit, so raising that knob did nothing.

**The fix, as one class:** the parameter family's request against every buffer
it crosses on its way in.

1. **Declared parameters it cannot price.** With a string or array parameter,
   `nros-rmw-zenoh` cannot price the request: the store's caps belong to
   nros-params. It used to fall back to the user-service slot, which phase-461
   W3 had unfloored to the user services' own demand. It now REFUSES the build
   and names `NROS_PARAM_SERVICE_INBOX_BYTES`. nros-node's existing const assert
   then checks that statement against the request nros-node prices. Measured:
   a statement of 1,024 is refused by nros-node, 4,096 builds, and the 25-name
   call lands.
2. **Nothing declared.** The builtin fallback is now `max(user slot, 1024)`,
   the floor the single table always had. It is never the unfloored user demand
   (one declared `AddTwoInts` server put that at about 20 B).
3. **A priced slot with no builtin table.** The table exists only when the
   application's queryable count is known. Without it, the parameter services
   draw user-service rings, so a priced or stated builtin slot now raises those
   rings, with a warning that names the remedy. Measured before:
   `BUILTIN_INBOX_BYTES` 4096 was stated and a 1,024 B user ring still dropped
   the request.

The rule is `nros-rmw-zenoh/build_param_slot.rs`. It is included by `build.rs`
and by `tests/builtin_inbox_slot.rs` (6 tests), because a build script's own
tests never run. Negative control: putting back the pre-fix rule (unfloored
user slot, and abstaining to it) fails 2 of the 6 tests.

**The diagnostic.** In nros-node, `MessageTooLarge` (the transport's inbox
dropped the request on arrival) is now its own kind, `InboxOverflow`, separate
from `BufferTooSmall` (the executor's buffer). Its line names the transport
knobs first, because nros-log truncates a line at its buffer, and the first
draft was cut off before reaching them. Live, on the undeclared image:

```
param service /param_talker/set_parameters: request dropped by the TRANSPORT
inbox (not the 4096 B executor buffer). zenoh: NROS_PARAM_SERVICE_INBOX_BYTES,
or NROS_SERVICE_INBOX_BYTES with no declared endpoints; or declare `params:`
(issue 1352)
```

With `NROS_SERVICE_INBOX_BYTES=4096` set, the same call lands. Unit test:
`an_inbox_drop_and_a_buffer_overflow_are_reported_as_different_kinds`. It fails
if `MessageTooLarge` is classified as `RequestTooLarge` again.

Gates: `just check fast`, `just check test-targets` on Rust 1.99.0, and tier 2
`just ci matrix build` (which ended with "L3 passed — 3 cross ELF(s) checked").

**Not covered, and not measured:**

- XRCE. Its `XRCE_SERVICE_REQUEST_BUFFER_SIZE` is not derived from the
  parameter shape at all. Filed as
  [1722-xrce-param-request-buffer-not-derived.md](../1722-xrce-param-request-buffer-not-derived.md).
- A builtin table that is too SMALL. When the application declares more
  queryables than `ZPICO_MAX_QUERYABLES` leaves room for, some builtin services
  take a spare user ring at the user-service size. zenoh does not know how many
  builtin services nros-node will create, so this case is not handled here.
- The phase-461 W4 drop counter, and the RAM half of the issue (the island
  link). Both stay with phase-461.
- A Zephyr or other embedded image. Every live measurement above is native.
