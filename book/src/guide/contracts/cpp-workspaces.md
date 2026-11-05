# C / C++ multi-node workspaces

<!-- type: how-to · level: 2 -->

This page now holds only the contract sections of the C and C++ workspace
walkthrough. The workspace layout, the node and bringup packages, the image
and the entry it generates moved to
[Organize a larger workspace](../build/organize-a-larger-workspace.md) and
[Mix languages](../build/mix-languages.md).

The reference C++ workspace is
[`examples/workspaces/cpp/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces/cpp).

## Contracts for a C or C++ workspace

### Telling the build what the component creates

`NROS_EXECUTOR_MAX_CBS`, the subscriber slots and the arena are `const` sizes
compiled into nros-node before a component TU is compiled, while a typed
component wires itself in its CONSTRUCTOR, at runtime. Nothing the macros emit
can supply those sizes — it exists only after linking. So somebody has to state
what the image creates, ahead of the build.

That statement is a **contract sidecar** beside the launch file, not something
in this CMakeLists. `<stem>.contract.yaml` next to `<stem>.launch.xml` in the
bringup package:

```yaml
# src/demo_bringup/launch/system.contract.yaml
version: 1

nodes:
  talker:
    paths:
      on_timer:                                  # a path with a timer trigger
        trigger: { timer: { rate_hz: 1 } }       # IS the periodic callback
        output: [chatter]
    pub:
      chatter: {}
  listener:
    sub:
      chatter: {}

topics:
  /chatter:
    type: std_msgs/msg/Int32
    pub: [talker/chatter]
    sub: [listener/chatter]
```

`nodes:` keys are LOCAL endpoint names — an absolute path there is concatenated
onto the node's FQN and matches no topic. `topics:`, `services:` and `actions:`
wire those endpoints to absolute names and carry the type. `paths:` is where
timers live: the model records a path with no `input` as the periodic callback.

The build folds it into the resolved SystemModel and sizes the image's pools
from it; nothing in the CMakeLists takes an argument for it. The tasks that
build on this sidecar have their own pages, for every language:

- [Declare QoS in a contract](declare-qos-in-a-contract.md)
- [Size an image from its contract](size-an-image-from-its-contract.md)
- [Contracts for a bringup with no launch file](contracts-for-a-bringup-with-no-launch-file.md)
- [Fix a contract refusal](fix-a-contract-refusal.md)

## Design background

- [RFC-0122](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0122-behaviour-bringups-road-witnesses-cadence-backed-ci.md): behaviour bringups, single-node witnesses, and cadence-backed CI.
  The paired examples as image rows of behaviour bringups in the language
  workspaces.
