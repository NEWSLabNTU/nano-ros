---
id: 682
title: "`NROS_SESSION_MODE=peer` failed as an opaque `ConnectionFailed`, because peer mode is compiled out and nothing said so"
status: resolved
type: bug
area: rmw/zenoh
related: [issue-0673]
---

## Symptom

```
$ NROS_SESSION_MODE=peer build/cargo-fixtures/linux/nros-relwithdebinfo/listener
[ERROR] nros: RMW session open failed — ConnectionFailed
nros: application error: NodeRegister("native_rs_listener")
```

Immediate, on both zenoh fixture roots, no router involved. `NROS_SESSION_MODE`
is a documented knob whose accepted values are `client` and `peer`, and the book
promised peer-to-peer twice.

## Cause

`nros-zpico-build` writes the zenoh-pico config the shim compiles against, and
it hardcoded

```
#define Z_FEATURE_MULTICAST_TRANSPORT 0
#define Z_FEATURE_SCOUTING 0
#define Z_FEATURE_SCOUTING_UDP 0
```

with no knob and no comment. Peer mode rests on exactly those, so it has never
worked in any nano-ros build. Turning them off is a legitimate size decision —
three more code paths in a library whose job here is fitting on an MCU, and
every nano-ros deployment reaches its peers through a router or an agent. What
was wrong is that nothing anywhere said so, in either direction:

- The request travelled all the way to `z_open`, which had no multicast link to
  bring up and returned the same `ConnectionFailed` a wrong locator or a dead
  router produces. **The one error the build could have explained exactly was
  the one it explained least.**
- The book advertised the opposite: "two zenoh-pico devices can communicate
  directly without any router", "Peer-to-peer: Yes (no router needed)".

## Four tests covered this path and none could fail

- `nano2nano::test_peer_mode_communication` ended at
  `skip!("peer mode may not be supported — listener exited early")` — a guess
  about the one thing it was positioned to answer, so an absent capability and a
  regression read identically.
- `zenoh_integration`'s three peer tests each wrapped `open()` in
  `match { Ok(..) => assert.., Err(e) => println!("expected in some
  environments") }`. A test that accepts either outcome asserts nothing; all
  three reported green over a capability that was never present.

## Fix

- **One constant, two emissions.** `nros_zpico_build::MULTICAST_TRANSPORT`
  (documented, `false`) now writes both the C `#define`s and a Rust
  `ZPICO_PEER_MODE_SUPPORTED` in `shim_constants.rs`, so the library the session
  layer talks to and the session layer itself cannot disagree about whether peer
  mode can work.
- **Refuse where the reason is known.** `Session::open` returns
  `TransportError::Unsupported` for peer mode on a shim without multicast, after
  logging what to do about it. Kept under nros-log's 256-byte buffer — the first
  draft overflowed and printed `…`, which is the same failure one level in.
- **The tests measure.** All four assert against the compiled capability through
  one shared helper (`assert_peer_open_matches_build`): refusal is REQUIRED when
  the feature is off, a working session is REQUIRED when it is on. The nano2nano
  case greps the refusal marker rather than timing out, and skips in 0.3 s
  instead of 5 s, naming the compiled fact.
- **The book stops promising it** (`rmw-backends.md`, `rmw-zenoh-protocol.md`).
  Claims elsewhere that contrast zenoh with XRCE's agent are left alone — "no
  agent" is true; "no router" was not.

Enabling peer mode is now one edit: flip `MULTICAST_TRANSPORT`, rebuild, accept
the footprint. The tests flip with it.

## Known gap, deliberately recorded

`test_session_open_with_env_scouting_disabled` and
`test_session_explicit_props_override_env` used peer mode to exercise
env-var/property precedence. The refusal now short-circuits before the property
merge, so they assert the refusal and no longer cover precedence. That coverage
needs a CLIENT-mode test against a live router; it is not written here rather
than left looking covered.

## Verified

- `NROS_SESSION_MODE=peer` prints the reason and fails `Unsupported`.
- `zenoh_integration` 14/15 (the 15th is issue 0465's `ZPICO_MAX_SESSIONS=1`).
- `nros-rmw-zenoh` unit suite 68/68.
