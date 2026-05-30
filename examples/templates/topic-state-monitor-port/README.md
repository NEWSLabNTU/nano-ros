# topic_state_monitor — port template (Phase 209.G, first iteration)

Synthetic in-tree port modeled on Autoware's `topic_state_monitor` (system/),
the survey's smallest Tier-1 candidate (465 SLOC upstream — a multi-topic
liveness watchdog publishing `diagnostic_msgs/DiagnosticArray`). This iteration
**does not vendor upstream source** (separate session — needs an Autoware
checkout + `autoware_*` msg codegen); the .cpp here exercises the same compat
surface a real port hits beyond the MVP smoke (multiple subs in one Node,
per-topic diagnostic tasks, `Updater::update()` from the spin loop).

## Build + run

```bash
cd examples/templates/topic-state-monitor-port
cmake -B build -S . -DNROS_RMW=zenoh
cmake --build build -j

zenohd -l tcp/127.0.0.1:7447 &
./build/topic_state_monitor &
# Publish to topics "a" and "b" from any nano-ros / ROS 2 talker.
# `topic_state_monitor` publishes `/diagnostics` every 1 s with per-topic
# OK / WARN / ERROR based on the last-seen age.
```

## What this commit validates

- Multiple `create_subscription<M>` calls in one Node (was: smoke had one).
- Per-topic `diagnostic_updater::Updater::add(name, cb)` registrations.
- `Updater::update()` driven from the main loop — rate-limited self-publish.
- `RCLCPP_INFO` + `RCLCPP_WARN`/`ERROR` macros over the compat surface.
- Full stock-ROS-2 `find_package(...)` + `ament_auto_add_executable` +
  `ament_target_dependencies` shape on top of the compat module.

## Gap surfaced (filed against Phase 209.A)

`nros::Node::create_subscription`'s **callback overload is SFINAE-restricted
to plain `void(*)(const M&)` function pointers** (`std::enable_if<
std::is_convertible<F, void(*)(const M&)>::value>`). rclcpp accepts capturing
lambdas / `std::function`. A direct line-for-line port of upstream
`topic_state_monitor` — which captures `this` in the subscription callback
to update its members — **does not compile** as-is.

Workaround used here: hoist per-subscription state into globals + stateless
function-pointer callbacks. The upstream port would need the same rewrite or
poll via `try_recv_raw` in the spin loop.

**Right fix (209.A follow-up):** the compat header's `create_subscription`
allocates a heap-stored `std::function<void(const M&)>` per subscription and
delivers via a fn-pointer trampoline + the nros FFI's user-data slot. That
restores upstream-source compile-unchanged for the (very common) capturing-
lambda subscription pattern.

## Next iteration (real upstream port)

1. Vendor the upstream `topic_state_monitor/` (Apache 2.0) under `vendor/`.
2. Generate the Autoware message deps via `nros generate cpp <pkg>` per package
   (or 209.E's bulk form once shipped) — `tf2_msgs`, `autoware_*` deps the
   upstream source includes.
3. Land the 209.A `std::function`-subscription follow-up so the upstream
   captured-lambda callbacks compile unchanged.
4. `cmake --build` + boot under `native_sim` (Zephyr) + verify the diagnostics
   message stream matches the upstream behaviour.
5. Distill the workflow into a book page (`book/src/getting-started/
   port-a-ros2-node.md`).
