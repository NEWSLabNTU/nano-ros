---
id: 2327
title: "Nothing states which C third-party components a given image compiled in (Mbed TLS?), and the zenoh-pico fork lives on a personal account"
status: open
type: enhancement
area: book, provenance
purpose: [industrial]
related: [phase-523]
found: 2026-11-07
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round two. Found 2026-11-07 on release `nros-0.6.0-nros7` in a pristine `ubuntu:24.04` with Jazzy, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n (runs: `docs/development/adoption-probes/runs/2026-11-07-round-2-{industrial,education}.md`).

## Measured

- I11: Third-party code says Mbed TLS is compiled in only with `link-tls`. `nros setup` provisions mbedtls for this board anyway, and no command tells you whether this image has `link-tls`.
- The zenoh-pico fork is `jerry73204/zenoh-pico`, branch `nano-ros-1.10.1`: 81 commits on top of 1.10.1. A security reviewer will flag a personal-account fork. Reproducing the fork page needs `gh api`.

## Next

Report per image which C components and features were linked, for example in `nros doctor --report` against a built image. Move the forks under the organisation.
