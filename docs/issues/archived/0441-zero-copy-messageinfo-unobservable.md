---
id: 441
title: "test_zero_copy_message_info observes nothing — the listener emits no MessageInfo, and the zero-copy feature changes no output"
status: resolved
type: bug
area: testing
related: [issue-0422, issue-0429, issue-0157, issue-0164]
---

## Symptom

```
zero_copy::test_zero_copy_message_info
  Zero-copy listener did not emit 2 sequence markers: Timeout
```

Deterministic on freshly built fixtures.

## Cause

Verified by running the fixture directly against a router:

```console
$ NROS_LOCATOR=tcp/127.0.0.1:47455 \
    examples/native/rust/listener/target-zero-copy/nros-relwithdebinfo/listener
liveliness keyexpr: @ros2_lv/…/node
nros: session open
Subscriber data keyexpr: 0/chatter/std_msgs::msg::dds_::String_/*
# grep -c 'Waiting for' → 0     (the readiness wait, zero_copy.rs:147)
# grep -c 'seq='        → 0     (the count wait, zero_copy.rs:157)
```

The listener runs correctly — session opens, subscriber declares — and prints
neither string. Same grep-drift class as #0429, and phase-277's slimming of the
demo listener is the same origin: `examples/native/rust/listener/src/lib.rs`
now logs only `Subscriber created for topic: /chatter` and `I heard: [{}]`.

## Why this one is NOT a copy of the 0429 fix

#0429 retargeted `nano2nano` at the zenoh publisher shim's trace
(`… with attachment: seq=N, ts=…, gid=[..]`), which is the authoritative source
for what the PUBLISHER stamps. That works there because those tests verify
publisher-side sequence/GID semantics.

This test's stated purpose is different:

> Test that MessageInfo (sequence number, GID) is correctly passed through
> **the zero-copy trampoline.**

That is a RECEIVE-side property. Observing the publisher's trace would assert
that the talker stamped an attachment — which #0429 already covers — and would
silently stop testing the zero-copy receive path while still passing. A green
test that no longer tests its subject is worse than the current red.

## The deeper problem

The zero-copy fixture is not distinguishable from the plain one at the output
level:

```console
$ grep -c 'cfg(feature' examples/native/rust/listener/src/lib.rs
0
$ grep -n unstable-zenoh-api examples/native/rust/listener/Cargo.toml
64:unstable-zenoh-api = ["nros/unstable-zenoh-api"]
```

The feature only propagates to `nros` — it changes which receive path the
runtime takes, but the example has no `cfg` branch and prints the same lines
either way. So there is currently NO observable difference between the
zero-copy and non-zero-copy listeners, and nothing for this test to assert on.

The receive path does parse the attachment
(`nros-rmw-zenoh/src/shim/subscriber.rs:979`, `MessageInfo::from_attachment`),
so the DATA exists — it just never reaches any output.

## Fix — needs a decision

1. **Give the receive path a trace line**, mirroring the publisher shim's, and
   have the test observe that. Restores the receive-side assertion and would
   also give #0429's class a proper receive-side channel. Most faithful to the
   test's purpose.
2. **Assert on the zero-copy path some other way** (e.g. a counter or a
   `MessageInfo` returned through the API in-process, rather than scraping
   stdout). Better long-term — stdout scraping is what made this a class — but
   a larger change.
3. **Retire the test** if the zero-copy trampoline is covered elsewhere. Needs
   someone to confirm that coverage exists; it should not be assumed.

Whichever is chosen, the readiness wait (`"Waiting for"`) needs the same
treatment, and per CLAUDE.md the greps should use `nros_tests::output::*`
constants rather than literals.

## Notes

Found triaging #0422 on freshly rebuilt fixtures. Not caused by phase-336.

## Resolution (2026-08-06)

The issue's analysis holds and its warning was the useful part: retargeting at
the publisher's trace (0429's fix) would have gone green while silently no
longer testing the receive path. So the assertion moved instead of being
weakened.

**Why neither obvious repair works.** Un-slimming the example cannot work —
`CallbackCtx` exposes **no** `MessageInfo` accessor at all, so the line the test
looked for had never come from the receive path. The
`FnMut(&M, Option<&MessageInfo>)` shape lives on the executor's
`.message_info()` subscription builder, which the `Node`/`ExecutableNode` API a
demo is written against never reaches. And adding a `cfg` branch to the example
would break phase-338 W1's portability gate, which asserts every platform copy
of a program is byte-identical after normalization; putting a native-only
zero-copy branch in seven firmware copies to satisfy one test is the wrong
trade.

**What landed.** A purpose-built `packages/testing/nros-tests/bins/
message-info-observer`, which registers through `.message_info()` and prints two
lines per message: the standard `I heard: [...]`
(`output::LISTENER_LOG_PREFIX`, so it is a drop-in for the plain delivery
assertions) and `seq=<n> gid=<hex> ts=<t>`
(`output::MESSAGE_INFO_LOG_PREFIX`, which only the info-carrying callback can
produce). A `None` MessageInfo logs a loud ABSENT error rather than staying
quiet — a silent skip would read as "no messages", which is this issue's own
failure mode.

The marker is a CONSTANT, not a literal, per the rule this issue is an instance
of. The fixture pair is the observer built with and without
`unstable-zenoh-api`, differing in exactly that one feature — which is what
makes "both emit the same `seq=`/`gid=`" a statement about the trampoline rather
than about two unrelated binaries. The example's zero-copy fixture row is
retired: its output was byte-identical to the plain build, so it proved nothing.

All three `zero_copy` tests were broken the same way, not just the one filed —
the other two waited on `"Waiting for"`, which the slimmed example also stopped
printing. All three now pass.

## Verification

```
nros-tests::zero_copy  3 tests run: 3 passed
```

And the positive result the test now actually asserts, from the zero-copy build
against a live router:

```
seq lines: 7   ABSENT: 0
seq=1 gid=e0ff33caf0ff0300e0fe1360d8ff0700 ts=1786012909627000000
seq=2 gid=e0ff33caf0ff0300e0fe1360d8ff0700 ts=1786012910623000000
```

Monotonic sequence, stable GID, through the zero-copy receive path. That is the
property the test claimed to check and previously could not.
