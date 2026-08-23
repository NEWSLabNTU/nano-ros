---
id: 763
title: "Every ROS 2 interop test stopped a shared daemon and dropped the peer's
  domain — one `Middleware` value produced two different environments"
status: resolved
type: bug
area: testing, interop
related: [issue-0761, issue-0705, rfc-0058]
---

## Problem

Fixing issue 0761 turned two `workspace_features_e2e` lifecycle cases red with
`ConnectionRefusedError: [Errno 111] Connection refused` on the ros2 daemon's
XML-RPC socket. A controlled experiment settled the attribution — the same tier
1, with only those files reverted:

| | real failures |
|---|---|
| baseline | 0 |
| with 0761's poll | 2 (lifecycle, a different pair each run) |

So the poll caused it. But the poll was not the defect; it was the load that
made a standing defect fire.

## Cause — three things, and the first is the one nobody would guess

**1. Every ROS 2 setup began with `ros2 daemon stop`.**
`ros2_env_setup_with_locator` led with it. Under a parallel suite that is a
CROSS-TEST KILL: ros2cli keys its daemon on `ROS_DOMAIN_ID` **alone**
(`get_ros_domain_id()` — no RMW in the key), nothing in that family set a
domain, so ~1500 tests shared domain 0's single daemon and each setup stopped
the daemon the others were mid-query against. 0761's poll ran that setup up to
40 times per case instead of once, so the window widened until it was hit.

