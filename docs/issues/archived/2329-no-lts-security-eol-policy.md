---
id: 2329
title: "The book has no long-term support line, security advisory process, SBOM or end-of-life policy"
status: resolved
type: enhancement
area: book, policy
purpose: [industrial]
related: [phase-523, issue-2268]
found: 2026-11-07
resolved_in: "adoption-r2-provenance branch: SECURITY.md with a private reporting channel; a CycloneDX SBOM per release; the policy in the book"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round two. Found 2026-11-07 on release `nros-0.6.0-nros7` in a pristine `ubuntu:24.04` with Jazzy, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n (runs: `docs/development/adoption-probes/runs/2026-11-07-round-2-{industrial,education}.md`).

## Measured

- I13: the book says only the newest release is supported, nothing is backported, release assets stay published, and nano-ros is pre-1.0. "security", "CVE" and "SBOM" appear nowhere.
- A product team shipping for five years also needs:
  - how to report a vulnerability, and how advisories are published;
  - an EOL statement;
  - how long the pinned Rust channel and the GitHub-hosted forks stay fetchable;
  - mirroring guidance for the forks;
  - how a pin relates to the ROS 2 distro's own EOL.

## Next

This is a maintainer policy decision (issue 2268 ruled "latest only"). At minimum, add a `SECURITY.md` with a reporting channel, link it from the book, and generate an SBOM per release asset. LTS stays out of scope until 1.0, and the book should say so.

## Resolution (2026-11-07)

Maintainer ruling 2026-11-07: minimal policy plus an SBOM.
- `SECURITY.md`: report privately through GitHub Security Advisories, now
  enabled on the repository. Only the newest release is supported, with no LTS
  before 1.0; a release reaches end of life when the next is published; fixes
  are published as advisories.
- `scripts/release/gen-sbom.py` writes a CycloneDX 1.5 SBOM of every crate
  compiled into the asset's binaries and the PX4 archives (`cargo tree -e
  normal,build`, host target, the module's own feature set). The release
  publishes it as `nros-linux-x86_64.cdx.json` and installs it as
  `share/nros/sbom.cdx.json`. Measured on this branch: 223 components.
- Support status › Release support states the policy, the reporting link and
  the SBOM.
