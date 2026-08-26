---
rfc: 0082
title: "A demonstrable CAN stack: reproduce what ROS ships, in a container"
status: Draft
since: 2026-08
last-reviewed: 2026-08-26
implements-tracked-by: [phase-387]
supersedes: []
superseded-by: null
---

# RFC-0082 — A demonstrable CAN stack

> [RFC-0081](0081-can-link-for-zenoh-rs.md) built a CAN link for zenoh-rs and
> phase-378 proved it: two ROS 2 nodes exchange a topic over CAN with no router
> and no TCP, and a zenoh-pico peer interoperates on the same bus. All of that
> ran on one laptop with hand-assembled parts. This RFC turns it into something
> a stranger can run, because that artifact is the argument for everything
> after it. **[OPEN]** marks unresolved points.

## 1. Position

**The demo container comes before the upstream PRs, not after.**

A survey of `eclipse-zenoh` found **zero CAN prior art** — no issue, no PR, no
code, no RFC, in `zenoh`, `zenoh-c`, `zenoh-pico` or `roadmap`. A SocketCAN
transport is entirely greenfield there, and links are not pluggable: `LinkKind`
is a closed enum behind compile-time features, so a CAN link can only ever be an
in-tree change. That combination — novel transport, no discussion, mandatory
in-tree — is the shape of a pull request that gets sat on.

Their own new-feature checklist asks for "Integration tested", "Examples
provided", and closes with *"Can this feature be split into smaller, incremental
PRs?"*. A container that runs ROS 2 over CAN in one command answers the first
two directly and makes the third a conversation rather than a rebuff.

So the order is: **container, then issue, then PRs.**

## 2. The version reality, which is not what it looks like

This section exists because an earlier reading of it was wrong, and the wrong
version is the kind of mistake that produces a demo which proves nothing.

`rmw_zenoh` does **not** consume released zenoh-c. Every live branch — `humble`,
`jazzy`, `kilted`, `lyrical`, `rolling` — pins the same raw commit in
`zenoh_cpp_vendor/CMakeLists.txt`:

```cmake
# By default, use commit id from branch ROS/zenoh-2687c5135.
set(zenoh_c_commit 05bd370343b5161ca9269649b9a914c9c2dc4170)
if(CARGO_VERSION VERSION_GREATER_EQUAL "1.75" AND CARGO_VERSION VERSION_LESS "1.88")
  set(zenoh_c_commit b31348fa7f94f44f1f7b049c111a710e970a2725)
endif()
```

Both are commits on a **fork** of zenoh-c 1.8.0, and both resolve — through the
vendored `Cargo.lock` — to zenoh core `2687c5135`. That commit is **not on
`main` and not on `release/1.8.0`**: it is `main` as of 2026-04-01
(`e5db0ce8a`) plus one patch, labelled 1.8.0.

Three consequences:

* **Distro choice is orthogonal to zenoh version.** Moving from Humble to
  Rolling changes the `rmw_zenoh_cpp` package version and the zenoh-cpp header
  pin, and changes the zenoh core not at all. There is no ROS 2 distro that gets
  you closer to zenoh `main`.
* **"Matching 1.8.0" is not the same as matching ROS.** The 1.8.0 tag is
  2026-03-13; what ROS builds is a month later and off-branch. Work built on the
  tag is close enough to interoperate — phase-378 demonstrated exactly that —
  but it is not the same code, and a container must not claim otherwise.
* **rmw_zenoh is a full minor behind.** zenoh and zenoh-c `main` are at 1.10.0;
  rmw_zenoh is on 1.8.0-and-a-bit. Nobody upstream has moved it, so whether
  rmw_zenoh even compiles against zenoh-c 1.10 is unknown and untested.

**The container therefore pins what ROS actually ships**, exactly: zenoh-c
`05bd370` and zenoh `2687c5135`. It is a reproduction, not an approximation.

## 3. The port to `main` is nearly free, which decides the branch topology

`LinkMulticastTrait` is **byte-identical** between `release/1.8.0` and `main`:

```
$ git diff origin/release/1.8.0 origin/main -- io/zenoh-link-commons/src/multicast.rs
(empty)
```

`LinkKind`, `LinkManagerBuilderMulticast::make`, `LinkAuthId` and the
`io/zenoh-links/` layout are likewise unchanged. A multicast link crate written
against one compiles against the other, with three mechanical exceptions: the
manager and inspector types now derive `Debug`, each link crate carries a
`[features] uring = []` stanza, and `io/zenoh-link/Cargo.toml` has a `uring`
fan-out list every link must join.

(The **unicast** side did move — every I/O method gained
`priority: Option<Priority>`, plus `supports_priorities()` and `get_fd()`. A
unicast CAN link would be real work. Ours is multicast, so it is not.)

So one source tree serves every target, and the branches differ only in what
they sit on:

| branch | base | purpose |
| --- | --- | --- |
| `feat/can-link` | zenoh `main` (1.10.0) | the upstream PR |
| `feat/can-link-ros` | `2687c5135` | what the container builds |
| ~~`feat/can-link-1.8`~~ | `release/1.8.0` | retired once the above exists |

Keeping three lines would be a maintenance tax for nothing; the 1.8.0 line was
only ever a stepping stone to the ROS revision and is redundant the moment
`feat/can-link-ros` is proven.

## 4. Design

### 4.1 Shape

