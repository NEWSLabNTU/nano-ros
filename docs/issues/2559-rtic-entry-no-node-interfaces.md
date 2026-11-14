---
id: 2559
title: "The RTIC entry registers no parameter services and no /rosout writer, so its images opt out of the stock node interfaces"
status: open
type: limitation
severity: medium
area: [runtime, entry, rtic]
related: [phase-556, rfc-0036, rfc-0060, 1766]
found: 2026-11-14
---

**Found by** phase-556 W13 step 3, which made the stock node interfaces (the
six parameter services, `/rosout` and `/parameter_events`) the default for
every image (user ruling 2026-11-13, option (c)). The RTIC entry cannot honour
that default, so its images state `node_interfaces = "none"` (user ruling
2026-11-14, "RTIC out").

## What happens

`nros::main!` on a board whose entry framework is RTIC (board word
`rtic-mps2-an385`) splices the nodes into the framework's `#[init]` and emits
neither `apply_param_services` nor the lifecycle wiring, on either arm
(launch or self-bringup). Since issue 1766 a declaration it cannot honour is
refused at compile time rather than dropped, so with the stock default on, an
RTIC image that states nothing would not compile. The refusal now names the
switch.

The `/rosout` half has the same cause: the executor-owned writer (phase-556
W13 step 2) is created from the owned-spin and Zephyr shapes' spin, which the
RTIC run task does not reach.

## The images that opt out

- `examples/workspaces/rust/src/pubsub_bringup/system.toml`:
  `baremetal_talker_rtic`, `baremetal_listener_rtic`
- `examples/workspaces/rust/src/service_bringup/system.toml`:
  `baremetal_service_server_rtic`, `baremetal_service_client_rtic`
- `examples/workspaces/rust/src/action_bringup/system.toml`:
  `baremetal_action_server_rtic`, `baremetal_action_client_rtic`
- `packages/testing/nros-tests/bins/rtic-run-plan-e2e/src/qemu_rtic_main_e2e_bringup/system.toml`:
  `mps2-an385-baremetal`

## What closing it needs

The RTIC entry registers the parameter services after the per-node `register`
calls, as the owned-spin entry does, and pumps `/rosout` from its run task
(or a dedicated software task). Then the seven opt-outs are removed and the
RTIC row in RFC-0036's opt-out list goes.

## Interim

The opt-out is per image and documented in RFC-0036 and the phase-556 W13
progress block. An RTIC image is invisible to `ros2 param` and publishes
nothing on `/rosout`, as before the default flipped.
