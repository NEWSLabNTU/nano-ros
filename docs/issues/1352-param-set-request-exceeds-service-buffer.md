---
id: 1352
title: "A set_parameters request for a node's declared parameters does not fit
  the service buffer, and is dropped with no build-time warning"
status: open
type: bug
area: rmw, zenoh, params, sizing
severity: high
related: [issue-1270, issue-1271]
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

[phase-461](../roadmap/phase-461-service-inbox-per-family.md) owns this issue:
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
