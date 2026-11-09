---
rfc: 0125
title: "Topic visibility and locality: an image-internal topic leaves the wire"
status: Draft
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: []
supersedes: []
superseded-by: null
---

# RFC-0125 — Topic visibility and locality: an image-internal topic leaves the wire

**Status:** Draft

**Realizes, for the board,** the contract grammar in ros-launch-manifest
`docs/design/visibility-and-exports.md` (`exports:` on a manifest scope, a
`visibility:` shorthand, `closed`, and the shared `locality()` function). The
Linux realizer's companion is play_launch `docs/design/topic-visibility-linux.md`.
Builds on [RFC-0060](0060-launch-toolchain-three-layers.md) (one resolver,
three layers), [RFC-0100](0100-rmw-agnostic-sizing-model.md) (the sizing
descriptor) and [RFC-0038](0038-zero-copy-data-transport.md) (in-place
receive). Phase: none yet.

## Summary

The contract (rlm) says which consumers a topic may have; placement and the
image layout say where its endpoints run. nano-ros reads both through the one
shared `locality()` function and, for a topic every endpoint of which is in one
image, does one of two things:

- **Tier 1, any topic in an image that opts in:** deliver locally always, and
  send on the wire only while a remote subscriber is matched.
- **Tier 2, a declared non-public, closed, image-local topic:** remove the
  wire side at build time: no zenoh declaration, no liveliness token, no
  publisher or queryable slot, no wire buffer in the sizing descriptor.

Nothing changes for a contract without `exports`/`visibility`, except Tier 1.

## Motivation

Today every topic in a nano-ros image pays for the wire, whatever its readers:

- zenoh-pico's local loopback (`Z_FEATURE_LOCAL_SUBSCRIBER`,
  `Z_FEATURE_LOCAL_QUERYABLE`) is compiled on for every target
  (`packages/rmw/zenoh/nros-zpico-build/src/lib.rs`, `local_loopback = 1`;
  `zephyr/cmake/nros_rmw_zenoh.cmake`). It is ADDITIVE: the local subscriber
  fires and the wire publish still happens.
- Every declared publisher holds a slot in a fixed table, and the sizing
  descriptor sizes its wire buffer for the worst case.
- `rmw_message_info_t.from_intra_process` exists in the ABI
  (`packages/core/nros-rmw-abi/include/nros/rmw_entity.h`) but no backend
  sets it.
- `use_intra_process_comms` is refused at compile time
  (`packages/api/nros-cpp/include/nros/options.hpp`).

On a microcontroller the slots and buffers are the RAM budget. A controller
that runs its estimator and its control loop in one image pays for a network
endpoint that nothing outside ever reads.

What other MCU systems learned, and why the declaration lives in the contract
rather than in a per-transport list, is in the rlm document; this RFC does not
repeat it.

## Design

### D1. Read, never derive

nano-ros consumes the model's `visibility` and `closed`, and calls rlm's
`locality(model, placement, grouping)` with its own grouping: which nodes
share an image. It never computes locality itself (the two-derivations rule:
issues 0460, 0491). The grouping is the image layout `nros configure`
already resolves from `system.toml`.

### D2. Tier 1: matched-only wire publish

For every publisher, the backend delivers to same-image subscribers directly
and publishes on the wire only while a remote subscriber is matched:

- **zenoh:** zenoh-pico's publisher matching status / matching listener. W0
  verifies the pinned zenoh-pico exposes it under the feature set we build;
  if it does not, Tier 1 on zenoh waits for it and Tier 2 is unaffected.
- **Cyclone DDS:** same-participant delivery is already local; the wire send
  is skipped when no remote reader is matched (`dds_get_publication_matched_status`).
- **XRCE:** the agent owns matching; Tier 1 is not available, and the
  locality report says so.
- **uORB:** already board-local by construction.

A sample delivered locally sets `from_intra_process`.

Tier 1 is safe for a public topic: a `ros2 topic echo` on the host becomes a
remote match and the wire turns on. The cost is the discovery delay before
the first remote sample.

### D3. Tier 2: compile out the wire side

For a topic whose model `visibility` is not `public`, whose `closed` is true
and whose locality is `local` for this image, codegen emits a LOCAL-ONLY
endpoint:

- no `z_declare_publisher`/subscriber/queryable, no liveliness token;
- no slot in `ZPICO_MAX_PUBLISHERS`/`_SUBSCRIBERS`/`_QUERYABLES` (the
  entity inventory stops counting it; the pool floors of issues 1015/1033 are
  unchanged because they belong to the consumer);
- no wire bound in the sizing descriptor: `wire_bound_bytes` covers only
  endpoints that can reach the wire;
- dispatch is a direct call into the subscriber's callback (or its in-place
  slot, RFC-0038), with `from_intra_process` set.

A Tier-2 topic is invisible to `ros2 topic list` and to every host tool. That
is the point, and the reason Tier 2 needs a declaration.

### D4. Fail loudly

- `nros configure` refuses an image whose model says a non-public topic has an
  endpoint outside its scope (rlm `export-violated`), naming the topic, the
  scope and the endpoint.
- A non-public topic whose `closed` is unknown (a node with no contract and
  no metadata probe) stays on the wire, and `nros configure` prints why
  (`closure-unknown`). Unknown is never treated as closed.

### D5. Debug switch

`nros run --expose <scope>` builds the image with Tier 2 off for that scope:
every endpoint is declared, so host tools see it. The build prints that the
optimization is off. The contract does not change.

### D6. Locality report

`nros configure` writes, per image, a locality table beside the sizing
descriptor: each topic, its class, its tier, and the bytes Tier 2 removed. A
change that turns a local topic into a wire topic is a reviewable diff.

## Alternatives considered

- **Infer internal topics with no declaration.** Static analysis knows only
  declared endpoints; a later `echo`, a bag or another tree is invisible to
  it. Inference can SUGGEST (rlm `export-unused`, play_launch's run report),
  never decide. Tier 1 is the safe part of this idea.
- **A per-RMW config knob** (zenoh locality, DDS partition) per topic. That is
  today's dialect problem; the rlm document lists the drift it causes.
- **`scope: image | host`** in the contract. Images and hosts are placement,
  not interface; the same launch tree is placed differently on each target.

## Work items (phase to be opened)

- **W0.** Verify zenoh-pico matching support at our pin and feature set.
- **W1.** Consume `visibility`/`closed`/`locality()` once the rlm grammar
  ships; carry the class into the entity inventory.
- **W2.** Tier 1 on zenoh and Cyclone; set `from_intra_process`.
- **W3.** Tier 2 codegen: local-only endpoints, inventory and sizing
  descriptor changes. Measured acceptance: one image's static RAM before and
  after (`just mem-report`), and a pair test that a host tool cannot see a
  Tier-2 topic while an exported one still delivers.
- **W4.** `--expose`, the locality report, the `nros configure` refusals.

## Decisions (2026-11-10)

1. **Tier 1 is opt-in per image** (`[image.<id>] matched_only_wire = true`)
   until it is measured on every backend. Its cost, the discovery delay
   before the first remote sample, is a behaviour change for an image that
   is public today; it becomes the default once a pair test on each backend
   shows that delay and the saving.
2. **Tier 2 needs no C/C++ API change.** A Tier-2 endpoint is created by the
   generated entry, never by user code, so codegen alone chooses the
   local-only form. User code keeps calling the same publish and subscribe
   API.
3. **Grammar decisions are rlm's** (no tree-wide `internal` level;
   `export-unused` is static) and are recorded in its
   `docs/design/visibility-and-exports.md`.
