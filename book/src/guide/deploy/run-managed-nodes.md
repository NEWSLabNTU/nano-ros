# Run managed nodes

<!-- type: how-to · level: 3 -->

**Needs first:** [Parameters and launch files](../../tutorial/parameters-and-launch-files.md#set-it-in-the-launch-file)

Use a managed node when something outside the node decides when it starts
work: a supervisor, a test, or an operator running `ros2 lifecycle set`.

A managed node is ROS 2's lifecycle node (REP-2002). It has a state machine
(`unconfigured`, `inactive`, `active`, `finalized`) and five services that
read and drive it. nano-ros follows stock ROS 2 here:

- Each managed node has its own machine, under the node's own name:
  `/sensor_node/get_state`, `/sensor_node/change_state` and the rest.
- `ros2 lifecycle nodes` lists exactly the image's managed nodes.
- A node that is not managed has no machine. It publishes from boot, as a
  plain `rclcpp::Node` does.

## Which nodes are managed

A node is managed when one of these says so, and in no other case:

| Where | What you write |
| --- | --- |
| The launch file | `<lifecycle_node>` instead of `<node>` |
| The node's contract | `lifecycle: true` |
| The node's C++ code | a class deriving from `rclcpp_lifecycle::LifecycleNode` |

The order the image builds its nodes in decides nothing. A node with none of
the three stays plain, whether or not the bringup has a launch file.

A node marked in the launch file needs no code of its own for this: its
machine runs default transition callbacks, which all succeed.

## Before you start

- A workspace with a launch file, as at the end of
  [Parameters and launch files](../../tutorial/parameters-and-launch-files.md).
- ROS 2 sourced on the host, for the `ros2 lifecycle` verbs.

## Steps

```tree
sensor_ws/
└── src/
    └── sensor_node_bringup/
        ├── launch/
        │   └── system.launch.xml  (changed)
        └── system.toml            (changed)
```

1. Mark the node managed in the launch file. Change the talker's `<node>` to
   `<lifecycle_node>`; nothing else in the element changes:

   ```xml file=src/sensor_node_bringup/launch/system.launch.xml
   <?xml version="1.0"?>
   <launch>
     <arg name="topic" default="/sensor/reading"/>

     <lifecycle_node pkg="sensor_node" exec="sensor_node" name="sensor_node">
       <param name="period_ms" value="500"/>
       <remap from="/reading" to="$(var topic)"/>
     </lifecycle_node>
     <node pkg="monitor_node" exec="monitor_node" name="monitor_node">
       <remap from="/reading" to="$(var topic)"/>
     </node>
     <node pkg="server_node" exec="server_node" name="server_node"/>
     <node pkg="client_node" exec="client_node" name="client_node"/>
     <node pkg="action_server_node" exec="action_server_node" name="action_server_node"/>
     <node pkg="action_client_node" exec="action_client_node" name="action_client_node"/>
   </launch>
   ```

2. Say in the bringup's `system.toml` which state managed nodes boot to.
   Add a `[lifecycle]` block; its presence also builds the lifecycle services
   into the image:

   ```toml
   [lifecycle]
   autostart = "active"
   ```

   | `autostart` | The node boots to | Transitions run at boot |
   | --- | --- | --- |
   | `"none"` | `unconfigured` | none |
   | `"configure"` | `inactive` | configure |
   | `"active"` | `active` | configure, then activate |

3. Build and run the image as before.

## What ROS 2 sees

The stock `ros2 lifecycle` verbs work against the image as they do against
an rclcpp system:

- `ros2 lifecycle nodes` lists `/sensor_node` only; the other five nodes are
  plain.
- `ros2 lifecycle get /sensor_node` answers `active`, as `autostart` asked.
- `ros2 lifecycle set /sensor_node deactivate` moves it to `inactive`, and
  `ros2 lifecycle list /sensor_node` names the transitions open from there.

## What publishers send in each state

A managed node's publishers send only while the node is `active`. In
`unconfigured`, `inactive` and `finalized`, `publish` returns success and
the message is dropped. Each publisher prints one line, for its first
dropped message:

```text
publishing on `/sensor/reading` while its node is not active: the message is dropped, and so is every message until the node is activated
```

- The publishers open as the node activates, before its `on_activate`
  runs, so a message published inside `on_activate` goes out.
- They close after `on_deactivate` runs, and stay closed when an activation
  fails.
- nano-ros opens and closes them itself, on every ROS 2 edition. A call to a
  publisher's own `on_activate()` is accepted and does nothing.
- Subscriptions, services, clients, timers and `/rosout` are not gated: they
  work in every state.

In C++, a publisher held as `rclcpp_lifecycle::LifecyclePublisher` behaves
the same way: it opens before `on_activate` runs, so a message it publishes
there goes out, and it closes again if `on_activate` does not return
`SUCCESS`.

## One boot state per image

`autostart` is one value for the whole image: every managed node boots to
it. A launch file or contract that states a different value for two nodes of
one image is refused at build time, naming both nodes. Give the image one
value, and drive any node that should differ with `ros2 lifecycle set` after
boot.

## A program that makes its own nodes managed

A hand-written program (its own `main`) can make nodes managed in its code
instead of in a launch file. The build cannot see those nodes, so it reserves
service slots for one managed node only. With some backends, XRCE among them,
the extra nodes' lifecycle services then fail to register at boot.

Say how many nodes the program makes managed:

```toml
[system]
features = ["lifecycle"]

[lifecycle]
managed_nodes = 2
```

Each managed node gets its own five lifecycle services, and the build reserves
room for all of them. `managed_nodes` needs `lifecycle` in a `features` list,
and must be at least 1.

If the services still fail to register, a C++ `rclcpp_lifecycle::LifecycleNode`
reports it: its `ok()` is false and `error_what()` names the lifecycle
services. Check `ok()` after constructing the node, as for any `rclcpp::Node`.

## Write the node's own callbacks

<!-- tabs -->

#### Rust

A Rust component made managed by the launch file runs the default
callbacks. A component cannot supply its own transition callbacks yet.

#### C

A C component made managed by the launch file runs the default callbacks. A
component cannot supply its own transition callbacks yet.

A hand-written C program binds each node to its executor with
`rclc_executor_node_init` before it creates the node's entities. The
executor refuses an entity on a node that is not bound, and the message names
that call. A generated entry binds every node for you.

#### C++

Derive from `rclcpp_lifecycle::LifecycleNode` and override the transitions
you need, with upstream's signatures. The rest default to `SUCCESS`:

```cpp
#include <rclcpp_lifecycle/lifecycle_node.hpp>

class ManagedTalker : public rclcpp_lifecycle::LifecycleNode {
  public:
    explicit ManagedTalker(rclcpp::NodeHandle h)
        : rclcpp_lifecycle::LifecycleNode(h, "managed_talker") {}

    CallbackReturn on_configure(const rclcpp_lifecycle::State&) override {
        pub_ = create_publisher<std_msgs::msg::Int32>("/chatter");
        return pub_.get() != nullptr ? CallbackReturn::SUCCESS : CallbackReturn::FAILURE;
    }

  private:
    rclcpp_lifecycle::LifecyclePublisher<std_msgs::msg::Int32>::SharedPtr pub_;
};
```

The class is managed because its code says so: its constructor registers
the lifecycle services, and it needs no `<lifecycle_node>` in the launch
file.

<!-- /tabs -->

## What a managed node does not do yet

There is no per-node `autostart`. Every managed node in an image boots to
the image's one value (above).

## See also

- [`system.toml` reference](../../reference/system-toml.md)
- [Contract format](../../reference/contract-format.md): the `lifecycle` fact
- [rclcpp compatibility](../../reference/rclcpp-compatibility.md): `rclcpp_lifecycle::LifecycleNode`
