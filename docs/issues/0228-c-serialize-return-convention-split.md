---
id: 228
title: "generated C serialize conventions split: messages return 0/-1 + out-param, services return byte-count/negative — same concept, two ABIs"
status: open
type: tech-debt
area: codegen
related: []
---

## Finding (deep audit 2026-07-17, D)

`packages/cli/rosidl-codegen/templates/message_c.h.jinja:81` emits
`int32_t <msg>_serialize(msg, buf, cap, size_t* serialized_size)` (0/-1 +
out-param) while `service_c.h.jinja:47` emits Request/Response `_serialize`
returning the byte count directly (negative = error). Same concept, two
incompatible return conventions — every consumer that handles both must
special-case, and porting code between msg and srv silently miscompiles the
error handling.

## Fix sketch

Pick the message convention (matches the C++ emitter), emit the service one
as a deprecated alias for one release, migrate in-tree consumers
(hand-rolled AddTwoInts CDR paths in examples are already generated-free
after #218's scope — check the srv consumers in zephyr/nuttx service
examples).
