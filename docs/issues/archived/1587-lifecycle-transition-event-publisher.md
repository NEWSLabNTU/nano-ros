---
id: 1587
title: "REP-2002's `~/transition_event` publisher is missing, so a supervisor
  cannot watch a managed node's transitions — and the price everyone quoted
  for it was measured against the wrong QoS"
status: resolved
type: gap
area: [api, core, cli]
severity: medium
found: 2026-09-29
resolved_in: "this PR (feat/lifecycle-transition-event)"
related: [0460, 1270, 1341, 1378, 1033, 1015, 1589, phase-467, phase-417, phase-444]
---

## What was true

REP-2002's *communication interface* is five services **plus** a
`lifecycle_msgs/msg/TransitionEvent` publisher on `~/transition_event`. We
shipped the five services and no publisher. `TransitionEvent` appeared in the
tree only in a round-trip unit test, the generated `nros-lifecycle-msgs` crate
and two schema/size test tables; the string `transition_event` appeared as a
topic nowhere.

The consequence is not cosmetic. A supervising node — a launch-side health
monitor, `rqt_lifecycle`, anything that manages a fleet of managed nodes —
cannot observe a transition passively. It has to poll `~/get_state`, which
costs a round trip per node per tick and still cannot distinguish "went
inactive" from "went inactive and came back" between two polls.

This was the last `gap` row in `docs/reference/api-parity-ledger/lifecycle.json`
(`c:lifecycle_change_state`) and the thirteenth row of
[phase-467](../../roadmap/phase-467-rmw-gap-closure-design-study.md)'s study.

## The pricing, which is the part this file exists for

phase-467 §"Row 9" priced this as **the most expensive of its thirteen rows**,
and told whoever took it to decide the QoS before sizing anything. Its point 5:

> **If the publisher is TRANSIENT_LOCAL** — which is what REP-2002 / rcl use
> for `~/transient_event`-class topics — it also costs a **queryable** on
> zenoh (issue 1378: a transient-local publisher IS a queryable) […] Decide
> the QoS BEFORE sizing anything.

That premise is what made the row expensive, and it is **false**. The cost of
a sixth lifecycle queryable would have been real: `ZPICO_MAX_QUERYABLES`
defaults to **8** on an embedded build, while `[param_services]` (6) and
`[lifecycle]` (5) already claim **eleven** between them (issue 0460), so a
transient-local announcement publisher would move the derived floor of every
lifecycle image and, on an image that declares its entities, be the difference
between booting and dying in `ServiceServerCreationFailed`.

### Measured

Twice — 2026-09-29 and 2026-10-01 — against a live upstream node, inside the
`ros2` distrobox on humble:

```console
$ ros2 run lifecycle lifecycle_talker &
$ ros2 topic info -v /lc_talker/transition_event
Type: lifecycle_msgs/msg/TransitionEvent
Endpoint type: PUBLISHER
QoS profile:
  Reliability: RELIABLE
  History (Depth): UNKNOWN
  Durability: VOLATILE
  Lifespan: Infinite
```

`RELIABLE` / `VOLATILE` / infinite lifespan is `rmw_qos_profile_default`, which
`rcl_lifecycle`'s `rcl_lifecycle/src/com_interface.c` takes from
`rcl_publisher_get_default_options()` and never overrides. Our
`QoSProfile::QOS_PROFILE_DEFAULT` is that profile byte for byte —
KEEP_LAST(10) / RELIABLE / VOLATILE — so this is not even a divergence to
state.

### What the row therefore costs, per lifecycle-enabled image

| pool | delta | why |
| --- | --- | --- |
| `max_publishers` (`ZPICO_MAX_PUBLISHERS`) | **+1** | the publisher |
| `max_liveliness` (`ZPICO_MAX_LIVELINESS`) | **+1** | the pool counts publishers |
| `max_queryables` (`ZPICO_MAX_QUERYABLES`) | **0** | VOLATILE declares no cache queryable |
| `tl_publishers` / retention pool | **0** | not transient-local, so `nros_sizing_descriptor::transient_local_publishers_over` does not see it and must not |
| `infra_queryables` | **0** | unchanged, still 5 |

