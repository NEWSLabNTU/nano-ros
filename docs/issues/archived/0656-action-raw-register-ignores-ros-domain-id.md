---
id: 656
title: "The raw action register path declares every channel on domain 0, whatever ROS_DOMAIN_ID says"
status: resolved
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


## Fixed 2026-08-17

The fix sketch says "thread the executor's resolved domain into both raw
register paths". The reason that had not happened is that **there was no
executor domain to thread**: `open_in` passed `config.domain_id` into
`RmwConfig` and dropped it. No field on `Executor`, none on `NodeRecord`, no
accessor on the session. `Node` keeps its own — which is exactly why its path
works and the executor paths do not.

So:

* `Executor` gains `domain_id`, set in `open_in` beside `set_node_identity`,
  where the config is still in scope. Same shape, and the identity half was
  already being kept there;
* `set_domain_id()` / `domain_id()` for bindings that build from an existing
  session (`from_session_ptr_in` has no config, so 0 is a FLOOR there rather
  than a choice — stated because it is a real remaining gap for that entry);
* the typed path's five literal `.with_domain(0)` become `self.domain_id`;
* thirteen raw-path `ServiceInfo`/`TopicInfo` chains across three functions
  gain `.with_domain(domain_id)`, captured before the session borrow for the
  same reason `node_name`/`ns` already were.

### Gate — narrowed deliberately

`check-literal-domain-id` rejects `with_domain(<integer literal>)` in the
runtime crates. It does NOT attempt the issue's fuller phrasing ("every chain
reaching a `create_*` carries a non-literal `with_domain`"): that needs
dataflow, and a half-working version would either miss chains or flag the many
`Info` values that never reach a `create_*`. The literal is the mechanical half
and the half that was actually written — five times in this file alone.

Two false positives on the first run, both correct code:
`tests/rtic_integration.rs` pinning `with_domain(42)`. An integration test
asserting behaviour ON domain 42 must say 42; that is the opposite of this
defect, which is SHIPPED code unable to express the session's domain. `tests/`,
`benches/` and `examples/` are excluded.

Verified by reintroducing the historical literal: the gate names
`action.rs:208`.

### VERIFIED ON THE WIRE 2026-08-18 — and the first fix was INCOMPLETE

Once `ros-humble-rmw-zenoh-cpp` was installed the repro in this issue became
runnable, and running it found that the executor fix alone did not work:

| `c_action_server` build | declares under `ROS_DOMAIN_ID=42` |
| --- | --- |
| pre-fix (Aug 16) | `0/fibonacci/_action/…` |
| **executor fix only** | **`0/…` — still wrong** |
| executor + C binding | `42/…` |

The middle row is the finding. `from_session_ptr_in` takes a session and no
config, so the C binding's executor floored its domain to 0 and the raw-path fix
faithfully used that 0. This issue's own text called that "a real remaining gap,
stated rather than papered over" — and it was still shipped as fixed, because a
green compile and a green gate looked like enough.

The missing half is one line beside `set_primary_identity`, where the value was
already in scope:

```rust
rust_exec.set_domain_id(u32::from(support_ref.domain_id));
```

All three queryables now declare on `42/`, on a live `rmw_zenohd`:

```
queryable 42/fibonacci/_action/send_goal/…
queryable 42/fibonacci/_action/cancel_goal/…
queryable 42/fibonacci/_action/get_result/…
```

### Superseded: what could not be verified before



The repro in this issue needs `zenohd` plus the built C example, and this host
has no `rmw_zenoh_cpp` — every action test here reports
`[SKIPPED:capability]` (1 passed, 4 skipped, and they skip identically before
and after this change). So "declares on 42" is confirmed by reading
(`keyexpr.rs:33` formats `domain_id` into the prefix) and by the gate, **not**
measured on the wire. Anyone with a ROS router should run the repro above before
treating the wire behaviour as proven.

`tests/action_raw_goal_e2e.rs` (issue 0454 / phase-354 W3) is unblocked in
principle — it can now be pointed at a non-zero domain — but that is worth doing
on a host where it can actually run.
