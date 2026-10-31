---
id: 2168
title: "The installed probe cannot install a published release, so the pinned-project track never runs with a newer CLI installed"
status: open
type: limitation
severity: low
area: [ci, testing, installed-sdk]
related: [phase-517, RFC-0095, RFC-0097]
found: 2026-10-31
---

**Home: [phase-517](../roadmap/archived/phase-517-user-flow-setup-build-flash.md)** — the one part of W16 its Done note records as not run.

## What

The `pinned` installed track (`just probe pinned`, a `probe-platforms.yml`
matrix row) proves that a project whose `nros-toolchain.toml` pins
`0.6.0-nros4` builds with that release: the launcher fetches it through the
release's own `install.sh` and runs it. W16 asked for the stronger shape, a
NEWER CLI installed while the project pins an older one. That shape has
never run.

The probe always installs the asset it builds from the branch, versioned
`0.0.0-probe`, which sorts BELOW every real release. So the run that went
green (run 40928169451) proves "a pin other than the installed version wins",
but not "an older pin wins over a newer install". `install.sh` fronts
`~/.nros/bin/nros` at the newest installed version, which is the pinned
`0.6.0-nros4` in that run, so the probe never sees a newer front hand off
to an older pin.

## Fix direction

* Give `run-bootstrap-probe.sh` an option to install a PUBLISHED release
  (`install.sh --version <v>` from the GitHub release) instead of the branch's
  asset, for the installed track only.
* Run the `pinned` track with it at the current `[tool.nros] version`
  (`0.6.0-nros5` today) installed. The verifier's front check then expects
  `0.6.0-nros5` as the front, with the pinned `0.6.0-nros4` still answering
  inside the project.
* Keep the branch-asset run as well: it is what tests the branch.
