---
id: 1341
title: "every nano-ros zenoh ACTION SERVER fails to start on main: the shim
  refuses TRANSIENT_LOCAL and the action's own `/status` publisher asks for it"
status: open
type: bug
area: rmw, core
severity: high
related: [issue-0902, issue-1332, issue-1256, phase-428, phase-455]
---

## Symptom, measured

On `main` at `d4025ceba`, in the `ros2` distrobox, a freshly built
`examples/native/rust/action-server` (zenoh) exits 1 at boot:

```
[WARN]  nros: qos: service '/fibonacci/_action/send_goal' asked for KEEP_LAST(10); this image's receive ring holds 4. Granting 4 and advertising it to the graph. Raise ZPICO_SUBSCRIBER_RING_DEPTH to keep more.
[ERROR] nros: qos: publisher '/fibonacci/_action/status' refused — durability TRANSIENT_LOCAL — the shim keeps no historical samples
[ERROR] nros: node declaration failed — NodeError::Transport(IncompatibleQos)
nros: application error: NodeRegister("native_rs_action_server")
```

`packages/testing/nros-tests/bins/action-server-concurrent` dies harder, at
`create_action_server` rather than at node declaration:

```
thread 'main' panicked at src/main.rs:55:10:
Failed to create action server: ActionCreationFailed
```

So on this backend there is currently **no way to create an action server at
all**.

## It is new, and the before/after was run rather than reasoned

Three builds of the same fixture against one `rmw_zenohd`, 8 s each
(`tmp/live/qos-regression.sh` in the run that filed this):

| fixture build | result |
| --- | --- |
| `build/cargo-fixtures/linux-3263301353/…/action-server`, built 2026-09-12 06:57 | exit 1, the refusal above |
| `build/cargo-fixtures/linux-553222167/…/action-server`, built 2026-09-08 21:14 | `[INFO] Waiting for action goals`, exit 124 (the 8 s timeout — i.e. running) |
| `build/cargo-fixtures/linux/…/action-server`, built 2026-09-12 06:49 | exit 1, the refusal above |

The 2026-09-08 binary predates the change; both of today's carry it.

## Cause

`b0ea5a04b` (phase-428 W9, 2026-09-12 00:19) made `shim/qos.rs::admit` refuse
`TRANSIENT_LOCAL` durability locally instead of only withholding it from the
advertised mask:

```rust
QoSDurabilityPolicy::TransientLocal => {
    refuse(kind, name, "durability",
           "TRANSIENT_LOCAL — the shim keeps no historical samples");
    return Err(TransportError::IncompatibleQos);
}
```

The action `/status` publisher has asked for `TRANSIENT_LOCAL` since
`5ef18844b` (2026-05-27): `QOS_PROFILE_ACTION_STATUS_DEFAULT`
(`nros-rmw/src/traits.rs:1015`) mirrors `rcl_action_qos_profile_status_default`,
which is `KEEP_LAST(1) / RELIABLE / TRANSIENT_LOCAL`, and
`nros-node/src/executor/action.rs:306` and `:806` pass it to
`create_publisher`.

The commit message states the sweep it did: *"A no-op for every default caller
(nros-node passes services_default at all ten service/action sites)."* That is
true of the ten SERVICE sites and false of the two PUBLISHER sites beside them —
the one profile in the tree whose durability is not `VOLATILE` is exactly the
one an action server creates. The class is CLAUDE.md's "fix the class, then
prove the sweep": the sweep enumerated services and the miss was a publisher.

## Blast radius

* Every zenoh action SERVER image, Rust and C/C++ alike (the profile is chosen
  in `nros-node`, below the language surface).
* Interop cell `native-action-rust-zenoh-r2n` — recorded `fail` in
  `.config/interop-verdicts.toml` on the strength of this. Its sibling
  `native-action-rust-zenoh-n2r` passes: nano-ros is the CLIENT there and
  creates no `/status` publisher.
