---
id: 2168
title: "The installed probe cannot install a published release, so the pinned-project track never runs with a newer CLI installed"
status: resolved
type: limitation
severity: low
area: [ci, testing, installed-sdk]
related: [phase-517, RFC-0095, RFC-0097]
found: 2026-10-31
resolved: 2026-10-31
---

**Home: [phase-517](../../roadmap/archived/phase-517-user-flow-setup-build-flash.md)** — the one part of W16 its Done note records as not run.

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

## Progress (2026-10-31)

* **The option exists.** `PROBE_PLATFORM=pinned-newer` (`just probe
  pinned-newer`) installs the published release `[tool.nros]` pins, fetched
  with its checksum from the GitHub release, instead of the branch's
  asset. The verifier's expected front comes from the harness
  (`PROBE_EXPECT_FRONT`), not a literal.
* **Its first run found a launcher bug** (run 41151393188). The fetch and
  the hand-off worked: inside the project `0.6.0-nros4` answered, fetched by
  `0.6.0-nros5`'s launcher. The build then failed at `nros model-path`,
  `resolver pin changed`. The pinned toolchain's cmake runs the `nros` on
  PATH, which is the `0.6.0-nros5` front. That front saw the inherited
  `NROS_TOOLCHAIN_DISPATCH` marker and ran ITSELF, so it disagreed with the
  model the pinned binary had just written. `dispatch::decide` now honours
  the marker only for the binary it names: that version's store binary, or
  one reporting that version. Any other descendant still follows the pin
  (`an_inherited_marker_does_not_stop_another_binary_from_dispatching`).
* **Still open.** The broken decision is made by the FRONT, a released
  binary, so the track cannot pass until a release carrying this fix is the
  one `[tool.nros]` pins. It stays out of `probe-platforms.yml`'s matrix
  until then, with a comment there. Close this issue when that release is
  cut, the pin moves, and the track is green.

## Resolution

Release `0.6.0-nros6` is published from main at `3f0526fe` and carries the
launcher fix, and `[tool.nros]` pins it. `pinned-newer` is back in
`probe-platforms.yml`. On run 41160765910 it installs `0.6.0-nros6`, enters
a project pinned to `0.6.0-nros4`, and builds there. The pinned toolchain's
cmake now reaches the `0.6.0-nros6` front, which hands off to the pin
instead of running itself. The front check expects `0.6.0-nros6`. The
branch-asset `pinned` track is green in the same run.