The claim that the TL count flows through one derivation is preserved by the
publisher simply **not being** a transient-local publisher: nothing was added
to `transient_local_publishers_over`, no second spelling exists, and
`a_lifecycle_image_pays_one_publisher_and_no_queryable` asserts all four deltas
as a difference between a lifecycle image and the same image without the
feature.

So the answer to "is this unacceptable for embedded, and does it need a
Kconfig opt-out?" is **no, and it does not**. One publisher slot is inside the
budget the derivation already computes, it is derived rather than conjured
(the bringup states the `lifecycle` feature, in either of the two spellings
`capability_enabled` ORs), and an image that declares its entities gets the
slot automatically on both roads because both read
`DerivedEntityKnobs::max_publishers` — `leaf_entity_env` emits
`ZPICO_MAX_PUBLISHERS` from it on the cargo road and
`NanoRosEntityFacts.cmake` emits `NROS_DECLARED_MAX_PUBLISHERS` from it on the
cmake one.

Checked the one place an image could be stranded: `zephyr/Kconfig`'s
`CONFIG_NROS_MAX_PUBLISHERS` defaults to **-1, "derive"**, not to a literal, so
a Zephyr lifecycle image follows the inventory rather than a number someone
typed before this term existed. (`CONFIG_NROS_MAX_QUERYABLES` is the one that
defaults to a literal 8, and it is the pool this change does not touch.) No
lifecycle image is embedded in this tree today anyway — the capability demos
are native-only, for the alloc reason stated at the top of
`examples/workspaces/features/src/demo_bringup/system.toml`.

## Resolution

1. **The record is taken at the one place a transition happens.**
   `LifecyclePollingNodeCtx::trigger_transition` is the single funnel for
   `ros2 lifecycle set`, `nros_executor_lifecycle_change_state`,
   `nros_cpp_lifecycle_change_state`, `nros_cpp_lifecycle_autostart` and the
   safe `LifecycleCallbacks` road. Recording there rather than at each caller
   is why a caller added later is announced without being told to announce.
   Failed transitions announce (rcl does, and the failure is exactly what a
   supervisor needs); rejected ones do not, because none ran.
2. **The executor drains and publishes**, during `spin_once`, because it is
   what owns the session. `LifecycleRuntimeState::process` is one method rather
   than two calls at each of the two spin sites.
3. **The queue is bounded and its losses are counted.**
   `TRANSITION_ANNOUNCEMENT_QUEUE` is 4; the longest burst a source in this
   tree can produce before the first spin is 2 (`autostart` runs configure then
   activate from `__nros_entry_setup`). Overflow increments a saturating
   counter that the drain reports once, the posture
   `zpico_session::reply_slot_refusals` already takes for the other bounded
   table in this stack.
4. **The gate was widened in the same commit** (CLAUDE.md's issue-0196 rule).
   `check-infra-queryable-counts` counted `create_lc_srv` sites only and scoped
   its `create_publisher` counter to `action.rs`, so it could not see a
   lifecycle publisher at all. A third group holds
   `LIFECYCLE_SERVICE_PUBLISHERS` to the `create_lc_pub` sites, the mirror scan
   reaches the entity inventory's copy, and four new self-test probes cover a
   second publisher, a drifted constant, a blind pattern and a drifted mirror.

## What remains, written down rather than left implied

**The arity.** The correlator scores the row `differs` because ours takes two
arguments and rclc's takes three; the third is `bool publish_update`. It is
**not** owed, and the reason is not "we did not get to it":
`nros_lifecycle_state_machine_t` is a pure state machine that owns no
publisher — the divergence settled at `c:lifecycle_node_t`, whose platform
constraint (static entity storage; the service buffers belong to the entities
the executor already owns) is named there. On that handle `publish_update` has
no referent and could only ever be ignored, and an argument that must be
ignored is worse than an absent one. On the executor road, where the publisher
lives, announcement is unconditional — which is what rclcpp does with the flag
on every user-facing `change_state`.

**No live-peer cell yet.** The assertions here are against `MockPublisher`,
which records the bytes a subscriber would receive and decodes them back
through the GENERATED `Deserialize`, so they are about the wire and not about
our writer's intent. What they do not prove is that a stock `ros2 topic echo
/<node>/transition_event` sees it — the same thing issue 1589 records for
`/rosout`, and the same blocker: a live graph cell. That is an interop cell to
add beside `rosout_interop.rs`, not a hole in this change.
