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

## Measured 2026-08-20 — the fixture variant WORKS; peer mode itself does not connect

The mechanism the fix direction below asks for already exists and was verified
end to end:

* a `[[fixture]]` row's `env = { ZPICO_MULTICAST_TRANSPORT = "1" }` reaches the
  build (`fixtures-manifest.py` emits it in the `<env>` field;
  `fixtures-build.sh` exports it around `cargo build`);
* the env is part of the GROUP signature (`_nros_fixture_variant_sig`), so the
  pair lands in its own artifact root — `build/cargo-fixtures/linux-14372940`
  rather than `linux/` — and the default native tree keeps its footprint. Do NOT
  author a `target_dir` for this: phase-340 W2 strips an authored dir, because a
  dir now names a group instead of opting the row out;
* the built shim reads `pub const ZPICO_PEER_MODE_SUPPORTED: bool = true`, and
  the session layer stops refusing;
* the test side needs no new machinery either —
  `groups::FixtureVariant::plain().with_env(&[…])` + `select_row` is exactly the
  `link-tls` pattern (`build_native_talker_tls`).

**Beware `NROS_FIXTURE_ID` when measuring this.** The single-node cargo builder
narrows with `--id` and IGNORES that env var (it prints a line saying so and
builds every row). A first attempt at this issue read the const out of two
pre-existing group dirs, saw `false` twice, and concluded the row's `env` had
not reached the build — it had never been built.

What does NOT work is peer mode itself. With the flag-on pair, the listener is no
longer refused, gets past the 0682 guard, and then fails to open:

```
nros: Executor::open failed (Transport(ConnectionFailed))
[ERROR] nros: RMW session open failed — ConnectionFailed
```

`ZenohSession::new` dials nothing in peer mode (`SessionMode::Peer => None`) and
leaves zenoh-pico on its defaults: `Z_CONFIG_MULTICAST_SCOUTING_DEFAULT "true"`
and `Z_CONFIG_MULTICAST_LOCATOR_DEFAULT "udp/224.0.0.224:7446"` — no `#iface=`.
Nothing in the examples or the backend supplies one; the only `multicast_*`
properties anywhere in the tree are the in-crate tests turning scouting OFF. So
the remaining work is a way to name the interface (an env → `multicast_locator`
property, say), not a fixture question.

## Do not land the fixture variant alone

The rows and resolvers were built and then REVERTED rather than committed,
because with the connection still failing there is no honest disposition for
`test_peer_mode_communication`:

* pointing it at the peer pair and asserting delivery makes tier 1 red;
* pointing it at the peer pair and skipping is issue 0650's skip-to-green, one
  level further in than the skip this issue was filed to remove;
* committing rows nothing resolves adds a fixture build to every native lane for
  no coverage.

Note the test's TAIL is already this defect: after all the assertions,
`received_count == 0` falls through to two `eprintln!("[INFO] …")` lines and
PASSES. The run above — session open failed, zero messages — was reported green.
That has to become an assertion in the same change that makes the connection
work.

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
