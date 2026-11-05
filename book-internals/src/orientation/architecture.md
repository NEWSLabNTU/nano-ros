# Architecture

The whole-system design is written once, in
[`docs/design/ARCHITECTURE.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/ARCHITECTURE.md).
That document narrates the system end to end and links into the RFC behind
each part. This page is a reading order for it, not a second copy.

## Read in this order

1. **The layers.** Application, nano-ros core, RMW backend, platform. Each
   layer can be swapped on its own.
   [RFC-0001](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0001-architecture-overview.md)
   has the crate stack and the directory map.
2. **The feature axes.** RMW × platform × ROS edition (ARCHITECTURE §2,
   [RFC-0005](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0005-rmw-layer-design.md),
   [RFC-0006](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0006-portable-rmw-platform-interface.md)).
3. **How an image is built.** There are three roads (cargo, CMake, west), and
   each carries knobs to the compiler differently:
   [`book-internals/src/pipeline/canonical-build-path.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/book-internals/src/pipeline/canonical-build-path.md).
   `nros configure` is the single resolver every road calls
   ([RFC-0120](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0120-nros-configure-one-resolver.md)).
4. **How an image runs.** A target is a launcher or flasher, a console and a
   link, executed from one RunPlan
   ([RFC-0118](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0118-target-execution-model.md)).
5. **What a board is.** A board is a descriptor plus a registry row. Every
   support claim is generated from the registry
   ([RFC-0114](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0114-board-support-registry-primitives.md)).
6. **The CLI's shape.** A user tier and an `nros dev` tier
   ([RFC-0117](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0117-cli-command-tiers.md)),
   extended by runner and command packages
   ([RFC-0119](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0119-cli-extension-packages.md)).

The user-facing view of the same system, with fewer moving parts, is the
Book's [Architecture Overview](../../background/architecture.html).

## What backs the Book's claims

The Book's introduction states its benefits without evidence, for readers.
This is the evidence, and the place to update when a claim changes.

- **"One node, any board."** The `example_portability` test normalizes every
  platform's copy of each example and fails if any two differ. Two execution
  models are declared exceptions, with written reasons: bare-metal deferred
  dispatch, and Zephyr's component shape. The test names them rather than
  hiding them.
- **"Fits a microcontroller."** The core is `no_std`; `alloc` and `std` are
  opt-in features. Whether an image needs an allocator is the backend's call:
  XRCE allocates only at entity setup, zenoh-pico wants an allocator (a bump
  allocator suffices on bare-metal), and Cyclone DDS allocates per message.
  `scripts/rmw-alloc-sites.py` reports every allocation site.
- **"Checked, not only tested."** Kani harnesses and Verus proofs cover CDR
  serialization, scheduling and protocol correctness. The current counts and
  how to run them are in [Formal Verification](../testing/verification.md).