* `just native test-action-completion` (phase-455 W2's own lane) cannot run —
  its server fixture is `action-server-concurrent`, the second failure above.
* Consequently **issue 0902 route 1 / issue 1332's option 1 cannot be
  measured**: reading the reply-slot counter with a live `rmw_zenoh_cpp` peer
  needs a zenoh action server to attach the peer to.

## Fix shape — a decision, not a patch

The two obvious repairs are not equivalent and this issue does not pick one:

1. **Grant `VOLATILE` for this publisher and advertise `VOLATILE`.** Honest
   about behaviour, and it BREAKS RxO with a stock peer: an `rclcpp` action
   client's `/status` subscription is `TRANSIENT_LOCAL`, and a `VOLATILE`
   writer does not match a `TRANSIENT_LOCAL` reader. Interop would fail at the
   status topic instead of at boot.
2. **Serve depth-1 transient-local for real.** `rcl_action_qos_profile_status_default`
   is `KEEP_LAST(1)`, so "the last status sample, replayed on match" is one
   message per action server, not a history cache. This is the option that
   keeps the advertised profile true AND matches a stock peer.

What phase-428 W9 restored — advertising only what the image serves — should
not be given back; the question is which of the two the shim does at the
`/status` publisher.

Until then, a zenoh action server does not start, so this is not a latent
mismatch.

## What phase-455 W5 built, and what it measured (2026-09-13)

Option 2 is implemented. The startup failure is gone and the mechanism is
verified live; the issue stays OPEN because its acceptance names a sample the
server never publishes. See issue 1361.

### The mechanism, measured off a live peer rather than recalled

`rmw_zenoh_cpp` 0.1.9 builds its endpoints on zenoh's `ze_advanced_publisher` /
`ze_advanced_subscriber` (`nm -D --undefined-only librmw_zenoh_cpp.so`). Read
from a router started with `RUST_LOG=zenoh=debug`, a stock TRANSIENT_LOCAL pair
on `/tl_probe` produces exactly this:

```
Declare queryable  77/tl_probe/std_msgs::msg::dds_::String_/…/@adv/pub/<zid>/<eid>/_
Declare subscriber 77/tl_probe/std_msgs::msg::dds_::String_/…
Declare subscriber 77/tl_probe/std_msgs::msg::dds_::String_/…/@adv/pub/**
Route query    for 77/tl_probe/std_msgs::msg::dds_::String_/…/@adv/**
Route query    for 77/tl_probe/std_msgs::msg::dds_::String_/…/@adv/pub/<zid>/<eid>/_
```

So a transient-local publisher's cache is a QUERYABLE under an `@adv/pub`
suffix, and a subscriber that joins issues a GLOBAL history query at
`<ke>/@adv/**` which intersects it. That global query is the one the late-joiner
case rides on; the per-publisher query beside it is late-joiner DETECTION via a
zenoh liveliness token under the same prefix, which W5 does not declare.

### The action server starts, and the graph says what is served

```
[DEBUG] Publisher data keyexpr: 78/fibonacci/_action/status/action_msgs::msg::dds_::GoalStatusArray_/TypeHashNotSupported
[DEBUG] liveliness keyexpr: @ros2_lv/78/…/MP/%/%/fibonacci_action_server/%fibonacci%_action%status/…/1:1:1,1:,:,:,,
[INFO] Waiting for action goals
```

`1:1:1,1` is RELIABLE : **TRANSIENT_LOCAL** : KEEP_LAST,1 — the advertised
durability is the served one, and `ros2 topic info -v` agrees:

```
QoS profile:
  Reliability: RELIABLE
  History (Depth): KEEP_LAST (1)
  Durability: TRANSIENT_LOCAL
```

A stock `ros2 action send_goal /fibonacci example_interfaces/action/Fibonacci
'{order: 5}'` returns `Goal finished with status: SUCCEEDED`.

### The retention serves a late joiner, with a negative control

Three seconds after the goal terminated, with nothing publishing in between, two
late joiners on the same topic differing only in their own durability:

```
### late joiner with --qos-durability transient_local: exit=0
status_list: []
### late joiner with --qos-durability volatile: received nothing (timed out)
```

The transient-local reader gets the publisher's retained sample; the volatile
one, RxO-compatible with the same writer at the same moment, gets nothing. The
difference is the history query, so the retention path is what delivered it.

### Why the issue stays open

The retained sample is `status_list: []`, and that is faithful: **no terminal
status sample is ever published by a nano-ros action server, to anyone, on any
backend.** A subscriber attached BEFORE the goal — which therefore saw every
sample there is — saw `status: 1` (ACCEPTED) and then the empty array, and
nothing else. `complete_goal_raw` removes the goal from `active_goals` before
`publish_status_array` runs. That is issue 1361, it is in `nros-node` below the
RMW seam, and no durability setting can deliver a sample that was never sent.

This issue's own defect — the shim refusing a profile it is not free to
decline — is fixed. Close it when 1361 makes the terminal sample exist and this
acceptance can be re-run.

### The TRANSIENT_LOCAL sweep W9 owed, by ENTITY KIND

W9's commit said "a no-op for every default caller (nros-node passes
`services_default` at all ten service/action sites)", which enumerated SERVICES
and missed the PUBLISHER beside them. The whole population, swept 2026-09-13:

