---
id: 1348
title: "Every zenoh action server now fails to register: the shim refuses
  TRANSIENT_LOCAL and the ROS action `/status` topic is REQUIRED to ask for it"
status: open
type: bug
area: [rmw, core]
related: [1146, 1256, phase-448, phase-454]
---

## What

`b0ea5a04b` (2026-09-11, "grant the QoS the shim can serve, refuse the rest")
made `nros-rmw-zenoh`'s QoS shim REFUSE `DURABILITY_TRANSIENT_LOCAL` with
`TransportError::IncompatibleQos`, on the reasoning that the shim keeps no
historical samples so serving VOLATILE as TRANSIENT_LOCAL would be a silent
downgrade.

A ROS action server's `/<action>/_action/status` publisher is
TRANSIENT_LOCAL **by the action spec** — `rcl_action` pins it, and nano-ros's
action layer creates it that way, not the user. So the refusal is raised against
a profile no caller chose, during the action server's own registration, and
every action server on the zenoh backend now fails to come up.

## Measured 2026-09-12, all three languages, qemu mps2-an385 + a live `rmw_zenohd`

**Rust** (`examples/mps2-an385-freertos/rust/action-server`), which names the
cause outright:

```
[WARN]  nros: qos: service '/fibonacci/_action/send_goal' asked for KEEP_LAST(10);
        this image's receive ring holds 4. Granting 4 and advertising it to the graph.
[ERROR] nros: qos: publisher '/fibonacci/_action/status' refused — durability
        TRANSIENT_LOCAL — the shim keeps no historical samples
[ERROR] nros: node declaration failed — NodeError::Transport(IncompatibleQos)
Application error: NodeRegister("freertos_rs_action_server")
```

**C** (`c/action-server`) — the same fault one layer up, reported as a bare `-1`:

```
Action server created: /fibonacci
[nros] .../c/action-server/src/main.c:236
       nros_executor_add_action_server(&app.executor, &app.action_server) -> -1
```

**C++** (`cpp/action-server`) — `-12`, which is `NROS_CPP_RET_NOT_ALLOWED`, which
is `node_error_to_cpp_ret(IncompatibleQos)`:

```
[nros] .../cpp/action-server/src/main.cpp:117
       node.create_action_server(srv, "/fibonacci") -> -12
```

The clients are affected in the obvious way: `c/action-client` reports `Action
server did not appear within 10s: -2`, `cpp/action-client` reports `No goal
response from server (order=10, ret=-2)`.

This is not FreeRTOS-specific — nothing in the path is. It is where it was found,
because phase-448 W3 was running every FreeRTOS image against a live router.

## Why no lane caught it

The zenoh action e2e cells are RTOS cells: they need a fixture build, QEMU and a
router, so they run in `just ci` / the matrix lanes and NOT in the `pull_request`
`CI` context, which is compile-plus-source-gates (CLAUDE.md's "PR cheap, batch
thorough"). The commit is one day old at filing.

## What the fix has to decide, and it is not obvious

The refusing commit's principle is right in general: serving VOLATILE where
TRANSIENT_LOCAL was asked for is an inverted behaviour, not a weakened one, and
`KEEP_ALL` is refused on the same reasoning. The problem is that ONE profile in
the tree is not a request — it is a protocol constant:

1. **Grant TRANSIENT_LOCAL as VOLATILE for every caller**, reported once. Undoes
   the commit's point for every user profile too.
2. **Let the ACTION layer state that its `/status` publisher tolerates VOLATILE**,
   so the refusal stays for profiles a user actually wrote. This is the shape
   the reliability arm already has ("asked for BEST_EFFORT; RELIABLE is granted.
   Over-delivery, not loss") — a per-site grant with a stated reason.
3. **Keep refusing and let action servers stay down on zenoh.** Only tenable if
   actions-on-zenoh are declared unsupported, which nothing else in the tree says.

(2) looks right and is not this issue's to make.

## Not fixed here

phase-448 W3 found it while measuring stack depth and did not fix it: a QoS
policy decision inside an unrelated PR is exactly the unattributable diff the
tree's one-subject rule exists to prevent. W3's measurement worked around it with
a throwaway probe that grants the policy, which is recorded in issue 1146.
