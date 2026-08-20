---
id: 711
title: "zenoh PEER mode is now buildable but still not EXERCISED: `nano2nano` spawns prebuilt fixtures, which are built without it"
status: open
type: bug
area: testing, rmw-zenoh
related: [issue-0682, issue-0393, issue-0650, issue-0400]
---

## Where this stands

`nros_zpico_build::MULTICAST_TRANSPORT` was a hard-coded `const bool = false` —
a deliberate size decision (issue 0682), since multicast transport, scouting and
multicast declarations are three more code paths in a library whose point is
fitting on an MCU. The consequence was that
`nano2nano::test_peer_mode_communication` could only ever SKIP: the single build
in the tree refuses peer mode up front, so nothing anywhere executed the peer
path.

Half of that is now fixed. The flag is a build INPUT, `ZPICO_MULTICAST_TRANSPORT`,
default unchanged (`false`), read through one function that feeds BOTH emitters —
the three C `#define`s and the Rust `ZPICO_PEER_MODE_SUPPORTED` — so the C
library and the session layer still cannot disagree about what was compiled. It
carries a `rerun-if-env-changed` edge, because it is read directly rather than
through `env_usize` (which emits one automatically) and a build input with no
rebuild edge is issue 0475's class.

Verified: with `ZPICO_MULTICAST_TRANSPORT=1` the generated const reads
`ZPICO_PEER_MODE_SUPPORTED: bool = true`, the session layer stops refusing, and
the test RUNS instead of skipping.

## What is still missing

The test does not pass, and the reason is structural rather than a bug in the
peer path:

```
Failed to build native-rs-talker: Test fixture is STALE …
  binary: build/cargo-fixtures/linux/nros-relwithdebinfo/talker
```

`nano2nano` does not exercise an in-process session the way
`zenoh_integration`'s multi-session cell does. It SPAWNS prebuilt fixture
binaries (`native-rs-talker`, the listener), and those come from
`build-test-fixtures`, which builds them without this flag. So even with a
freshly built fixture tree, the two processes the test starts still refuse peer
mode, and an env exported around the test crate cannot change that.

## Why the obvious shortcut is wrong

Exporting `ZPICO_MULTICAST_TRANSPORT=1` around `just build-test-fixtures` would
work, and would build EVERY native fixture with multicast — polluting the shared
tree every other test reads, changing the footprint of artifacts unrelated to
this question, and re-staling the lot on the next alternation. That is the
reason `test-zpico-multisession` owns its own target dir (issue 0393): the value
is a build input, and two configurations cannot share one output tree.

A lane was written and then removed rather than shipped, because it could not
pass: a recipe nobody can run green is a trap, and making it "skip cleanly"
instead would be exactly the skip-to-green issue 0650 exists to forbid.

## Fix direction

Give peer mode a fixture VARIANT — a coordinate in `examples/fixtures.toml`
whose row builds the talker/listener pair with `ZPICO_MULTICAST_TRANSPORT=1`
into its own artifact root, and have `test_peer_mode_communication` resolve THAT
pair rather than the default one. Then a `test-zpico-peer` lane is a thin
wrapper, the way `test-zpico-multisession` is.

The capability assertion the removed lane carried is worth keeping when it comes
back: read `ZPICO_PEER_MODE_SUPPORTED` out of the generated `shim_constants.rs`
BEFORE running, so a lane that accidentally built the default shim fails loudly
instead of skipping green.
