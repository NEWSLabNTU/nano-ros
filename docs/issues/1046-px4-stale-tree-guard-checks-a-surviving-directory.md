---
id: 1046
title: "The PX4 SITL stale-tree guard asserts a module DIRECTORY, which outlives the build that linked it — so it passes on exactly the tree it exists to reject"
status: open
type: bug
area: testing
severity: medium
found: 2026-09-04
related: [phase-325, 0196, 0445, 0859]
---

# A guard that diagnoses the problem in prose and cannot detect it

## The guard

`packages/testing/nros-px4-sitl-test/tests/px4_uorb_interop_e2e.rs:65-75`:

```rust
let module_dir = build_dir.join("external_modules/modules/nros_uorb_demo");
assert!(
    module_dir.is_dir(),
    "SITL tree at {} was built WITHOUT the uORB interop example (no {}).\n\
     PX4 takes one EXTERNAL_MODULES_LOCATION per build and they share this \
     build dir, so another root (e.g. `just px4 build-sitl-cpp`) built last.\n\
     Rebuild with:\n    just px4 build-sitl-example",
    ...
);
```

Its message is a correct and complete diagnosis of the failure mode. Its
assertion cannot observe that failure mode.

## Measured 2026-09-04

On a tree whose last SITL build was `just px4 build-bridge-example` (the
phase-325 W3.3 work), i.e. exactly the "another root built last" case the
message describes:

| module | module dir | `bin/px4-<mod>` shim | occurrences in `bin/px4` |
| --- | --- | --- | ---: |
| `nros_uorb_demo` | present | present | **0** |
| `nros_uorb_bridge` | present | present | 8 |

Read with:

    strings -a build/px4_sitl_default/bin/px4 | grep -c nros_uorb_demo

**Both the module directories and the `bin/px4-<mod>` shims survive across
builds.** Only the last root's modules are actually linked into `bin/px4`. So
the guard passes, the test proceeds, and it dies at `nros_uorb_demo start` with
a command-not-found — which is precisely the confusing failure the guard was
written to replace with a clear one.

The guard checks an artifact that outlives the thing it is a proxy for.

## Why this is the repo's own named class

CLAUDE.md, issue 0196: *"Build-side stale probes must watch the same inputs as
test-side gates — a probe that misses `generated/**` lets a museum binary pass
every sweep."* Same shape, different input: here the probe watches a directory
while the claim is about a **binary's contents**.

It is also the 0445 shape — a verdict that is absorbing because the thing it
reports on never ran — and the 0859-0862 shape, where a stale artifact produced
a confident wrong answer. The distinguishing feature in all of them is a check
that cannot tell *present* from *current*.

## Direction

**Assert on binary CONTENT, not directory presence.** The cheapest form is what
measured it above — the module's command name appears in `bin/px4`:

```rust
let px4 = build_dir.join("bin/px4");
let linked = std::fs::read(&px4).map(|b|
    b.windows(NAME.len()).any(|w| w == NAME.as_bytes())).unwrap_or(false);
assert!(linked, "... built WITHOUT {NAME} (its directory survives from an \
        earlier build; the BINARY does not contain it) ...");
```

A byte-scan of a ~59 MB binary is milliseconds and needs no PX4 boot. Booting
SITL and checking the command registers is stronger and much slower; the scan is
enough to turn a confusing runtime failure into the message that already exists.

**Keep the existing message.** It is right. Only the predicate is wrong.

## Not covered

* Whether `px4_xrce_e2e.rs` has the same shape. It guards
  `path.join("Tools").is_dir()` (`:42`), which is a *tree* check rather than a
  built-module check, so it is probably a different question — unverified.
* Whether anything else in the tree proxies "was X built into this artifact" by
  a directory test. Not swept.
* The `bin/px4-<mod>` shims surviving is itself worth a look: a shim for a module
  that is not in the binary is a second stale artifact, and it may be what makes
  the runtime failure read as "command exists but does nothing" rather than
  "command not found". Not investigated.
