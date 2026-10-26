# Phase 519 — The RMW daemons: the zenoh router and the XRCE agent

**Status (2026-10-26). Open; nothing started. W1 and W2 are deferred.** This
phase owns the work that lands on the host-side process a device talks
through, rather than on the device: `rmw_zenohd` for zenoh and
`MicroXRCEAgent` for XRCE. Neither is shipped by nano-ros. The router is ROS's
own (RFC-0075) and the agent is provisioned by `nros setup --tool xrce-agent`.
What nano-ros owns is what it hands them, and how a user starts them.

**Prior:** [phase-516](archived/phase-516-low-severity-issues-with-no-home.md)
W1, which landed the router `downsampling` rule (RFC-0074 enforcement point 1)
and closed [issue 0506](../issues/archived/0506-transport-band-unbounded-preemption.md).
W1 and W2 below are its two items that did not land. They moved here on
2026-10-26, with the rest of phase-516 done.

---

## Why a phase

Router-side and agent-side work kept landing in phases that were about
something else: the ingress budget (phase-462, phase-516), the router's
provenance (phase-362), DDS isolation for the agent (issue 1009). Each one
archived when its own work landed, leaving the daemon half with no home. This
phase is that home.

## Work items

### W1 — a stated interface for the router rule — DEFERRED

*Moved from phase-516 W1d.* An `[image.<id>]` deployment fact that lowers to
the `downsampling` item's `interfaces`, so a router that is not an edge router
applies the rule only on the link facing the device.

**Why.** The rule matches on keyexpr only, so it caps that topic toward every
consumer the router forwards it to. On an edge router (one router per device
segment, which is what `just <platform>::zenohd <model>` starts), every such
consumer is the device, and that is correct. On a router shared with other
consumers, two things go wrong silently:

- another zenoh-pico device on the same topic gets the lowest declared rate;
- a client-mode ROS 2 node, such as `rosbag record`, gets it too, and records
  a fraction of the stream with no error.

A peer-mode host node is not affected, because it links to its peers directly
(measured in phase-516 W1b). An all-serial image is already targeted by
`link_protocols: ["serial"]`. This item is the same targeting for an IP link.

**Never inferred.** The interface name (`eth1`, `enp3s0`) belongs to the host
running the router. The model knows only the device's locator. A wrong guess
does not fail: the rule then matches nothing, looks deployed, and saves nothing.

**Trigger.** The first deployment that puts two devices, or a device and a
recording host node, on one router. Nothing in the tree does yet.

**Done when** a stated interface lowers to `interfaces` (one item per
interface, so devices on different links keep their own rates). It must be
refused when the image's link cannot carry it. A cell shows a consumer on
another link receiving full rate while the device's link is capped.

### W2 — upstream: a burst term for zenoh `downsampling` — DEFERRED

*Moved from phase-516 W1e.* zenoh's interceptor enforces a minimum interval
per keyexpr, so the router half of the ingress budget runs at burst 1
(RFC-0074 "As lowered" point 1). A token-bucket mode (`freq` plus `burst`)
would let it carry the declared burst too.

**Owner: a maintainer.** It is a feature request to eclipse-zenoh, not
a change in this tree. Until it lands, the lowering stays at burst 1.

**Done when** the request is filed and linked here. If upstream ships it, the
lowering gains the term, behind a check of the router's zenoh version.

### W3 — the ingress budget at the XRCE agent

RFC-0074 enforcement point 1 exists only for zenoh. An XRCE subscriber asks
the agent for data with a `uxrDeliveryControl`, and nano-ros asks for
everything (`packages/rmw/xrce/nros-rmw-xrce/src/subscriber.c`: every field
unlimited, `min_pace_period = 0`). Read in the pinned agent (v2.4.3,
`third-party/xrce/agent`):

- `min_pace_period` is carried on the wire and never read, so it paces nothing;
- `max_bytes_per_second` is enforced, as a token bucket in
  `Reader::read_task` whose capacity defaults to one second of the rate.

So the lever that exists is a byte rate with a one-second burst. It is not a
message rate. It is the DEVICE that asks for it, so unlike the zenoh rule it
needs no deployment step. What it does to the excess is untested. The read
blocks on the bucket, so the excess presumably queues in the agent's DDS
reader under its history QoS rather than being dropped. That would make the
device read stale samples, which could be worse than reading fewer.

**Done when** a measurement says whether `max_bytes_per_second`, derived from
`rate_hz` and the type's bound size, bounds the device's receive rate, and
whether what it delivers is fresh. Then either the subscriber requests it from
the declared ingress (with an RFC-0074 amendment), or RFC-0074 records why
XRCE has no enforcement point 1.

### W4 — one door for starting the agent

The router has one: `nros_router_exec <locator> [model]`, run by every
`just <platform>::zenohd`, resolved by `nros_zenohd_bin`, and gated by
`check-zenohd-flag-invocations`. The agent has `just xrce setup` and
`just xrce build-agent`, and nothing that starts it. The READMEs that need it
type it by hand (`MicroXRCEAgent udp4 -p 8888` in
`examples/px4/rust/companion/README.md` and
`examples/workspaces/bridge-xrce/README.md`). That restates the transport and
port the image's domain already owns (RFC-0109 U8: the domain's `locator` is
the owner). The test harness also pins the agent's DDS side to loopback
(`apply_fastdds_profile`, issue 1009), which a hand-started agent does not get.

**Done when** `just <platform>::xrce-agent` (or one shared verb) starts the
provisioned agent on the transport and port the image's locator states. The
READMEs then use it, and a gate keeps a hand-typed `MicroXRCEAgent` command out
of the docs the way `check-zenohd-flag-invocations` does for the router.

## Issues homed here

None yet. W1 and W2 carry no issue. They are the remainder of
[issue 0506](../issues/archived/0506-transport-band-unbounded-preemption.md),
which phase-516 closed.

## Acceptance

- [ ] Each work item is done, or closed with its reason written in this doc.