| user | kind | note |
| --- | --- | --- |
| `QOS_PROFILE_ACTION_STATUS_DEFAULT` → `executor/action.rs:306`, `:806`, `executor/node.rs:811`, `:1096` | **publisher** ×4 | the only transient-local PRESET in the tree, and the one this issue is about. All four are `create_publisher` on `<action>/_action/status` |
| `rust_qos_talker_pkg` / `c_qos_talker_pkg` / `cpp_qos_talker_pkg` / `mixed_qos_talker_pkg` | **publisher** ×4 | RELIABLE + TRANSIENT_LOCAL + KEEP_LAST(10) on `/qos_chatter` / `/chatter`. Served by W5; the depth is clamped to 1 and the clamp is advertised |
| `rust_qos_listener_pkg` / `c_qos_listener_pkg` / `cpp_qos_listener_pkg` / `mixed_qos_listener_pkg` | **subscriber** ×4 | the same profile on the read side. **Still REFUSED on zenoh** — the subscriber half is not built |
| `qos_overrides.<topic>.{publisher,subscription}.durability = transient_local` (launch) | **publisher or subscriber** | `QoSOverrideRole` has exactly two variants, applied at `executor/node.rs:258/318/388/451`. A publisher override is served; a subscription override is refused |
| `nros-rmw-cyclonedds/src/graph.cpp:157` / `:357` | **publisher + subscriber** | the `ros_discovery_info` latched pair. Cyclone's own backend, untouched by W5 |
| `QOS_PROFILE_PARAMETERS` (the six parameter services, `executor/spin.rs:8768`) | **service** | **VOLATILE**, not transient-local — it was TransientLocal until issue 0793 fixed it on 2026-08-25. The "parameters preset is transient-local too" premise in W5's brief is out of date |
| `QOS_PROFILE_PX4` | publisher + subscriber | **VOLATILE** deliberately, with a test guarding it: a TL reader does not match PX4's volatile writers |
| `nros-cpp`'s `qos_table::rosout()` / `rclcpp::RosoutQoS` | **not an endpoint** | TRANSIENT_LOCAL, reached only by a `static_assert`; nano-ros publishes no `/rosout` |
| every other hit | not an endpoint | C/C++ enum values, the cffi tables, the sizing/override vocabularies, gate scripts, tests, and the upstream mirrors under `docs/reference/api-surface` and `api-parity-ledger` |

No service or client in the tree requests TRANSIENT_LOCAL, and no launch
override can reach one. The sweep command:

```sh
git grep -n -E "TRANSIENT_LOCAL|TransientLocal|transient_local" -- packages examples scripts config
```
