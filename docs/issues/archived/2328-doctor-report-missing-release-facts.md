---
id: 2328
title: "`nros doctor --report` omits the full release tag, the Rust toolchain, the ROS 2 and router versions, tool hashes and an image digest"
status: resolved
type: enhancement
area: cli, provenance
purpose: [industrial]
related: [phase-523, issue-2267, issue-2334]
found: 2026-11-07
resolved_in: "adoption-r2-provenance branch: `nros doctor --report` prints the Rust toolchain, the ROS 2 install, tool digests, the lock's sources and the image digest"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round two. Found 2026-11-07 on release `nros-0.6.0-nros7` in a pristine `ubuntu:24.04` with Jazzy, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n (runs: `docs/development/adoption-probes/runs/2026-11-07-round-2-{industrial,education}.md`).

## Measured

- I12: the report prints `nros 0.6.0` (not `0.6.0-nros7`), board status, the tool versions and the zenoh-pico and mbedtls commits.
- It does not print the Rust toolchain (1.99.0), the ROS 2 edition, the `rmw_zenohd` version, the tools' sha256, the crate list, or any hash of the built image.
- It is the one document the book offers to attach to a review, and it cannot stand alone.

## Next

Add those fields. When run inside a workspace with a built image, include the image's sha256 and its linked components (issue 2327).

## Resolution (2026-11-07)

- New lines: `rust` (`rustc -Vv` release, commit, host); `ros 2` (`$ROS_DISTRO`
  and the `rmw_zenoh_cpp` version from `AMENT_PREFIX_PATH`); each image's
  edition; each toolchain package's installed sha256 or source origin; and a
  `lock` section with the lock's `[rust]`, every `[source.*]` tree a build read,
  and the sha256 of each image `nros run` last started (read from its
  `plan.json`).
- The full release tag (`0.6.0-nros8`, not `0.6.0`) is issue 2334's fix.
