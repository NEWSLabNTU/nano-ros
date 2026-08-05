---
id: 428
title: "every CycloneDDS runtime test fails at node registration — session opens, register does not"
status: open
type: bug
area: rmw
related: [issue-0422, issue-0095, phase-321]
---

## Symptom

Every CycloneDDS runtime test in `native_api` fails; every non-Cyclone test in
the same file passes. Clean split, so this is the backend, not the harness.

```
FAIL  test_native_cyclonedds_rust_service
FAIL  test_native_cyclonedds_rust_action
FAIL  test_native_cyclonedds_rust_talker_to_listener::{C,Cpp}
FAIL  test_native_cyclonedds_talker_to_rust_listener::{C,Cpp}
FAIL  test_threadx_linux_cyclonedds_service
PASS  test_native_service_server_starts::{C,Cpp}      # zenoh, same file
```

## Reproduce without the harness

```console
$ examples/native/rust/service-client/target-cyclonedds/nros-relwithdebinfo/service-client
nros: session open (rmw=cyclonedds)
nros: application error: NodeRegister("native_rs_service_client")
```

The session OPENS — so the backend loads, registers its vtable, and Cyclone
initialises. Node registration is what fails.

## What has been ruled out

- **Stale fixtures.** The binary is newer (2026-08-05 18:52) than every core
  source (nros-core, 2026-08-03 10:37), and rebuilding the native lane does not
  change the result.
- **Missing backend.** `nm` finds 79 Cyclone symbols in the binary, and the
  session-open line reports `rmw=cyclonedds`.
- **Environment.** `zenohd`, `ROS_DISTRO=humble` and `ros2` are all present;
  the zenoh tests in the same file pass.
- **The phase-336 profile work.** The failure reproduces from a directly-invoked
  binary with no profile machinery involved, and the same binary's zenoh sibling
  works.

## What blocks the diagnosis

`NodeRegister` is a COLLAPSED error. `decl_err_from_node`
(`packages/api/nros/src/node_runtime.rs:1437`) maps every `NodeError` except
`ExecutorFull` to `NodeDeclError::Runtime`, which surfaces as this opaque
string:

```rust
fn decl_err_from_node(e: nros_node::NodeError) -> NodeDeclError {
    match e {
        nros_node::NodeError::ExecutorFull => NodeDeclError::ExecutorFull,
        _ => NodeDeclError::Runtime,
    }
}
```

Issue 0095 widened exactly this seam once, to let `NROS_EXECUTOR_MAX_CBS`
surface by name instead of an opaque `NodeRegister`. The same argument applies
to whatever Cyclone is returning here: the message names the node but not the
reason, and `RUST_LOG=debug` / `NROS_LOG=debug` add nothing.

**First step for whoever picks this up:** widen `decl_err_from_node` (or add a
debug-only passthrough) so the underlying `NodeError` variant reaches the
message. Guessing at the cause before that is how a plausible-but-wrong fix
lands.

## Notes

Found triaging issue 0422. Five of that issue's ~19 failures are this one bug.
Not caused by phase-336 — the same set failed on a fresh clone before that work
existed.
