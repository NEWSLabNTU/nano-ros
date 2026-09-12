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