What it defended against is real, and is the thing that makes this class
confusing: the daemon caches `RMW_IMPLEMENTATION` and the rest of the
environment from whoever started it, then serves every later caller from that
stale snapshot (ros2cli#1238). But the answer to "the shared daemon holds
someone else's settings" is *do not consult a shared daemon* — not *restart the
shared daemon*, which is what turned a correctness hazard into a race.

**2. The host backend dropped the peer's domain.** `Middleware::Zenoh` has
carried a `domain_id` since phase-309, and its own doc comment says a peer and
the nano-ros node MUST share it because it is the first segment of the
`rmw_zenoh` keyexpr. `HostRosEnv::env_snippet` matched `{ locator, .. }` and
threw it away. `DockerRosEnv` exported it. **One `Middleware` value, two
different environments depending on the backend** — and nothing caught it
because each backend was only ever read on its own.

That single omission produced both visible symptoms: peers that cannot discover
each other (wrong keyexpr segment), and every zenoh test landing on domain 0's
shared daemon.

**3. The remedy was already in the tree.** `workspace_features_e2e:850` reads
`lifecycle get --no-daemon --spin-time 0.1`. The `lifecycle nodes` poll one call
above it — 200 ms for up to 20 s, so up to 100 daemon queries — never got it.
Third instance of "the remedy exists one call over" in a single day (#0757's
drain, #0761's poll, this).

## Fixed 2026-08-23

* **`--no-daemon` on every query that supports it** (13 sites). Each does its own
  discovery with the caller's env, so a stale daemon cannot poison a result and
  a daemon stop is unnecessary. The `ros2 daemon stop` is gone.
* **`ros2_env_setup_zenoh(distro, locator, domain_id)`** is now the single place
  that decides a zenoh peer's environment, and exports `ROS_DOMAIN_ID`
  unconditionally. The two-argument helper delegates with domain 0 as an
  explicit CHOICE rather than an absence, so its 27 callers are untouched.
* **The host backend passes `domain_id` through**, so both backends agree.
* **Gate:** `both_backends_agree_on_middleware_and_domain` asserts both backends
  export the same middleware and domain for the same `Middleware`.
  Mutation-verified: restoring the `{ locator, .. }` match fails it with
  `host backend drops the domain for Zenoh { … domain_id: 42 }`. It asserts on
  the vars that decide whether two peers can SEE each other, not on the whole
  snippet — the backends legitimately differ in how they reach the install.

### A half-fix on the way, worth recording

The first pass converted the READ queries and left `param set` / `param
describe` on the daemon while removing the `daemon stop` that used to refresh
it. `test_ros2_param_set_reconfigures_live_read` then failed with `Node not
found` — a regression introduced by an incomplete sweep of the very class being
fixed. Converted; params 9/9.

## Residual — named, not silently accepted

Four commands cannot bypass the daemon in Humble: `service call`, `topic pub`,
`topic hz`, `action send_goal`. They still ride a shared daemon that nothing
refreshes. Because the daemon key is the domain alone, two tests on the SAME
domain with DIFFERENT RMWs would share a daemon holding the first one's
middleware. Today zenoh sits on domain 0 and the cyclone fixture pairs on 50–58,
so they do not collide — but that is an accident of current assignments, not a
guarantee. Per-RMW (ideally per-test) domains are the real fix, and the
now-honoured `domain_id` is what makes it expressible.

`--no-daemon` also costs per-call latency: every query does its own discovery
instead of reading a cached graph. That is the trade — correctness (each query
uses the caller's RMW and locator; no test can kill a singleton another test is
using) against work per call.

## Follow-up sweep 2026-08-23 — every remaining hand-rolled site

The fix above unified the zenoh env; this swept the rest of the tree for the
same shape. Two more defects, one of which had been shipping a wrong answer.

**`spawn_domain_bridge` bridged the wrong RMW.** Its hand-built setup line
dropped `RMW_IMPLEMENTATION`, so the bridge ran on the image default
`rmw_fastrtps_cpp` while both callers build `Middleware::Cyclonedds` peers and
`docker/ros-editions/Dockerfile` sets no default. The lane passes only because
two RTPS vendors interoperate on a plain topic; over zenoh the same bypass is a
bridge with no shared wire. The comment defending it — "the bridge sets domains
itself" — was true and irrelevant: the domain is inert there
(`InitOptions::set_domain_id` from the YAML outranks the env), which says
nothing about the RMW. `spawn_zenoh_router` shared the bypass, opting the router
out of the snippet that decides which INSTALL it comes from — router and RMW
link one `libzenohc` (RFC-0075, drift class 0609). Both route through
`docker_run` now, which also removed a duplicated container-invocation block.

**Three RMW probes hardcoded `humble`**, in a module whose premise is that the
host's edition varies. On a jazzy host the source fails, the probe returns
false, and every guarded test SKIPS — green having measured nothing on exactly
the host it existed to exercise. One `is_ros2_package_available(distro, pkg)`
now, public signatures unchanged.

**Gate:** `check-ros-env-spelling`. Exemptions structural where possible
(per-language comments; a literal `<distro>` cannot execute, so it is prose
wherever it appears), otherwise a path-keyed allowlist with a reason each —
never a glob, which would silently cover the next script added. `#` is not a
comment in Rust, because `r#"source /opt/ros/…"#` is the likeliest bypass shape.

## Residual, part 2 — the same class one field over

`NROS_LOCATOR` is a nano-ros variable, so a docker `rmw_zenoh_cpp` peer ignores
the `Middleware::Zenoh` locator entirely: the host backend writes a real
`ZENOH_SESSION_CONFIG_URI` pointing at it, the docker backend does not. It works
only because every lane uses the default `tcp/127.0.0.1:7447` with
`--network host`. A non-default locator would be honoured by one backend and
dropped by the other — exactly the shape this issue is about, one field over
from what `both_backends_agree_on_middleware_and_domain` currently guards.
Extending that test to the locator is the obvious next step, and
`spawn_zenoh_router`'s readiness poll hardcodes `127.0.0.1:7447` for the same
reason.

Three smaller ones, all recorded rather than silently carried: the docker Zenoh
arm uses `;` where the other arms use `&&`, so a failed `source` still runs the
inner command against an unsourced ROS; `/opt/nros-overlay` is a permanent no-op
guarding a path RFC-0075 deleted; `HostRosEnv::available()` checks the distro
but never the RMW its `Middleware` selects, so a cell can report available and
then fail at runtime instead of skipping.

## Verification

Tier 1, two samples on the final state: `Real failures: 0 / 0`, matching
baseline exactly (the 3 remaining reds are `skip!` panics the junit rewrite
converts). One intermediate sample had a single unrelated red
(`case_16_rust_xrce_action`, 34 s in-sweep, 2.8 s solo, not reproduced) — the
in-sweep class, and a different test each run.

**Sources:** [ros2cli#1238](https://github.com/ros2/ros2cli/issues/1238) (the
daemon inherits the first invoking shell's environment),
[ros2cli#702](https://github.com/ros2/ros2cli/issues/702) (daemon fails to
respond), [Working with multiple RMW
implementations](https://docs.ros.org/en/rolling/How-To-Guides/Working-with-multiple-RMW-implementations.html)
(verify the daemon is not running with the previous RMW),
[rmw_zenoh#242](https://github.com/ros2/rmw_zenoh/issues/242).