```
nros/docker/can-demo/
  Dockerfile        multi-stage: builders for zenoh-c and zenoh-pico, ROS runtime
  entrypoint.sh     creates vcan0, runs the demo, asserts, prints evidence, exits
  config/           three session configs: talker, listener, pico
  README.md         prerequisites, what it proves, what it does not
  run.sh            host-side wrapper, so CI can call one thing later
```

**Builder — zenoh-c.** Clones zenoh-c at `05bd370`, adds
`transport_can = ["zenoh/transport_can"]` to **both** its manifest and
`build-resources/opaque-types/Cargo.toml`, redirects the zenoh git dependency at
our fork branch, and builds with `--features unstable,shared-memory,transport_can`
— the feature set `rmw_zenoh` itself uses, since `zenoh_cpp_vendor` passes
`-DZENOHC_BUILD_WITH_UNSTABLE_API=TRUE` and `--features=shared-memory`. Matching
it is not optional: `unstable` and `shared-memory` move struct layouts, and with
no `DT_SONAME` a mismatch is silent memory corruption rather than a link error.

**Builder — zenoh-pico.** Builds `z_pub`/`z_sub` from the nros fork with
`Z_FEATURE_LINK_CAN=1` and `BATCH_MULTICAST_SIZE=63`.

**Runtime.** `ros:humble-ros-base` plus both artifacts, `can-utils`, and
`demo-nodes-cpp`. The library is substituted by `LD_LIBRARY_PATH`, which works
because `librmw_zenoh_cpp.so` and `rmw_zenohd` carry `libzenohc.so` as a plain
`DT_NEEDED` with no `RPATH` or `RUNPATH`, and a cargo feature adds no C API.

### 4.2 What the demo runs

One bus, three peers, no router and no TCP endpoint anywhere:

| peer | identifier | role |
| --- | --- | --- |
| ROS 2 talker | `0x100` | publishes `/chatter` |
| ROS 2 listener | `0x101` | subscribes |
| zenoh-pico | `0x200` | subscribes, standing in for the island |

The entrypoint creates `vcan0` in the container's **own** network namespace —
verified to need only `--cap-add=NET_ADMIN`, not `--privileged`, and no host
interface. The host must have the `vcan` module available; that is the one
prerequisite the container cannot supply for itself.

### 4.3 Self-verifying, or it is not evidence

The demo asserts and exits nonzero on failure. A script that prints logs and
leaves the reader to judge proves nothing to a reviewer who has never seen the
system. It checks that the ROS listener received every message, that the pico
peer received them too, and that frames appeared on all three identifiers.

Three hazards, each found the hard way in phase-378 and each mitigated here:

* **Convergence is not instant.** Multicast peers learn of each other on the
  next periodic `Join`, 2.5 s apart, so peer counts immediately after startup
  are a staircase by open order. The entrypoint **polls for readiness**; a fixed
  sleep would be flaky and a short one would look like a hang.
* **The receive buffer overruns.** A container's `vcan` has no bit rate, so a
  burst arrives as fast as memory allows and the kernel drops the overflow
  before the link sees it — measured at 31% of messages lost, with no error
  anywhere. The configs set `so_rcvbuf` explicitly.
* **The pico batch-size trap.** zenoh-pico advertises `Z_BATCH_MULTICAST_SIZE`
  in `Join` regardless of its link MTU and rejects any peer whose value differs,
  so a stock build never associates. The symptom is one INFO line on the pico
  side and nothing at all on the other. The build pins 63 and the demo fails
  loudly rather than quietly showing two working ROS nodes beside a silent peer.

One hazard disappears by construction: a fresh network namespace per run means
the stray-peer problem — a leftover `ros2` daemon holding an identifier, which
cost an hour in phase-378 — cannot occur.

### 4.4 [OPEN] Where the sources come from

The Dockerfile can take the zenoh fork either from the **build context** or by
**pinned clone**. Build context works today against local branches and needs no
push; pinned clone is what makes the image self-contained for a stranger, and
requires pushing `feat/can-link-ros`.

Starting with build context, because it is unblocked and the switch is a one-line
change. **Pushing the fork is a decision for the repository owner**, and until it
happens the container is reproducible for us and not for a reviewer — which is
the whole point of building it, so this should not stay open long.

## 5. What this proves, and what it does not

**Proves:** zenoh runs over CAN; ROS 2 topics cross a CAN bus with no router and
no TCP; an MCU-class zenoh-pico peer interoperates on the same wire; the whole
thing is reproducible from a pinned, stated set of revisions.

**Does not prove, and the README must say so:**

* **Services, actions, parameters and graph introspection do not work.** A zenoh
  multicast transport routes pushed data only — `mcast_groups` appears solely in
  `pubsub.rs`, never in `queries.rs` or `token.rs`. No CAN link can fix this.
* **Nothing about timing.** `vcan` has no bit rate and no arbitration, so every
  latency and bandwidth figure remains analytic. Hardware is untouched.
* **Bus load is the publisher's whole output**, not the subscribers' interest,
  and no interceptor runs on a multicast face to filter it.

A demo that oversells is worse than none, because the first reviewer to try a
service call will conclude the rest was oversold too.

## 6. Out of scope

The upstream issue text, the three PRs, any `rmw_zenoh` change, CI wiring, and
hardware. Each is its own phase in this campaign.
