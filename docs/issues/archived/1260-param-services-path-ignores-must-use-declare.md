---
id: 1260
title: "`nros` did not compile with `param-services`: two callers discarded
  `declare_parameter`'s #[must_use] result, and no merge-gating lane built
  that feature"
status: resolved
type: bug
area: api, ci
severity: high
resolved_in: "fix(#1260): both results handled; compile-smoke checks param-services"
related: [issue-1226, issue-1163]
---

## Symptom

Every image that declares the `param_services` capability failed to COMPILE on
main. Measured on a downstream (Autoware Safety Island, native posix,
cyclonedds): nros-cpp's size probe builds `nros` and stops at

```
error: unused return value of `Executor::<'s>::declare_parameter` that must be used
    --> packages/api/nros/src/node_runtime.rs:1243:13
error: unused return value of `Executor::<'s>::declare_parameter` that must be used
    --> packages/api/nros/src/node_runtime.rs:1673:21
error: could not compile `nros` (lib) due to 2 previous errors
```

Since phase-426 W4 the per-component inline parameter store is gone, so a
component that calls `declare_parameter` NEEDS `param_services`; without it
boot halts at the first declare (`Unsupported`, code -16). So this was every
image with parameters: declare the capability and it would not compile,
leave it out and it would not boot.

On Zephyr it failed one step earlier, for a second reason. The same bringup
built for MR-CANHUBK344 (zenoh, Zephyr 4.4) stopped in nros-c's size probe at

```
error: `param-services` allocates: add "alloc" to this crate's features
   --> packages/core/nros-node/src/lib.rs:261:1
```

followed by ten E0433 / E0308 errors that cascade from it.

## Cause

`8cb7888f1` (phase-428 W6) put `#[must_use]` on `Executor::declare_parameter`,
for the reason its own doc gives: "A declare that failed and was ignored is a
parameter the program believes it has." `[workspace.lints.rust] warnings =
"deny"` (`Cargo.toml:663`) turns an unused result into an error even under a
plain `cargo check`.

Two callers in the `nros` facade were not updated, and both sit behind
`#[cfg(feature = "param-services")]`:

- `apply_param_services` (`node_runtime.rs:1243`), which seeds the launch
  `<param>` initials;
- the `EntityKind::Parameter` arm (`node_runtime.rs:1673`), which declares a
  component's source-recorded parameter.

No merge-gating lane compiles `nros` with that feature. `compile-smoke` checks
`nros-c` / `nros-cpp` in `C_API_SHIPPED_FEATURES`
(`std,rmw-cffi,platform-posix,ros-humble`), which does not include it;
`check-c`, which adds it (`lanes.just`, the `param_dir` build), runs on schedule
only. `nros-c`'s `param-services` forwards to `nros/param-services`, so that
one line would have caught both sites. The rule was right and the lane that
checks it did not run -- issue 1226's shape.

### The Zephyr lane

`zephyr/CMakeLists.txt` lowers the capability with
`string(APPEND _nros_cpp_features ",param-services")` and nothing else.
`param-services` REQUIRES `alloc` (phase-361 W8.e: a capability names its
requirement and the consumer spells it; it never switches it on), and the
zenoh feature string carries `alloc` only transitively, through
`nros-rmw-zenoh` -- the shape the same file's xrce and cyclonedds notes warn
against. The C-for-C++ nros-c string copies `alloc` only when the C++ string
spells it (the issue 1156 block further down), so nros-c's size probe built
`nros` with `param-services` and no `alloc`.

Sweep, run for every `#[must_use]` declare in the family:

```
git grep -n -E '\.declare_parameter(_on|_with_descriptor|_with_descriptor_on)?\(' -- 'packages/**/*.rs'
```

Besides the two facade sites it found two test binaries discarding the result
as a bare statement: `nros-tests/bins/param-chatter-talker` (`start_value`) and
`nros-tests/bins/sim-clock-listener` (`use_sim_time`, which that fixture's own
comment calls "THE POINT OF THE FIXTURE"). Every other caller binds, asserts or
tail-returns it.

## Fix

- `apply_param_services`: a refused launch `<param>` is logged by name and
  refuses the boot (`Err`), rather than leaving a parameter the program
  believes it has and `ros2 param get` does not.
- The `EntityKind::Parameter` arm returns the new
  `NodeDeclError::ParameterRejected`, split out of `Runtime` for the reason
  issue 0736 split `UnknownPublisher`: a refused parameter and a rejected
  transport handle are different faults, and one opaque variant made them the
  same line on a serial console.
- The two test binaries `assert!` the declare, as `param-two-node-talker`
  already did.
- `compile-smoke` checks `nros-c` / `nros-cpp` once more with
  `param-services`. Verified by reverting the facade fix: that line then fails
  on both call sites (rc 101, 10 s cold); with the fix it passes in 2 s.
- `zephyr/CMakeLists.txt` appends `,alloc,param-services` where it lowers the
  capability. Measured on MR-CANHUBK344 (zenoh, Zephyr 4.4): the image links,
  with DTCM at 83,192 B of 128 KB (63.47%) against 83,072 B without the
  capability.

## Not verified here

The two test binaries were not compiled: `param-chatter-talker` needs `nros
sync`'s generated `std_msgs`, and `sim-clock-listener` needs the zenoh-pico
submodule, neither present in the worktree this was written in. The change to
each is `assert!(<bool>, "<msg>")`.

The Zephyr image was built, not booted. The parameter services allocate from
the heap at runtime, which a link does not measure, and no merge-gating lane
configures a Zephyr image, so nothing yet keeps the `alloc` spelling from
regressing.

The downstream end-to-end run that found this -- Autoware planning_simulator
plus the island, heartbeat cut and restored -- passed with this fix applied:
the island stopped the vehicle (4.25 -> 0.00 m/s), MRM recovered, and the
vehicle resumed.
