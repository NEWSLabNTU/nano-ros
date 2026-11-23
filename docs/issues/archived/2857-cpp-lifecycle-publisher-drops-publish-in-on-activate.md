---
id: 2857
title: "A C++ `rclcpp_lifecycle::LifecyclePublisher` drops a message published inside `on_activate`, though the node's publishers are open by then"
status: resolved
type: bug
severity: medium
area: [cpp-api, lifecycle]
related: [phase-554, issue-2428, issue-2858]
found: 2026-11-23
---

**Home: [phase-554](../../roadmap/phase-554-per-node-lifecycle.md) W6/W9.**
Found while landing W6 (#4634); batched with W9, whose C++ cell is what proves
the fix.

## What happens

W6 gives every publisher of a managed node the node's publish gate, which the
node's lifecycle machine opens BEFORE the `on_activate` callback runs
(`LifecyclePollingNodeCtx::trigger_transition`). RFC-0128 D3 asks for that
order, because rclcpp's base `LifecycleNode::on_activate` activates the
node's entities first, and W7's acceptance publishes from `on_activate`. The
Rust and C publishers follow it.

The C++ header keeps a second flag of its own. `LifecyclePublisher<M>`
(`include/rclcpp_lifecycle/lifecycle_node.hpp`, `SimpleManagedEntity`) is
inactive until `activate_managed_entities()` runs, and the activate trampoline
calls that only AFTER the user's callback returned `Success`:

```cpp
static uint8_t tramp_activate(void* self) {
    ...
    CallbackReturn r = dispatch(n, n->cb_activate_, ..., &LifecycleEngine::on_activate);
    if (r == CallbackReturn::Success) {
        n->activate_managed_entities();
    }
```

So a `LifecyclePublisher::publish` inside `on_activate` returns without
sending: the header drops it before it reaches the runtime, whose gate is
already open. A plain `rclcpp::Publisher` created on the same node sends.

## Not measured yet

No lane runs a C++ image with a managed node (W9 adds the cell). The
behaviour is read from the header, not observed.

## Fix direction

One gate, not two. Either the trampoline activates the entities before the
callback (and deactivates them again if the transition does not land in
`active`, as the runtime gate does), or `LifecyclePublisher` stops keeping a
flag and defers to the runtime gate, which already drops and logs once. The
second removes the duplicate state; keep `on_activate()` / `on_deactivate()` /
`is_activated()` as rclcpp's surface, reading the node's state.

Acceptance: W9's C++ cell publishes from `on_activate` and a host subscriber
receives it.

## Resolution (2026-11-23)

The activate trampoline (`tramp_activate` in
`include/rclcpp_lifecycle/lifecycle_node.hpp`) activates the node's managed
entities BEFORE the user's `on_activate`, as the runtime gate does, and
deactivates them again if the hook does not return `Success`. `deactivate`,
`cleanup` and `shutdown` still close them after a hook that succeeded. That is
the first option above; `LifecyclePublisher` keeps its flag, which now follows
the node's state.

Measured on W9's C++ cell (`lifecycle_pair_interop`, case `cpp`): a host
`ros2 topic echo` receives `lc_alpha: activated`, which the example publishes
from `on_activate`, on each activation. Negative control: with the entities
activated after the hook, the same test fails and the topic carries only the
timer's counts.
