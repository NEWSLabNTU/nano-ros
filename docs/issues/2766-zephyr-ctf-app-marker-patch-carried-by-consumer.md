---
id: 2766
title: "Zephyr callback tracing calls sys_trace_app_marker, but the CTF event that defines it is a patch only the island carries; other users get no marker events"
status: open
type: bug
severity: low
area: [zephyr, tracing]
related: [phase-478, issue-2641]
found: 2026-11-20
---

GitHub: NEWSLabNTU/nano-ros#4008

**Home: [phase-478](../roadmap/phase-478-island-developer-experience-findings.md) I33** (found by the safety island).

## What happens

nano-ros's Zephyr callback trace sink, `nros_zephyr_trace_marker`
(`zephyr/nros_platform_zephyr_shims.c`, installed by the Zephyr sched-task
entry), calls `sys_trace_app_marker` through a weak reference; callback
tracing (`packages/core/nros-node/src/executor/callback_trace.rs`) and issue
2641's `rclc_trace_marker` / `rclcpp::trace_marker` all reach it. Stock
Zephyr has no such event. It comes from a CTF patch that the consumer carries,
not nano-ros:

- `patches/zephyr/0002-ctf-app-marker-event.patch`: the `app_marker` CTF event;
- `patches/zephyr/0001-ctf-trace-k_thread_priority_set.patch`: a CTF fix for
  `k_thread_priority_set`;
- `patches/zephyr/0003-tracing-allow-an-out-of-tree-backend.patch`: lets an
  image name its own tracing backend;

applied on every Zephyr build by the island's `scripts/apply-zephyr-patches.sh`,
because `nros setup board` rewrites `zephyr/`. Without 0002 the weak
reference is null, so the sink skips the call: any other nano-ros user with
CTF tracing on builds an image whose callback and application markers emit
nothing, with no warning. (The island's own trace code calls the symbol
directly, so its trace image fails to link without the patch.)

## Wanted

nano-ros carries these patches in `zephyr/patches/` beside the ones it
already ships (`native-sim-ipproto-ip-4.4.patch` and the others), and
`nros setup board` applies them, so a Zephyr CTF image links and emits the
marker event with no consumer patch. The island then drops its copies and
`apply-zephyr-patches.sh`. Upstreaming the event to Zephyr is a separate step.

## Acceptance

- A Zephyr image with CTF tracing on and callback tracing enabled, built on a
  fresh `nros setup board` checkout with no consumer patches, has a CTF
  stream holding the `app_marker` events (checked with babeltrace or the
  in-tree CTF reader).
- The island's patch directory is gone on its nano-ros-next branch.
