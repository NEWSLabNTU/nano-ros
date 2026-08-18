---
id: 675
title: "`test_ros2_service_xrce_client` — the nano-ros XRCE service CLIENT gets no reply from a ROS 2 service server, 3/3 solo"
status: open
type: bug
area: rmw/xrce
related: [phase-366]
---

## Symptom

```
thread 'test_ros2_service_xrce_client' panicked at
packages/testing/nros-tests/tests/xrce_ros2_interop.rs:646:5:
nano-ros XRCE service client got no reply from the ROS 2 service server —
XRCE-DDS reverse service interop regression. Output:
[INFO] nros: session open
[INFO] Service call failed, retrying: Runtime
[INFO] Service call failed, retrying: Runtime
[INFO] Service call failed, retrying: Runtime
[INFO] Service call failed, retrying: Runtime
```

The session opens. Every call returns `Runtime` and no reply arrives inside the
budget (~28 s per attempt).

## Not a flake, and not a load artifact

- **3/3 SOLO.** nextest retried three times with nothing else running;
  every attempt failed identically. That separates it from the documented
  in-sweep load flakes.
- **The FORWARD direction passes**: `test_xrce_service_ros2_client` — nano-ros
  as the service SERVER, ROS 2 as the client — is green in 4.06 s on the same
  host, same agent, same XRCE stack. So the agent, the ROS 2 side and the
  transport are all functioning; it is specifically nano-ros-as-client that gets
  no reply.

## Not caused by phase-366

Found while completing phase-366 and checked before filing:

- The fixture binary is `examples/native/rust/service-client/target-xrce/…`,
  **mtime 2026-08-07** — eleven days older than any commit in that phase. The
  binary under test contains none of the phase's changes.
- The eight fixtures the run did rebuild were all `rmw-cyclonedds` (plus the
  zenoh→cyclone bridge); no XRCE fixture was touched.
- Phase-366's diff is panic-handler placement: `nros::main!()` emits a provider
  only on non-hosted targets, and the per-platform table rows it removed were
  the four embedded ones. A native hosted fixture takes neither path.

Worth noting separately: an eleven-day-old binary that the staleness probe still
calls fresh means this test has not exercised current code for eleven days, so
the regression window is wide and its far edge is not established here.

## Not diagnosed

Root cause NOT determined. `Runtime` is the error the client surfaces, which
says the call failed, not why. Whether the request reaches the ROS 2 server at
all (agent-side bridging of the request topic) versus the reply failing to route
back has not been established — that is the first thing to check, and a fresh
rebuild of the XRCE service-client fixture should come before any of it, so the
test is measuring current code rather than 2026-08-07's.

Filed rather than patched: it is outside the phase that found it, it is
reproducible on demand, and the forward-direction pass makes it a narrow target.
