---
id: 418
title: "Raw action feedback/result payloads carry an extra CDR header, so they are wire-incompatible with ROS 2 and with nano-ros's own typed path"
status: resolved
type: bug
area: rmw
related: [phase-338, rfc-0069, issue-0035, issue-0433]
resolved_in: RFC-0069 option A (0403a8b53 + consumer sniff deletion)
---

Raw action feedback/result carried a SECOND CDR encapsulation header inside the
envelope, so raw↔raw was self-consistent but raw↔{ROS 2, nano-ros typed} corrupted
on decode. RFC-0069 (Accepted, option A) removed it: the producer
(`nros/src/node.rs` `complete_goal`/`publish_feedback`) writes the body with a
header-less `CdrWriter::new`, matching ROS 2's single envelope.

Consumer follow-through: the `nros-node` executor's `payload_has_cdr_encap` sniff
was a second instance of #35 (a leading `int32` of 256 is byte-for-byte the LE
encap header), so it now splices unconditionally (`read_action_field` + the
feedback + result paths). The C/C++/ffi audit is clean: `nros-c` frames
deterministically (strip + prepend, no value sniff), and the `nros-cpp` trampoline
sniff is harmless because the upstream C ABI always delivers `[encap][fields]`.

Verified 2026-08-05: `action_envelope_tests` (3 cases) pass; a native TYPED
action-client ↔ Node-class action-server pair decodes result `[0, 1, 1]` with no
`DeserializationError`/`ServiceRequestFailed`; `ros2 action send_goal --feedback`
against the Node-class server returns feedback, result and `SUCCEEDED` (humble +
rmw_zenoh_cpp).

Wire-format change: v0.5↔v0.6 nano-ros action pairs are incompatible on
feedback/result (accepted — the ROS 2 compatibility is the product). 14 of 18
action Runtime cells re-verified green (raw↔raw); the remaining 4 are BLOCKED by
build defects that PREDATE this change, not by the envelope: freertos c/cpp on the
sizes-header stub (`nros_config_generated.h`, the 0268 class) and nuttx c/rust on
issue 0433 (kernel re-staged after the entries link). Those blockers are tracked
under 0433 + the runtime-E2E umbrella 0422, not here.
