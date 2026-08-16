---
id: 653
title: "A ROS-less host has no zenoh router at all, so the getting-started path for the DEFAULT rmw cannot be run — RFC-0075 scoped this consequence to the interop lanes only"
status: open
type: design
severity: high
area: provisioning, docs, rmw
related: [issue-0374, rfc-0075, phase-362, rfc-0056]
---

## The gap

[RFC-0075](../design/0075-zenoh-router-provenance-and-the-unstable-seam.md)
retired the vendored `zenohd` and made the router come from ROS
(`rmw_zenoh_cpp/rmw_zenohd`). Its Consequences section accepts exactly one
casualty:

> * **A ROS-less host cannot run the zenoh interop lanes.** Correct: those lanes
>   test interoperation with ROS 2. They should `skip!` with a reason rather than
>   run against a router no user deploys.

That reasoning is sound **for the interop lanes** — a lane that tests
interoperation with ROS 2 has no business passing without ROS 2. But the
consequence is not confined to those lanes, and the RFC does not say so.

**zenoh-pico runs in client mode and needs a router for any two-process
example**, interop or not. A native talker and a native listener are two
processes. So after phase-362 the getting-started path — `nros setup native`,
default `--rmw zenoh`, run the talker, run the listener — has no router on a
host without ROS, and nothing in the tree provides one:

```
$ nros setup native --rmw zenoh --dry-run
nros setup: native (rmw zenoh) needs 2 package(s):
  zenoh-pico             source 1.7.2 — submodule …
  mbedtls                source 3.x — submodule …
```

No router. And the recipe that used to start one now resolves the ROS binary
or fails:

```
$ just native zenohd          # scripts/dev/zenohd.sh
ERROR: no `rmw_zenoh_cpp/rmw_zenohd` under /opt/ros (ROS_DISTRO=unset).
```

## Why this was not caught

The claim it contradicts is in the book's own getting-started page, under a
heading that asks the question directly (`book/src/getting-started/installation.md`):

> ## Do I need ROS 2 installed?
>
> **For the getting-started path, no.** Verified on a host with no ROS 2 at
> all: … the Rust/C/C++ first node builds and publishes against the in-tree
> `zenohd`.

The verification named in that sentence was real — and its subject, "the
in-tree `zenohd`", no longer exists. phase-362 W5 updated
`book/src/design/rmw.md` (the pairing matrix) and stopped there; the
getting-started page was never swept, so the page still promises a no-ROS path
and still points at a binary that was deleted. That is the
[issue-0196](README.md) shape one level up: the *documentation* half of a
retirement was narrower than the retirement.

`nros setup native --dry-run` does not warn, because from its point of view
nothing is missing — the router is simply not a package it provisions any more.

## What is actually true today

| host | `--rmw zenoh` | `--rmw cyclonedds` |
|---|---|---|
| ROS 2 installed | works — router is `rmw_zenohd` from `/opt/ros` | works |
| no ROS 2 | **no router exists** | works — Cyclone is in-process, no daemon |

So there IS a working ROS-less getting-started path; it is
`--rmw cyclonedds`, which is not the default and which the page does not
present as the answer to its own question.

## Direction

Not exclusive; (1) is the honest-documentation floor and is being done with
this issue, (2) and (3) are the actual decision.

1. **Say what is true.** The page must stop promising a no-ROS default path and
   name `--rmw cyclonedds` as the ROS-less route, with `NROS_RMW_ZENOHD` as the
   escape hatch for a user who has a router by some other means. Done alongside
   [issue 0374](archived/0374-zenohd-has-no-prebuilt-so-nros-setup-native-source-builds-it.md).
2. **Decide whether zenoh should remain the default RMW for a getting-started
   host.** It is the right default for a ROS user and now the wrong one for
   everybody else. Making the default conditional on ROS being present is a
   discoverability trade, not an obvious win — a user who follows two different
   pages and gets two different RMWs is worse off than one who is told once.
3. **Or restore a router for ROS-less hosts**, as an explicitly non-interop
   convenience. RFC-0075's argument against a vendored router is that
   *validating a router nobody runs* is worthless — which is an argument about
   what the TEST LANES should use, and does not by itself forbid shipping one
   for local single-host use. If taken, it must not become a second thing that
   drifts: the interop lanes would still be required to use the ROS router, and
   the convenience router would need to be visibly labelled as not-under-test.

## Not this issue

Whether the tree's `zenohd --listen …` invocations still work. They do not —
`rmw_zenohd` ignores argv and reads `ZENOH_CONFIG_OVERRIDE` — but that is a
separate defect with a separate fix, tracked as
[issue 0654](0654-zenohd-invocations-name-a-retired-binary.md).
