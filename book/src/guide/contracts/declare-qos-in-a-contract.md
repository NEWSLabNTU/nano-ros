# Declare QoS in a contract

<!-- type: how-to · level: 2 -->

**Needs first:** [the contract walkthrough](cpp-workspaces.md#telling-the-build-what-the-component-creates) ·
[Project layout](../../tutorial/grow-your-workspace.md)

Declare an endpoint's QoS in its contract when you want the image to reserve
memory for the history it keeps, rather than for the backend's default.

QoS itself is the ROS 2 idea you already know: see
[About Quality of Service settings](https://docs.ros.org/en/rolling/Concepts/Intermediate/About-Quality-of-Service-Settings.html).
What differs in nano-ros is that every queue is static. The build sizes the
receive rings, the executor arena and the transient-local retention from what
the contract declares, before any of your code is compiled. A QoS you pass in
code and never declare is a QoS the image did not reserve for.

## Before you start

- A bringup that builds and runs, with a contract beside its launch file
  (`<bringup>/launch/<stem>.contract.yaml`), or beside `system.toml` for a
  bringup with no `launch/` directory
  ([Contracts for a bringup with no launch file](contracts-for-a-bringup-with-no-launch-file.md)).
- The endpoint is already declared under `nodes:` and wired under `topics:`.
- The call site registers the endpoint on an **absolute** name (`"/chatter"`,
  not `"chatter"`). A contract states resolved names, and the build will not
  match a relative one.

## Steps

1. **Add a `qos:` block to the endpoint.** Edit the endpoint's row under
   `nodes.<node>.pub` or `nodes.<node>.sub`. The keys are those of the contract
   format's `qos` section: `reliability`, `durability`, `history`, `depth`,
   `lifespan`, `liveliness`, `deadline`, `lease_duration`.

   ```yaml
   nodes:
     qos_listener:
       sub:
         qos_chatter:
           qos:
             history: keep_last
             depth: 5
             durability: transient_local
   ```

   Use `history: keep_last` with the `depth:` you mean. `keep_all` has no
   static bound, so the build cannot size a buffer from it.

   A `qos:` block under `topics.<name>` sets the topic's QoS for every endpoint
   on it; an endpoint's own block overrides it.

2. **Make the call site state the same QoS.** The contract says what the build
   reserved; the code says what the endpoint asks for. Each language holds the
   two together differently.

<!-- tabs -->

#### Rust

Pass the profile to the `_with_qos` registration. Rust checks at registration:
where the code asks for a deeper queue than the contract reserved, it takes the
contract's depth and logs a warning; where the code asks for a shallower one, it
refuses the registration. From
`rust_qos_talker_pkg/src/lib.rs`:

```rust
pub fn qos_profile() -> QoSProfile {
    QoSProfile::default().reliable().transient_local().depth(5)
}

let pub_chatter =
    node.create_publisher_for_topic_with_qos::<Int32>("/qos_chatter", qos_profile())?;
```

#### C

Build an `rclc_qos_t` and pass it to `rclc_subscription_init_with_qos` (or
`rclc_publisher_init_with_qos`). At registration, a C subscription whose depth,
reliability or durability differs from the contract is refused. From
`c_qos_listener_pkg/src/QosListener.c`, whose contract row would state
`depth: 1`:

```c
static rclc_qos_t qos_profile(void) {
    rclc_qos_t q = RCLC_QOS_DEFAULT;
    q.reliability = RCLC_QOS_RELIABILITY_RELIABLE;
    q.durability = RCLC_QOS_DURABILITY_TRANSIENT_LOCAL;
    q.history = RCLC_QOS_HISTORY_KEEP_LAST;
    q.depth = 1;
    return q;
}

const rclc_qos_t qos = qos_profile();
self->sub = rcl_get_zero_initialized_subscription();
rcl_ret_t rc = rclc_subscription_init_with_qos(
    &self->sub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32), "/chatter", &qos);
```

#### C++

Make the QoS a `constexpr` and assert it against the contract beside the call.
A disagreement then fails to compile, naming the topic and both values.
`NROS_ASSERT_DECLARED_PUB_QOS` does the same for a publisher. From
`examples/workspaces/cpp/src/listener_pkg/src/Listener.cpp`, whose contract row
is `chatter: { qos: { depth: 1 } }`:

```cpp
constexpr ::rclcpp::QoS kChatterQos = ::rclcpp::QoS(1);
NROS_ASSERT_DECLARED_QOS(::std_msgs::msg::String::TYPE_NAME, "/chatter", kChatterQos,
                         "\"/chatter\"");
::rclcpp::Result r =
    ::rclcpp::bind_subscription<::std_msgs::msg::String, Listener, &Listener::on_msg>(
        node, "/chatter", this, kChatterQos);
```

<!-- /tabs -->

3. **Rebuild the image.**

   ```bash
   nros build linux_rust_qos
   ```

   Among the stage lines, the resolve line names the file the build sized the
   image from:

   ```text
   nros build:   resolved → <workspace>/build/demo_bringup__linux_rust_qos/resolved.toml (max_cbs <n>, nodes <n>, digest <digest>)
   ```

   If it ends `(entity counts not derived; configured defaults kept)` instead,
   the contract did not describe the whole image: see
   [Fix a contract refusal](fix-a-contract-refusal.md).

## Example

The `features` workspace's QoS pair, in
[`examples/workspaces/features`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces/features).
A Rust talker publishes `/qos_chatter` as transient-local with `KEEP_LAST(5)`,
and the zenoh backend sizes its retention ring from the deepest transient-local
depth the contract declares, so a late joiner receives the last five samples.
This is `src/demo_bringup/launch/rust_qos.contract.yaml`, beside
`rust_qos.launch.xml`:

```yaml
version: 1

nodes:
  reliable_talker:
    paths:
      on_tick:
        trigger: { timer: { rate_hz: 1 } }
        output: [qos_chatter]
    pub:
      qos_chatter:
        qos:
          history: keep_last
          depth: 5
          durability: transient_local
          reliability: best_effort

  qos_listener:
    sub:
      qos_chatter:
        qos:
          history: keep_last
          depth: 5
          durability: transient_local
    pub:
      qos_ok:
        qos:
          history: keep_last
          depth: 10
          durability: volatile

topics:
  /qos_chatter:
    type: std_msgs/msg/Int32
    pub: [reliable_talker/qos_chatter]
    sub: [qos_listener/qos_chatter]
  /qos_ok:
    type: std_msgs/msg/Int32
    external: sub
    pub: [qos_listener/qos_ok]
```

The talker's code asks for `reliable`; the publisher row says `best_effort`
because the bringup's `system.toml` overrides it with a
`qos_overrides./qos_chatter.publisher.reliability` parameter, and the contract
has to state the same fact as the override.

## If it doesn't work

- **The build refuses with "QoS statement(s) disagree between the launch
  contract and the `qos_overrides.*` parameters".** A launch or `system.toml`
  `qos_overrides.*` parameter and the contract state one policy two ways, or
  the parameter states one the contract omits. Make the contract state the
  same value; the message names the topic, the parameter and the contract row.
- **A C++ build fails with "nros: the QoS depth passed for topic … disagrees
  with the depth declared for that topic in the contract sidecar".** The call
  site and the row differ. The `declared_depth_agrees<declared, passed>`
  diagnostic beside it carries both numbers. Change whichever is wrong.
- **A Rust image logs `declared depth: taking KEEP_LAST(…)` at boot.** The code
  asked for more than the contract reserved, and the registration took the
  contract's depth. Raise the contract row to keep more.
- **A registration fails at boot and the log says the contract declares a
  deeper queue, or a stronger policy, than the code.** Taking it would widen
  what the code narrowed. Lower the contract row or change the call site.

## See also

- [Size an image from its contract](size-an-image-from-its-contract.md): what
  the declared depths size, and how to see it.
- [Contract format reference](https://github.com/NEWSLabNTU/ros-launch-manifest/blob/v0.3.12/docs/format-reference.md#qos):
  every `qos` key, generated from the grammar this release pins.
- [QoS, status events, and discovery](../rmw/qos-status-events.md): the QoS
  policies at run time.
