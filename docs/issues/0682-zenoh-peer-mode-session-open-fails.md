---
id: 682
title: "`NROS_SESSION_MODE=peer` fails to open a zenoh session on native, and the test that would say so guesses instead of asserting"
status: open
type: bug
area: rmw/zenoh
related: [issue-0673]
---

## Symptom

Run any native zenoh example with peer mode and no router:

```
$ NROS_SESSION_MODE=peer build/cargo-fixtures/linux/nros-relwithdebinfo/listener
[ERROR] nros: RMW session open failed — ConnectionFailed
nros: Executor::open failed (Transport(ConnectionFailed)); proceeding with
      NullNodeRuntime — `run_plan` register calls will fail loud.
nros: application error: NodeRegister("native_rs_listener")
```

Reproduced on both zenoh fixture roots (`build/cargo-fixtures/linux` and
`…/linux-3263301353`), immediately, no router involved.

## Why this is a bug and not a documented limitation

`book/src/user-guide/rmw-backends.md` states the opposite, twice:

> In **peer mode**, two zenoh-pico devices can communicate directly without any
> router.

> Peer-to-peer capable (no mandatory bridge process)

and the comparison table lists **Peer-to-peer: Yes (no router needed)** for the
zenoh backend. `NROS_SESSION_MODE` is a documented knob
(`nros-c/docs/configuration.md`, `executor/types.rs:450`) whose accepted values
are `client` and `peer`.

## The test cannot tell you which it is

`nano2nano::test_peer_mode_communication` covers exactly this path and ends at:

```rust
nros_tests::skip!("peer mode may not be supported — listener exited early");
```

"may not be supported" is a guess, and it is the one thing the test is
positioned to answer. A capability the build genuinely lacks and a capability
that regressed produce the same green lane — the anti-pattern CLAUDE.md names
("tests must fail on unmet preconditions"), one level up: the precondition here
is not unmet, it is unmeasured.

This skip was invisible until `ros-humble-rmw-zenoh-cpp` was installed and the
sweep dropped from 167 skips to 7.

## Not yet diagnosed

Peer mode needs multicast scouting, and a session CAN open in peer mode on this
tree — `zenoh_integration`'s `ZENOH_MULTICAST_SCOUTING` test does it and passes.
So the question is what differs between that path and the example's: whether the
examples need a scouting/multicast setting they do not set, whether loopback
lacks a joinable multicast interface here, or whether the feature is compiled
out of the shim (`Z_FEATURE_LINK_UDP_MULTICAST`). Not established — filed with
the reproducer rather than a guess.

## What to fix, in order

1. **Make the test measure.** Probe peer support once, explicitly: skip with a
   reason that names the missing capability, or FAIL. Either is honest; the
   current text is neither.
2. Then diagnose the session-open failure above.
3. If peer mode is genuinely unsupported for native examples, the book has to
   say so — it currently promises it.
