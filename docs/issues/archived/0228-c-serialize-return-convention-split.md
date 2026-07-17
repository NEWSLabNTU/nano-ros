---
id: 228
title: "generated C serialize conventions split: messages return 0/-1 + out-param, services return byte-count/negative — same concept, two ABIs"
status: resolved
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

## Resolution (2026-07-17, phase-294)

Converged services AND actions (the split was wider than filed — the action
Goal/Result/Feedback emitters shared the count-return shape) onto the
message convention: `(msg, buffer, buffer_size, size_t* serialized_size) ->
0/-1`. Five template emitters converted; the signature gains a parameter so
stale callers break loudly at compile time — no deprecation alias (all
consumers in-tree, source distribution young).

All 28 consumer files migrated (scripted with per-file verification; rc
guards replace both `< 0` and value-positive patterns). Verified: native C
service pair 5+7=12 and action pair full-Fibonacci round-trip e2e on
regenerated typesupport; all five platforms' fixture families rebuilt;
`test_rtos_service_e2e` + `test_rtos_action_e2e` + native service lanes
37/37 PASS. Phase doc: docs/roadmap/archived/phase-294-*.md.
