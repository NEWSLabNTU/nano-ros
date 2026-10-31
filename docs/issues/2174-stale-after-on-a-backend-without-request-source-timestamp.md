---
id: 2174
title: "A server contract stating `stale_after:` builds silently on XRCE-DDS, whose requests carry no source timestamp, and the backend descriptor has no place to say so"
status: open
type: enhancement
area: [orchestration, rmw, xrce]
found: 2026-10-31
related: [1942, phase-509]
---

## What is missing

play_launch phase 95 added the contract facts `srv.<e>.stale_after` and
`srv.<e>.idempotent` (rlm v0.2.5; `SrvContract.stale_after_ms`). The first
one states that the server drops a request older than the bound. A server can
do that only if it knows when the request was sent, which here means
`ServiceInfo.source_timestamp` (issue 1942). By the table in
`packages/core/nros-core/src/service_info.rs`, zenoh and Cyclone DDS fill
it, and XRCE-DDS fills `None`. So an image whose server runs on XRCE and whose
contract says `stale_after:` claims something the code cannot do. play_launch
charges the claim in its fault-reaction arithmetic, and nothing on the
nano-ros side says it is false. RFC-0052's contract table records that these
facts derive nothing on the image (play_launch phase 96 W4.3); this issue is
the check that was not added.

## Why it was not added with the docs

The check has to know which backends fill a request's source timestamp.
RFC-0071 forbids core (where `nros-orchestration-ir`, which holds the other
bake-time contract checks such as `blocking_calls`, lives) from naming a
backend: a backend declares what it can do in its `nros-rmw.toml`. The
descriptor has no slot for a fact like this one. `[rmw.capabilities]` maps a
capability to a backend cargo FEATURE that the umbrella forwards, so a key
with no feature behind it is not a fit. The choices:

1. A new descriptor table, e.g. `[rmw.facts] request_source_timestamp =
   true`, read by `cargo-nano-ros`'s descriptor reader and
   `check-rmw-descriptors` and exposed like `backend_declares_capability`,
   with the check in `nros-cli-core`, which may consult the resolver.
2. A check in `nros-cli-core` that names `xrce` directly. This is smaller,
   and against RFC-0071's direction.

Then decide whether it is a warning (the `[safety]` + backend precedent in
`planner::check_plan_file`, closest because it is a declared property the
backend silently cannot carry) or a refusal (`validate_tier_platform_applicability`,
knobs the target cannot honour). The proposal is a warning, reported where
`blocking_calls::check_model`'s notes are reported on the three bake roads,
using the server node's `execution.deploy.<fqn>.rmw` and otherwise the
image's resolved RMW (only codegen-system knows the latter today).
