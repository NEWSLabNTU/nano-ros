---
id: 1559
title: "`env_lock()` is private to `env.rs`'s test module, so it serialises WRITERS against writers and not against the READERS one module over — `check::build` fails nondeterministically"
status: open
type: bug
area: [core, testing]
severity: medium
found: 2026-09-29
related: [0607, 1313, 1394, 0196, 0952]
---

## What

`just ci gate` failed at `check::build` (gate `node-std-tests`) on a branch
touching only `cmake/**` and one cmake test:

```
---- init::ros_args_refusal_tests::from_env_honours_the_domain_override stdout ----
thread '...' panicked at packages/api/nros/src/init.rs:787:9:
assertion `left == right` failed
  left: ""
 right: "tcp/agree:7447"
```

Re-running the suite standalone: **3/3 green, 89 passed.** The single test with
the right features (`-p nros --lib --features env,std`) also passes.

## The mechanism, fully determined

`tcp/agree:7447` appears exactly once in the tree —
`packages/api/nros/src/env.rs:543`:

```rust
let _l = env_lock();
let _g = EnvGuard::set("NROS_LOCATOR", "tcp/agree:7447");
```

`EnvGuard` mutates **process-global** environment. `env.rs` and `init.rs` are
modules of the SAME lib test binary, and cargo runs tests as threads in ONE
process, so that write is visible to every other test while it is live.

The failing test reads the environment twice and compares:

```rust
let env = init()?;                                      // read 1
let ctx = Context::from_env(...with_domain_id(wanted))?;// read 2
assert_eq!(ctx.locator, env.locator);
```

Read 1 landed while the guard was live (`"tcp/agree:7447"`), read 2 after it
dropped (`""`). That is exactly the reported `left`/`right`.

## Why it is 0196 and not just a flake

There IS a lock. `env_lock()` is defined at `packages/api/nros/src/env.rs:315`
and it is the stated safety argument for all three `unsafe` blocks in
`EnvGuard` — `// SAFETY: serialised via env_lock().`, written three times.

Measured:

| | count |
| --- | --- |
| `env_lock()` call sites in `env.rs` | 13 — every mutating test takes it |
| `env_lock()` call sites in `init.rs` | **0** |

`env_lock()` is a private `fn` inside `env.rs`'s `mod tests`. It is not
reachable from `init.rs`, so the readers there cannot take it even if their
author knew to. The lock's REACH (one module) is narrower than the rule it
enforces (no test in this binary observes process env concurrently with a
mutation) — issue 0196's shape, and the reason the `SAFETY` comment is true
locally and false for the binary.

This is the **fourth** sighting of "tests race on process env" here: issues
0607, 1313 and 1394 are all archived instances. So the fix is the CLASS, not
this test.

## What it costs

A nondeterministic `check::build`, which is the failure mode issue 0952 is
about: a red that is not a verdict teaches people to re-run reds, and the next
real regression in that lane reads exactly like this one.

## Fix direction (not decided)

Hoist the lock so both sides can take it — a `pub(crate)` test-support module
holding `env_lock()` and `EnvGuard`, with `env.rs` keeping its 13 call sites and
every test that READS process env taking it too. The sweep is "every test in the
`nros` lib binary that reads the environment", not just the one that failed;
`init.rs`'s `init()` / `Context::from_env` / `Context::default_from_env` tests
are the known population and must be enumerated, not guessed.

Worth considering instead of a lock, since three archived issues did not end the
class: make the readers not depend on ambient process state at all — a `from_env`
that takes an explicit environment map in tests has no race to serialise. A lock
is the smaller change; only one of them removes the shape.

## Acceptance

`cargo test -p nros --lib --features env,std` passes under sustained parallel
load (the condition that produced the red — `check-build` at `-P32`), and no
test in that binary reads process env without holding the shared lock.
