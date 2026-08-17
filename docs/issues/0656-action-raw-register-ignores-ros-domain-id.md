---
id: 656
title: "The raw action register path declares every channel on domain 0, whatever ROS_DOMAIN_ID says"
status: open
type: bug
area: rmw
related: [issue-0454, phase-338, phase-354]
---

## Symptom

A C action server started with `ROS_DOMAIN_ID=42` prints

```
Domain ID: 42
Action server created: /fibonacci
```

and then declares its queryables on **domain 0**:

```
Declare queryable 2 (0/fibonacci/_action/send_goal/example_interfaces::action::dds_::Fibonacci_SendGoal_/TypeHashNotSupported)
Declare queryable 5 (0/fibonacci/_action/cancel_goal/action_msgs::srv::dds_::CancelGoal_/TypeHashNotSupported)
Declare queryable 7 (0/fibonacci/_action/get_result/example_interfaces::action::dds_::Fibonacci_GetResult_/TypeHashNotSupported)
```

(from `RUST_LOG=zenoh=debug zenohd`, 2026-08-17). A peer that honours the
domain queries `42/fibonacci/_action/send_goal/…` and matches nothing; every
goal times out with no diagnostic on either side.

## Where

`Executor::register_action_server_raw*` / `register_action_client_raw*`
(`packages/core/nros-node/src/executor/action.rs`, the ~601 and ~1063 blocks)
build their `ServiceInfo` / `TopicInfo` with `.with_namespace(&ns)` and
`.with_node_name(…)` but **never `.with_domain(…)`**, so the domain defaults to
0. `Node::create_action_*_raw_sized` (`executor/node.rs`) does pass
`self.domain_id`, and the polling arms in `nros-c` / `nros-cpp` pass the
resolved domain — so which spelling you get depends on which registration path
your language binding takes.

The typed path has the same bug written more explicitly: `action.rs` ~207 ends
its `ServiceInfo` chain with a literal `.with_domain(0)`.

## Why it stayed invisible

Every existing action test runs both peers through paths that drop the domain
identically, so they agree on `0/…` and pass. The mismatch only appears when one
side honours `ROS_DOMAIN_ID` and the other does not — which is exactly what
happened when the phase-354 W3 raw-goal probe (a POLLING client, which does
honour it) was pointed at the C `action-server` example (an executor/raw server,
which does not).

Same shape as the domain split-brain in issue 0161: the value is read, printed,
and then not used where it counts.

## Impact

- `ROS_DOMAIN_ID` does not isolate actions. Two unrelated action graphs on one
  network share keyexprs and can see each other's goals.
- Cross-binding action pairs silently fail to discover whenever the two sides
  take different registration paths.

## Repro

```
zenohd -l tcp/127.0.0.1:17452 --no-multicast-scouting   # RUST_LOG=zenoh=debug
NROS_LOCATOR=tcp/127.0.0.1:17452 ROS_DOMAIN_ID=42 examples/native/c/action-server/build-zenoh/c_action_server
grep -oE 'Declare queryable [0-9]+ \([^)]*_action[^)]*\)' <zenohd log>
```

The declared keys begin `0/`, not `42/`.

## Fix sketch

Thread the executor's resolved domain into both raw register paths and drop the
literal `.with_domain(0)` on the typed one — one domain source, as
`resolve_session_and_domain` already provides for publishers and the polling
action arms.

**Gate it, or it comes back on the next path.** The class is "an entity is
declared without the session's domain"; a check that every `ServiceInfo::new` /
`TopicInfo::new` chain reaching a `create_{service,client,subscription,
publisher}` carries a non-literal `with_domain` would cover the sites the two
fixes touched and the ones nobody has written yet.

## Blocked work

`tests/action_raw_goal_e2e.rs` (issue 0454 / phase-354 W3) cannot exercise a
non-zero domain until this is fixed; it runs both peers on the default domain
and says so at the call site.
