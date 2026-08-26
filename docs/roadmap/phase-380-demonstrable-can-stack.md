# Phase 380 — A demonstrable CAN stack

**Status (2026-08-26). PROPOSED — nothing started.**

Implements [RFC-0082](../design/0082-a-demonstrable-can-stack.md). First phase of
the upstreaming campaign: build the artifact that makes the argument, before
writing the argument.

**Depends on** phase-378, which produced the link and proved it on `vcan0`.
**Blocks** the upstream issue and the three PRs, deliberately.

---

## 1. Shape

```
zenoh fork:  feat/can-link-ros          new branch off 2687c5135, the ROS revision
             feat/can-link             unchanged, sits on main, the PR target
             feat/can-link-1.8         retired at W1

nros:        docker/can-demo/Dockerfile      multi-stage
             docker/can-demo/entrypoint.sh   creates vcan0, runs, asserts, exits
             docker/can-demo/config/*.json5  talker, listener, pico
             docker/can-demo/run.sh          host-side wrapper
             docker/can-demo/README.md
```

## 2. Waves

Ordered so the risky, previously-unproven parts come first: the container can
only be built on top of a link that works at the ROS revision, and that is not
the revision phase-378 used.

| | What | Proves | State |
| --- | --- | --- | --- |
| **W0** | Branch `feat/can-link-ros` off `2687c5135`; port the six commits; full suite plus `vcan0` E2E on the host | the link works at the revision ROS actually builds, which phase-378 never tested | |
| **W1** | Retire `feat/can-link-1.8`; confirm `feat/can-link` still green on `main` | one source, two live targets, no drift | |
| **W2** | Dockerfile builder: zenoh-c `05bd370` patched and built, CAN presence verified in the artifact | the ABI-matched library builds reproducibly from pinned sources | |
| **W3** | Dockerfile builder: zenoh-pico with `Z_FEATURE_LINK_CAN=1`, `BATCH_MULTICAST_SIZE=63` | the island end builds in the image | |
| **W4** | Runtime stage, configs, `vcan0` in the container's own netns, ROS talker → ROS listener | ROS 2 over CAN inside a container, no router, no TCP | |
| **W5** | Add the zenoh-pico peer; three peers on one bus | the island story, end to end, in one command | |
| **W6** | Self-verification: assert counts, print frame tallies, exit nonzero on failure | it is evidence rather than a log dump | |
| **W7** | `README.md` + `run.sh`; state plainly what it does not prove | a stranger can run it and not be misled | |

W0 is the gate. If the link does not work at `2687c5135` the container premise
is wrong and the stack choice has to be revisited before anything is built on it.

## 3. Acceptance criteria

**W0.**
* `cargo test -p zenoh-link-can` passes at `2687c5135`.
* `cargo check -p zenoh --features transport_can` succeeds; the default build is
  unchanged; clippy clean.
* All five `vcan0` end-to-end tests pass on the host at that revision.
* The interop check still passes: a zenoh-pico peer and a zenoh-rs peer exchange
  a fragmented 189-byte payload both ways.
* If any of these fail, the failure is characterised before proceeding — the
  point of this wave is to find out, not to get past it.

**W1.**
* `feat/can-link` still passes its full suite on `main`.
* `feat/can-link-1.8` deleted, and the phase-378 doc updated to say where its
  content went. A branch nobody maintains is worse than no branch.

**W2.**
* The builder stage clones zenoh-c at exactly `05bd370` — the SHA is written in
  the Dockerfile, not derived at build time.
* Both manifests are patched, including `build-resources/opaque-types`. A build
  that patches only the parent fails with `no sigatures found for building
  generic z_take_from_loaned`, which names nothing relevant; a comment in the
  Dockerfile says so.
* Features are `unstable,shared-memory,transport_can`, matching what
  `zenoh_cpp_vendor` passes.
* The stage verifies the CAN link is present in the built `.so` and fails the
  build if not, rather than trusting the feature flag.

**W3.**
* zenoh-pico builds with the CAN link and `BATCH_MULTICAST_SIZE=63`, pinned by
  commit.
* The stage verifies `_z_open_can` is present in the artifact.

**W4.**
* `docker run --cap-add=NET_ADMIN` — **not** `--privileged`, and no host CAN
  interface — creates `vcan0` inside the container.
* `ldd` shows `rmw_zenohd` resolving our `libzenohc.so`, printed in the output as
  evidence rather than assumed.
* A ROS 2 talker and listener exchange every message, with `connect` empty and
  CAN the only `listen` endpoint. No router process runs.

**W5.**
* A zenoh-pico subscriber on the same bus receives the ROS topic.
* `candump` shows traffic on all three identifiers.

**W6.**
* The entrypoint exits **nonzero** if the listener missed a message, if the pico
  peer received none, or if any peer failed to appear.
* Readiness is **polled**, not slept on. A fixed sleep is flaky, and a short one
  is indistinguishable from the 2.5 s convergence interval.
* Message counts and per-identifier frame tallies are printed.
* Deliberately failing the run — a wrong `BATCH_MULTICAST_SIZE`, say — produces a
  nonzero exit and a message naming the cause. An assertion never exercised in
  its failing direction is not known to work.

**W7.**
* The README states the host `vcan` module prerequisite.
* It states, without hedging, that services, actions, parameters and graph
  introspection do **not** work over this transport, and why.
* It states that `vcan` has no bit rate or arbitration, so the demo says nothing
  about timing.
* It names the exact pinned revisions and what they correspond to in ROS.

## 4. Test method

**Tier 0 — the host.** W0 runs the phase-378 suite at the ROS revision.

**Tier 1 — the image.** `docker build` is a test: the builder stages verify their
own artifacts.

**Tier 2 — the run.** `docker run` is the end-to-end test and returns a status.

**Tier 3 — the negative.** At least one deliberately broken configuration, to
show the assertions fire.

## 5. Risks

**The ROS revision may not behave like the tag.** `2687c5135` is off-branch and a
month newer than `release/1.8.0`. Nothing suggests the multicast path differs,
but phase-378 never ran there, so W0 exists to find out rather than assume.

**`vcan` inside a container depends on the host kernel.** The module cannot be
loaded from inside without privileges the demo deliberately does not take. On a
host without it the container fails at the first step; the README must make that
the first thing it says.

**The image is large and slow to build.** A Rust toolchain, a C toolchain and a
ROS base. Acceptable for an artifact whose purpose is to be run once by someone
deciding whether to take the feature seriously — but it is why `run.sh` exists,
so nobody has to remember the flags.

**[OPEN] The fork is not pushed.** Until `feat/can-link-ros` is on a public
remote the image builds from the local working copy, so it is reproducible for us
and not for a reviewer. That is the entire purpose of the artifact, so this
should not stay open past the upstream issue. See RFC-0082 §4.4.
