---
id: 356
title: "`px4_e2e.rs` builds SITL against `examples/px4/rust/uorb/{talker,listener}`, retired by phase-277 W7 — the recipe recorded the retirement, the test did not"
status: resolved
type: bug
severity: medium
area: testing, px4
related: [issue-0314, issue-0351, issue-0354, phase-316, phase-325]
---

## Finding (2026-07-31, while doing phase-316 W1)

`packages/testing/nros-px4-sitl-test/tests/px4_e2e.rs:75` points
`EXTERNAL_MODULES_LOCATION` at:

```rust
let externals = project_root().join("examples/px4/rust/uorb");
assert!(
    externals.join("talker/Cargo.toml").is_file()
        && externals.join("listener/Cargo.toml").is_file(),
    "examples/px4/rust/uorb/{{talker,listener}} not found at {}",
    externals.display()
);
```

That directory does not exist and is not tracked. `examples/px4/rust/` contains
one entry — `companion/`.

## The retirement is recorded one file over

`just/px4.just` says so explicitly, in a comment written when it happened:

> phase-277 W7: the former `build-sitl` recipe pointed `EXTERNAL_MODULES_LOCATION`
> at the README-only `examples/px4/rust/uorb/` placeholder — it compiled no
> nano-ros module and the placeholder dir is retired; `build-sitl-cpp` is the
> real `EXTERNAL_MODULES_LOCATION` build.

So the recipe was migrated and annotated, and the test that reaches for the same
path was not. Three phases have passed since.

## Why it did not surface

The test is behind `just px4 test-sitl`, which needs a provisioned
PX4-Autopilot tree and a ~10-minute SITL build, so it runs on demand rather than
in any tier. It also hard-fails honestly (`assert!`, not a silent early return —
per CLAUDE.md), so when it *is* run the diagnosis is immediate. Nothing is
silently green; the lane is simply never exercised.

Note it is `--test px4_e2e` in `just px4 test-sitl` alongside `px4_xrce_e2e`, so
that whole recipe currently cannot pass.

## Ways to fix

**A. Retire the test** — smallest, and arguably correct today. The in-firmware
uORB surface is now `packages/testing/nros-px4-register-check/` (phase-316 W3.1),
whose build IS its assertion; `just px4 build-sitl-cpp` covers it. A second SITL
test that builds the same thing adds nothing until there is a uORB example with
runtime behaviour to observe.

**B. Repoint it at the register-check module** — keeps a SITL lane, but the
module has no runtime behaviour to assert beyond linking, which `build-sitl-cpp`
already proves. Mostly duplicates A's coverage at 10 minutes a run.

**C. Write the uORB talker/listener the test wants.** This is phase-316 **W4.2**,
which is BLOCKED on W4.1 — deciding what a nano-ros uORB example is for, given
PX4 already ships `uxrce_dds_client`. Not resolvable here.

**Recommended: A**, with a pointer to W4.2 so that whoever writes the interop
example knows a runtime lane is missing rather than merely disabled. Deleting a
test that cannot run is honest; leaving it is a lane that reads like coverage in
a directory listing and is not.

## The general shape

Same family as issue 0354: the *consumer* of a retired path was updated, and a
second consumer of the same path was not — here with the extra sting that the
retirement was written down, in prose, in a file 40 lines away. A grep for the
retired path at retirement time would have found both. Retiring a path is a
sweep, not an edit.

## Resolved 2026-07-31 — option A, and the test was worse than this issue said

Removed: `tests/px4_e2e.rs`, its `[[test]]` stanza, and `--test px4_e2e` from
`just px4 test-sitl`. `just px4 test-sitl` now runs Track B only and can pass.

**A second defect, found by reading the body before deleting it.** The retired
path was the reported symptom; it is not the worst thing here. The test does:

```rust
sitl.shell("nros_listener start")?;     // a NANO-ROS module
sitl.shell("nros_talker start")?;       // another NANO-ROS module
sitl.wait_for_log("recv: ts=", RECV_TIMEOUT)?;
```

Both endpoints are nano-ros. So even with the examples restored, this asserts
that nano-ros can read its own publication — a **loopback**. On this backend the
payload IS the PX4 struct, so the interesting failure is a layout or size
disagreement with PX4's `orb_metadata`, and a loopback is satisfied identically
by a correct encoding and a broken one: both ends share the bug. It measures the
harness, not the interop it is named for.

That makes deleting it right twice over, and it is why phase-325's acceptance for
the replacement (W2.4) names a **stock, unmodified PX4 consumer** — `listener
<topic>` in the pxh shell — rather than a second nano-ros module. Issue 0351's
thesis, in the one place it costs the most.

**What was preserved.** The `Px4Sitl` harness pattern is genuinely good and is the
scaffolding phase-325 W2.4 needs — boot, drive the pxh shell, wait for a log line,
dump a snapshot on timeout, SIGTERM the process group on `Drop`. It survives in
`px4_xrce_e2e.rs`, and the exact call sequence is recorded in phase-325 W2.4 so it
is not rediscovered.

**Coverage now.** Track A (in-firmware uORB) has no runtime lane and is honest
about it: `build-sitl-cpp` covers it at build level, `just px4 test-sitl`'s
comment says so, and `examples/px4/README.md` says so. That is strictly more than
before, when a permanently-failing test stood in for a runtime lane that did not
exist.
