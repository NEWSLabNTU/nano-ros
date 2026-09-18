---
id: 1348
title: "Every zenoh action server now fails to register: the shim refuses
  TRANSIENT_LOCAL and the ROS action `/status` topic is REQUIRED to ask for it"
status: resolved
type: bug
area: [rmw, core]
related: [1146, 1256, phase-448, phase-454]
resolved_in: phase-455 W5
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

## RESOLVED 2026-09-18 — fixed by phase-455 W5, which found the same defect from the other side

`feat(phase-455 W5, #1341, #1361): serve TRANSIENT_LOCAL on the publisher` is
the fix. It reached this defect through issue 1341 (a zenoh/ROS interop gap)
rather than through a FreeRTOS boot failure, and its reading is sharper than
this issue's:

> The zenoh shim refused TRANSIENT_LOCAL for every entity kind from phase-428
> W9, and the one profile in the tree whose durability is not VOLATILE is the
> one an action server creates — so no zenoh action server could start.
> `QOS_PROFILE_ACTION_STATUS_DEFAULT` mirrors
> `rcl_action_qos_profile_status_default` and is the action protocol's **wire
> contract, not a caller request this backend may decline**. W9's rule stays;
> the population it was applied to was wrong.

That is the correction this issue's own "What the fix has to decide" section was
reaching for. A publisher now SERVES the profile by query-on-match, the
mechanism `shim/qos.rs`'s comment had already named as the missing one, and the
shape was measured off a live stock `rmw_zenoh_cpp` 0.1.9 pair rather than
recalled.

**In the tree today** (`packages/rmw/zenoh/nros-rmw-zenoh/src/shim/qos.rs`):

> TRANSIENT_LOCAL is SERVED on a publisher since phase-455 W5 and refused on a
> subscriber

The refusal this issue reported is gone from the publisher path, which is the
path an action server's `/<action>/_action/status` publisher takes. The
remaining refusal is the subscriber side, which is not what failed here.

**What was NOT re-measured, stated rather than implied.** This issue's evidence
was three RUNNING mps2-an385 FreeRTOS images. Those were not rebuilt to confirm
the boot now succeeds: a linked worktree re-roots the inherited `FREERTOS_DIR`
/ `LWIP_DIR` onto itself, where those SDK trees do not exist, and the build dies
in `lan9118_lwip.h` on a missing `lwip/err.h` — issue 1280's class, unrelated to
this defect, and it survived an explicit re-export of both paths.

So this is closed on the FIX's own measurement plus the code path, not on a
repeat of the original reproduction. If a FreeRTOS action-server image is built
for any other reason, its boot is the confirmation; `example_e2e`'s action cells
on the zenoh backend are the standing check.
