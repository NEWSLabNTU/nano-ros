---
id: 2264
title: "The air-gapped install recipe has no offline form for the apt prerequisites and does not say how release assets are named"
status: open
type: bug
area: book, offline
purpose: [industrial]
related: [phase-523, rfc-0116]
found: 2026-11-05
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round one (2026-11-05); round two counts what is left.

Found by phase-523 W3 adoption probe round one, 2026-11-05, on release `nros-0.6.0-nros6` (2026-10-31, 318 commits behind `main` at the time) in a pristine `ubuntu:24.04` container, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n.

## Measured

- I13, vendored-tree.md "Air-gapped / mirrored CI": no way to carry the apt host prerequisites; a guessed asset URL `…/download/0.6.0-nros6/…` → `curl: (22) The requested URL returned error: 404` (the tag is `nros-0.6.0-nros6`). Worked around with an `apt-get --download-only` helper container and `install.sh --dry-run` to learn the tag.
- I14 (`NROS_OFFLINE=1` still reached rustup and cargo) is the release lacking issue 2197's fix (`CARGO_NET_OFFLINE`, landed 11-01); the rustup half is phase-523 "Left". Not refiled.

## Next

The recipe lists the apt packages with a download-only command, and names the release tag and asset pattern (or a command that prints them).
