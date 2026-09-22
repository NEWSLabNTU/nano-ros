---
id: 1461
title: "The staleness probe exempts a regenerated-in-place header but not the
  `.stamp` beside it, so a rebuild that changes nothing marks unrelated fixtures
  STALE — and the C pubsub coordinates have produced no runtime result for 18
  days behind that verdict"
status: open
type: bug
area: testing, build
severity: medium
related: [0442, 0445, 0196, 0834, 0088, 0222]
---

## Symptom

Three of the ten `native_example_pubsub_e2e` cases refuse to launch:

```
FAIL case_2_c_zenoh   FAIL case_5_c_cyclone   FAIL case_8_c_xrce

c talker fixture not built: Test fixture is STALE — a source is newer than the
built binary:
  binary: examples/native/c/talker/build-cyclonedds/c_talker
  newer:  .../nros-c-generated/nros/nros_config_generated.h.stamp
```

Nothing ran. The C++ and Rust arms of the same suite pass on the same build.

## Cause of the `.stamp` half — fixed here

Measured on the cyclonedds leaf after a rebuild that touched only C++ headers:

```
2026-09-21 00:46  nros_config_generated.h        <- unchanged
2026-09-21 16:03  nros_config_generated.h.stamp  <- moved
```

The emitter writes the `.h` only when its content changes and touches the
`.stamp` on every build. That is exactly the shape `REGENERATED_INPLACE_HEADERS`
exists for — cbindgen output whose "newer than my binary" says nothing about the
fixture's inputs — one directory down and per-build rather than in-tree. The
exemption list names three headers by path and no stamps, so the stamp reads as
an edit event.

This is issue 0196's shape again: an exemption list narrower than the rule it
encodes.

**The fix carries a guard that is not a formality.** A `.stamp` is exempt only
while the `.h` it stamps exists beside it. A `.stamp` with NO `.h` is issue
0834's absorbing state — cargo is up to date, so the build script never re-emits
the byproduct, the POST_BUILD copy has nothing to copy, and ninja records
success forever. Exempting that would hide the one signal a developer gets. Both
directions are in the selftest.

## The other half is NOT fixed, and it is older than it looks

With the stamp exempted, the zenoh leaf names the **header itself**:

```
2026-09-21 00:37  examples/native/c/talker/build-zenoh/c_talker
2026-09-21 15:56  .../build-zenoh/.../nros_config_generated.h
```

The fixture build rewrote the header and did not relink the binary that includes
it. Either the header's content genuinely changed and the dependency edge is
missing (the 0088/0475 family — a consuming target with no edge to a generated
input), or the emitter rewrites it unconditionally in this build dir while
write-if-changed holds in the cyclonedds one. The two leaves behave differently,
which is the first thing to explain.

The probe's own absorbing-verdict counter (issue 0445) says how long this has
been true:

```
NOT RUN: 8th consecutive stale verdict for this fixture, first 18d ago.
This coordinate has produced no runtime result since then — whatever it
would have done is being absorbed by this message (issue 0445). If the
rebuild does not clear it, suspect the probe before trusting the verdict.
```

**Eighteen days and eight verdicts.** Every C pubsub coordinate has been
reporting a message instead of a result since then, and the counter that says so
is exactly the mechanism issue 0445 added for this case. It worked; nobody read
it.

## Repro

```sh
just build-test-fixtures lane=native
cargo nextest run -p nros-tests --cargo-profile nros-relwithdebinfo \
  -E 'binary(native_example_pubsub_e2e)' --no-fail-fast
stat -c '%y %n' examples/native/c/talker/build-zenoh/c_talker \
  examples/native/c/talker/build-zenoh/cargo/*/nros-c-generated/nros/nros_config_generated.h
```

## Acceptance

* A config-header `.stamp` beside its header is not an edit event; without the
  header it still is. **Done** — `Exemption::ConfigHeaderStamp`, both directions
  in the selftest, and the exemption is reported in the probe's accounting line
  so a reader can see it was applied.
* The C talker leaves relink when their generated config header changes, or the
  header stops being rewritten when its content has not changed. Which of the
  two is the defect is answered by diffing the emitted header against the
  previous content, not by rebuilding until it passes.
* The three C pubsub cases produce a runtime result.
* Whatever those cases have been hiding for 18 days is stated, because "it was
  stale" is not a test result.
