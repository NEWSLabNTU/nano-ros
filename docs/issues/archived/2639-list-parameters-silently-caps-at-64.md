---
id: 2639
title: "`list_parameters` answers at most 64 names and says nothing, so `ros2 param list` and `ros2 param dump` miss the rest"
status: resolved
type: bug
area: [params, rmw]
severity: high
found: 2026-11-16
resolved: 2026-11-16
resolved_in: "the change that filed it"
related: [phase-478, issue-1271]
---

**Home: [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md), I20.**
Reported by the Autoware Safety Island (phase-15 P3) on nano-ros 5b9e8844a4.

## What happened

`ros2 param list /controller` showed 64 of the controller's 149 parameters,
and `ros2 param dump` the same 64. The streaming `list_parameters` handler
stopped at `WIRE_SEQ_CAP` (64, the capacity of the GENERATED
`rcl_interfaces` message) and only logged the cut on the device (issue 1271):
`ListParametersResult` has no field for it, so the client saw a complete
looking answer. `ros2 param dump` then sends every listed name in ONE
`get_parameters`, and the same cap refused a request naming more than 64.

The selection was also not rclcpp's: a prefix matched any string start
(`robot` selected `robotics`), and `depth` counted every separator with `<=`.

## Resolution (2026-11-16)

- `list_parameters` answers EVERY selected name. The handler streams: names
  are written straight out of the store and each prefix by the first name
  that carries it, so no reply-side storage bounds the count. The cut and its
  log line are gone.
- The get, describe and get-types handlers take a name list as long as the
  store (`NAME_LIST_CAP`, the larger of 64 and `MAX_PARAMETERS`); they stream
  too, so the message's 64 was never theirs.
- With no `params:` declaration to derive from, the configured buffer size
  (`NROS_PARAM_SERVICE_BUFFER_SIZE`, 4096) is a FLOOR under what a full store
  needs: the listing of `MAX_PARAMETERS` names at the longest name, and the
  `get_parameters` of all of them holding scalars. The inbox slot keeps the
  same floor under the request naming every one. A stated size still wins,
  and a reply that does not fit is still dropped whole and reported
  (`ReplyTooLarge`), never shortened.
- The selection is rclcpp's (`NodeParameters::list_parameters`): a prefix
  matches itself or itself followed by `.`, and `depth` counts the
  separators after it, exclusively; `0` is recursive.
