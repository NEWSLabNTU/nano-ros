---
id: 2459
title: "The first push of the starter image leaves its GHCR package private, so no student can pull it until an org admin flips it"
status: resolved
type: limitation
severity: medium
area: [release, install, ci]
related: [phase-551]
found: 2026-11-12
resolved: 2026-11-17
---

**Home: [phase-551](../../roadmap/phase-551-starter-image.md)** (RFC-0127 Q1:
the image is public).

## What

GHCR creates a container package PRIVATE when it is first pushed, whatever
the repository's visibility. `release-nros.yml`'s `starter-image` job (W4)
will make that first push of `ghcr.io/newslabntu/nano-ros-starter` at the
next release. Until an org admin changes the package's visibility, an
anonymous `docker pull` is refused, and the book's one command
(`book/src/start/starter-image.md`) fails for every seat.

Nothing a workflow can do fixes it: changing a package's visibility is not
in the REST API for `GITHUB_TOKEN`; it is a one-time setting in the org's
package page. Later pushes keep the visibility the package has.

## Why nothing noticed

`host-tests.yml`'s `image-track` logs into GHCR before it pulls, so its own pull
succeeds against a private package, and it would have moved `latest` to an
image no student can pull.

## Fix

- `scripts/ci/starter-latest.sh` asks the registry as nobody (an anonymous
  token, then the manifest) before probing. The image exists (the logged-in
  lookup found it), so a refusal means private: the job is red, names the
  settings page, and `latest` does not move. Checked by hand: a public GHCR
  image answers 200, and `newslabntu/nano-ros-starter` (no package yet)
  answers `denied`.
- `docs/release/registry-publishing.md`'s checklist gains the one-time step.

Stays open until the first release has pushed the image and an admin has
set it public; the `image-track` run that moves `latest` is the evidence.

## Resolution (2026-11-17)

Fixed by 80c4b6a5bb (`latest` never moves to a starter no student can
pull). Measured 2026-11-17: an anonymous `GET
ghcr.io/v2/newslabntu/nano-ros-starter/manifests/latest` returns 200, so the
package is public. The host-tests image probe for the starter passed on
2026-11-16. The release-nros run 43806377053 (2026-11-16) failed at `build +
push starter 0.6.0-nros11`, which is a separate fault, not this one.
