# Phase 563 — Topic visibility and locality: an image-internal topic leaves the wire

**Status (2026-11-14). OPEN.** W0 DONE; W2 (Tier 1, opt-in per image) OPEN
and first; W1, W3 and W4 BLOCKED on the rlm release that ships `visibility`,
`closed` and `locality()`.

Implements [RFC-0125](../design/0125-topic-visibility-and-locality.md).
Opened by phase-555 W3: the RFC had decisions and W0 done but no phase. Tier 1
does not need the rlm grammar (RFC-0125 D2 and Decision 1: it is opt-in per
image), so it starts now, and the grammar-dependent items wait for rlm.

## Why

Every endpoint an image declares costs a wire entity: a publisher slot, a
liveliness token, a slice of the sizing descriptor's wire bound. It also
sends on the wire whether or not anything off the image listens. A topic
whose publisher and subscribers all sit in one image needs none of that.
RFC-0125 removes it in two tiers. Tier 1 keeps the endpoint and stops the wire
send while no remote subscriber is matched. Tier 2 compiles the wire side out
for a topic the contract declares closed and local.

## Work items

### W0 -- The zenoh mechanism

- **Status:** DONE (2026-11-10, RFC-0125 D2; W0 findings on #3372)

zenoh-pico's publisher write filter (`z_publisher_put`, needs only
`Z_FEATURE_INTEREST`, on for every target) already skips the wire while
nothing matches. A Tier-1 publisher is declared with `Z_LOCALITY_REMOTE` so a
same-image subscriber does not count as a match, and the backend delivers to
same-image subscribers itself.

### W2 -- Tier 1: matched-only wire publish, opt-in per image

- **Status:** OPEN

`[image.<id>] matched_only_wire = true` (RFC-0125 Decision 1) turns it on for
one image; the default stays off until W2's measurements justify it.

- **zenoh:** declare the image's publishers with `Z_LOCALITY_REMOTE` and
  deliver to same-image subscribers in the backend. `REMOTE` turns off
  zenoh-pico's own session-local loopback for that publisher, so the
  backend's local delivery must reach every same-image subscriber.
- **Cyclone DDS:** skip the wire send while
  `dds_get_publication_matched_status` reports no remote reader;
  same-participant delivery is already local.
- **XRCE:** not available (the agent owns matching); the image says so when
  the key is set, rather than ignoring it.
- **uORB:** board-local by construction; nothing to do.
- A sample delivered locally sets `from_intra_process` (in the ABI and the
  C++ message info today; no backend sets it).

**Acceptance:** per backend, a pair test of an image with the key set: a
same-image subscriber receives every sample with no remote peer attached;
the wire carries nothing until a host subscriber matches; then the host
subscriber receives. The test records the delay before the first remote
sample and the wire traffic saved, which is the evidence for changing the
default.

### W1 -- Consume `visibility`, `closed` and `locality()`

- **Status:** BLOCKED (the rlm release that ships the visibility grammar; not tagged)

Read the model's `visibility` and `closed`, and call rlm's
`locality(model, placement, grouping)` with nano-ros's own grouping, the image
layout `nros configure` resolves (RFC-0125 D1). Carry the class into the
entity inventory.

### W3 -- Tier 2: local-only endpoints

- **Status:** BLOCKED (W1)

Codegen emits a local-only endpoint for a non-public, closed, local topic: no
wire declaration, no liveliness token, no pool slot, no wire bound in the
sizing descriptor (RFC-0125 D3).

**Acceptance:** one image's static RAM before and after (`just mem-report`),
and a pair test that a host tool cannot see a Tier-2 topic while an exported
one still delivers.

### W4 -- `--expose`, the locality report, the refusals

- **Status:** BLOCKED (W1 and W3)

`nros run --expose <scope>` (RFC-0125 D5), the per-image locality table beside
the sizing descriptor (D6), and the `nros configure` refusals for
`export-violated` and the `closure-unknown` notice (D4).
