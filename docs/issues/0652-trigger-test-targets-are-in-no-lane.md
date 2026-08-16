---
id: 652
title: "`required-features` test targets are in NO lane, so one of them has been
  failing unobserved and a stale feature forward sat in the manifest"
status: open
type: bug
area: testing
related: [issue-0319, issue-0599, phase-359, phase-329]
---

## Symptom

`nros-tests` declares test targets behind `required-features`. Nothing enables
those features:

```
$ grep -rn 'trigger-test' just/*.just justfile
(no matches)
```

So `just check`, `just ci` and `just test-all` all build the package WITHOUT
them, and the targets are never compiled, never linked, never run. Two things
were found sitting behind that:

**1. `trigger_conditions` fails.** Run it and it panics immediately:

```
thread 'test_guard_condition_with_zenoh' panicked at
packages/testing/nros-tests/tests/trigger_conditions.rs:32:48:
Failed to open session: Transport(InvalidConfig)
```

0.15 s, so nothing about the router timing out — `Executor::open` rejects the
config outright. `require_zenohd()` passed (the test skips otherwise) and
`build/zenohd/zenohd` exists, so this is not a missing-router skip; it is a
real failure. The file was last touched by a phase-122 API rename, so it has
plausibly been failing since long before this issue.

Verified pre-existing: it fails identically with and without the phase-359 W10
manifest change made in the same session, so nothing recent caused it.

**2. A stale feature forward.** `trigger-test` carried `"nros-node/std"`. None
of the three targets uses nros-node's std-only surface, and removing it leaves
`wake_latency` (3 tests) and `component_runtime` (3 tests) green — 6/6. It made
this crate one more grantor of the core's `std` flavour for as long as nobody
looked, which is exactly what phase-359 is removing elsewhere.

## Why this is the 0319 class

Issue 0319: *a backend's own test suite belongs in a `check-*` lane, never as a
named step on the `ci` line* — a red sat on main for two days because
`just check` never ran it. This is the same shape one level down: the gate is
not missing, the TARGET is unreachable. A `required-features` target that no
recipe enables is indistinguishable from a deleted one, except that it still
looks like coverage when you read the tests directory.

Issue 0599 is the sibling observation for a whole lane ("reports OK when it
skipped everything").

## Scope

`trigger-test` and `component-runtime-test` are the two features seen here;
the audit should enumerate every `required-features` target in the workspace
rather than fix these two by name — that is the "fix the CLASS" rule, and the
enumeration is a one-liner over `Cargo.toml` `[[test]]` blocks.

## Options

1. **Put them in a lane.** They are host tests needing only zenohd; a
   `check-trigger-tests` recipe alongside the other per-component lanes is the
   cheap version. Requires fixing `trigger_conditions` first, or the lane lands
   red.
2. **Gate the audit instead.** A check that every `required-features` value
   appears in at least one recipe — cheaper to keep honest, and it fails loudly
   the next time someone adds a target behind a new feature.
3. **Delete what is not wanted.** If `trigger_conditions` is obsolete (the
   guard-condition API has moved since phase 122), removing it is more honest
   than a target nobody runs.

(2) is worth doing regardless of which of (1)/(3) is chosen for these targets.

## Reproduce

```
cargo nextest run -p nros-tests --features trigger-test,component-runtime-test \
  --test trigger_conditions --test wake_latency --test component_runtime
```
