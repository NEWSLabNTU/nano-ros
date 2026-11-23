---
id: 2920
title: "No node on XRCE publishes `/rosout`: its QoS asks for a 10 s lifespan, which the XRCE backend does not honour"
status: resolved
type: bug
severity: medium
area: [xrce, logging, qos]
related: [phase-556, issue-2107, issue-1973]
found: 2026-11-24
resolved: 2026-11-24
---

## What happened

Every node of a Zephyr XRCE image logged this at boot, and then published no
log records:

```text
<err> nros: qos: publisher '/rosout' refused — the backend does not honour LIFESPAN
<err> nros: /rosout could not be created for node /talker (Transport(IncompatibleQos(QoSPolicyMask(64)))); its log records are not published. A publisher slot the image did not size for is the usual cause.
```

Seen on mps2/an385 (Zephyr 3.7.2, QEMU) with the C talker leaf
(`examples/zephyr/c/talker`), once per node, in every XRCE image built from it.
The second line's guess ("a publisher slot") was wrong for this case.

## Why

The executor creates one `/rosout` writer per node with
`nros_node::rosout::qos_bounded()`, which sets `lifespan_ms: 10_000` to match
upstream's `rcl_qos_profile_rosout_default`. The profile went through the same
check as an application's: `validate_against` the backend's
`supported_qos_policies`. XRCE's mask has no LIFESPAN, so the writer was
refused. The refusal path is the backend's, not the board's, so every XRCE
image on every platform lost `/rosout`.

## Decision: a runtime-owned endpoint's profile is a preference

**Is the lifespan essential to `/rosout`?** No. Upstream's 10 s lifespan bounds
how old a record a LATE-JOINING subscriber is replayed, and that replay exists
because upstream's `/rosout` is TRANSIENT_LOCAL. Ours is VOLATILE
(`qos_bounded`, the documented divergence), so there is no replay to bound; the
lifespan only limits how long an undelivered record may wait in a writer's
queue. Dropping it changes nothing a subscriber can observe beyond that. On a
TRANSIENT_LOCAL `/rosout` (`nros_node::rosout::qos()`, opt-in) a backend
without lifespan would replay records older than 10 s, which is a degradation,
not a loss of the topic. Upstream's own design guidance is that an RMW
REFUSES a policy it cannot honour when it is requested; the question is who
requested it.

**The rule** (applies to every backend through its own mask, and to every
endpoint the runtime creates for itself, never to one topic):

- A profile the RUNTIME chose for its own endpoint (`/rosout`,
  `/parameter_events`, the parameter and lifecycle services,
  `transition_event`) is a preference. Before validation it is fitted to the
  backend's `supported_qos_policies` by the step-down rule best-available
  fields already used (issue 1973): TRANSIENT_LOCAL → VOLATILE, a manual
  liveliness → AUTOMATIC, a finite deadline, lease or lifespan → none, and a
  policy the backend honours not at all → `SYSTEM_DEFAULT`. Each dropped
  policy is reported once per endpoint at INFO level, naming the endpoint, the
  policy and why. History and depth have no weaker form, so a runtime profile
  that needs one a backend lacks is still refused.
- A profile an APPLICATION supplied stays a requirement and is refused loudly,
  naming the policy, as before. So is anything a person folds onto a runtime
  profile afterwards: a per-topic QoS override from the launch plan, a
  declared contract, or an explicit profile passed to
  `rclc_rosout_publisher_init` / `nros_cpp_rosout_publisher_create` (those
  calls are runtime-owned only when the caller passes no profile).

The action servers' and clients' internal services use the stock services
profile, which every backend honours, so they need no fit and are left as
they are.

## Fix

- `nros_rmw::QoSProfile::fit_to_supported(mask) -> (profile, dropped)`, with the
  step-down rule extracted from `resolve_best_available` into one private
  `step_down` both share, now covering lifespan and MANUAL_BY_NODE.
- `nros_node::executor::fit_runtime_qos` applies it and reports what it
  dropped. The executor's `/rosout` and `/parameter_events` writers pass
  `ProfileOrigin::Runtime` through `create_raw_publisher_handle_on`, which fits
  the base profile before folding overrides and the declared contract; the
  parameter and lifecycle services and `transition_event` fit theirs (until
  now they skipped validation altogether). The C and C++ rosout entry points
  fit their default profile the same way.
- The `/rosout could not be created` line no longer guesses at a publisher
  slot; the line before it names the refused policy.
- Tests: `nros-rmw` `runtime_profile_tests` (lifespan dropped and named; a
  supported profile unchanged; every policy steps down by the best-available
  rule; history/depth still refused), and `nros-node`
  `a_backend_without_lifespan_still_gets_a_rosout_writer` (a mock with XRCE's
  mask: the writer comes up with no lifespan, and an application publisher
  asking for the same lifespan is refused naming LIFESPAN). The latter fails
  with the fit disabled.
